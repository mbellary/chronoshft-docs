# Prefetching for predictable trading systems

## From a cache miss to a measured Rust implementation

**Audience:** absolute beginners in CPU performance and low-latency Rust; a short Rust orientation is included below. **Scope:** CPU data prefetching, illustrated with a simplified market-data decision path. Examples assume a contemporary x86-64 machine with 64-byte cache lines; verify the actual target. No latency number in this chapter is a universal hardware constant.

The learning path follows five questions: **when should data arrive, what does it displace, what resources does it consume, what happens to the latency tail, and does the layout make useful anticipation possible?** Each section turns one question into code or an experiment.

The accompanying [Rust lab](prefetch-lab/src/main.rs) is executable learning material. It is not a trading engine, an exchange feed handler, or a claim of production performance. Production readiness requires the measurement and integration work described later.

### Learning outcomes

By the end, you should be able to:

1. Explain what a prefetch can change and what it cannot guarantee.
2. Identify useful independent work between a hint and its demand load.
3. Estimate a starting lookahead distance and explain why it changes under load.
4. Implement a bounded, optional prefetch in Rust without unsafe pointer arithmetic.
5. Design experiments that expose cache pollution, wasted traffic, and tail regressions.
6. Decide whether to ship, retune, or remove a prefetch on a particular deployment.

## Before you start: a first look at Rust

