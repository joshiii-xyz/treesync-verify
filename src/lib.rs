//! Explicit-policy verification for two local filesystem trees.
//!
//! The verifier never follows directory symlinks during traversal. A report
//! distinguishes `different`, `inconclusive`, and `identical_under_policy` so
//! an omitted comparison dimension is never presented as full equivalence.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::time::UNIX_EPOCH;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ENTRIES: usize = 100_000;
pub const MAX_ERRORS: usize = 32;
pub const MAX_DIFFERENCES: usize = 2_048;
pub const MAX_FILE_HASH_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompareMode {
    Bytes,
    Metadata,
}

impl fmt::Display for CompareMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes => formatter.write_str("bytes"),
            Self::Metadata => formatter.write_str("metadata"),
        }
    }
}

impl FromStr for CompareMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "bytes" => Ok(Self::Bytes),
            "metadata" => Ok(Self::Metadata),
            other => Err(format!("unknown comparison mode {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeSummary {
    pub entry_count: usize,
    pub file_count: usize,
    pub directory_count: usize,
    pub symlink_count: usize,
    pub other_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Difference {
    pub path: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub schema_version: u32,
    pub mode: CompareMode,
    pub verdict: String,
    pub left: TreeSummary,
    pub right: TreeSummary,
    pub differences: Vec<Difference>,
    pub omitted: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

impl EntryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone)]
struct EntryRecord {
    kind: EntryKind,
    size: Option<u64>,
    mode: Option<u32>,
    mtime_unix_ns: Option<i64>,
    hash: Option<String>,
    hash_error: Option<String>,
    hash_limited: bool,
    symlink_target: Option<String>,
    symlink_issue: Option<String>,
    hardlink_group: Option<usize>,
    sparse: Option<bool>,
}

#[derive(Debug, Default)]
struct TreeSnapshot {
    entries: BTreeMap<String, EntryRecord>,
    errors: Vec<String>,
    hardlink_groups: BTreeMap<(u64, u64), usize>,
    next_hardlink_group: usize,
}

/// Compare two local trees under one explicit policy.
pub fn compare_trees(left: &Path, right: &Path, mode: CompareMode) -> ComparisonReport {
    let left_snapshot = load_tree(left);
    let right_snapshot = load_tree(right);
    let left_summary = summarize_tree(&left_snapshot);
    let right_summary = summarize_tree(&right_snapshot);
    let omitted = omitted_dimensions(mode);
    let mut differences = Vec::new();
    let mut notes = vec![format!(
        "verdict is limited to the selected {mode} policy; omitted dimensions are listed in omitted"
    )];
    let mut inconclusive = !left_snapshot.errors.is_empty() || !right_snapshot.errors.is_empty();
    let mut paths = BTreeSet::new();
    paths.extend(left_snapshot.entries.keys().cloned());
    paths.extend(right_snapshot.entries.keys().cloned());

    for path in paths {
        let left_entry = left_snapshot.entries.get(&path);
        let right_entry = right_snapshot.entries.get(&path);
        match (left_entry, right_entry) {
            (None, Some(right_entry)) => {
                add_difference(
                    &mut differences,
                    Difference {
                        path: path.clone(),
                        kind: "missing_left".to_owned(),
                        detail: "entry exists only in the right tree".to_owned(),
                    },
                );
                add_symlink_issue(&mut differences, &path, "right", right_entry);
            }
            (Some(left_entry), None) => {
                add_difference(
                    &mut differences,
                    Difference {
                        path: path.clone(),
                        kind: "missing_right".to_owned(),
                        detail: "entry exists only in the left tree".to_owned(),
                    },
                );
                add_symlink_issue(&mut differences, &path, "left", left_entry);
            }
            (Some(left_entry), Some(right_entry)) => {
                compare_entry(
                    &path,
                    left_entry,
                    right_entry,
                    mode,
                    &mut differences,
                    &mut inconclusive,
                );
            }
            (None, None) => unreachable!("path came from one of the entry maps"),
        }
    }

    if differences.len() == MAX_DIFFERENCES {
        notes.push(format!(
            "difference output is capped at {} records",
            MAX_DIFFERENCES
        ));
        inconclusive = true;
    }
    let verdict = if inconclusive {
        "inconclusive"
    } else if differences.is_empty() {
        "identical_under_policy"
    } else {
        "different"
    };
    ComparisonReport {
        schema_version: SCHEMA_VERSION,
        mode,
        verdict: verdict.to_owned(),
        left: left_summary,
        right: right_summary,
        differences,
        omitted,
        notes,
    }
}

/// Render a concise, deterministic explanation for a JSON report.
pub fn explain_report(report: &ComparisonReport) -> String {
    let mut lines = vec![
        format!("verdict: {}", report.verdict),
        format!("mode: {}", report.mode),
        format!("left entries: {}", report.left.entry_count),
        format!("right entries: {}", report.right.entry_count),
        format!("differences: {}", report.differences.len()),
    ];
    if !report.omitted.is_empty() {
        lines.push("omitted:".to_owned());
        lines.extend(report.omitted.iter().map(|item| format!("- {item}")));
    }
    if !report.left.errors.is_empty() {
        lines.push("left errors:".to_owned());
        lines.extend(report.left.errors.iter().map(|item| format!("- {item}")));
    }
    if !report.right.errors.is_empty() {
        lines.push("right errors:".to_owned());
        lines.extend(report.right.errors.iter().map(|item| format!("- {item}")));
    }
    if !report.differences.is_empty() {
        lines.push("differences:".to_owned());
        lines.extend(report.differences.iter().map(|difference| {
            format!(
                "- {} [{}]: {}",
                difference.path, difference.kind, difference.detail
            )
        }));
    }
    lines.join("\n")
}

fn omitted_dimensions(mode: CompareMode) -> Vec<String> {
    match mode {
        CompareMode::Bytes => vec![
            "regular-file metadata including permissions, timestamps, and size policy".to_owned(),
            "hardlink topology".to_owned(),
            "sparse-file indicators".to_owned(),
        ],
        CompareMode::Metadata => vec!["regular-file byte content".to_owned()],
    }
}

fn compare_entry(
    path: &str,
    left: &EntryRecord,
    right: &EntryRecord,
    mode: CompareMode,
    differences: &mut Vec<Difference>,
    inconclusive: &mut bool,
) {
    add_symlink_issue(differences, path, "left", left);
    add_symlink_issue(differences, path, "right", right);
    if left.kind != right.kind {
        add_difference(
            differences,
            Difference {
                path: path.to_owned(),
                kind: "kind".to_owned(),
                detail: format!(
                    "left is {}, right is {}",
                    left.kind.as_str(),
                    right.kind.as_str()
                ),
            },
        );
        return;
    }
    match mode {
        CompareMode::Bytes => compare_bytes(path, left, right, differences, inconclusive),
        CompareMode::Metadata => compare_metadata(path, left, right, differences),
    }
}

fn add_symlink_issue(
    differences: &mut Vec<Difference>,
    path: &str,
    side: &str,
    entry: &EntryRecord,
) {
    if let Some(issue) = entry.symlink_issue.as_deref() {
        add_difference(
            differences,
            Difference {
                path: path.to_owned(),
                kind: "symlink_issue".to_owned(),
                detail: format!("{side} symlink is {issue}"),
            },
        );
    }
}

fn compare_bytes(
    path: &str,
    left: &EntryRecord,
    right: &EntryRecord,
    differences: &mut Vec<Difference>,
    inconclusive: &mut bool,
) {
    match left.kind {
        EntryKind::File => {
            if left.hash_limited || right.hash_limited {
                *inconclusive = true;
                add_difference(
                    differences,
                    Difference {
                        path: path.to_owned(),
                        kind: "unverified_bytes".to_owned(),
                        detail: format!(
                            "file exceeds the {} byte hashing bound",
                            MAX_FILE_HASH_BYTES
                        ),
                    },
                );
            } else if left.hash_error.is_some() || right.hash_error.is_some() {
                *inconclusive = true;
                add_difference(
                    differences,
                    Difference {
                        path: path.to_owned(),
                        kind: "unverified_bytes".to_owned(),
                        detail: format!(
                            "hash error: left={}, right={}",
                            left.hash_error.as_deref().unwrap_or("none"),
                            right.hash_error.as_deref().unwrap_or("none")
                        ),
                    },
                );
            } else if left.hash != right.hash {
                add_difference(
                    differences,
                    Difference {
                        path: path.to_owned(),
                        kind: "content".to_owned(),
                        detail: "regular-file SHA-256 differs".to_owned(),
                    },
                );
            }
        }
        EntryKind::Symlink => {
            if left.symlink_target != right.symlink_target {
                add_difference(
                    differences,
                    Difference {
                        path: path.to_owned(),
                        kind: "symlink_target".to_owned(),
                        detail: format!(
                            "left={:?}, right={:?}",
                            left.symlink_target, right.symlink_target
                        ),
                    },
                );
            }
        }
        EntryKind::Directory | EntryKind::Other => {}
    }
}

fn compare_metadata(
    path: &str,
    left: &EntryRecord,
    right: &EntryRecord,
    differences: &mut Vec<Difference>,
) {
    if left.mode != right.mode {
        add_difference(
            differences,
            Difference {
                path: path.to_owned(),
                kind: "permissions".to_owned(),
                detail: format!("left={:?}, right={:?}", left.mode, right.mode),
            },
        );
    }
    if left.kind == EntryKind::File {
        if left.size != right.size {
            add_difference(
                differences,
                Difference {
                    path: path.to_owned(),
                    kind: "size".to_owned(),
                    detail: format!("left={:?}, right={:?}", left.size, right.size),
                },
            );
        }
        if left.mtime_unix_ns != right.mtime_unix_ns {
            add_difference(
                differences,
                Difference {
                    path: path.to_owned(),
                    kind: "mtime".to_owned(),
                    detail: format!(
                        "left={:?}, right={:?}",
                        left.mtime_unix_ns, right.mtime_unix_ns
                    ),
                },
            );
        }
        if left.sparse != right.sparse {
            add_difference(
                differences,
                Difference {
                    path: path.to_owned(),
                    kind: "sparse".to_owned(),
                    detail: format!("left={:?}, right={:?}", left.sparse, right.sparse),
                },
            );
        }
        if left.hardlink_group != right.hardlink_group {
            add_difference(
                differences,
                Difference {
                    path: path.to_owned(),
                    kind: "hardlink_topology".to_owned(),
                    detail: format!(
                        "left_group={:?}, right_group={:?}",
                        left.hardlink_group, right.hardlink_group
                    ),
                },
            );
        }
    }
    if left.kind == EntryKind::Symlink && left.symlink_target != right.symlink_target {
        add_difference(
            differences,
            Difference {
                path: path.to_owned(),
                kind: "symlink_target".to_owned(),
                detail: format!(
                    "left={:?}, right={:?}",
                    left.symlink_target, right.symlink_target
                ),
            },
        );
    }
}

fn add_difference(differences: &mut Vec<Difference>, difference: Difference) {
    if differences.len() < MAX_DIFFERENCES {
        differences.push(difference);
    }
}

fn load_tree(root: &Path) -> TreeSnapshot {
    let mut snapshot = TreeSnapshot::default();
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) => {
            push_error(
                &mut snapshot.errors,
                format!("root {}: {error}", display_path(root)),
            );
            return snapshot;
        }
    };
    if !metadata.is_dir() {
        push_error(
            &mut snapshot.errors,
            format!("root {} is not a directory", display_path(root)),
        );
        return snapshot;
    }
    visit_directory(root, Path::new(""), 0, &mut snapshot);
    normalize_hardlink_groups(&mut snapshot);
    snapshot
}

