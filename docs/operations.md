# Operations

## Safe execution

Run `treesync-verify compare LEFT RIGHT --mode bytes` or
`--mode metadata` against paths the operator intends to read. The tool does
not follow directory symlinks, extract archives, modify files, or upload
content. Bytes mode reads regular files up to the 64 MiB hash bound.

## Reports and retention

Comparison output is JSON on stdout. It contains counts, differences, issue
messages, and digests are kept internal to comparison, so it does not dump
file contents. Redirect reports to a location with appropriate permissions
and remove them under the local retention policy.

## Troubleshooting

- `identical_under_policy` means only the selected policy matched. Read the
  `omitted` array before treating the trees as interchangeable.
- `different` means an observable policy difference was found.
- `inconclusive` means a read, permission, depth, entry, or hash limit kept the
  verifier from proving the selected comparison.
- `treesync-verify explain report.json` prints the verdict, omitted dimensions,
  errors, and differences without rereading either tree.

## Recovery

The verifier is read-only. If another process changes a tree during traversal,
rerun the comparison after quiescing that process. The tool does not provide
backup or synchronization recovery.
