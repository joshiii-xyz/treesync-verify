use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_treesync-verify"))
}

fn temp_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "treesync-verify-cli-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("temporary root should be created");
    root
}

#[test]
fn version_and_help_are_available() {
    let version = Command::new(binary()).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains("treesync-verify"));
    let help = Command::new(binary()).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("compare"));
}

#[test]
fn compare_and_explain_are_json_and_text() {
    let root = temp_root("compare");
    let left = root.join("left");
    let right = root.join("right");
    fs::create_dir(&left).unwrap();
    fs::create_dir(&right).unwrap();
    fs::write(left.join("data"), b"same").unwrap();
    fs::write(right.join("data"), b"same").unwrap();
    let output = Command::new(binary())
        .args([
            "compare",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
            "--mode",
            "bytes",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["verdict"], "identical_under_policy");
    let report_path = root.join("report.json");
    fs::write(&report_path, &output.stdout).unwrap();
    let explained = Command::new(binary())
        .args(["explain", report_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(explained.status.success());
    assert!(String::from_utf8_lossy(&explained.stdout).contains("omitted:"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn different_trees_return_one() {
    let root = temp_root("different");
    let left = root.join("left");
    let right = root.join("right");
    fs::create_dir(&left).unwrap();
    fs::create_dir(&right).unwrap();
    fs::write(left.join("data"), b"left").unwrap();
    fs::write(right.join("data"), b"right").unwrap();
    let output = Command::new(binary())
        .args(["compare", left.to_str().unwrap(), right.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["differences"][0]["kind"], "content");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_tree_returns_inconclusive_two() {
    let root = temp_root("missing");
    let output = Command::new(binary())
        .args([
            "compare",
            root.join("missing").to_str().unwrap(),
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["verdict"], "inconclusive");
    fs::remove_dir_all(root).unwrap();
}
