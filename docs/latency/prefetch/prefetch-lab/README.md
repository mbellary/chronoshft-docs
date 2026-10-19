# Prefetch lab

A dependency-free Rust companion to `../prefetching-hft-chapter.md`.

This is a learning experiment for data lookahead. It does not implement a trading engine or measure per-event tick-to-trade tails.

## Run

Rust 1.85 or newer is declared in the manifest; validation used Rust 1.99.0 on x86-64 Linux. Only that installed toolchain and architecture were tested.

```sh
cargo test --offline
cargo build --offline --release
./target/release/prefetch-lab --help
./target/release/prefetch-lab 0 1048576 131072 40 random baseline.csv
./target/release/prefetch-lab 8 1048576 131072 40 random distance-8.csv
```

Arguments, in order:

1. Prefetch distance: 0, 1, 2, 4, 8, 16, 32, or 64. Zero disables hints.
2. Number of book records, each 64 bytes.
3. Number of decoded requests in the repeatedly replayed trace.
4. Repetitions of that trace.
5. Pattern: `random`, `sequential`, or `hot` (at most 64 symbols).
6. Optional CSV output path. An existing file at that path is overwritten.

The default footprint is 64 MiB of books plus request and sample storage. Use smaller counts for a quick smoke run.

The x86-64 implementation uses a T0 data hint. Other architectures use a no-op fallback, reported by the program. It does not provide an instruction-cache hint.

## Interpret

Each recorded sample is a batch duration divided by the number of requests in that batch. The reported p50, p99, and maximum describe **batch-average ns/item**, not individual events. No p99.99 claim is made.

Setup, warmup, and CSV output are outside the internal timing windows. Whole-process external profiling still includes them. The trace repeats unchanged, and lookahead restarts at every batch boundary. The chapter explains how these constraints differ from a live system.

Compare checksums between distances for identical inputs. Then compare repeated performance runs under controlled conditions. Do not infer a production gain from a small smoke test.

## Development checks

```sh
cargo fmt -- --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline
cargo rustc --offline --release -- --emit=asm
rg -n 'prefetch' target/release/deps/*.s
```