fn visit_directory(root: &Path, relative: &Path, depth: usize, snapshot: &mut TreeSnapshot) {
    if depth > MAX_DEPTH {
        push_error(
            &mut snapshot.errors,
            format!("depth exceeds {} at {}", MAX_DEPTH, display_path(relative)),
        );
        return;
    }
    if snapshot.entries.len() >= MAX_ENTRIES {
        push_error(
            &mut snapshot.errors,
            format!("tree exceeds {} entries", MAX_ENTRIES),
        );
        return;
    }
    let iterator = match fs::read_dir(root.join(relative)) {
        Ok(iterator) => iterator,
        Err(error) => {
            push_error(
                &mut snapshot.errors,
                format!("read_dir {}: {error}", display_path(relative)),
            );
            return;
        }
    };
    let mut children = Vec::new();
    for item in iterator {
        match item {
            Ok(entry) => children.push(entry),
            Err(error) => push_error(&mut snapshot.errors, format!("read_dir entry: {error}")),
        }
    }
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        if snapshot.entries.len() >= MAX_ENTRIES {
            push_error(
                &mut snapshot.errors,
                format!("tree exceeds {} entries", MAX_ENTRIES),
            );
            return;
        }
        let child_relative = relative.join(child.file_name());
        visit_entry(root, &child_relative, depth, snapshot);
    }
}

