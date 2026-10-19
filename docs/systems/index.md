# Systems

Build the software foundations for correct, predictable trading systems.

## General foundations

Learn how Rust type layouts, alignment, and padding work before applying them to a trading path. This material remains required alongside the HFT engineering chapter.

- [Memory alignment and padding](memory-alignment-padding/memory-alignment-padding.md): Rust type layouts, padding, and practical experiments.
- [Companion alignment lab](memory-alignment-padding/alignment_lab.rs)

## HFT system engineering

Apply the foundations to trading-system design: hot-state layout, cache-line ownership, safe wire decoding, SIMD boundaries, and page preparation.

- [Memory alignment for HFT](hft/memory-alignment-hft.md): design, check, and measure memory layouts for latency-sensitive trading applications.

This is a separate chapter with a different purpose; it complements the general chapter. Its referenced HFT companion program was not included in the supplied research.

## Related reading

- [Memory hierarchy — general foundations](../hardware/memory-hierarchy/memory-hierarchy.md)
- [Memory hierarchy for HFT — system engineering](../hardware/hft/memory-hierarchy-for-hft.md)
- [Pipeline flushes and state recovery](../latency/pipeline-flush/pipeline-flush-chapter.md)

Future topics include concurrency, allocation, event loops, and state management.
