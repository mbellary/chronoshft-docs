# Memory Hierarchy for HFT: From Textbook Concepts to Measured Rust Systems

!!! note "Purpose and companion availability"
    This HFT engineering chapter complements the [general memory hierarchy chapter](../memory-hierarchy/memory-hierarchy.md). Both are part of the learning path: read the general foundations first, then apply them to the market-data reaction path here.

    The referenced `memory-hierarchy-lab/` project was not included with the research documents. The [supplied validation notes](memory-hierarchy-validation.md) record the original author's checks; they are not a new verification run. The existing general memory lab is a separate companion.

**Audience:** beginners in computer architecture and low-latency systems programming.  
**Platform used for examples:** Linux on a conventional cache-coherent x86-64 server; 64-byte cache lines are an explicit teaching assumption.  
**Companion:** `memory-hierarchy-lab/`, a dependency-free Rust project with runnable examples and correctness tests.  
**Scope:** systems engineering of a market-data reaction path. The code is a learning scaffold, not an exchange-connected trading system.

## Learning path

Read Sections 1–3 before attempting machine tuning. Then work through the six dimensions in Sections 4–9 and assemble them in Sections 10–12.

| Stage | Question you should be able to answer | Practical result |
|---|---|---|
| Foundations | What does a memory access actually wait for? | Identify dependencies and the active working set |
| 1. Tail latency | What is slow, how often, and under what load? | A latency contract and a defensible measurement |
| 2. Cache lines | Which core owns each mutable byte? | A deliberate data layout and ownership model |
| 3. Virtual memory | Are we waiting for translation, residency, or both? | A startup and page-placement plan |
| 4. NIC and PCIe | When may the CPU read or reuse a packet buffer? | An explicit descriptor and buffer lifecycle |
| 5. Prefetching | Can we know an address before we need its data? | A measured experiment, or a decision to omit prefetching |
| 6. NUMA | Where do the thread, pages, and device actually live? | Verified topology and locality |
| Integration | What happens when traffic, inputs, or capacity exceed assumptions? | Bounded processing and explicit failure behavior |

## 1. Start with one market event

An exchange publishes an update: the best bid for instrument 17 changed. Your program must decode it, update its local market view, evaluate a strategy, apply risk checks, and perhaps transmit an order.

A **best bid** is the highest currently advertised buying price. An **order book** records resting orders or aggregated quantities at price levels. A **tick** is a permitted price increment. Our toy program stores integer tick counts, avoiding floating-point rounding in price representation.

```text
Exchange wire
    |
    v
NIC receives frame
    |  DMA into an application-accessible receive buffer
    v
Poll completion -> validate/decode -> update book -> strategy -> risk -> TX
                       |                 |              |
                       v                 v              v
                 packet bytes       instrument state  limits/position
                       \_________________|______________/
                                  memory accesses

Background work: recovery, diagnostics, persistence, configuration
```

Every arrow can contain waiting. A parser can be fast while the packet waited several microseconds in a receive queue. A risk function can execute in a few dozen instructions while one dependent load misses the cache. A process can report beautiful update times while silently dropping the packets that would have produced its worst results.

Our central engineering rule is:

> Define the event boundary, preserve correctness, bound work and ownership transfers, then measure the full latency distribution under the intended arrival pattern.

Treating the path as a pipeline helps organize ownership and budgets. It does **not** make a general-purpose CPU deterministic. Hardware contention, the scheduler, firmware, interrupts, translation activity, and device queues remain possible sources of variation. We aim for a tested operating envelope and explicit behavior outside it. A finite benchmark cannot prove an absolute worst-case bound.

### A beginner's Rust vocabulary

The companion project uses a few recurring types:

| Rust construct | Meaning in this chapter |
|---|---|
| `u64`, `u32` | Unsigned integers occupying 8 and 4 bytes |
| `struct` | A record grouping fields |
| `[u8; 24]` | An inline array of exactly 24 bytes |
| `&[u8]` | A borrowed view of bytes, without copying them |
| `Vec<T>` | A growable array backed by contiguous heap storage |
| `&mut T` | An exclusive mutable borrow, useful for one-owner state |
| `Result<T, E>` | Either success `Ok(T)` or an explicit error `Err(E)` |
| `Option<T>` | Either a value `Some(T)` or absence `None` |
| `?` | Return early if an operation reports an error |
| `Arc<T>` | Shared ownership with atomic reference counting |

“Heap” means dynamically allocated storage, not inherently remote or uncached storage. A `Vec` allocated at startup can be an excellent hot-path representation. Growing it while processing a packet introduces allocation and possibly copying. Rust ownership reduces accidental sharing; it does not automatically select good cache layouts.

## 2. The hierarchy is three interacting systems

Textbook diagrams often show a staircase. Keep that picture, but add translation and coherence.

```text
                         CPU CORE
                 registers / execution units
                            |
                      L1 data cache
                            |
                         L2 cache
                            |
                shared or partitioned LLC
                            |
                    memory controller
                            |
                         local DRAM

Address translation: virtual address -> TLB -> physical address
                                      |
                                  TLB miss
                                      v
                              hardware page-table walk

Coherence: another core's cached copy may supply data or require
           ownership transfer; DRAM is not involved in every miss.

Device path: NIC <-> PCIe/root complex <-> coherent memory/cache fabric
```

**Registers** are storage directly used by instructions. **Caches** hold copies of memory in fixed-size lines. **DRAM** holds much larger amounts of data at higher access latency. The **LLC** is the last-level cache, often called L3 on this class of server. CPU instruction caches and instruction TLBs matter too: an enormous generated parser or heavily inlined strategy can stress them even if its data is small.

The **working set** is the data and code actively reused over a relevant interval. A book may occupy 2 GB, while the current strategy repeatedly touches only 16 KB of selected instruments. That smaller set is often the first optimization target.

The **TLB**, or translation lookaside buffer, caches virtual-to-physical mappings. It answers “where is this page?” A data cache answers “do we already have these bytes?” A translation hit does not imply a data-cache hit.

**Cache coherence** keeps cores' views of shared memory consistent according to the machine's rules. It works at cache-line granularity, which can be larger than the variable you intended to share. Memory ordering adds the rules for communicating the relative order of operations across threads; coherence alone is insufficient for a correct queue.

### Planning scale, not universal timing constants

Use this table to understand relative scale. These deliberately broad, illustrative orders of magnitude are **not measurements of your machine** or contractual ranges. Generation, frequency, topology, contention, memory configuration, and measurement method can move results outside them.