fn visit_entry(root: &Path, relative: &Path, depth: usize, snapshot: &mut TreeSnapshot) {
    let absolute = root.join(relative);
    let metadata = match fs::symlink_metadata(&absolute) {
        Ok(metadata) => metadata,
        Err(error) => {
            push_error(
                &mut snapshot.errors,
                format!("metadata {}: {error}", display_path(relative)),
            );
            return;
        }
    };
    let kind = if metadata.is_file() {
        EntryKind::File
    } else if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.file_type().is_symlink() {
        EntryKind::Symlink
    } else {
        EntryKind::Other
    };
    let hardlink_group = hardlink_group(&metadata, snapshot);
    let mut record = EntryRecord {
        kind,
        size: Some(metadata.len()),
        mode: metadata_mode(&metadata),
        mtime_unix_ns: modified_unix_ns(&metadata),
        hash: None,
        hash_error: None,
        hash_limited: false,
        symlink_target: None,
        symlink_issue: None,
        hardlink_group,
        sparse: sparse_indicator(&metadata),
    };
    match kind {
        EntryKind::File => {
            if metadata.len() > MAX_FILE_HASH_BYTES {
                record.hash_limited = true;
            } else {
                match hash_file(&absolute) {
                    Ok(hash) => record.hash = Some(hash),
                    Err(error) => record.hash_error = Some(error.to_string()),
                }
            }
        }
        EntryKind::Symlink => match fs::read_link(&absolute) {
            Ok(target) => {
                record.symlink_target = Some(display_path(&target));
                record.symlink_issue = symlink_issue(root, relative, &target);
            }
            Err(error) => record.symlink_issue = Some(format!("unreadable: {error}")),
        },
        EntryKind::Directory | EntryKind::Other => {}
    }
    snapshot.entries.insert(display_path(relative), record);
    if kind == EntryKind::Directory {
        visit_directory(root, relative, depth + 1, snapshot);
    }
}

