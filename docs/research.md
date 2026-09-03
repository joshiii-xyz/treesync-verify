# Research notes

Research date: 2026-09-03.

Primary sources:

- Rust [`symlink_metadata`](https://doc.rust-lang.org/std/fs/fn.symlink_metadata.html)
  documents metadata inspection without following the final symlink.
- Rust [`read_dir`](https://doc.rust-lang.org/std/fs/fn.read_dir.html)
  documents directory iteration and its entry error behavior.
- Rust [`read_link`](https://doc.rust-lang.org/std/fs/fn.read_link.html)
  documents retrieving a symlink's stored target.
- Unix [`MetadataExt`](https://doc.rust-lang.org/std/os/unix/fs/trait.MetadataExt.html)
  documents device, inode, mode, link count, and allocated block fields used
  when available.
- The [`sha2` crate documentation](https://docs.rs/sha2/latest/sha2/)
  documents the digest implementation used for bounded regular-file hashes.

Issue and discussion review:
[Rust filesystem issues](https://github.com/rust-lang/rust/issues?q=is%3Aissue+filesystem+metadata)
and [Rust users filesystem discussions](https://users.rust-lang.org/search?q=filesystem%20symlink)
were treated as context only, not as normative behavior.

Distribution signal: Cargo package metadata and a standalone CLI repository are
prepared for crates.io. Package availability or download counts are
distribution signals, not evidence of willingness to pay.

Evidence grade: the cited Rust documentation supports the API behavior and
available metadata. The policy split, limits, and inconclusive verdict are
design inferences selected to make omitted dimensions explicit.

Rejected alternatives include rsync-like synchronization, which would add
write and recovery behavior, and a kernel-level profiler, which would claim
observations outside a userspace tree walk. Decision: keep the release to
read-only local snapshots, two explicit policies, and bounded evidence.