Rust is a compiled language: source text becomes a machine-code executable before it runs. Cargo is its standard project and build tool. If you have never used Rust, follow the [official installation instructions](https://www.rust-lang.org/tools/install), then the Rust Book's [Hello, Cargo](https://doc.rust-lang.org/book/ch01-03-hello-cargo.html) walkthrough. You do not need to implement a network stack to begin this chapter.

The lab contains two main files. `Cargo.toml` describes the project. `src/main.rs` contains the program. `cargo test` runs its correctness checks; `cargo build --release` produces an optimized executable. Debug builds are useful for development but are not suitable evidence for hot-loop performance.

```rust
fn accepted_quantity(available: u64, requested: u64) -> u64 {
    requested.min(available)
}

fn main() {
    let available = 7_u64;
    let mut total = 0_u64;
    total += accepted_quantity(available, 10);
    println!("accepted: {total}");
}
```

`fn` defines a function. `main` is the entry point. `u64` is an unsigned 64-bit integer. `let` introduces a variable; `mut` allows its value to change. `-> u64` declares the return type. A final expression without a semicolon is the returned value. This program prints seven because only seven units are available.

A `struct` groups fields into one record. A `Vec<T>` owns a growable array of values of type `T`. `&value` borrows a value, and `&[T]` borrows a sequence without copying it. `for` repeats work over a sequence. `if` chooses a branch. `Some(value)` and `None` represent presence and absence, and `if let Some(x) = ...` enters its block only when a value exists.

Rust ownership controls who may use or modify memory and when. A borrow must not outlive its object. The lab uses immutable shared data to keep the first example simple. More advanced networking code must preserve the same validity rules while also satisfying its queue or device protocol.

An intrinsic is a function exposing a processor capability. The lab isolates its architecture-specific intrinsic in a small wrapper. You can understand the main algorithm first and study that wrapper later. The notation `process::<8>(...)` chooses a compile-time parameter of eight; it lets the compiler build a version specialized for that lookahead distance.

Read the chapter once for the mental model, run the unmodified lab, and then change one experimental variable at a time.

## 1. Begin with the trading path

A market-data message says something changed at a venue: a price, quantity, trade, or order-book level. A trading application decodes the message, updates its model, evaluates a decision, checks limits, and may send an order.

```text
Market-data NIC
      |
      v
Receive --> validate/decode --> update book --> strategy --> risk --> transmit
               |                    |             |          |
          packet bytes        symbol state    parameters   limits

      <------ specify your tick-to-trade measurement boundary ------>
```

The NIC is the network interface card. A tick is a market-data update. The order book records available prices and quantities. Risk checks enforce constraints such as position or order-size limits. These definitions explain the example; real systems have additional stages and failure paths.

**Latency** is elapsed time for one event. **Throughput** is events processed per second. **Jitter** is variation in elapsed time. The median, p50, is the value at or below which half the observations fall. p99.99 is the corresponding 99.99th percentile. Neither percentile is the mathematical worst case.

Suppose version A handles most updates in 800 ns but occasionally takes 20 μs. Version B usually takes 850 ns and rarely exceeds 2 μs. A latency-sensitive system may prefer B, depending on its explicit service objective. These numbers are invented to illustrate a decision, not measured results.

Write the objective before optimizing. For example: “Reduce the processing latency of decoded updates while keeping p99.99 below the agreed budget and avoiding additional packet loss under the specified burst trace.” A tail improvement does not excuse an incorrect risk decision; a faster median does not excuse violating that budget.

## 2. A beginner's model of memory

A register holds values the CPU is directly working on. Caches hold recently accessed memory close to the core. Main memory has much larger capacity, with a longer access path. A typical simplified arrangement is:

```text
                      one CPU core
              +--------------------------+
              | registers                |
              |                          |
              | L1 instruction | L1 data |
              +----------+---------------+
                         |
                        L2
                         |
                  shared last-level cache
                         |
                  memory controller
                         |
                  local or remote DRAM
```

This is an orientation diagram, not a complete model of a particular processor. Cache sharing, inclusivity, capacities, associativity, and interconnect topology differ. Consult the processor-specific material linked from [Intel's optimization reference hub](https://www.intel.com/content/www/us/en/developer/articles/technical/intel64-and-ia32-architectures-optimization.html).

A **cache line** is the unit of transfer and coherence for our model: 64 adjacent bytes. An 8-byte integer load may cause an entire line to be fetched. An aligned line can hold eight 8-byte integers.

```text
One assumed 64-byte line
+--------+--------+--------+--------+--------+--------+--------+--------+
| u64[0] | u64[1] | u64[2] | u64[3] | u64[4] | u64[5] | u64[6] | u64[7] |
+--------+--------+--------+--------+--------+--------+--------+--------+
   read one value                 potentially fetch the whole line
```

A **hit** finds the line at the cache level being considered. A **miss** must obtain it elsewhere. An L1 miss can hit in L2; a cache miss is not automatically a DRAM access. If the program cannot proceed until the value arrives, that load contributes to a dependency stall.

A **TLB**, or translation lookaside buffer, caches virtual-to-physical address translations. A TLB miss and a data-cache miss are different events. Prefetching is not a replacement for arranging page allocation, address translation, and NUMA placement deliberately. NUMA means different regions of memory have different access paths depending on the core that uses them.

### 2.1 Demand, hardware, and software fetching

A demand load says “I need this value.” Hardware prefetching predicts future accesses from patterns observed by the processor. Software prefetching lets the program supply an address before it needs the value. A compiler can also insert software prefetch instructions.

```text
Demand only:
    independent work --> load X --> [wait for X] --> compute with X

Useful lookahead:
    hint X -------------> independent work -------> load X --> compute
           \________ fetch overlaps work ________/
```

**A prefetch is a hint.** It is not a promise that the line will arrive, a request to keep it resident, a memory fence, a lock, or a completed read of the future value. On x86-64, `_mm_prefetch` targets data access; it is not a general mechanism for warming an execution path's instruction cache. Rust documents implementation-dependent hint behavior and the possibility that the processor ignores a request. See the [Rust intrinsic documentation](https://doc.rust-lang.org/stable/core/arch/x86_64/fn._mm_prefetch.html).

### 2.2 Correcting the starting axioms

The proposed five dimensions are useful. Several initial statements need more precise engineering versions:

| Starting idea | Version to use in an implementation review |
|---|---|
| A successful prefetch arrives after an instruction is scheduled | It must provide useful overlap before demand use and remain useful until that use; scheduler timing is not the definition |
| Prefetching inherently evicts another line | It can occupy an invalid entry, hit an existing line, be dropped, or cause replacement; pollution is a risk to measure |
| Keep risk code permanently pinned in L1 | Ordinary application code cannot guarantee L1 residency; code and data generally use separate L1 caches |
| Wasted requests are harmless during a lull | They can still cost instructions, capacity, energy, translation work, and interference |
| Software hints guarantee deterministic execution timing | They make selected addresses explicit; service time and completion remain variable |
| L2 stream compiler mechanism | Hardware stream/stride prefetchers and compiler-inserted software hints are different mechanisms |
| AoS implies pointer chasing | A flat `Vec<Record>` is contiguous AoS; pointer chasing follows dependent addresses |
| p50 measures the average | p50 is the median; the arithmetic mean is another statistic |
| Standard deviation measures tail contribution | It summarizes spread and cannot isolate the rare-event tail or establish causality |

These distinctions prevent an attractive explanation from becoming an incorrect optimization plan.

## 3. Dimension one: temporal precision

**Engineering axiom:** issue a useful request early enough to overlap its latency, but avoid excessive time between arrival and use.

Let:

- `t_issue`: when the prefetch request is issued;
- `t_ready`: when the line becomes available at the cache level of interest;
- `t_use`: when the demand access needs it;
- `t_evict`: when that available copy is displaced, if it is displaced.

A simplified fully hidden access has:

```text
t_issue < t_ready <= t_use < t_evict

Too late:    issue ---- demand/use ---- ready
                             [stall]

Useful:      issue ---- ready ---- use
                       [resident]

Too early:   issue ---- ready ---- eviction ---- use ---- refetch
```

A late prefetch may still shorten a demand miss because some fetching has already happened. An L2-resident line may help even when the demand access still misses L1. Success is not a binary “instant L1 hit or failure.”

### 3.1 Estimate a distance, then sweep it

Suppose a loop processes independent decoded updates. If each iteration takes approximately `C` cycles and a relevant fetch takes `L` cycles, an initial estimate is:

```text
lookahead distance D ≈ ceil(L / C)
```

For an invented example with `L = 180` cycles and `C = 30` cycles/update, try a distance near six updates. A practical sweep could be `0, 1, 2, 4, 8, 16, 32, 64`; zero means disabled.

This is a starting model. It omits contention, varying lookup costs, overlap from out-of-order execution, and the extra work of issuing hints. If the optimized loop gets faster, the same number of iterations provides less time. If queueing increases fetch latency, the needed lead time increases. Re-measure after changing the algorithm or layout.

Use consistent units. A cycle count is not a nanosecond count. Core frequency varies, and a timestamp counter's tick rate need not be the instantaneous core frequency. Do not convert TSC ticks using the advertised turbo clock.

### 3.2 The address must be known early

Consider this expression:

```rust
let book = &books[requests[i].symbol];
```

The request stream tells us which book to access. If request `i + D` has already been decoded and is safely visible, we can obtain its symbol now and hint its book while processing request `i`.

```text
requests:    i       i+1       i+2       i+3       i+4
             |                                    |
          process                           read symbol ID
                                                  |
                                             hint books[ID]
             <------ four updates of work -------->
                                                  |
                                             demand book
```

This cannot predict a symbol hidden in a packet that has not arrived. It also cannot make an unowned ring-buffer slot safe to read. Lookahead is bounded by already available information and the ownership protocol.

### 3.3 Serial pointer chasing limits overlap

For `node = node.next`, the next address may itself be fetched from a missing line. A hint for `node.next` still requires loading the pointer first. Software cannot obtain an unknown address merely by issuing a hint.

Useful alternatives include compact index-based storage, processing several independent lookups together, or changing a tree representation. These create more independent addresses. They may improve performance without any explicit prefetch.

### 3.4 What to measure

Begin with a distance-versus-latency curve. Record layout, trace, working-set size, and deployment configuration for every point.

The exact delta `t_use - t_ready` is a conceptual quantity; ordinary application instrumentation usually cannot observe exact prefetch completion. Use controlled placement changes, latency measurements, and processor-specific counters or load sampling as evidence. Measuring elapsed time around the hint instruction measures its execution overhead, not completion of the memory request.

**Exercise:** explain why moving a hint ten source-code lines earlier may provide less overlap than moving it one loop iteration earlier. Answer: source lines do not quantify independent runtime work, and compilers can transform the instruction sequence.

## 4. Dimension two: cache pollution and occupancy

**Engineering axiom:** judge the benefit across the whole critical workload, including lines displaced by anticipatory work.

Imagine a worker repeatedly uses compact risk-limit data. It also speculates on several future book records. Those records might improve later book reads, but they can compete with the worker's other data.

```text
Before speculation, simplified data-cache set:
    [limits A] [limits B] [hot book] [free/replaceable]

After too much speculation:
    [future 1] [future 2] [future 3] [future 4]
         faster future books, potentially slower current risk reads
```

Real caches are set associative. Each address maps to a limited group of candidate slots, called a set. A working set smaller than total cache capacity can still suffer conflicts if too many live lines compete for the same set. “It fits in L1” is a useful hypothesis, not proof of residency.

Risk **instructions** normally occupy L1 instruction cache; risk **tables** occupy L1 data cache. Prefetching book data directly pressures the latter. Code can still encounter interference at unified lower cache levels, and additional instructions may affect the front end. Keep those mechanisms separate when diagnosing a regression.

### 4.1 Use a capacity estimate carefully

For `D` future updates, `k` distinct target lines per update, and a 64-byte line:

```text
lookahead target footprint ≈ D × k × 64 bytes
```

For `D = 16` and `k = 2`, that is 2 KiB of distinct future targets under the simplified assumption. This is not a prediction of actual cache occupancy. Repeated symbols can share lines; requests may be ignored; additional streams and replacement policy matter.

Also include packet data, descriptors, current state, lookup tables, stacks, and other workers sharing lower caches. Cache capacity is only one resource budget.

### 4.2 Apply the latency-delta idea at the right boundary

Your proposed accounting identity is useful:

```text
net benefit = latency saved on target work - induced cost elsewhere
```

But the two components are usually not separately observable. Start with a directly measurable end-to-end comparison:

```text
mean benefit = mean(L_baseline) - mean(L_prefetch)
tail benefit(q) = Qq(L_baseline) - Qq(L_prefetch)
```

Positive values favor prefetching at the selected boundary. Do not add individual stage p99 values to compute an end-to-end p99; the stages' extreme observations need not occur on the same event.

### 4.3 Run a pollution experiment

1. Establish a baseline for the complete book-plus-risk path.
2. Add only the book prefetch and keep the same inputs and decisions.
3. Increase lookahead distance and book footprint independently.
4. Observe whether risk-stage latency and end-to-end tails worsen.
5. Compare the selected target-cache hint where the architecture supports it.

T0, T1, T2, and NTA express locality intentions. They do not reserve cache ways. NTA is not a portable “avoid all cache pollution” switch. The exact placement and replacement effects depend on the processor.

**Exercise:** a book-only microbenchmark improves 15%, but the full decision path's p99.99 regresses 8%. Would you ship it? Not on that evidence: investigate the regression against the actual tail budget and workload objective.

## 5. Dimension three: accuracy, bandwidth, and outstanding requests

**Engineering axiom:** request only data whose expected benefit justifies its resource cost, particularly under the most demanding arrival pattern.

Accuracy, coverage, and timeliness describe different things:

| Quantity | Conceptual definition | Common trap |
|---|---|---|
| Accuracy | Useful prefetched lines / prefetched lines | Counting hint instructions as completed unique fetches |
| Coverage | Baseline demand misses avoided / baseline demand misses | Treating all cache hits as caused by prefetching |
| Timeliness | How often useful data is available early enough | Counting a eventually-used line as fully latency-hidden |

You must define “useful.” A prefetched line that would already have been resident may be referenced later without saving any latency. Several hints may name the same line. Dropped requests, merged requests, and hardware/software overlap make exact attribution difficult. Report PMU proxies by their documented names rather than calling them precise accuracy measurements.

### 5.1 Estimate the cost of being wrong

Let `R` be updates/s, `k` candidate lines/update, `a` the fraction of candidate lines that are useful, and `B` bytes/line. A simplified upper-level traffic estimate is:

```text
wasted candidate line bytes/s ≈ R × k × (1 - a) × B
```

For invented values `R = 5 million`, `k = 2`, `a = 0.75`, `B = 64`, the result is 160 million bytes/s, or about 160 MB/s in decimal units. This is an arithmetic illustration, not a DRAM measurement: resident, merged, or dropped requests may cause less external traffic, and coherence can introduce other traffic.

The effect can become nonlinear near resource saturation. A modest extra request rate can increase waiting time sharply when queues already have little spare capacity.

### 5.2 Outstanding misses are another finite budget

A core can track only a finite number of in-flight memory operations. Miss-status resources, line-fill buffers, queues, translations, and interconnect capacity limit useful concurrency. Which structures a request uses depends on the microarchitecture and target level; do not assume every software hint occupies an L1 fill buffer in the same way.

```text
Demand reads --------+
Hardware predictions +--> finite resources --> cache / coherence / DRAM
Software hints ------+            |
                             pressure can delay
                             demand work
```

A simple queueing estimate is Little's law:

```text
average outstanding requests ≈ request arrival rate × mean service time
```

If 50 million relevant requests/s each spend an average 100 ns outstanding, the average is five outstanding requests in that modeled subsystem. Bursts and variance can require substantially more headroom. This estimate does not reveal a CPU's buffer count.

Intel's historical Broadwell-X event `L1D_PEND_MISS.FB_FULL` counts cycles in which a demand request was blocked by fill-buffer unavailability. It does **not** alone identify software prefetches as the cause. Use that example to understand semantics; select events documented for your own CPU. See [Intel's Broadwell-X event definitions](https://perfmon-events.intel.com/platforms/broadwellx/core-events/core/).

### 5.3 Translate this into the HFT setting

A market burst can increase receive work, book updates, and speculative memory requests together. NIC DMA, core accesses, and other cores can compete through parts of the platform, but kernel bypass does not imply a single shared “memory bus” with a fixed penalty. NIC placement, DDIO where supported, NUMA, receive layout, and cache sharing shape the interaction.

Test the transition from quiet traffic into a burst. A steady uniform load misses queue buildup, symbol shifts, and post-burst recovery. Even a perfectly predicted future access can be harmful if fetching it now delays more urgent current work.

**Exercise:** accuracy rises while p99.99 gets worse. Give two explanations. Possible answers: useful lines are issued too early and evict hot state; or useful requests still overload shared resources during bursts.

## 6. Dimension four: determinism and the tail

**Engineering axiom:** accept a performance change against an explicit latency distribution and failure budget under defined conditions.

Explicit software hints offer control over candidate addresses and source placement. They do not make cache replacement, coherence, OS scheduling, interrupts, frequency behavior, or memory service deterministic. General-purpose CPUs and operating systems do not become hard real-time systems because the hot loop has predictable branches.

Hardware prefetching is not intrinsically inappropriate for trading. Sequential packet and descriptor streams may benefit substantially. Equally, an irregular symbol-index lookup may offer semantic information that a software hint can exploit earlier.

### 6.1 Separate service time and response time

```text
Scheduled arrival -- queue wait -- worker starts -- processing -- completion
       |                            |                         |
       |                            +------ service time -----+
       +---------------- response time -----------------------+
```

A loop benchmark measures aspects of processing cost. Tick-to-trade may also include receive queues, decoding, cross-thread handoff, and transmit queues. Decide whether completion means creating an order object, publishing a transmit descriptor, NIC transmission, or an exchange response. Those are different boundaries.

If your load generator waits for the previous event to complete before creating the next, a stall also suppresses arrivals. That can hide the queueing a real independent market feed would create: **coordinated omission**. Prefer scheduled independent arrivals and record actual queueing or lateness. Histogram correction is useful under stated assumptions but cannot recreate arbitrary dropped traffic. The [HdrHistogram project](https://github.com/HdrHistogram/HdrHistogram) documents interval recording and coordinated-omission correction.

### 6.2 Collect enough tail evidence

At p99.99, the upper fraction is `0.0001`. With one million independent observations, only about 100 lie above the population threshold in expectation. With ten thousand, only about one does. Neither is a guarantee of accurate estimation, especially with clustered bursts and nonstationary behavior.

Report sample count, duration, repeated runs, and trace conditions. Use time windows to reveal phase changes. If you estimate confidence intervals, account for dependence—for example by resampling appropriate time blocks rather than blindly treating consecutive events as independent.

Useful reports include p50, p99, p99.9, p99.99, observed maximum, deadline violations, drops, and queue depth. The observed maximum applies only to the observation period. Standard deviation can supplement these; it cannot replace tail statistics.

### 6.3 Identify hardware/software interaction experimentally

Where platform controls are documented and available on a test machine, compare:

| Hardware prefetch | Software hint | What the comparison helps investigate |
|---|---|---|
| On | Off | Deployment baseline |
| On | On | Incremental effect under normal hardware behavior |
| Off | Off | Cost of removing hardware prediction |
| Off | On | Software effect without that hardware configuration |

Keep those controls scoped to a documented test environment. Controls, scope, privileges, and effects differ across CPUs. Do not copy arbitrary model-specific register writes from another processor.

The interaction between the two mechanisms is not necessarily additive. A software request can duplicate or complement hardware work; changing one can change the other's behavior. There is no universal “prefetchers fighting” counter. Rely on comparisons plus appropriately documented PMU evidence.

## 7. Dimension five: layout, locality, and stride

**Engineering axiom:** improve the usefulness and independence of accesses before adding instructions that anticipate them.

### 7.1 AoS and SoA solve different access problems

An array of structures, AoS, places complete records together:

```rust
#[repr(C)]
struct Quote {
    bid_ticks: u64,
    ask_ticks: u64,
    bid_qty: u64,
    ask_qty: u64,
}
// Vec<Quote>: [quote 0][quote 1][quote 2]...
```

This is flat contiguous storage. It does not require following pointers. A structure of arrays, SoA, places each field in its own contiguous sequence:

```rust
struct Quotes {
    bid_ticks: Vec<u64>,
    ask_ticks: Vec<u64>,
    bid_qty: Vec<u64>,
    ask_qty: Vec<u64>,
}
```

```text
AoS: [bid0 ask0 bq0 aq0] [bid1 ask1 bq1 aq1] [bid2 ask2 bq2 aq2]

SoA: bid: [bid0 bid1 bid2 ...]
     ask: [ask0 ask1 ask2 ...]
     bq:  [bq0  bq1  bq2  ...]
     aq:  [aq0  aq1  aq2  ...]
```

If the operation scans only asks, SoA avoids bringing unrelated fields into the scan and may support vectorization. If it needs all four fields of one randomly selected symbol, AoS may use fewer independent line fetches. SoA may introduce more streams, allocations, and TLB demand.

A hybrid is often appropriate: keep frequently used fields together in a compact hot record, and move infrequent metadata elsewhere. Array-of-structures-of-arrays layouts can balance vector operations and per-record locality. Choose based on actual access patterns.

### 7.2 Useful bytes, stride, and address predictability

A conceptual line-utilization measure is:

```text
utilization = distinct useful bytes accessed during the line's residency / line bytes
```

Reading one 8-byte field from an otherwise unused aligned 64-byte record gives 12.5% useful bytes for that residency interval. Scanning eight adjacent `u64` values can use the full line. Repeatedly reading the same eight bytes does not make byte utilization exceed 100%.

**Stride** is the byte difference between successive accesses in a stream. Reading `u64` elements sequentially gives an 8-byte element stride; cache-line requests arise only when crossing into new lines. The prefetch **distance** is how far ahead a request targets. These are different quantities.

There is no universal stride that keeps “L3-to-L1” optimally primed. Hardware mechanisms recognize particular patterns with model-specific limits. A contiguous layout can enable hardware prefetching so effectively that extra software hints add only overhead.

### 7.3 Alignment and false sharing

Two independent worker-owned counters in one line can cause coherence traffic when different cores write them. That is false sharing. Prefetching does not fix ownership contention. Appropriate padding or per-core state can help, but padding every symbol to 64 bytes can dramatically enlarge a working set.

The lab deliberately uses a 64-byte record to expose line-granularity lookahead. It spends 16 bytes on useful fields and the rest on padding. A compact 16-byte production record may be superior. Treat the lab layout as an experimental control, not a recommended default.

## 8. Rust implementation: anticipate data without changing decisions

Three Rust concepts are enough for the first version:

- `&T` borrows an existing object; the object must remain valid while borrowed.
- `&[T]` borrows a contiguous slice; `get(index)` returns an optional reference instead of indexing past its end.
- A const generic such as `const D: usize` lets the compiler specialize a function for a chosen distance.

### 8.1 A small architecture-specific wrapper

The lab's wrapper takes a reference to a live object and converts it to the pointer required by the intrinsic:

```rust
#[inline(always)]
fn prefetch_read<T>(value: &T) {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
        #[allow(unused_unsafe)]
        unsafe {
            _mm_prefetch::<_MM_HINT_T0>((value as *const T).cast::<i8>());
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = value;
}
```

Current Rust documents `_mm_prefetch` as safe; the block preserves compatibility with older stable declarations that required `unsafe`. It does not make an invalid pointer valid. The implementation intentionally avoids constructing an out-of-bounds pointer at all. On other architectures this wrapper issues no hint, and the lab prints that fact. Adding another architecture requires its own supported primitive and measurement.

T0 is the first experiment, not an optimality claim. Broadly, T0 expresses high locality, T1/T2 lower target levels, and NTA low temporal reuse. Exact interpretation is implementation-dependent. This wrapper hints only the line containing the address; it does not recursively fetch a record's pointers or necessarily fetch an entire multi-line object.

### 8.2 A bounded lookahead loop

The central loop is:

```rust
for (i, request) in requests.iter().enumerate() {
    if DISTANCE != 0 {
        if let Some(future) = i.checked_add(DISTANCE)
            .and_then(|j| requests.get(j))
        {
            if let Some(book) = books.get(future.symbol) {
                prefetch_read(book);
            }
        }
    }
    let qty = accepted_qty(&books[request.symbol], request);
    checksum = checksum.wrapping_add(qty);
}
```

`checked_add` handles even an excessive distance without integer wraparound. `get` handles the end of the batch. Demand access retains normal bounds checking; an invalid current symbol is a caller error and panics. A real decoder should validate symbol IDs before admitting work to its trusted processing representation.

Distance zero removes the hint at compile time. The current request is processed in the same order at every distance. Future requests supply addresses only; they do not authorize future decisions to run early.

The decision function accepts quantity when the toy book's ask is at or below the request's limit. All quantities and prices are integers. It is read-only and omits position changes, order state, exchange semantics, and actual risk policy. This makes equality across prefetch settings easy to test without pretending to implement trading correctness.

The checksum intentionally uses wrapping addition as a benchmark sink. Do not generalize wrapping arithmetic to financial exposure or position accounting; use explicitly validated arithmetic for that policy.

### 8.3 Ownership and synchronization still apply

The lab's books and decoded requests are immutable for the measured run. In a live system they may not be. A prefetch neither performs acquire/release synchronization nor establishes a happens-before relationship.

For a producer/consumer ring, acquire ownership of published entries according to the ring protocol before reading their fields. Do not read uninitialized slots to obtain a future address. Do not retain references across slot reuse. DMA buffers must follow their driver and device ownership rules. Data races remain invalid regardless of whether a prefetch instruction itself faults.

Memory locking can reduce paging risk; it does not pin lines in a CPU cache. Preallocation and page touching should occur during setup where appropriate; software hints are not a dependable way to populate missing pages.

## 9. Run the lab and interpret it honestly

The lab has no third-party crate dependencies. It provides random-symbol, sequential-symbol, and small-hot-set workloads. It repeats an identical generated trace so comparisons are reproducible. That repetition can favor cache residency and prediction; it does not represent every market trace.

From the accompanying `prefetch-lab` directory:

```sh
cargo test --offline
cargo build --offline --release

# Args: distance books requests repeats pattern [optional CSV output]
./target/release/prefetch-lab 0 1048576 131072 40 random baseline.csv
./target/release/prefetch-lab 8 1048576 131072 40 random distance-8.csv
./target/release/prefetch-lab 8 1048576 131072 40 sequential sequential.csv
./target/release/prefetch-lab 8 1048576 131072 40 hot hot.csv
```

CSV files are written after measurement and overwritten if their names already exist. Use separate filenames per run. Optional arguments, defaults, and accepted distances are shown by `--help`.

The initial book array is 64 MiB; request storage adds another allocation. Confirm the actual machine's cache sizes. Increase or decrease the footprint to study L1/L2/LLC-resident and larger-than-LLC cases; this size alone does not establish which level serves a load.

### 9.1 What the lab measures

It times chunks of at most 4,096 requests using `Instant`, then divides each chunk's duration by its item count. Samples are batch-average ns/item. It reports their median, p99, and maximum, plus a mean weighted by item count.

**Those are not per-event percentiles.** A large pause affecting one request can be diluted by the other requests in its batch. The lab intentionally does not print a misleading p99.99 tick-to-trade value.

Allocation, data generation, page initialization, sorting, and output occur outside timed chunks. A warmup pass uses the selected variant. Each chunk restarts lookahead, leaving a short unprefetched prefix; it does not carry prefetches over to the next chunk. This is controlled but differs from a continuous worker.

`black_box` discourages compiler elimination of work and results; it is a best-effort optimization barrier, not a serialization primitive or a correctness tool. See [Rust's `black_box` documentation](https://doc.rust-lang.org/std/hint/fn.black_box.html).

### 9.2 Read results as evidence for a hypothesis

If distance eight improves random lookups but does nothing for sequential lookups, a plausible explanation is that software-visible future symbol IDs add information for the irregular stream while hardware already handles the sequential case. That is an inference to investigate, not proof of the exact cache-level mechanism.

If every distance is slower for the hot-set case, issuing extra instructions for already resident data may be wasted work. If large distances regress, investigate pollution, lookahead overhead, and request pressure. Do not assume one explanation without supporting evidence.

Run fresh independent trials, alternate baseline and treatment order, and save raw samples. Record warmup duration and whether cold behavior matters. Do not compare one cold baseline with one warmed treatment. Avoid taking the best run from a large sweep as if it were an unbiased production estimate.

## 10. From a loop experiment to a production measurement

### 10.1 Confirm the generated code

Build with optimization, inspect assembly, and verify that the intended hint exists near the useful work:

```sh
cargo rustc --offline --release -- --emit=asm
rg -n 'prefetch' target/release/deps/*.s
```

Read the surrounding loop, not merely the matched mnemonic. Confirm whether address calculation, checks, or branches dominate. Const specialization can change generated code; repeated evaluation of an unchanged expression might be optimized. Keep correctness checks when they are needed.

The chapter's default build does not use `target-cpu=native`. You can test it on a controlled machine, but a binary compiled for the build machine's features must not be assumed safe or appropriate for a different deployment CPU.

### 10.2 Profile with counters after measuring latency

On Linux, useful initial commands are:

```sh
perf list
perf stat -e cycles,instructions,cache-references,cache-misses \
    ./target/release/prefetch-lab 0 1048576 131072 40 random
```

Event availability and semantics depend on the CPU and kernel. Generic `cache-misses` is not a reliable name for “all L1 misses.” Use the matching model's event definitions for L1/L2/LLC, load latency, TLB effects, and request pressure. Avoid raw event encodings copied from another model. [Linux's workload tracing documentation](https://docs.kernel.org/admin-guide/workload-tracing.html) introduces `perf` and its PMU interface.

This process-level `perf stat` command includes initialization, warmup, and output as well as the measured loop. It is an initial diagnostic, not exact attribution to the hot region. Production experiments should enable counters around the relevant region or use a long, separately attributable processing phase. Check whether counts were multiplexed and whether you had permission to measure them.

Treat profiling overhead as another experimental variable. Detailed sampling and per-event timestamping can perturb the tail you are investigating. Compare with a less instrumented validation run.

### 10.3 Build an independent-arrival replay harness

A useful next harness has a scheduled input trace, a preallocated receive queue, a worker, and a measurement collector outside the critical path:

```text
Trace: planned arrival time, symbol, payload
                   |
                   v
  Replay producer --> bounded queue --> trading worker --> completion record
       |                   |                 |                   |
  schedule/lateness     depth/drops      prefetch variant     latency collector
```

The producer must preserve the intended arrival schedule independently of completion. Record producer lateness and drops: a saturated generator invalidates the intended workload. Avoid letting a shared producer core or measurement collector become the bottleneck.

Preallocate event timestamps or histogram storage. Timestamp at a defined boundary. For cross-core or NIC clocks, establish synchronization and conversion rather than subtracting unrelated clocks. With TSC-based measurement, use an architecture-appropriate ordered sequence and document its overhead; do not insert fences simply to “wait for the prefetch.”

Measure both worker service time and full response time. For a batch receiver, include time waiting for the batch to fill if that is inside the product's latency boundary. Larger batches can help memory-level parallelism while worsening the latency of the first arriving packet. DPDK's [efficient-code guidance](https://doc.dpdk.org/guides/prog_guide/writing_efficient_code.html) discusses layout, locality, and the batching tradeoff.

### 10.4 Cover the conditions that can invalidate a win

| Axis | Suggested cases | What it can reveal |
|---|---|---|
| Access pattern | sequential, repeated hot symbols, random, abrupt symbol shift | hardware prediction and semantic lookahead |
| Footprint | small resident set through larger-than-LLC | dependence on cache capacity |
| Arrivals | quiet, steady, microbursts, prolonged burst, recovery | queueing and resource saturation |
| Locality | intended local NUMA placement; remote placement as a controlled test | topology sensitivity |
| Other work | intended co-tenants and shared-cache consumers | collateral interference |
| Ownership | read-mostly versus the actual update/coherence pattern | invalidation and state handoff costs |
| System state | steady warm operation, startup/reconnect if relevant | transient cache and translation behavior |

Treat cold recovery and ordinary steady-state objectives separately. Do not force cache flushes into every iteration unless that artificial condition is the deliberate subject of the experiment.

## 11. A concrete decision record for shipping

Use one short record per candidate change:

```text
Hypothesis:
  Future decoded symbol IDs expose random book addresses early enough
  to overlap demand misses without harming the complete decision path.

Change:
  One optional read hint per future book, fixed distance D, same event order.

Scope:
  CPU/model/microcode, kernel, Rust version, build flags, NIC/NUMA topology,
  affinity, hardware-prefetch setting, trace and working-set description.

Correctness:
  Identical decisions, valid ownership, no dependence on hint completion,
  defined behavior for invalid input and queue overflow.

Results:
  Run counts and durations; service and response distributions;
  deadline violations, drops, queue depth; relevant counter evidence.

Acceptance:
  Explicit improvement target and tail/drop guardrails, chosen before testing.

Rollback:
  Disable hint or deploy the baseline implementation if guardrails regress.
```

Configure the variant outside the per-event loop where practical. A small fixed dispatch to specialized handlers can keep the common hot loop simple. Expose a no-prefetch option so the optimization remains reversible. Record which variant actually ran.

For an adaptive distance policy, account for its branch cost, measurement delay, hysteresis, and unstable feedback under bursts. Begin with a stable fixed distance and a complete measurement record. Adaptive policy is another optimization to validate, not an automatic upgrade.

“Production-ready” means more than inserting `_mm_prefetch`: validated input, bounded storage, defined queue behavior, safe lifetime and concurrency rules, predictable allocation behavior, accurate measurements, observable regressions, and a deployment-specific performance case. The hint must remain optional for correctness.

## 12. Guided exercises: from understanding to implementation

### Exercise A — establish the no-hint baseline

Run distance zero for all three patterns at several book counts. Save the environment and raw batch samples. Explain why a 64-symbol hot set and a large random set need not have similar results.

**Expected lesson:** working-set residency and access pattern can dominate any gain from explicitly fetching ahead.

### Exercise B — draw the distance curve

Sweep supported distances with a fixed random trace. Repeat the sweep in a different order. Plot mean batch ns/item versus distance and include variation across runs. Choose a candidate from repeated evidence, then validate it on a different trace.

**Expected lesson:** the best distance is a property of a workload and deployment, and tuning can overfit.

### Exercise C — remove the padding

Replace the lab's book with a compact record containing only its two meaningful fields. Re-run tests and adjust the size assertion to reflect the intended new layout. Keep useful field values and request streams identical.

**Expected lesson:** increasing useful records per line can outperform speculative fetching. One hint can cover multiple adjacent records; repeated requests may become redundant.

### Exercise D — test AoS against SoA

Make separate ask and quantity vectors. Compare both single-symbol decisions and an ask-only scan. Keep totals and semantics identical.

**Expected lesson:** AoS/SoA decisions depend on which fields are accessed together, and a single winning benchmark does not settle every path.

### Exercise E — introduce a controlled competitor

Add a small risk-table read to each decision, retaining correct results. Increase the book lookahead while tracking the complete path. Keep risk values in the returned checksum so the compiler cannot trivially discard the reads.

**Expected lesson:** a faster target access can coexist with a slower overall path. A meaningful pollution experiment must preserve the competitor's actual work.

### Exercise F — measure the actual tail

Build the independent-arrival harness from section 10.3. Replay a trace with a quiet phase, a burst, a symbol-distribution change, and recovery. Record per-event service and response latency, drops, and queue depth. Compare baseline and the selected candidate on repeated runs.

**Expected lesson:** a microbenchmark identifies candidates; the complete workload decides whether they are useful.

## 13. Review questions with short answers

**Why not prefetch immediately before every load?** There may be almost no independent work to overlap the request, while instruction overhead remains.

**Can I rely on a hint to fault in pages before trading starts?** No. Explicitly initialize/touch the intended memory during setup and use appropriate OS facilities where required.

**Does a larger distance always hide more latency?** It creates more potential lead time but also a longer opportunity for eviction and more speculative pressure.

**Can highly accurate prefetching be slower?** Yes. The data may already be cached, arrive at an unhelpful time, or compete with more urgent work.

**Should I disable hardware prefetching for determinism?** Only consider it as a documented experiment on the target system; default assumptions are insufficient evidence.

**Can I add stage p99.99 values to obtain total p99.99?** No. Measure the total interval on the same events.

**Is a normal data hint an instruction-cache prefetch?** No. Diagnose instruction-fetch problems separately; investigate code size, layout, and control flow using the target's tools.

**When is no software prefetch the right result?** Whenever simpler layout/access changes solve the problem, the hardware already handles it, useful addresses cannot be known early, or measured end-to-end guardrails do not improve.

## References and how to use them

The original outline linked several practitioner articles. This chapter treats them as motivation rather than hardware specifications. Architecture-specific behavior should be checked against the exact deployed CPU and toolchain. Links were consulted on 19 October 2026; vendor “stable” and “latest” URLs can change.

1. [Intel optimization reference hub](https://www.intel.com/content/www/us/en/developer/articles/technical/intel64-and-ia32-architectures-optimization.html): entry point for current manuals and processor-specific guidance.
2. [Intel optimization manual, archived April 2012 edition](https://www.intel.com/content/dam/doc/manual/64-ia-32-architectures-optimization-manual.pdf): historical discussion in chapter 7 of prefetch placement, scheduling distance, and overhead. Do not transfer its CPU-specific recommendations to modern hardware without checking.
3. [Rust `_mm_prefetch`](https://doc.rust-lang.org/stable/core/arch/x86_64/fn._mm_prefetch.html): available hints and intrinsic behavior; Rust API safety declarations can differ across toolchain versions.
4. [Rust pointer module](https://doc.rust-lang.org/std/ptr/index.html): raw-pointer validity and operations; instruction-level fault suppression does not relax Rust's memory rules.
5. [Rust `black_box`](https://doc.rust-lang.org/std/hint/fn.black_box.html): limits of the optimization barrier used in the lab.
6. [DPDK prefetch API, version 21.08](https://doc.dpdk.org/api-21.08/rte__prefetch_8h.html): an example of architecture-dependent data-plane hint APIs, not a Rust dependency or CPU timing specification.
7. [DPDK efficient-code guidance](https://doc.dpdk.org/guides/prog_guide/writing_efficient_code.html): locality, layout, and processing tradeoffs; the linked live documentation can track a development release.
8. [Intel Broadwell-X PMU events](https://perfmon-events.intel.com/platforms/broadwellx/core-events/core/): illustrative event semantics. Obtain a different catalog for a different CPU.
9. [Linux workload tracing](https://docs.kernel.org/admin-guide/workload-tracing.html): introduction to `perf` and performance counters.
10. [HdrHistogram](https://github.com/HdrHistogram/HdrHistogram): histogram design, interval recording, and coordinated-omission considerations.
11. [AMD Zen 5 software optimization guide](https://docs.amd.com/v/u/en-US/58455_1.00): a processor-family-specific starting point for AMD deployments; revalidate hint choices and PMU interpretation there rather than assuming Intel behavior.

The formulas, toy workloads, example numbers, decision record, and exercises are teaching models supplied by this chapter. They are not measurements from an exchange or a vendor's performance promises.
