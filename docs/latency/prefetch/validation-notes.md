# Validation notes

Validated on 19 October 2026 using installed `rustc 1.99.0` and `cargo 1.99.0`, on x86-64 Linux.

## Completed

- Dependency-free offline release build succeeded.
- `cargo fmt -- --check` passed after formatting.
- `cargo clippy --offline --all-targets -- -D warnings` passed.
- Both unit tests passed. They verify the toy decision against a known result, equivalence at all supported distances, empty input, an extreme lookahead value, and the deliberately chosen record size/alignment.
- Executed all eight supported distances for each of three access patterns: 24 smoke configurations, with 16,384 books, 8,192 requests, and three repetitions each.
- Each pattern produced identical checksums across distances: random 278985; sequential 283404; hot 409887. These checksums identify these exact small test inputs; they are not trading outputs.
- One-book, one-request execution and optional CSV output succeeded.
- Inspected emitted optimized assembly: `prefetcht0` instructions were present in the seven nonzero-distance variants.
- Markdown code fences checked for balance.

## Limits

These checks establish that the example builds and executes as described on the tested environment. They do not establish a latency improvement, a per-event p99.99, live-network behavior, hardware-counter attribution, or readiness for production trading. No claim is made that all supported minimum-toolchain versions or fallback architectures were tested.

The package intentionally excludes generated binaries, local benchmark timing samples, and build artifacts. Build it on the machine you intend to study. Follow the chapter's independent-arrival measurement plan before drawing conclusions about latency tails.