| Operation | Illustrative unloaded scale | What to investigate |
|---|---:|---|
| L1 dependent load | A few cycles; roughly 1 ns at some server frequencies | Is frequently reused state compact? |
| L2 dependent load | Several to a few tens of cycles | Does the active set spill out of L1? |
| LLC hit | Tens of cycles; sometimes many tens of ns | Which slice and core supplied the line? |
| Local DRAM access | Many tens to over 100 ns | Is this a dependent miss or part of an overlapped stream? |
| Remote NUMA access | Often higher than local, with topology-dependent variation | Is the path crossing an interconnect? |
| Page fault or descheduling | A different timescale, potentially microseconds or longer | Why is OS work in the critical interval? |
| NIC/PCIe transfer | Device-, operation-, and load-specific | Measure actual RX/TX boundaries |

At 3 GHz, one cycle is approximately 0.333 ns. A four-cycle operation is about 1.33 ns at that frequency. “1–4 cycles means less than 1 ns” therefore cannot be a general rule. Also distinguish core cycles from invariant timestamp-counter ticks.

An L1 miss does not always pay an independent sequence of L2 + L3 + DRAM penalties that you can sum. Lookups, translation, speculation, and other work may overlap. Processor manuals describe the relevant microarchitecture; use them when interpreting events and dependent-load behavior. [Intel optimization manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel64-and-ia32-architectures-optimization.html)

### Latency and bandwidth are different

**Latency:** how long until one needed value becomes usable?  
**Bandwidth:** how many bytes per second can be moved in aggregate?

```text
Dependent pointer chain:  A -> B -> C -> D
                          wait wait wait
                          next address is not known early

Independent array work:  A   B   C   D
                         |   |   |   |
                         several misses may overlap
```

Four serial misses at an assumed 80 ns each suggest about 320 ns of memory waiting in a simplified model. Four independent misses may overlap substantially. Multiplying all misses by a single DRAM number misses this distinction.

A simple classroom average, `0.99 × 1 ns + 0.01 × 80 ns = 1.79 ns`, says nothing about whether the expensive accesses cluster in the one event that matters most. HFT engineering asks which dependent accesses occur on the decision path and how they behave during bursts.

## 3. Establish the measurement contract first

Before changing layout, write down:

1. **Boundary:** packet decoded to decision ready, or hardware RX timestamp to hardware TX timestamp?
2. **Traffic:** messages per second, burst sizes, symbol popularity, packet sizes, and recovery traffic.
3. **Correctness:** no undetected feed gaps, bounded capacities, and explicit invalid-state behavior.
4. **Objective:** for example, a chosen p99.9 threshold at a specified offered load, with a separate drop and deadline-miss target.
5. **Environment:** CPU, firmware, kernel, compiler flags, NIC configuration, affinity, page policy, and background work.

Thresholds must come from the system's requirements. This chapter supplies no universal HFT latency target.

```text
arrival ---- waiting in RX/queue ---- start processing ---- ready ---- TX
   |                                     |                 |         |
   +------------ response latency --------+-----------------+         |
                                         +-- service time --+         |
   +------------------ wire-to-wire interval -------------------------+
```

Choose exact endpoints when using these labels. Software “ready” and hardware “transmitted” are different events. Hardware timestamps and application clocks may require clock-domain synchronization and calibration before subtraction is meaningful.

## 4. Dimension 1 — Latency determinism and tail jitter

### 4.1 Read percentiles correctly

Sort measured latencies. The nearest-rank p99 is the value at rank `ceil(0.99 × N)`, counting from one. In a million samples, roughly 10,000 observations lie beyond the p99 rank and 1,000 beyond p99.9, subject to ties. With only 1,000 samples, p99.9 is based on one extreme rank and is highly unstable.

```text
Most messages                               rare stalls
|||||||||||||||||||||||||||||||||            |       |
+------------------------------------------------------> latency
     p50               p99                p99.9      max

This is a conceptual distribution, not measured data.
```

Report count, duration, p50, p99, p99.9, maximum, drops, and deadline misses. Use time windows so a bad second is not buried inside a good hour. Merge underlying histograms or samples rather than averaging independently computed percentiles.

“p100” is the observed maximum under a percentile convention. It is **not** a bound on future executions. Longer runs have more opportunity to observe rare events. Always label it **observed max** and report the run duration.

### 4.2 Separate three explanations for a spike

| Explanation | Example | Useful evidence |
|---|---|---|
| Work increased | More book levels scanned after a burst | Input shape, iteration counts, queue occupancy |
| One operation became slower | Cache-line handoff or translation walk | Relevant hardware events and controlled variants |
| Execution stopped | Scheduler interruption or firmware activity | Scheduling traces, IRQ counters, platform telemetry |

A cache miss count is a clue, not proof of causation for a particular outlier. Averages over a second cannot identify what happened in one 5 µs interval. Use targeted tracing and controlled A/B changes to strengthen attribution. Firmware system-management interrupts can be difficult to observe directly from an application.

Frequency changes, C-state wakeups, interrupts, sibling hyperthreads, shared cache pressure, and allocator slow paths are hypotheses to test. Do not disable a collection of settings and infer that all of them helped. Fixed high activity can increase temperature and eventually cause throttling; long runs matter.

### 4.3 Avoid coordinated omission

A closed-loop benchmark does this:

```text
send one update -> wait until handled -> send the next
```

During a stall, it sends no new work. A real market can continue sending, so the test omits the waiting time of messages that would have arrived.

An open-loop generator schedules arrivals independently of completions:

```text
scheduled arrivals:    a  b  c  d  e  f
worker:                a [----- stall -----] b c d e f
response latency:      includes each message's wait since scheduled arrival
```