fn normalize_hardlink_groups(snapshot: &mut TreeSnapshot) {
    let mut counts = BTreeMap::new();
    for entry in snapshot.entries.values() {
        if let Some(group) = entry.hardlink_group {
            *counts.entry(group).or_insert(0usize) += 1;
        }
    }
    for entry in snapshot.entries.values_mut() {
        if entry
            .hardlink_group
            .is_some_and(|group| counts.get(&group).copied().unwrap_or(0) < 2)
        {
            entry.hardlink_group = None;
        }
    }
}

fn hardlink_group(metadata: &Metadata, snapshot: &mut TreeSnapshot) -> Option<usize> {
    #[cfg(unix)]
    {
        if metadata.is_file() && metadata.nlink() > 1 {
            let key = (metadata.dev(), metadata.ino());
            if let Some(group) = snapshot.hardlink_groups.get(&key) {
                return Some(*group);
            }
            let group = snapshot.next_hardlink_group + 1;
            snapshot.next_hardlink_group = group;
            snapshot.hardlink_groups.insert(key, group);
            return Some(group);
        }
    }
    let _ = (metadata, snapshot);
    None
}

fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut total = 0u64;
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        total = total.saturating_add(size as u64);
        if total > MAX_FILE_HASH_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file grew beyond hash bound while being read",
            ));
        }
        hasher.update(&buffer[..size]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn symlink_issue(root: &Path, relative: &Path, target: &Path) -> Option<String> {
    if target.is_absolute() {
        return Some("escape".to_owned());
    }
    let mut components = relative
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .components()
        .collect::<Vec<_>>();
    for component in target.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if components.pop().is_none() {
                    return Some("escape".to_owned());
                }
            }
            Component::Normal(value) => components.push(Component::Normal(value)),
            Component::RootDir | Component::Prefix(_) => return Some("escape".to_owned()),
        }
    }
    let mut resolved = PathBuf::new();
    for component in components {
        resolved.push(component.as_os_str());
    }
    if fs::symlink_metadata(root.join(resolved)).is_err() {
        Some("broken".to_owned())
    } else {
        None
    }
}

