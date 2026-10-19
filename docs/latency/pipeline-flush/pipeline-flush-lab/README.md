# Pipeline flush lab

A dependency-free Rust companion to the pipeline-flush chapter. The models teach correctness and measurement boundaries; they do not connect to an exchange or implement a production trading engine.

Requires Rust 1.73 or newer by declared API usage; verified here with Rust/Cargo 1.99.0 on Linux x86-64. Minimum-version compatibility was not separately tested.

From this directory:

```sh
cargo test --offline --all-targets
cargo test --offline --release --all-targets
cargo clippy --offline --all-targets -- -D warnings
cargo run --release --offline -- branch same
cargo run --release --offline -- branch alternating
cargo run --release --offline -- branch random
cargo run --release --offline -- queue
cargo run --release --offline -- recovery
cargo run --offline --example publication
```

- `branch`: 40 batches after warm-up; reports elapsed batch time divided by item count. This is not per-event tail latency or an isolated branch penalty. Input preparation is outside timing, but whole-process performance counters include preparation.
- `queue`: an open-loop virtual-time model. Results are simulated, not measured hardware latency. One configuration has spare capacity; the other is overloaded.
- `recovery`: ordered updates, a gap, a disabled engine, and a validated snapshot/replay cutover.
- `src/lib.rs`: the bounded recovery engine, fixed-capacity pending writer, queue model, and eight correctness tests.
- `examples/publication.rs`: the chapter's safe one-shot Release/Acquire example.
- `examples/chapter_snippets.rs`: other chapter snippets compiled in separate modules. Dead-code and needless-Boolean warnings are intentionally allowed for these illustrative snippets.

To inspect the actual machine instructions:

```sh
cargo rustc --release --offline --bin pipeline-flush-lab -- --emit=asm
```

Find `branch_work` in `target/release/deps/*.s` and distinguish the data-dependent conditional jump from the loop-back jump. Compiler changes can change the generated code.

On a host with hardware-counter access:

```sh
perf stat -r 5 -e cycles,instructions,branches,branch-misses \
  ./target/release/pipeline-flush-lab branch random
```

Counter access was restricted in the authoring environment; no PMU results are claimed.

## Correctness boundaries

The book contains eight quantity levels with assignment updates, one session identifier, and one global sequence. It does not model an actual exchange protocol, prices, orders, redundant feeds, or recovery I/O. Replay is capped at 64 entries and must be ordered and contiguous after the snapshot boundary. Recovery publication assumes exclusive ownership. `can_trade()` indicates only this miniature engine's state eligibility, not a complete trading authorization or risk decision.

`Pending<N>` keeps unsent bytes after partial writes or WouldBlock. Its work budget counts calls, not nanoseconds. It requires a nonblocking socket when used in a latency-sensitive loop. On fatal connection errors, the caller must reconcile the session and outstanding orders instead of blindly retransmitting.

No unsafe code, external dependencies, kernel bypass bindings, or real network traffic are used by the tests. Real NIC timestamps, production latency objectives, and exchange recovery require deployment-specific integration and validation.
