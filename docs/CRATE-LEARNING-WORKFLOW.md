# 9-Step Crate Learning & Specification Workflow

Use this standardized workflow to analyze, specify, and document each crate in the `nexus` repository—bridging low-level primitive implementation with system-level architecture and composable Agent Skills.

---

### 1. What the Crate Does
Provides a concise high-level summary of the crate's core purpose, primary data structures, and runtime/compile-time model. It establishes the crate's single responsibility within the broader low-latency stack without getting bogged down in implementation details.

### 2. When to Use the Crate (and When *Not* to Use It)
Defines the exact workload triggers, latency budgets, and data invariants that justify adopting this primitive. Explicitly listing anti-patterns and "when not to use" boundaries prevents over-engineering cold paths or violating strict constraints (such as SPSC vs. MPMC).

### 3. Where to Use the Crate
Maps the crate to its exact architectural layer inside a sub-100 microsecond system—such as network framing edges, lock-free inter-core IPC boundaries, in-memory state engines, or kernel-bypass driver interfaces. This clarifies how data flows into and out of the primitive across thread and process boundaries.

### 4. Why to Use the Crate
Articulates the concrete performance, memory layout, and safety guarantees over standard library types or general-purpose third-party crates. Highlights specific mechanical sympathies such as zero hot-path allocations, cache-line alignment, deterministic ABIs, branchless execution, and `no_std` support.

### 5. How to Use the Crate
Provides minimal, copy-pasteable usage patterns demonstrating primary API idioms, feature flags (e.g., `default` vs. `no_std`), and common pitfalls like endianness conversion or initialization bounds. Includes a quick-reference cheat sheet mapping common developer needs to exact types and error modes.

### 6. Define the Crate Capabilities and Contracts
Decomposes the crate's public surface into cohesive, testable functional capabilities rather than mirroring internal source files 1:1. Each capability defines a clear boundary of responsibility, separating compile-time guarantees from runtime operations.
* **6.1. Define the Specifications for Each Capability:** Documents every capability using the standardized two-layer template: a **language-agnostic contract** (purpose, layout, invariants, precomputation, behavioral math, and performance constraints) paired with a **Rust target binding** (exact signatures, attributes, and error payloads). These specs serve as the passive source of truth for implementation and testing.

### 7. Define the Source-to-Capabilities Matrix
Maps physical repository modules (`.rs` files and proc-macro crates) many-to-many against the defined capability IDs alongside verification targets (unit tests, `Miri`, `Loom`, `trybuild`, and cycle benchmarks). This enforces complete code traceability and ensures no public behavior or edge case is left unspecified.

### 8. Define the Use Cases
Documents concrete, real-world systems scenarios powered by the crate (such as tagged pointers, binary wire headers, or ring-buffer envelopes), explicitly calling out hardware, concurrency, and ABI caveats. Each documented scenario serves as the direct blueprint for an active **Use Case Agent Skill**.

### 9. Define the Capabilities-to-Use-Cases Matrix
Acts as the cross-reference routing table linking each real-world Use Case (Step 8) to the exact Capability Specifications (Step 6.1) it composes. This gives engineers and orchestrator Agent Skills an immediate lookup map of which specification contracts must be loaded and satisfied to build a given feature.
