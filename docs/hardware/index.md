# Hardware

Understand the processor and memory behavior behind trading-system performance.

## General foundations

Build a general understanding of memory access, caches, and processor behavior. The memory hierarchy foundations remain required alongside the HFT engineering chapter.

- [Memory hierarchy and latency](memory-hierarchy/memory-hierarchy.md)
- [Branch prediction for low-latency trading systems](branch-prediction/branch-prediction-chapter.md)

### Companion material for the foundations

- [Memory hierarchy lab](memory-hierarchy/memory-lab/README.md) and [verification notes](memory-hierarchy/verification.md)
- [Branch prediction lab](branch-prediction/branch-prediction-lab/README.md)
- Standalone HTML editions: [memory hierarchy](memory-hierarchy/memory-hierarchy.html) and [branch prediction](branch-prediction/branch-prediction-chapter.html)

## HFT system engineering

Apply the memory foundations to a market-data reaction path: tail latency, cache-line ownership, virtual memory, NIC buffers, prefetching, and NUMA placement.

- [Memory hierarchy for HFT](hft/memory-hierarchy-for-hft.md): connect architecture concepts to bounded processing and measured Rust systems.
- [Memory hierarchy for HFT validation notes](hft/memory-hierarchy-validation.md): supplied records of the original companion checks.

This is a separate chapter with a different purpose; it complements the general chapter. Its referenced HFT companion project was not included in the supplied research. The general memory lab above remains available for the foundations.

## Related reading

- [Memory alignment and padding — general foundations](../systems/memory-alignment-padding/memory-alignment-padding.md)
- [Memory alignment for HFT — system engineering](../systems/hft/memory-alignment-hft.md)
- [Prefetching](../latency/prefetch/prefetching-hft-chapter.md)
- [Pipeline flushes](../latency/pipeline-flush/pipeline-flush-chapter.md)
