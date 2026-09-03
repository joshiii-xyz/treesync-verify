# Limits and non-goals

The 0.1.0 MVP has these limits:

- Linux is the only platform covered by release evidence.
- The verifier handles local directory trees only. It does not compare remote
  mounts, archives, object stores, containers, or deployment targets.
- Symlink targets are checked lexically and for direct existence without
  following directory symlinks during traversal. Complex runtime resolution,
  mount namespaces, and race-free filesystem snapshots are not claimed.
- Hardlink topology, permissions, and sparse indicators are reported only
  where the host exposes the required metadata.
- Regular files larger than 64 MiB are not hashed. Equal size alone is
  inconclusive in bytes mode.
- Directory traversal is capped at 100,000 entries and depth 256.
- The tool does not synchronize, copy, delete, repair, back up, or restore
  files. It is not rsync or a deployment system.