fn summarize_tree(snapshot: &TreeSnapshot) -> TreeSummary {
    let mut summary = TreeSummary {
        entry_count: snapshot.entries.len(),
        file_count: 0,
        directory_count: 0,
        symlink_count: 0,
        other_count: 0,
        errors: snapshot.errors.clone(),
    };
    for entry in snapshot.entries.values() {
        match entry.kind {
            EntryKind::File => summary.file_count += 1,
            EntryKind::Directory => summary.directory_count += 1,
            EntryKind::Symlink => summary.symlink_count += 1,
            EntryKind::Other => summary.other_count += 1,
        }
    }
    summary
}

fn push_error(errors: &mut Vec<String>, error: String) {
    if errors.len() < MAX_ERRORS {
        errors.push(error);
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn metadata_mode(metadata: &Metadata) -> Option<u32> {
    #[cfg(unix)]
    {
        Some(metadata.mode() & 0o7777)
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

fn modified_unix_ns(metadata: &Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|value| i64::try_from(value.as_nanos()).ok())
}

fn sparse_indicator(metadata: &Metadata) -> Option<bool> {
    #[cfg(unix)]
    {
        if metadata.is_file() {
            let allocated_bytes = metadata.blocks().saturating_mul(512);
            return Some(allocated_bytes < metadata.len());
        }
    }
    let _ = metadata;
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;

    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn temp_root(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("treesync-verify-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temporary root should be created");
        root
    }

    fn cleanup(root: &Path) {
        fs::remove_dir_all(root).expect("temporary root should be removed");
    }

    #[test]
    fn empty_trees_are_equal_under_bytes_policy() {
        let root = temp_root("empty");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        let report = compare_trees(&left, &right, CompareMode::Bytes);
        assert_eq!(report.verdict, "identical_under_policy");
        assert!(!report.omitted.is_empty());
        cleanup(&root);
    }

    #[test]
    fn equal_files_have_equal_bytes() {
        let root = temp_root("equal");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("data.txt"), b"same").unwrap();
        fs::write(right.join("data.txt"), b"same").unwrap();
        let report = compare_trees(&left, &right, CompareMode::Bytes);
        assert_eq!(report.verdict, "identical_under_policy");
        cleanup(&root);
    }

    #[test]
    fn content_mismatch_is_reported() {
        let root = temp_root("content");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("data.txt"), b"left").unwrap();
        fs::write(right.join("data.txt"), b"right").unwrap();
        let report = compare_trees(&left, &right, CompareMode::Bytes);
        assert_eq!(report.verdict, "different");
        assert!(report.differences.iter().any(|item| item.kind == "content"));
        cleanup(&root);
    }

    #[cfg(unix)]
    #[test]
    fn permission_mismatch_is_reported_in_metadata_mode() {
        let root = temp_root("permissions");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("data.txt"), b"same").unwrap();
        fs::write(right.join("data.txt"), b"same").unwrap();
        fs::set_permissions(right.join("data.txt"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(left.join("data.txt"), fs::Permissions::from_mode(0o644)).unwrap();
        let report = compare_trees(&left, &right, CompareMode::Metadata);
        assert_eq!(report.verdict, "different");
        assert!(
            report
                .differences
                .iter()
                .any(|item| item.kind == "permissions")
        );
        cleanup(&root);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_and_broken_link_are_reported() {
        let root = temp_root("links");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        symlink("/outside", left.join("escape")).unwrap();
        symlink("missing", right.join("broken")).unwrap();
        let report = compare_trees(&left, &right, CompareMode::Bytes);
        assert_eq!(report.verdict, "different");
        assert!(
            report
                .differences
                .iter()
                .any(|item| item.kind == "symlink_issue")
        );
        cleanup(&root);
    }

    #[cfg(unix)]
    #[test]
    fn hardlink_pair_is_observable_in_metadata_mode() {
        let root = temp_root("hardlinks");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("a"), b"same").unwrap();
        fs::hard_link(left.join("a"), left.join("b")).unwrap();
        fs::write(right.join("a"), b"same").unwrap();
        fs::write(right.join("b"), b"same").unwrap();
        let report = compare_trees(&left, &right, CompareMode::Metadata);
        assert_eq!(report.verdict, "different");
        assert!(
            report
                .differences
                .iter()
                .any(|item| item.kind == "hardlink_topology")
        );
        cleanup(&root);
    }

    #[test]
    fn permission_denial_or_missing_root_is_inconclusive() {
        let root = temp_root("denial");
        let left = root.join("left");
        let report = compare_trees(&left, &root, CompareMode::Bytes);
        assert_eq!(report.verdict, "inconclusive");
        assert!(!report.left.errors.is_empty());
        cleanup(&root);
    }

    #[test]
    fn unicode_and_unusual_names_are_sorted() {
        let root = temp_root("names");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("spä ce.txt"), b"x").unwrap();
        fs::write(right.join("spä ce.txt"), b"x").unwrap();
        let report = compare_trees(&left, &right, CompareMode::Bytes);
        assert_eq!(report.verdict, "identical_under_policy");
        cleanup(&root);
    }

    #[test]
    fn large_file_hashing_is_bounded() {
        let root = temp_root("large");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        File::create(left.join("large"))
            .unwrap()
            .set_len(MAX_FILE_HASH_BYTES + 1)
            .unwrap();
        File::create(right.join("large"))
            .unwrap()
            .set_len(MAX_FILE_HASH_BYTES + 1)
            .unwrap();
        let report = compare_trees(&left, &right, CompareMode::Bytes);
        assert_eq!(report.verdict, "inconclusive");
        assert!(
            report
                .differences
                .iter()
                .any(|item| item.kind == "unverified_bytes")
        );
        cleanup(&root);
    }

    #[test]
    fn deterministic_report_omits_absolute_roots() {
        let root = temp_root("deterministic");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        fs::write(left.join("data"), b"fixed").unwrap();
        fs::write(right.join("data"), b"fixed").unwrap();
        let first =
            serde_json::to_string(&compare_trees(&left, &right, CompareMode::Bytes)).unwrap();
        let second =
            serde_json::to_string(&compare_trees(&left, &right, CompareMode::Bytes)).unwrap();
        assert_eq!(first, second);
        cleanup(&root);
    }

    #[test]
    fn explain_contains_policy_and_verdict() {
        let root = temp_root("explain");
        let report = compare_trees(&root, &root, CompareMode::Metadata);
        let text = explain_report(&report);
        assert!(text.contains("verdict: identical_under_policy"));
        assert!(text.contains("mode: metadata"));
        cleanup(&root);
    }

    #[test]
    fn deep_tree_limit_is_reported() {
        let root = temp_root("depth");
        let mut current = root.join("left");
        fs::create_dir(&current).unwrap();
        for index in 0..(MAX_DEPTH + 2) {
            current = current.join(format!("d{index}"));
            fs::create_dir(&current).unwrap();
        }
        let report = compare_trees(&root.join("left"), &root.join("left"), CompareMode::Bytes);
        assert_eq!(report.verdict, "inconclusive");
        assert!(
            report
                .left
                .errors
                .iter()
                .any(|item| item.contains("depth exceeds"))
        );
        cleanup(&root);
    }

    #[test]
    fn file_write_fixture_is_read_back() {
        let root = temp_root("write-read");
        let path = root.join("fixture");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"read back").unwrap();
        drop(file);
        assert_eq!(fs::read(&path).unwrap(), b"read back");
        cleanup(&root);
    }
}
