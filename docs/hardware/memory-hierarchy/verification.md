# Verification notes

Checked on 19 October 2026 using Rust 1.99.0.

- All five binaries compiled and ran in release mode, using Cargo offline.
- `cargo test --release --offline` passed, including the test that checks the
  scrambled cycle visits every element exactly once before returning.
- The matrix program verified both traversal sums agree. Both quick and default
  workloads ran successfully for the matrix and dependent-walk programs.
- The layout and sharing programs passed their embedded correctness assertions.
- The chapter HTML contains eight embedded SVG diagrams, with unique IDs and no
  broken internal table-of-contents anchors. Each standalone SVG parsed as XML.
- The complete ZIP excludes compiled artifacts and contains the HTML, editable
  Markdown, diagrams, and Cargo source project.

The HTML is self-contained and requires no scripts or remote fonts. Browser
preview was unavailable in this environment, so visual rendering was not
verified in a browser. Static HTML and SVG structure checks passed.

Timings are machine-dependent. The chapter's example latency table is an
explicitly hypothetical teaching model; it is not a record of measured results.
Rust 1.66 is the declared minimum version; compatibility was not separately
tested with that older toolchain.
