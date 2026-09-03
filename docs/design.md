# Design

## Snapshot model

Traversal uses `symlink_metadata` and sorted directory entries. Directories
are traversed without following symlinks. Each entry records its kind, size,
permissions where available, modification time, symlink target and issue,
hardlink relation where observable, sparse indicator where observable, and a
SHA-256 digest for regular files within the hash bound.

## Comparison policies

`bytes` compares path presence, entry kind, regular-file bytes, and symlink
targets. It omits permissions, timestamps, hardlink topology, and sparse
indicators. `metadata` compares path presence, kind, regular-file size,
permissions, timestamps, symlink targets, hardlink topology, and sparse
indicators. It omits regular-file byte content.

The report always lists omitted dimensions. An equal result is named
`identical_under_policy`, not `identical`, because no policy in the MVP covers
every filesystem property.

## Failure behavior and limits

Missing roots, permission failures, unreadable metadata, hash failures, and
hash-size limits make the result `inconclusive`. Differences that are
observable are retained alongside the uncertainty. Traversal is capped at
100,000 entries and depth 256. Hashing is capped at 64 MiB per regular file.

## Portability boundary

The traversal and process-independent report format use portable Rust APIs.
Unix metadata exposes permissions, device and inode values, and allocated
blocks. The release evidence covers Linux only; filesystem-specific metadata
outside those fields is not claimed.
