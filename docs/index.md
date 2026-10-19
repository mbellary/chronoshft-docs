# HFT learning path

A growing collection of chapters, worked examples, and Rust experiments for understanding high-frequency trading systems.

| Category | Focus |
| --- | --- |
| [Microstructure](microstructure/index.md) | Markets, order books, matching, and execution mechanics |
| [Systems](systems/index.md) | Data layout, concurrency, and application design |
| [Hardware](hardware/index.md) | CPU execution, caches, and memory architecture |
| [Latency](latency/index.md) | Predictable performance, measurement, and optimization |
| [Networking](networking/index.md) | Market-data transport, order connectivity, and packet processing |
| [Backtesting](backtesting/index.md) | Historical replay, simulation, and strategy evaluation |

## Start with the foundations and existing chapters

1. [Memory hierarchy and latency](hardware/memory-hierarchy/memory-hierarchy.md): understand where data lives and why access patterns matter.
2. [Memory alignment and padding](systems/memory-alignment-padding/memory-alignment-padding.md): connect Rust layouts to memory access.
3. [Branch prediction](hardware/branch-prediction/branch-prediction-chapter.md): understand speculation and measure its effects.
4. [Pipeline flushes](latency/pipeline-flush/pipeline-flush-chapter.md): connect recovery, queues, and application correctness.
5. [Prefetching](latency/prefetch/prefetching-hft-chapter.md): investigate when anticipating data improves predictable performance.

## Apply the memory foundations to HFT systems

Both the general memory chapters above and these HFT-specific chapters are required. The foundations explain memory concepts; the HFT engineering chapters apply them to building and measuring trading systems. They remain separate documents under their top-level categories.

- **Hardware → HFT system engineering:** [Memory hierarchy for HFT](hardware/hft/memory-hierarchy-for-hft.md) and its [supplied validation notes](hardware/hft/memory-hierarchy-validation.md).
- **Systems → HFT system engineering:** [Memory alignment for HFT](systems/hft/memory-alignment-hft.md).

The HFT companion lab sources referenced by the research were not included. Each new chapter identifies this limitation and distinguishes its companion from the existing general labs.

Each chapter has one primary category. Category overviews link to related material across the learning path. New chapters will be added as the documentation grows.
