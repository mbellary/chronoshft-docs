# Branch prediction lab

Companion to the chapter “Branch Prediction for Low Latency Trading Systems in Rust”. Requires a stable Rust toolchain supporting `std::hint::black_box` (Rust 1.66 or later); tested here with rustc and cargo 1.99.0. No external crates, network downloads, or real trading connectivity are used.

```bash
cargo test
cargo run --release -- verify
cargo build --release
./target/release/branch-prediction-lab bench branch mixed 65536 1000
./target/release/branch-prediction-lab bench eager mixed 65536 1000
./target/release/branch-prediction-lab bench table mixed 65536 1000
./target/release/branch-prediction-lab samples eager mixed 100000 10
cargo rustc --release -- --emit=asm
```

Modes: `bench` measures repeated batch scans; `samples` times each decision with clock overhead included. Implementations: `branch`, `eager`, `table`. Profiles: `steady`, `mixed`, `chaotic`, `regime`. Final arguments are records and passes, both positive integers. `verify` checks policy equivalence separately.

This is a synthetic, stateless policy experiment. It is not a market replay, full risk engine, arrival scheduler, or production latency measurement. The table version is intentionally redundant to demonstrate representation; inspect the emitted assembly to see whether a table access remains. Approval counting may be unrolled or vectorized.

`bench` excludes data generation and warmup from its timer. Whole-process perf collection includes them. Collect counters around a region for precise per-event attribution. Do not compare sample-mode percentiles with amortized batch times as if they measure the same quantity.

Validation performed: debug and release equivalence tests passed, all three release throughput modes ran with matching checksums on the mixed profile, and the sample mode ran. Optimized assembly was generated and inspected. Hardware event access was blocked in the validation environment (`perf_event_paranoid=4`), so there is no claimed branch-miss or IPC result. The smoke timing runs were not controlled benchmark comparisons and are intentionally omitted.

To collect hardware counters on a suitable Linux machine, use the commands and interpretation guidance in the chapter. Preserve the machine’s security policy and use authorized PMU access.
