# Release record

Release status: implementation in progress until the evidence below is
recorded for the exact commit and tag.

Before publication, run from the repository root:

```bash
cargo +stable fmt --all -- --check
cargo +stable check --all-targets --locked
cargo +stable clippy --all-targets --all-features --locked -- -D warnings
cargo +stable test --all-targets --locked
RUSTDOCFLAGS=-Dwarnings cargo +stable doc --no-deps --locked
cargo +stable package --locked
cargo +stable publish --dry-run --locked
cargo +stable install cargo-audit --version 0.22.2 --locked
cargo audit
cargo audit --file fuzz/Cargo.lock
```

The finite fuzz gate is:

```bash
timeout --foreground 300s env RUSTUP_TOOLCHAIN=nightly cargo fuzz build report-json
timeout --foreground 60s env RUSTUP_TOOLCHAIN=nightly cargo fuzz run report-json -- -max_total_time=10 -verbosity=0 -print_final_stats=1
```

Publish only after local tests, CI, Security, CodeQL, and the tag package
workflow pass on the same commit. Verify the registry checksum, docs.rs,
GitHub release, and a fresh install in an isolated Cargo home. Record exact
outputs in `qa/evidence/` and update the portfolio index after reading files
back.
