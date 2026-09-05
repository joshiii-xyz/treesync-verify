# treesync-verify

treesync-verify proves whether two local directory trees match under an
explicit bytes or metadata policy.

Status: released v0.1.0.

CI: https://github.com/joshiii-xyz/treesync-verify/actions

## Install

```text
cargo install treesync-verify
```

## Quick start

```text
treesync-verify compare LEFT RIGHT --mode bytes
treesync-verify compare LEFT RIGHT --mode metadata
treesync-verify explain report.json
```

## What it solves

Build and artifact workflows can inspect path presence, content, metadata,
symlink issues, hardlink topology, permissions, and sparse indicators without
silently treating an omitted property as proven equal.

## How it works

The verifier performs sorted, read-only traversal with `symlink_metadata` and
does not follow directory symlinks. Bytes mode hashes regular files up to a
64 MiB bound. Metadata mode compares observable metadata without reading file
content. Reports use `different`, `inconclusive`, or
`identical_under_policy` verdicts.

## Commands and library API

The CLI provides `compare` and `explain`. The library exposes
`compare_trees`, `explain_report`, `CompareMode`, and `ComparisonReport`.
Use `treesync-verify --help` for the complete option list.

## Output and exit codes

- Exit code 0 means the selected policy matched.
- Exit code 1 means an observable difference was found.
- Exit code 2 means the result is inconclusive or the report could not be read.

Every report lists its `mode`, `omitted` dimensions, `differences`, and read
errors. The traversal limit is 100,000 entries at depth 256. Files above 64
MiB are not hashed in bytes mode.

## Safety and data handling

The tool reads local metadata and bounded regular-file bytes. It does not
