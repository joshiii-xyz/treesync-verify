# Product brief

treesync-verify answers whether two local directory trees match under a
policy that names the comparison dimensions and the dimensions it omits.

Target users are engineers reviewing generated trees, build outputs, staged
artifacts, and local synchronization results who need a machine-readable
reason for equivalence or uncertainty.

The first commands are:

```text
treesync-verify compare LEFT RIGHT --mode bytes
treesync-verify compare LEFT RIGHT --mode metadata
treesync-verify explain report.json
```

Current alternatives include recursive diff tools, ad hoc checksums, and
synchronization tools. They may answer content equality while leaving
permissions, symlinks, hardlinks, sparse allocation, or skipped metadata
implicit. The switching wedge is a small report that makes those omissions
and uncertainty visible without becoming a synchronizer.

Evidence and inference are separate. Filesystem API behavior is grounded in
the primary sources linked in [`docs/research.md`](research.md). The
description of alternative weaknesses and the switching wedge is a product
inference for this focused MVP, not a market-size or adoption claim.

Non-goals are remote synchronization, backup, extraction, deployment, and
claims of a race-free snapshot across a live filesystem.