The generator must keep up, record its own lateness, and report dropped or unsent work. Histogram correction can help under a justified known arrival interval, but cannot reconstruct arbitrary market bursts or replace a credible load model. [HdrHistogram correction semantics](https://github.com/HdrHistogram/HdrHistogram/blob/master/src/main/java/org/HdrHistogram/AbstractHistogram.java)

### 4.4 Time small operations honestly

The lab uses `Instant` around decode and update. Its samples include clock-reading and instrumentation overhead. This is useful for learning the process, not measuring an individual cache hit. Measure an empty bracket to understand the scale of overhead; blindly subtracting its mean from every sample does not repair instrumentation distortion.

For throughput, time a large batch and divide by operation count. For per-event tails, retain individual observations or an appropriately configured histogram. A batch average can hide one slow event. Keep sample storage thread-local and preallocated; print and sort after the timed loop.

`black_box` discourages optimizations that would remove the work, but it is not a hardware fence or a complete proof of benchmark validity. Inspect optimized code when a result appears implausible. Timestamp-counter microbenchmarks additionally require architecture-specific ordering and migration checks; an unfenced counter read is not a beginner-safe replacement for `Instant`.

**Exercise:** run the lab repeatedly. Compare p50 and observed max across runs. Explain why a stable p50 and changing maximum are compatible. Then list what the lab cannot tell you about wire-to-wire latency.

## 5. Dimension 2 — Cache lines, ownership, and layout

### 5.1 Think in lines rather than fields

A typical 64-byte cache line holds eight `u64` values. Accessing one byte can require bringing the whole line into a cache. A line is a transfer/coherence unit; a page is a translation/protection unit. They solve different problems.

```text
One assumed 64-byte line
byte 0                                                         byte 63
| bid price | bid qty | flags | sequence | unused / other state ... |
     8 B       4 B      4 B       8 B

Reading adjacent fields can reuse the same fetched line.
```

Packing fields used together can help locality. Keeping unrelated writers apart can help concurrency. These goals conflict if you indiscriminately pad everything.

### 5.2 False sharing is accidental communication

```text
BEFORE: different logical variables, same coherence unit
line X: [ RX thread's counter ][ strategy thread's counter ][ ... ]
          core 2 writes           core 3 writes
                     ownership moves back and forth

AFTER: independent write ownership
line X: [ RX counter ][ padding ................................ ]
line Y: [ strategy counter ][ padding .......................... ]
```

The variables need not be logically shared. One writer and one reader of different fields on the same line can also interfere. Two readers alone do not create write-driven false sharing.

When a writer obtains exclusive ownership, other cached copies may be invalidated. Subsequent readers or writers can trigger coherence traffic. This is not necessarily a round trip to DRAM, and modern interconnects are not accurately described as one universal shared bus.

Use `perf c2c`, where supported, to investigate contended lines and their accessing code. HITM-related observations indicate transfers involving modified data, but true sharing can produce them too. Map the line to actual fields before declaring false sharing. [Linux false-sharing guide](https://kernel.org/doc/html/latest/kernel-hacking/false-sharing.html)

### 5.3 Rust alignment: what it guarantees

```rust
#[repr(C, align(64))]
#[derive(Clone, Copy, Default)]
pub struct HotQuote {
    pub bid_ticks: u64,
    pub bid_qty: u32,
    pub flags: u32,
    pub last_seq: u64,
}

assert_eq!(std::mem::align_of::<HotQuote>(), 64);
assert_eq!(std::mem::size_of::<HotQuote>(), 64);
```

`repr(C)` provides C-style field layout rules; `align(64)` raises the type's alignment. This type has 24 bytes of fields and rounds to a 64-byte size. A properly allocated `Vec<HotQuote>` honors that alignment. Its elements start 64 bytes apart.

An aligned structure larger than 64 bytes still spans multiple lines. A smaller unaligned structure does not always straddle a line. A split access does not universally double latency, because cache state and overlapping work matter. Use alignment when you have a particular layout goal, then check size, offsets, and access patterns. Avoid `repr(packed)` as a general optimization: it can create unaligned fields and restrict legal references. [Rust layout rules](https://doc.rust-lang.org/reference/type-layout.html)

The cost of the example is substantial. At 100,000 instruments, 64-byte records occupy 6.4 MB; a hypothetical 24-byte representation occupies 2.4 MB. Padding can reduce false sharing while increasing cache and TLB pressure. If one thread owns all instruments, the smaller form may be better.

### 5.4 Hot/cold splitting

Put the fields needed for each decision together. Move strings, venue descriptions, audit metadata, and historical statistics to colder storage.

```text
Before: [symbol string][description][bid][qty][history pointer][limits]...
                                      ^       ^               ^
                                      the reaction path needs these

After:  hot[id]  -> [bid][qty][limit or limit index][status]
        cold[id] -> [symbol string][description][history]...
```

Keep cold data out of hot lines, but avoid making every required hot field another pointer chase. The best split follows measured access frequency and the exact strategy.

### 5.5 AoS versus SoA

**Array of structures (AoS):** one record per instrument.

```text
[price0 qty0 flags0] [price1 qty1 flags1] [price2 qty2 flags2]
```

**Structure of arrays (SoA):** one array per field.

```text
prices: [price0 price1 price2 ...]
qtys:   [qty0   qty1   qty2   ...]
flags:  [flag0  flag1  flag2  ...]
```

```rust
struct QuotesSoA {
    prices: Vec<u64>,
    quantities: Vec<u32>,
}

fn total_quantity(q: &QuotesSoA) -> u64 {
    q.quantities.iter().map(|&x| u64::from(x)).sum()
}
```

AoS is often convenient when one update reads or writes most fields of one instrument. SoA is promising when a scan uses only one or two fields across many instruments. Scanning eight quantities should not need to fetch eight padded records. SoA can also make vectorization easier, but does not guarantee SIMD instructions or a faster variable-length packet parser.

An intermediate layout groups small blocks of field arrays. Try it only after AoS/SoA measurements justify the added complexity. For market data, dense IDs mapped during setup often simplify the hot path more than exotic layouts do.

**Exercise:** estimate bytes touched for a quantity-only scan of 1,024 instruments in the padded AoS and in a `Vec<u32>`. Then compare measured batch throughput. Separately measure random single-instrument updates. Expect different winners.

## 6. Dimension 3 — Virtual memory, TLBs, and page faults

### 6.1 Translation and residency are separate

```text
Virtual address = [ virtual page number | offset within page ]
                             |
                         TLB lookup
                        /          \
                     hit            miss
                      |               |
                      |         page-table walk
                      |          /           \
                      |      mapped          fault
                      |         |              |
                      v         v              v
                 physical address       kernel handles fault
                      |
                 data cache lookup / memory access
```

This is a conceptual dependency diagram, not an exact hardware schedule; some cache and translation work can overlap.

A TLB miss can be handled by a hardware page-table walk without entering the kernel. Page-table entries may already be cached. A **page fault** requires OS handling; it may be a minor first-touch fault or a much more expensive operation involving storage. Equating every TLB miss with a page fault produces bad diagnoses.

### 6.2 Calculate translation footprint

For an aligned 64 MiB region:

| Page size | Pages needed |
|---|---:|
| 4 KiB | 16,384 |
| 2 MiB | 32 |
| 1 GiB | One mapping can cover the region, if available and appropriately mapped |

MiB and GiB are powers of two: `1 MiB = 1,048,576 bytes`. Larger pages can cover more useful bytes per translation. But TLBs often have different capacities for different page sizes and multiple levels. Actual page-size support is platform-specific; huge pages do not guarantee that the whole application fits in the TLB. Linux documents explicit HugeTLB allocation and supported pools. [HugeTLB documentation](https://cdn.kernel.org/doc/html/latest/admin-guide/mm/hugetlbpage.html)

### 6.3 Four actions that are often confused

| Action | What it addresses | What it does not establish |
|---|---|---|
| Allocate storage | Reserves program-visible capacity | Physical residency and desired node placement |
| Write-touch pages | Exercises first-touch allocation before live processing | Permanent cache or TLB residency |
| Lock memory with `mlock` | Keeps successfully locked pages resident | L1 residency or huge-page mappings |
| Use larger page mappings | Reduces the number of translations for a region | Core affinity or correct NUMA placement |

Locking calls can fail because of resource limits or privileges. Check return values and verify the result; if locking is required by your deployment contract, failure must prevent activation. Stack growth and later copy-on-write can still introduce faults outside prepared regions. [Linux `mlock` interface](https://man7.org/linux/man-pages/man2/mlockall.2.html)

Explicit HugeTLB mappings and transparent huge pages are different mechanisms. THP behavior includes promotion, allocation, and compaction policies, which can affect latency. Changing a process to use large pages should be a controlled configuration experiment with verified mappings, rather than an assumption based on a boot setting. [Linux THP documentation](https://kernel.org/doc/html/latest/admin-guide/mm/transhuge.html)

### 6.4 A useful startup sequence

1. Discover topology and choose the intended processing core and memory node.
2. Establish affinity and memory policy before allocating the critical regions.
3. Allocate book state, queues, packet pools, and telemetry storage.
4. Write-touch the regions from the intended context; perform required memory locking.
5. Exercise representative parsing, update, and decision branches before activation.
6. Verify affinity, placement, page sizes, lock success, and capacities.
7. Enable live processing only after these checks succeed.

Illustrative setup code, outside the reaction loop:

```rust
let mut scratch = vec![0_u8; 64 * 1024 * 1024];
let actual_page_size = 4096; // Teaching value; query the OS in real setup code.
for offset in (0..scratch.len()).step_by(actual_page_size) {
    std::hint::black_box(&mut scratch)[offset] = 1;
}
std::hint::black_box(&mut scratch);
```

The write matters: merely reserving vector capacity does not touch all pages. A read of untouched anonymous memory may encounter shared zero-page behavior. This example neither requests huge pages nor locks memory, and it does not permanently warm caches or TLBs. Those are separate actions.

Putting executable code on huge pages requires its own mapping/toolchain arrangements. Data huge pages do not automatically remap text. For a beginner, first reduce unnecessary data footprint and observe faults/walk activity before making executable mapping changes.

**Exercise:** compare a sequential scan and a randomized one-location-per-page scan. Keep the useful data and page count recorded. Explain why changing access order can affect both prefetch effectiveness and translation pressure, making it an imperfect isolated TLB experiment.

## 7. Dimension 4 — NIC buffers, DMA, PCIe, and queues

### 7.1 Follow ownership through receive and transmit

**DMA**, direct memory access, lets a device transfer data to or from host memory without the CPU copying every byte. DMA is used by ordinary kernel networking too. Kernel bypass primarily changes who controls the packet-processing path and buffers; it does not invent DMA.

**MMIO**, memory-mapped I/O, exposes device registers in an address range. An ordinary cached packet-buffer read and an MMIO register read are different operations. PCIe latency is not one reusable 100–400 ns constant covering both.

```text
APPLICATION / DRIVER                         NIC

allocate/register buffer pool
post free RX buffer descriptors -----------> owns posted buffers
                                             |
                                        receives packet
                                             |
                                        DMA writes payload
                                             |
poll completion <--------------------------- publishes completion
    |
application owns completed payload
    |
validate -> consume or retain explicitly
    |
return/repost buffer ----------------------> available for NIC reuse
```

Completion ordering, DMA visibility, and device barriers are defined by the driver/device API. Rust atomics alone do not implement a portable NIC driver. Ordinary language memory ordering describes communication between CPU threads; device access requires the relevant platform rules.

If a parser stores a pointer into a packet buffer and then reposts the buffer, the NIC may overwrite bytes still in use. A sound Rust wrapper must make this ownership transition explicit. Never assume that because packet bytes are mapped into userspace they can be retained indefinitely.

### 7.2 What kernel bypass changes

A conventional socket receive path typically includes kernel protocol processing and a copy into the application's receive buffer, though operating systems offer alternatives. Kernel-bypass frameworks can expose polling and application-accessible packet pools, reducing some transitions and copies. Their APIs differ: DPDK is a packet-processing framework, while Onload provides socket acceleration with different integration choices.

DPDK's poll-mode model emphasizes queue ownership and local per-core resources. Batching improves amortization, while run-to-completion avoids an inter-core handoff for every packet. [DPDK poll-mode driver guide](https://doc.dpdk.org/guides-22.07/prog_guide/poll_mode_drv.html)

Avoid turning that into “bypass is always faster.” A small copy of the hot fields can make ownership simpler and reduce long-lived buffer pressure. Retaining an entire packet to avoid copying 24 bytes may be a poor trade.

On some platforms, incoming DMA can interact with LLC placement. Hence “the NIC writes DRAM, then the CPU always fetches DRAM” is also too simplistic. Measure the actual device, firmware, platform, and traffic.

### 7.3 Choose a pipeline before choosing a queue

```text
Run to completion:
core A: RX -> parse -> book -> strategy -> risk -> TX
        benefits: local state, fewer transfers
        cost: one core must handle the required work

Split pipeline:
core A: RX -> parse -> [bounded SPSC queue] -> core B: book -> strategy -> TX
                       transfer + waiting
        benefits: divides work and ownership
        cost: coherence, queueing, overload policy
```

SPSC means **single producer, single consumer**. It describes who is allowed to use the endpoints. “Asynchronous” describes interaction; it does not make queueing latency vanish. A second core can increase latency if the original core already has enough capacity.

### 7.4 Capacity is a correctness decision

Let arrivals run at 8 million messages/s for 100 µs while the consumer handles 5 million/s. Starting empty, the simplified backlog is:

`(8,000,000 − 5,000,000) × 0.0001 = 300 messages`.

At 5 million/s, draining 300 messages with arrivals stopped takes about 60 µs. A larger ring prevents loss in this example, but does not make the waiting messages fresh. Sustained arrival rate above service capacity cannot be solved by any finite ring.

For each full queue, decide whether to reject, halt, backpressure, or apply a protocol-justified coalescing rule. Incremental book updates usually cannot be silently discarded. Dropping an “unimportant” delta can make all later decisions wrong. Track occupancy and age alongside latency.

### 7.5 Publication with acquire and release

The lab provides a safe `u64` SPSC ring. Its endpoint types cannot be cloned, and operations require exclusive `&mut self` access. Payload slots are atomic integers, avoiding a custom unsafe generic storage implementation.

```text
PRODUCER                                      CONSUMER
check reusable space: acquire tail
write payload slot
publish new head: release -----------> acquire head: observe publication
                                             read payload slot
check reusable space: acquire tail <--- release tail: publish consumption
overwrite that slot only after reuse is allowed
```

The head publication orders a payload write before the consumer's subsequent payload read when the acquire observes that publication. The tail publication similarly protects slot reuse. `Relaxed` payload operations are sufficient here because those publication edges provide the ordering and ownership discipline. `Relaxed` on all operations would not establish it. See the language's precise definitions before changing an ordering. [Rust atomic orderings](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html)

Usage:

```rust
use memory_hierarchy_lab::channel;

let (mut tx, mut rx) = channel::<64>();
assert_eq!(tx.try_push(42), Ok(()));
assert_eq!(rx.try_pop(), Some(42));
assert_eq!(rx.try_pop(), None);
```

`64` is the capacity fixed at compile time. The implementation checks that capacity is a power of two and uses `index & (N - 1)` to wrap an index into its array. Monotonic counters use wrapping arithmetic; occupancy remains at most `N`, and `N` is restricted below half the counter space. Tests explicitly cross counter overflow.

Head and tail occupy separate padded regions. This avoids placing the two independently written counters in one assumed cache line, but their readers and writers still incur true sharing. Adjacent payload slots can also share lines. This implementation is deliberately simple and publishes every element. More elaborate cached indices and batching change the tradeoffs.

The queue is specialized to `u64` values or IDs; passing an ID does not solve the lifetime of the referenced object. Generic queues must address initialization, moves, destruction, aliasing, and endpoint shutdown. Use an appropriately reviewed implementation rather than casually generalizing this example with raw pointers. DPDK's ring documentation illustrates explicit ownership modes and bulk operations. [DPDK ring library](https://doc.dpdk.org/guides/prog_guide/ring_lib.html)

The stress test checks ordered delivery across two threads; it is not an exhaustive memory-model proof. Neither a nonblocking attempt nor the name “lock-free” guarantees a fixed wall-clock completion time when threads can be descheduled. The spinning loop used in the test is a test driver, not a complete production shutdown or overload policy.

**Exercise:** draw the two publication edges from memory. Then explain what prevents a producer from overwriting a slot before the consumer has read it. Finally, specify what your market-data processor should do when `try_push` returns `Err`.

## 8. Dimension 5 — Hardware and software prefetching

### 8.1 First make the next address knowable

Hardware prefetchers can recognize common streams. They are often helpful for contiguous processing. Software prefetching supplies a hint that an address may be needed soon. Neither approach reserves a cache line indefinitely or guarantees a completion deadline.

```text
Too late:   prefetch X -> use X immediately -> stall remains
Useful:     prefetch X -> independent work -> use X
Too early:  prefetch X -> much other traffic -> X evicted -> use X -> miss
```

For a genuine pointer chain, you do not know the next address until the current node arrives. Prefetching cannot remove that dependency by itself. Dense arrays, stable indices, independent batches, or an auxiliary index may expose future addresses earlier.

### 8.2 Use a rough distance model

If observed miss latency is approximately 80 ns and useful independent processing is 10 ns per item, a first hypothesis is about eight items of lookahead. This is a starting experiment, not a prescription. Queueing, cache capacity, outstanding-request limits, instruction overhead, and workload variance can change the result. Intel explicitly treats useful scheduling distance as platform-dependent. [Intel software-prefetch discussion](https://www.intel.com/content/dam/doc/manual/64-ia-32-architectures-optimization-manual.pdf)

On x86-64, this guarded illustration shows the intrinsic:

```rust
#[cfg(target_arch = "x86_64")]
fn prefetch_if_present(values: &[u64], index: usize) {
    if let Some(value) = values.get(index) {
        // SAFETY: x86-64 provides SSE; this points to a live in-bounds value.
        unsafe {
            use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
            _mm_prefetch((value as *const u64).cast::<i8>(), _MM_HINT_T0);
        }
    }
}
```

This is a hint for temporal locality, not a command that places data in L1 at an exact time. The guard keeps pointer construction within a live object. The function is an optional experiment, not used by the companion's default update loop. [Rust prefetch intrinsic](https://doc.rust-lang.org/core/arch/x86_64/fn._mm_prefetch.html)

### 8.3 Keep the baseline honest

Compare no manual prefetching with several lookahead distances. Where firmware permits and you have a controlled test server, separately compare hardware-prefetch settings. Record p99.9, throughput, memory traffic, and the representative input mix. Repeat long enough to see steady-state thermal behavior.

Disabling hardware prefetchers is not a universal HFT best practice. Extra prefetches may increase traffic, displace useful data, or add instruction overhead; eliminating useful hardware prediction can hurt sequential workloads. Keep a change only when the evidence supports it. Intel's profiler guidance also notes that software prefetch can interfere with normal loads. [Intel CPU metrics reference](https://www.intel.com/content/www/us/en/docs/vtune-profiler/user-guide/2023-0/cpu-metrics-reference.html)

**Exercise:** explain why software prefetching probably has little value for repeatedly accessing a tiny array already resident in L1. Then propose a workload where you can supply a future address early enough to matter.

## 9. Dimension 6 — NUMA locality and affinity

NUMA means that memory access costs vary with location. A NUMA node is not always an entire socket; chiplet and sub-NUMA configurations can expose several nodes within one socket.

```text
NODE 0                                      NODE 1
[core A][private caches]                    [core B][private caches]
          |                                          |
    local memory fabric <--- interconnect ---> local memory fabric
       /          \                               /          \
   [DRAM 0]     [PCIe NIC 0]                   [DRAM 1]     [PCIe ...]

Desired: thread on A, its book/pool in DRAM 0, NIC path local to node 0
Remote:  thread on A reads pages in DRAM 1
```

Pinning controls where a thread may run. Memory policy controls where allocations are placed. Neither one proves the other. Existing pages, first-touch behavior, shared mappings, memory pressure, and balancing policy can affect actual location. Linux distinguishes policy scopes and provides interfaces to inspect placement. [Linux NUMA memory policy](https://cdn.kernel.org/doc/html/latest/admin-guide/mm/numa_memory_policy.html)

### 9.1 Discover before assigning numbers

On Linux, the following are inspection examples. Tool availability and permissions vary:

```sh
lscpu -e=CPU,CORE,SOCKET,NODE
numactl --hardware
cat /sys/class/net/<interface>/device/numa_node
```

Substitute a real network interface. A `-1` device node result means unknown/unassociated information rather than proof that every node is equally local. Inspect the underlying PCIe device when a virtual interface hides it.

Choose a physical core, check its sibling logical CPUs, and decide how other workloads use that physical core. Affinity does not reserve a core, suppress interrupts, or remove firmware activity. Keeping the hot thread off a busy sibling is often useful, but must be evaluated on the actual topology.

### 9.2 Launch with explicit placement

After verifying that CPU 4 belongs to node 0, an example launch is:

```sh
numactl --physcpubind=4 --membind=0 ./target/release/memory-hierarchy-lab
```

CPU 4 and node 0 are placeholders for a discovered pairing. `--membind` can fail under unavailable capacity; handle deployment failures. For a multithreaded application, assign each worker explicitly—launch affinity may otherwise restrict all newly created threads to the same allowed set.

An implementation in Rust can call an affinity library or the OS API during startup. Check the result and read the effective affinity back. Linux restrictions from containers and CPU sets may narrow the CPUs available to the process.

### 9.3 Verify after allocation

For a sufficiently long-lived process, inspect:

```sh
taskset -pc <pid>
numastat -p <pid>
cat /proc/<pid>/numa_maps
```

Inspect per-thread affinity for worker threads, not just the main thread. Page-size information is available in `/proc/<pid>/smaps`; validate the relevant mapping rather than assuming a global setting applied. A controlled experiment places identical work on a local core/node pair, then uses remote memory while keeping other conditions stable.

Avoid creating remote references accidentally through a startup allocator thread on another node. Packet pools, queues, logs, and recovery buffers can matter alongside the main book. A shared queue spanning sockets also transfers coherence ownership across the interconnect; huge pages do not remove that cost.

“Strictly avoid remote memory” is too broad as an absolute rule. Keep the reaction path local where practical; cold history or background reporting may tolerate remote access. Record what is allowed and test it. Even a single-socket deployment may have nonuniform topology.

**Exercise:** explain how a pinned worker can still use remote memory. Describe two different observations that would verify its CPU affinity and its page placement.

## 10. Assemble a bounded Rust reaction path

### 10.1 Specify the toy protocol

The lab accepts one 24-byte frame per update:

```text
byte offset    0              8        12                    20       24
               | sequence u64 | id u32 | best-bid ticks u64 | qty u32 |
               +--------------+--------+--------------------+---------+
All integers: little endian
```

The symbol ID is dense and established during setup. Sequence numbers are global for this single simulated feed. Each frame replaces the full best bid for one symbol; zero quantity clears it. This is **not** a real incremental depth-book protocol. It has no fragmentation, sessions, retransmission channel, multiple venues, or bid/ask crossing rules.

Actual exchange sequence semantics determine where gap detection belongs: packet, channel, instrument, or session. Never copy this global counter into an exchange adapter without matching its protocol.

### 10.2 Decode bytes explicitly

```rust
pub fn decode(frame: &[u8]) -> Result<Update, FeedError> {
    if frame.len() != 24 {
        return Err(FeedError::Length);
    }
    Ok(Update {
        seq: u64::from_le_bytes(frame[0..8].try_into().unwrap()),
        symbol: u32::from_le_bytes(frame[8..12].try_into().unwrap()),
        bid_ticks: u64::from_le_bytes(frame[12..20].try_into().unwrap()),
        bid_qty: u32::from_le_bytes(frame[20..24].try_into().unwrap()),
    })
}
```

`Update` and `FeedError` are defined in the companion library. Every slice length is guaranteed by the preceding check and constant offsets, so these particular `unwrap` calls cannot fail on a checked 24-byte frame. This is different from unwrapping arbitrary network operations.

Casting the packet pointer to `&Update` would entangle host alignment, compiler layout, byte order, and reference validity. Byte conversion is clear and lets the compiler optimize legal accesses. Bounds checks should be removed only after profiling and a convincing safety argument; do not introduce undefined behavior to save a suspected branch.

### 10.3 Update with one owner

```text
Frame
  |
  v
already halted? ------ yes --> reject
  |
  no
  v
length -> decode -> expected sequence -> valid fields -> valid ID
  |          any failure before mutation
  +--------------------------------------> halt feed state
  |
  all valid
  v
write one quote -> advance expected sequence -> success
```

The `Book` owns a startup-allocated `Vec<HotQuote>`. Its update takes `&mut self`; no other thread can concurrently borrow that book through this API. Each successful event performs bounded parsing and updates one record. No hashing, string lookup, vector growth, printing, or locking occurs in `ingest`.

It checks the next sequence number using `checked_add`, preventing silent integer wrap. It rejects an unknown symbol and a zero price with nonzero quantity. Every validation precedes quote mutation. On any error, it marks itself halted. Subsequent updates fail until an external recovery mechanism replaces or rebuilds the state; the example intentionally supplies no unchecked “resume” method.

```rust
use memory_hierarchy_lab::{Book, Update, encode};

let mut book = Book::new(1024, 1);
let bytes = encode(Update {
    seq: 1,
    symbol: 17,
    bid_ticks: 12_345,
    bid_qty: 20,
});
book.ingest(&bytes).unwrap(); // Known-good local example input.
assert_eq!(book.quote(17).unwrap().bid_ticks, 12_345);
```

The library exposes the last quote for diagnostics even after a halt. Therefore any strategy integration **must gate trading on feed validity**; obtaining `Some(&quote)` is not an authorization to trade. In a stronger production API, expose actionable views only through a validated state token or state-machine transition.

### 10.4 Integrate risk without corrupting units

Integer arithmetic still overflows. A toy check on price and quantity can use:

```rust
fn within_tick_quantity_limit(price_ticks: u64, qty: u64, limit: u64) -> bool {
    price_ticks.checked_mul(qty).is_some_and(|value| value <= limit)
}
```

The product's units are **tick-quantity**, not currency. A real notional limit must account for tick value, contract multiplier, quantity units, side, currency, and outstanding orders using validated conversion and overflow rules. Updating positions concurrently also needs an explicit ownership/consistency design. A fast check against stale risk state is not correct risk enforcement.

### 10.5 Keep rare work outside normal processing

```text
SETUP: allocate -> bind/place -> warm -> verify -> activate

LIVE: receive -> validate -> bounded update -> decision -> risk -> submit
        |
        +-> error / gap / stale feed / capacity failure
                         |
                         v
                  disable affected trading
                         |
                         v
                 recovery and reconciliation
                         |
                         v
                   validated reactivation
```

Operational errors still require a defined response. “No allocations on the hot path” does not mean silently ignoring failures to keep the loop fast. Queue a bounded diagnostic event if capacity permits, maintain explicit counters, and ensure safety behavior does not depend on a slow logging service.

## 11. Run, inspect, and extend the lab

From the companion project directory:

```sh
cargo test --offline
cargo run --release --offline
```

The project uses only the Rust standard library and requires a target with 64-bit atomics. It was checked with Rust 1.99.0. Release compilation enables optimization; the release profile retains some debug information for profiling.

The executable warms the toy book, then records 200,000 decode-and-update durations over 1,024 symbols. It allocates sample storage before timing and sorts afterward. Frame generation is outside the measured interval. Reading the clock, consuming the result, and an optimization barrier contribute overhead. The book's padded quote storage occupies approximately 64 KiB, not counting allocation metadata or other program state.

Output format:

```text
n=200000 p50=...ns p99=...ns p99.9=...ns observed_max=...ns
```

No expected latency is supplied: shared development environments are not representative HFT measurement rigs. This benchmark is closed-loop and single-threaded; it does not exercise the SPSC ring, a NIC, PCIe, huge-page mappings, affinity, or open-loop overload. Its results cannot establish a wire-latency claim.

### 11.1 What is verified

Eight tests cover quote size/alignment, decode/update/clear behavior, feed-gap rejection without quote mutation, malformed or invalid inputs, sequence exhaustion, full and empty ring behavior, repeated slot reuse, counter overflow, cross-thread ordered delivery, and nearest-rank percentile selection. Some tests cover more than one behavior.

These tests support the example's functional claims. They do not prove cache residency, exhaustive queue interleavings, absence of all hardware jitter, or suitability for a particular exchange. Those require additional platform-specific validation.

### 11.2 Inspect Linux counters in separate runs

On a machine with suitable tools and permissions, start with:

```sh
perf stat -e cycles,instructions,cache-references,cache-misses,page-faults,context-switches,cpu-migrations ./target/release/memory-hierarchy-lab
perf list
```

These process-wide counts include startup and reporting, unlike the per-update timing bracket. Generic cache events have CPU-dependent meanings; `cache-misses` is not a portable L1-miss counter. Find the platform's data-TLB, page-walk, L1/L2, LLC, and memory-stall events in its PMU documentation. Hybrid processors may expose separate PMUs. Event multiplexing can reduce reliability, so inspect the tool's reporting and measure smaller event sets when necessary.

For a deliberately shared, sufficiently long-running variant:

```sh
perf c2c record -- ./target/release/your-two-thread-variant
perf c2c report
```

The stock executable has no competing worker, so it is not a useful false-sharing experiment. Trace collection perturbs execution; compare profiled and unprofiled runs. If event access is unavailable, record the limitation rather than fabricating evidence or interpreting zero counts as proof of no contention.

### 11.3 A progression of meaningful experiments

| Experiment | Keep stable | Change | Readout |
|---|---|---|---|
| Footprint | Same update sequence and algorithm | Number of instruments | Tail latency versus working-set size |
| Access pattern | Same state size | Sequential, hot-skewed, randomized IDs | Locality sensitivity |
| Data layout | Same logical operations | Compact AoS, padded AoS, SoA | Update tails and scan throughput separately |
| False sharing | Same two-core workload | Adjacent versus separated counters | Runtime, cache-to-cache evidence |
| Translation | Same useful workload and placement | Verified page mappings | Walk activity and tails |
| Pipeline | Same messages and boundaries | Run-to-completion versus SPSC handoff | End-to-end tails, occupancy, CPU cost |
| NUMA | Same code, load, and core | Verified local versus remote pages | Tails and locality evidence |
| Prefetch | Same access trace and layout | No hint versus lookahead distances | Tails, throughput, traffic |

A layout change can also alter footprint and generated code. Record these secondary changes so an experiment does not claim more isolation than it achieves. Alternate A/B run order and repeat: thermal drift and background activity can bias a one-off comparison.

### 11.4 Keep an experiment record

```text
Hypothesis:
CPU/topology and memory configuration:
NIC/firmware/kernel/compiler:
Commit and release build flags:
CPU affinity / page policy / verified placement:
Input trace, offered rate, burst model, and duration:
Latency start and end boundaries / clock domain:
Warmup and instrumentation:
Sample count, p50, p99, p99.9, observed max:
Drops, rejects, queue occupancy/age, deadline misses:
Relevant counters and collection limitations:
Repeated-run variability:
Decision and remaining uncertainty:
```

## 12. From the scaffold to production readiness

Production readiness is a collection of established properties. Fast local timings establish only a small part of it.

| Property | Evidence required before activation |
|---|---|
| Correct decoding | Protocol conformance cases, malformed-input tests, byte-order and length handling, fuzzing where useful |
| Valid market state | Sequence/session logic, duplicate handling, snapshot-plus-delta reconciliation, stale-feed detection |
| Bounded resources | Maximum symbol/depth/queue capacities, startup reservation, measured overload behavior |
| Correct ownership | Packet and pool lifetimes, queue endpoint ownership, safe shutdown and reclamation |
| Risk correctness | Valid units, checked arithmetic, position/order consistency, cancellation and kill behavior |
| Measured latency | Open-loop representative replay, burst tests, drops and deadlines, long-run tails |
| Placement correctness | Read-back affinity, relevant page mappings and NUMA distribution, device locality |
| Operational behavior | Feed disconnects, TX errors, recovery transitions, telemetry backpressure, restart reconciliation |

### A worked design review

Suppose a prototype has a good p50 but a bad p99.9. It uses heap-allocated linked nodes for book levels, a shared mutex for strategy reads, and allocates a formatted string for each update.

Start by recording the exact latency boundary and verifying the test includes queue waiting. Then profile representative traffic. Plausible changes are:

1. Give one processing thread ownership of the live book, reducing mutex contention and mutable sharing.
2. Replace pointer-heavy hot state with dense IDs and fixed-capacity arenas or level arrays where protocol and price ranges allow it.
3. Put display strings and audit formatting outside the reaction path; publish bounded structured events instead.
4. Prepare memory before activation and verify placement on the intended node.
5. Re-measure correctness, tails, occupancy, and loss after each meaningful change.

There is no guarantee that one layout suits every book. A dense array covering an enormous sparse price range can consume far more memory than a compact indexed structure. An arena reduces allocation overhead but does not eliminate cache misses. A background reader of a mutable snapshot can still force coherence traffic or observe inconsistent state unless the publication protocol is correct.

Only after identifying remaining memory stalls should you consider page-size changes or software prefetching. The objective is a simpler, observable path with controlled failure behavior, not a maximum count of low-level techniques.

## 13. Revisit the original axioms with engineering precision

| Useful starting intuition | More precise production rule |
|---|---|
| Minimize tail latency | Define boundaries, offered load, deadlines, loss, and observed percentiles; finite tests do not prove a future maximum |
| An L1 miss is catastrophic | Investigate dependent misses on the decision path; overlapped and off-path misses have different impact |
| Align everything to 64 bytes | Verify line size and selectively separate writers; alignment does not shrink objects or guarantee a speedup |
| Two lines cost twice as much | Access cost depends on residency, dependencies, overlap, and coherence state |
| Huge pages pin the working set | Larger mappings improve translation reach; residency, affinity, and NUMA placement remain separate |
| A TLB miss means a fault | A hardware walk can resolve a valid mapping without OS fault handling |
| Kernel bypass removes DMA/copy costs | DMA exists with kernel networking; bypass changes the path and ownership, and some copies may still be useful |
| PCIe has one fixed latency | Measure the particular transfer, queueing, device, and completion boundary |
| Disable hardware prefetchers | Evaluate defaults and controlled alternatives on the real workload |
| Pinning guarantees locality | Verify both worker affinity and actual page/device placement |
| A bigger ring solves bursts | It trades loss tolerance against waiting time; overload needs an explicit policy |
| Rust makes a fast system safe | Safe Rust helps memory safety; protocol, ownership across FFI, risk logic, and performance still need evidence |

## 14. Check your understanding

1. Why can a 99% cache-hit rate coexist with unacceptable p99.9 event latency?
2. Why might a smaller, unpadded book beat a cache-line-aligned book owned by one thread?
3. What distinguishes a TLB miss, a data-cache miss, and a page fault?
4. What must the producer publish before a consumer can use a payload?
5. Why must a returned NIC buffer no longer be referenced by the parser?
6. Why can run-to-completion beat a two-core pipeline?
7. Why can a software prefetch have no effect on a dependent pointer chain?
8. Why can a worker pinned to node 0 still access node 1's memory?
9. Why is an observed maximum not a worst-case bound?
10. Why is a gap-free input stream more fundamental than saving one cache miss?

### Answer sketches

1. Misses can cluster on the critical dependency chain or coincide with bursts; a global hit rate loses timing and per-event structure.
2. Padding increases footprint and scan traffic without necessarily removing any competing writer.
3. Translation-cache lookup, data residency, and OS-assisted mapping/residency handling are distinct mechanisms.
4. It must write the payload and then publish availability using the queue's synchronization protocol; reuse requires a second ownership edge.
5. Reposting transfers reuse rights to the device; later DMA may overwrite the old packet.
6. A second stage adds publication, coherence, waiting, and scheduling interactions; parallel capacity helps only when needed.
7. The future address may be unknown until the current miss resolves, leaving no independent lead time.
8. Existing placement, a different first-touch thread, or allocation policy can locate pages remotely; CPU affinity alone does not move them.
9. It describes one finite sample and environment, not all possible future states.
10. Missing updates can invalidate every later decision. Faster processing of an incorrect book is still incorrect.

## Sources and further reading

The diagrams, calculations, toy protocol, and lab are original teaching material. Numerical examples are labeled assumptions rather than vendor guarantees. These primary references support the architecture and API details:

- [Intel optimization manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel64-and-ia32-architectures-optimization.html): microarchitecture-specific optimization and event interpretation.
- [Intel software-prefetch discussion](https://www.intel.com/content/dam/doc/manual/64-ia-32-architectures-optimization-manual.pdf): scheduling-distance tradeoffs; this linked edition is older, so use the processor-appropriate current manual as well.
- [Linux false-sharing guide](https://kernel.org/doc/html/latest/kernel-hacking/false-sharing.html): finding and investigating contended cache lines.
- [Rust type layout](https://doc.rust-lang.org/reference/type-layout.html), [atomic ordering](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html), and [prefetch intrinsic](https://doc.rust-lang.org/core/arch/x86_64/fn._mm_prefetch.html): language and API guarantees.
- [Linux HugeTLB](https://cdn.kernel.org/doc/html/latest/admin-guide/mm/hugetlbpage.html), [THP](https://kernel.org/doc/html/latest/admin-guide/mm/transhuge.html), [NUMA memory policy](https://cdn.kernel.org/doc/html/latest/admin-guide/mm/numa_memory_policy.html), and [`mlock`](https://man7.org/linux/man-pages/man2/mlockall.2.html): distinct mapping, placement, and residency controls.
- [DPDK poll-mode drivers](https://doc.dpdk.org/guides-22.07/prog_guide/poll_mode_drv.html) and [ring library](https://doc.dpdk.org/guides/prog_guide/ring_lib.html): queue ownership, polling, and batching; API details vary by release.
- [HdrHistogram correction implementation](https://github.com/HdrHistogram/HdrHistogram/blob/master/src/main/java/org/HdrHistogram/AbstractHistogram.java): what coordinated-omission correction actually assumes and records.

References checked on 19 October 2026. Match documentation versions to the compiler, kernel, CPU, and driver you deploy.
