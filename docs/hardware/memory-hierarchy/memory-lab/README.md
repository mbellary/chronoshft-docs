# Memory hierarchy: Rust companion labs

Requires an installed Rust toolchain (Rust 1.66+; checked here on 1.99.0).
No external crate dependencies. The `sharing` example requires a target with 64-bit atomics.

From this directory:

```sh
cargo run --release --bin basics
cargo run --release --bin matrix
cargo run --release --bin chase
cargo run --release --bin layout
cargo run --release --bin sharing
cargo test --release
```

Use `--offline` after `--release` if you want Cargo to avoid network access.

Short smoke checks:

```sh
cargo run --release --bin matrix -- --quick
cargo run --release --bin chase -- --quick
```

`matrix` compares traversal time across seven trials, alternating order.
`chase` measures five trials per footprint of dependent indexed steps.
Their output describes these loops, not universal hardware constants.
Allocation, construction, warm-up, and printing are outside timed regions.

The default matrix buffer is 32 MiB. The largest chase index buffer is 64 MiB;
cycle construction temporarily also uses a 64 MiB ordering vector.

The cycle-construction unit test checks that every element is visited before
returning to the start. The matrix executable checks the two sums agree.
The other executables print layout or correctness information; they are not
performance benchmarks. See the chapter for interpretation and limitations.
