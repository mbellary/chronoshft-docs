# Verification and scope

Verified on 19 October 2026 using Rust 1.99.0 and Cargo 1.99.0, Linux x86-64.

## Completed checks

- `cargo test --offline --all-targets`: eight library tests passed; binary and both examples compiled.
- `cargo test --offline --release --all-targets`: the same eight tests passed with optimization; examples compiled.
- `cargo clippy --offline --all-targets -- -D warnings`: passed. The chapter-snippet fixture permits dead code and the deliberately verbose Boolean example.
- `cargo run --offline --example publication`: ran and its Acquire/Release publication assertion passed.
- `branch random`: ran successfully. Optimized x86-64 assembly contains `testb $1, %r9b` followed by a conditional `je` choosing the arithmetic path, distinct from the loop condition. This is specific to this compiler/target build.
- `queue` and `recovery`: ran successfully with the expected behavior below.

## Deterministic queue-model output

These are virtual-time simulation results, not measured machine latency.

| Arrival interval | Service | Injected stall | p50 | p99 | p99.99 | Maximum |
|---|---|---|---|---|---|---|
| 100 ns | 30 ns | 200 ns | 30 ns | 30 ns | 90 ns | 230 ns |
| 20 ns | 30 ns | 200 ns | 100,220 ns | 198,220 ns | 200,200 ns | 200,220 ns |

There are 20,000 independently scheduled arrivals per scenario; the stall occurs at zero-based event index 5,000. Percentiles use nearest rank. The overloaded scenario has no finite buffer/drop model.

## Recovery demonstration

From a snapshot through sequence 9: update 10 is applied; update 12 detects a gap and disables decisions; update 11 does not bypass recovery; a trusted snapshot through 11 plus replay of 12 restores the model to Live with next sequence 13.

Tests cover gap handling, duplicate handling, successful candidate publication, failed recovery preserving old quantities, cutover completeness, session mismatch, invalid levels, sequence overflow, replay capacity, partial writes, WouldBlock, Interrupted, bounded write-attempt count, zero-length writes, frame capacities, and queue catch-up behavior.

## Limits

Hardware-counter collection was attempted with perf, but the host denied access (`perf_event_paranoid=4`; no supported accessible events). No cycles, IPC, branch-miss rate, or isolated misprediction cost is claimed. No privileged configuration was changed.

No actual NIC, exchange feed, EF_VI/DPDK integration, socket backpressure benchmark, wire-latency measurement, PGO training run, or production acceptance test was performed. TCP examples compile; partial-write behavior is exercised with a deterministic mock writer. Recovery is a deliberately small single-owner model. Declared Rust minimum version is based on API usage and was not tested with an older toolchain.

The standalone HTML has inline SVG illustrations, local CSS, and Unicode equations; it makes no external script, font, or image requests. Static structure and diagram XML were checked. Browser preview was unavailable because this environment prohibits local-file browser navigation and local HTTP serving, so browser layout was not visually verified.
