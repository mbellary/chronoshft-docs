# Branch Prediction for Low Latency Trading Systems in Rust

A beginner’s chapter from CPU behavior to production measurement

## What you will learn

Branch prediction lets a CPU begin work before it knows which instructions a program will need next. When that guess is wrong, useful work can be delayed. In a trading system, the consequences extend beyond one calculation: a delayed market update can hold up the messages behind it, leave a strategy using stale state, or delay an order decision.

The engineering objective is **correct decisions within a measured latency budget under realistic load**. Reducing branch misses is one possible means. It is not the objective itself.

This chapter follows six dimensions in three layers. It assumes no previous knowledge of CPU pipelines or Rust. You should be able to read a small program, run terminal commands, and work through integer comparisons. All trading rules and timing numbers used to explain concepts are educational examples, not claims about a particular trading venue or deployed system.

| Layer | Dimension | Question to answer before moving on |
|---|---|---|
| Microscopic | Instruction level metrics | Does speculation materially limit this workload? |
| Microscopic | Memory and caches | Did removing a branch introduce costly memory work? |
| Software | Code paths and complexity | What machine code implements the decision? |
| Software | Latency profile | Did the important latency distribution improve? |
| Macro | Market state and volatility | Does the improvement survive different event sequences? |
| Macro | Hardware platforms | What evidence justifies another architecture? |

The accompanying Rust lab has no external crate dependencies. It compares three implementations of a small policy, checks their agreement, and offers separate throughput and instrumented timing modes. It is a measurement exercise, not a complete order gateway.

## 1 A trading message and the CPU

An **instruction** is a small operation the processor executes, such as loading a value, comparing two integers, or jumping to another instruction. Your Rust compiler translates source code into those instructions. A **branch** changes where execution goes next. A conditional branch chooses according to a condition; a loop also usually needs a branch. Calls, returns, and indirect jumps create additional prediction problems.

A toy trading decision might be:

```rust
if order.enabled {
    evaluate_order(order);
} else {
    reject_order(order);
}
```

The CPU may have to wait for `enabled` to arrive from memory. Meanwhile, it wants to fetch and execute more instructions. Its predictor guesses the direction, and potentially the target, of control flow so that execution can continue.

**Figure 1 — Where a branch sits in the trading path**

```text
Market packet
     |
     v
Decode --> Update book --> Strategy --> Risk checks --> Encode order
              |                             |
              v                             v
       change best price?              approve or reject?

Measure a small decision locally, then measure the full path under load.
```

A **book** is a representation of outstanding buy and sell interest. A **tick** is an integer unit of price. A **hot path** is code executed frequently enough, or critically enough, to matter to the target latency. A **cold path** handles less frequent work such as diagnostics or recovery.

Do not assume every `if` becomes a hardware branch. The compiler might replace it with a conditional move, calculate a Boolean value, combine conditions, or remove the operation entirely. A `match` may become comparisons, a jump table, a lookup, or arithmetic. This is why inspecting the optimized binary is part of the workflow.

### The prediction lifecycle

A simplified modern CPU does the following:

1. Fetch instructions using predicted control flow.
2. Decode instructions into internal operations, often called micro-operations or uops.
3. Execute ready operations, potentially out of source order.
4. Resolve the branch when its inputs become available.
5. Retire valid results in program order so that the visible program state remains correct.

**Retirement** is the point at which an instruction’s result becomes part of the committed program execution. **Speculation** is work undertaken before the CPU knows it belongs to that execution.

**Figure 2 — A correct guess and a wrong guess**

```text
Time -------------------------------------------------------------->

Correct: predict APPROVE -> fetch -> execute -> resolve APPROVE -> retire

Wrong:   predict APPROVE -> fetch -> execute -> resolve REJECT
                                      wrong work --X
                                                           |
                                                           v
                                             redirect -> execute REJECT

X means discard speculative results, not rewind every cache effect.
```

Misprediction recovery is not a fixed universal number of cycles. Branch resolution dependencies, instruction availability, outstanding memory operations, and the specific processor influence the observed cost. For arithmetic practice only, 15 cycles at an assumed 3 GHz is 5 ns. That is a conversion example, not a promised penalty for your CPU or a complete estimate of the event’s delay.

Three prediction jobs are worth distinguishing. A direction predictor estimates taken versus not taken. A branch target buffer caches likely destinations so instruction fetching can move promptly. Indirect calls and jumps can have several destinations, while returns are often helped by a return-address stack. These are related mechanisms, not one interchangeable “history buffer.” Their details vary by CPU; this model is enough to begin reading measurements without assuming a universal implementation.

### A small predictor you can understand

Imagine a predictor with a two-bit counter for a branch:

```text
Strongly not taken <-> Weakly not taken <-> Weakly taken <-> Strongly taken
        predict NO             NO                YES             YES

Observed YES: move one state right, stopping at the right edge.
Observed NO:  move one state left,  stopping at the left edge.
```

Starting from strongly taken, trace the outcomes `taken, taken, not taken, taken`. The predictions are `taken, taken, taken, taken`: the unusual not-taken event misses, moves the state to weakly taken, and the next taken event strengthens it again. This teaching model resists changing its prediction after one unusual event. Modern predictors use more elaborate histories and multiple mechanisms. An alternating sequence is not automatically unpredictable: a history-based mechanism can learn some alternating patterns. An approximately 50/50 outcome is not automatically difficult either, if it correlates with earlier branches. Conversely, a strongly biased branch can still produce expensive rare misses.

## 2 Microscopic layer instruction level metrics

### Start with a hypothesis

Consider: “This approval check becomes unpredictable when approval outcomes change rapidly. Replacing short-circuit exits with cheap Boolean operations may reduce wasted speculative work.”

This predicts a possible change in branch misses, cycles per record, and perhaps latency. It does not predict that every branchless implementation wins.

The CPU exposes a **performance monitoring unit**, or PMU, that counts hardware events. `perf` is a Linux tool for reading such counters and profiling code. Intel VTune can organize supported events into a hierarchy of likely bottlenecks. Intel’s top-down approach distinguishes retiring work, bad speculation, front-end limits, and back-end limits; start with the dominant category rather than assuming branches are the problem. [Intel top-down analysis](https://www.intel.com/content/www/us/en/docs/vtune-profiler/cookbook/2024-0/top-down-microarchitecture-analysis-method.html)

### Four counters and three ratios

Let `I` be retired instructions, `C` cycles, `B` counted branches, and `M` counted branch misses over the same region and execution scope.

```text
IPC                  = I / C
Branch miss rate     = M / B
Branch MPKI          = 1000 * M / I
Cycles per record    = C / number of processed records
Misses per record    = M / number of processed records
```

**IPC** means instructions per cycle. **MPKI** means misses per thousand instructions. Event definitions differ by processor: verify what the counters actually count. Generic branch events can include more than the particular conditional decision under study.

Illustrative counts for one million records:

| Metric | Version A | Version B |
|---|---:|---:|
| Retired instructions | 100 million | 110 million |
| Cycles | 80 million | 60 million |
| Branches | 20 million | 10 million |
| Branch misses | 1 million | 0.1 million |
| IPC | 1.25 | 1.83 |
| Branch miss rate | 5% | 1% |
| Cycles per record | 80 | 60 |

Here B does more retired work but uses fewer cycles. That is consistent with less wasted speculative work. It is not proof of causation: memory behavior, vectorization, code layout, and other effects may also have changed. A separate version could reduce both instructions and cycles while reducing IPC. It could still be better.

A lower branch miss percentage can also hide more misses per message if the denominator grows. Always keep absolute counts and work-normalized metrics. Do not multiply aggregate misses by an assumed penalty and call the result exact time lost; recovery overlaps with other activity.

### A counter experiment on Linux

Build once in release mode. Choose a permitted logical core using `taskset -pc $$` or your environment’s CPU allocation. On a machine with different core types, use the same type throughout. From the extracted lab directory:

```bash
cargo test
cargo build --release
CORE=2  # Replace with an allowed logical CPU on your machine.

perf list

taskset -c "$CORE" perf stat -r 7 \
  -e '{cycles:u,instructions:u}' \
  -- ./target/release/branch-prediction-lab bench branch mixed 65536 1000

taskset -c "$CORE" perf stat -r 7 \
  -e '{branches:u,branch-misses:u}' \
  -- ./target/release/branch-prediction-lab bench branch mixed 65536 1000
```

Repeat for `eager` and `table`. `:u` requests user-space counting. Event groups ask perf to measure related events together. Too many simultaneous events may fail to schedule or be multiplexed; check the output and split groups when necessary. Repeated runs report run-to-run variation, not a per-message percentile. [perf stat](https://man7.org/linux/man-pages/man1/perf-stat.1.html), [perf event groups](https://man7.org/linux/man-pages/man1/perf-list.1.html)

These commands count the whole process, including generation, warmup, and output. Make the processing phase long enough to dominate for exploratory comparisons. For precise work-normalized counts, enable and disable PMU collection around the processing region, or use profiler region controls. Do not divide whole-process counts by only timed records and present the result as an exact kernel measurement.

If hardware events are unavailable, record the limitation and use an authorized machine with PMU access. Do not silently substitute elapsed time for a branch counter. This chapter’s verification environment blocked PMU collection, so no measured IPC or branch-miss improvement is claimed here.

### Locate the responsible branch

When collection is supported:

```bash
perf record -e branch-misses:u -g -- \
  ./target/release/branch-prediction-lab bench branch mixed 65536 1000
perf report
perf annotate
```

Use debug information to find hot functions and inspect their assembly. Sampling is statistical, and event attribution can have skid: a sample may land after the operation that caused it. Model-specific precise sampling or branch records can help where supported. A hot assembly line is a lead to investigate, not proof that the corresponding Rust statement caused every miss. Intel’s tuning guide explicitly discusses attribution limitations. [Intel branch misprediction tuning](https://www.intel.com/content/dam/develop/external/us/en/documents/tuningguide-intelxeonprocessor-scalablefamily-2ndgen-181827.pdf)

In VTune, collect Microarchitecture Exploration on a supported setup, identify hot code, and inspect Bad Speculation alongside memory and front-end metrics. Keep profiled diagnostic runs separate from final latency runs because profiling perturbs execution. [VTune hardware analysis](https://www.intel.com/content/www/us/en/docs/vtune-profiler/user-guide/2023-0/general-exploration-analysis.html)

**Checkpoint:** Explain how cycles per message could improve even when IPC falls. You have reduced instruction count more sharply than cycle count; the ratio alone cannot rank implementations.

## 3 Microscopic layer memory and cache hierarchy

A cache stores recently useful data near the core. An L1 data cache is small and close; L2 is typically larger; a last-level cache and main memory sit farther away. Instruction caches hold executable instructions. Exact sizes, sharing, associativity, and timing depend on the platform. A **cache line** is the unit moved and managed by a cache, commonly 64 bytes on the x86 systems considered here.

**Figure 3 — Speculation has two kinds of consequences**

```text
                         CPU core
                   +-------------------+
Predicted path --->| temporary results  |--X if prediction is wrong
                   +-------------------+
                            |
                       speculative load
                            v
                         L1 cache
                            |
                         L2 cache
                            |
                    Last level / memory

Wrong path results: discarded before architectural commitment.
Cache contents: may retain a fetched line, including after recovery.
```

The important correction is that speculative data is **not necessarily immediately thrown out of the cache**. A wrong-path load may bring in a line that remains after its consuming instructions are squashed. It may evict a useful line or use memory resources; it may also happen to fetch data needed later. Not every misprediction produces a new cache line, and the affected cache level depends on the request and implementation. Intel documents the distinction between architectural results and surviving microarchitectural effects. [Intel speculative execution terminology](https://www.intel.com/content/www/us/en/developer/articles/technical/software-security-guidance/best-practices/refined-speculative-execution-terminology.html)

The trading consequence is a hypothesis worth testing: a wrong path that touches an unrelated instrument’s state may disturb useful data for the current stream. It is not valid to label every cache miss “branch pollution.” Footprint, ordinary loads, prefetching, coherence, and contention can all contribute.

### Why doing both sides can be expensive

```rust
fn choose_price(want_bid: bool, bid: u64, ask: u64) -> u64 {
    let mask = 0u64.wrapping_sub(u64::from(want_bid));
    (bid & mask) | (ask & !mask)
}
```

If `want_bid` is true, `mask` is all one bits and selects `bid`; otherwise it is all zero bits and selects `ask`. `wrapping_sub` explicitly defines modular arithmetic, including `0 - 1`. Ordinary unsigned subtraction can panic with overflow checks enabled and is the wrong way to express that intent.

This example is reasonable when both prices are already available as ordinary values. It does not justify loading both sides of a large structure or calling two expensive functions. Arguments are evaluated before this function runs. Replacing a conditional with `choose_price(flag, load_a(), load_b())` forces evaluation of both loads at the language level, whereas the conditional may need only one.

**Critical correctness rule:** eager evaluation must be safe for every input it evaluates. Replacing `i < data.len() && data[i] == x` with `&` can panic when `i` is out of range. Replacing guarded division can evaluate division by zero. Replacing conditional state updates can execute a side effect that should never occur.

### Cache-aware alternatives

An array of fixed records can improve locality compared with chasing pointers through separately allocated nodes. But a dense array can waste space if identifiers are sparse. An array of structures keeps each record’s fields together; a structure of arrays can reduce bytes read when a calculation needs only a few fields across many records. Neither layout wins universally.

A lookup table also costs memory. Six Boolean flags produce 64 combinations; twenty flags produce 1,048,576 combinations. Even a one-byte result then needs roughly 1 MiB before auxiliary state. Replacing arithmetic with a dependent table load can move a workload from speculation-bound to memory-bound.

For your experiments:

1. Vary the number of instruments or records so that the working set fits in different cache levels and then exceeds them.
2. Compare sequential access with a representative symbol-access sequence.
3. Use model-supported L1/L2/last-level events; generic `cache-misses` alone does not establish the L1/L2 story.
4. Compare cycles per record and latency alongside the cache counters.
5. Keep data layout and access order constant while changing only the decision implementation, then test layout changes separately.

**Checkpoint:** Removing a branch reduces misses but increases end-to-end p99.99. One possible explanation is that extra loads lengthened the dependent memory path. Other explanations must still be investigated.

## 4 Software layer code paths and complexity

### Read a little Rust first

In the following examples, `struct` groups fields, `u64` and `u32` are unsigned integer types, `bool` is true or false, and `&Order` borrows an order without taking ownership. `-> bool` means the function returns a Boolean. A final expression without a semicolon is the return value.

Prices are represented in integer ticks so that this lesson avoids floating-point rounding. The limits are illustrative. A production implementation must derive units and valid ranges from its actual protocol and business rules.

```rust
#[derive(Clone, Copy, Debug)]
struct Order {
    price_ticks: u64,
    qty: u32,
    enabled: bool,
}

const MAX_QTY: u32 = 1_000;
const MAX_PRICE: u64 = 10_000;

fn approve_branch(o: &Order) -> bool {
    if !o.enabled { return false; }
    if o.qty == 0 { return false; }
    if o.qty > MAX_QTY { return false; }
    if o.price_ticks > MAX_PRICE { return false; }
    true
}
```

This is a small policy: approve only when enabled, quantity is positive and within its limit, and price is within its cap. It is not a full risk check. Real checks can include stale data, trading status, position, credit, session state, and other requirements. Preserve whatever checks your actual system requires.

### Basic blocks and cyclomatic complexity

A **basic block** is a straight-line sequence of instructions with control flow entering at the beginning and leaving at the end. A conditional jump splits the control-flow graph into paths.

**Figure 4 — The source decision graph**

```text
Entry -> enabled? --no--------------------------> REJECT
            |
           yes
            v
         qty > 0? --no--------------------------> REJECT
            |
           yes
            v
       qty <= max? --no-------------------------> REJECT
            |
           yes
            v
      price <= cap? --no------------------------> REJECT
            |
           yes
            v
          APPROVE
```

For a connected control-flow graph, cyclomatic complexity is `E - N + 2`, where `E` counts edges and `N` nodes under the chosen graph convention. A source-level approximation is independent decisions plus one. This example has four `if` decisions, yielding five under that convention. Tools differ in how they count Boolean operators, matches, and early exits; state the convention you use.

To measure this example by hand, connect both return outcomes to one shared exit. Count eight nodes (entry, four decisions, two outcomes, exit) and eleven edges: `11 - 8 + 2 = 5`. For a larger function, draw or export its control-flow graph, count with the same convention, and retain that graph beside the result. [NIST structured testing](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication500-235.pdf)

Cyclomatic complexity helps identify testing and maintenance difficulty. It is not a predictor miss rate. A larger function might execute the same highly predictable path every time; a one-decision function might be hard to predict. Splitting a function can lower a per-function metric without changing the total machine work.

### Three implementations of one policy

**Version A uses short-circuit exits.** It can reject early and avoid later work. Keep it as the readable reference.

**Version B evaluates cheap predicates eagerly.**

```rust
fn approve_eager(o: &Order) -> bool {
    o.enabled
        & (o.qty != 0)
        & (o.qty <= MAX_QTY)
        & (o.price_ticks <= MAX_PRICE)
}
```

For Boolean operands, `&` evaluates both sides; `&&` short-circuits. These particular operations are pure and defined for all inputs, so eager evaluation preserves the approval result. It does not preserve a “first rejection reason” interface, because that is a different output contract.

**Version C encodes the state and looks it up.**

```rust
const ALLOW: [bool; 16] = [
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, true,
];

fn approve_table(o: &Order) -> bool {
    let flags = usize::from(o.enabled)
        | (usize::from(o.qty != 0) << 1)
        | (usize::from(o.qty <= MAX_QTY) << 2)
        | (usize::from(o.price_ticks <= MAX_PRICE) << 3);
    ALLOW[flags]
}
```

Each predicate contributes one bit. Only `1111` in binary, or 15 in decimal, approves. The index is in `0..16` by construction. Ordinary safe indexing is sufficient; do not use unchecked access to chase a hypothetical bounds-check win.

This table intentionally teaches representation, not an expected optimization: for this policy it is redundant with checking that all flags are set. The compiler may simplify it. Tables become more useful when a small combination of flags maps to several actions, but table contents must preserve business semantics.

| Approach | Possible advantage | Possible cost |
|---|---|---|
| Early returns | Skip later work after rejection | Data-dependent control flow |
| Eager Boolean operations | Cheap uniform predicate evaluation | Always evaluates every predicate |
| Mask selection | Select already available scalar values | Extra operations and dependencies |
| Small table | Encodes bounded policy combinations | Index formation and possible dependent load |
| Large table | Replaces more computation | Cache footprint and maintenance burden |

### Inspect optimized assembly

From the lab directory:

```bash
cargo rustc --release -- --emit=asm
```

Read the `.s` file under `target/release/deps/` and locate the specialized `scan` functions. On x86, look for conditional jumps such as `je` and `jne`, condition-to-byte operations such as `setne`, and conditional moves such as `cmov`. Loop-control jumps remain even if the decision itself has no conditional jump. On AArch64 the instruction names differ, for example conditional selection can use `csel`.

Do not count every jump in the binary as an order-policy branch. Find the loop and the individual decision. Check whether a lookup survived optimization and whether the compiler unrolled or vectorized the loop. A vectorized batch scan is a different execution shape from one dependent event at a time.

The lab uses inlineable predicates and separate generic scan specializations to avoid selecting an implementation through a function pointer for every record. An outer `#[inline(never)]` on `scan` makes those functions easier to locate. Attributes influence compilation; they do not certify an instruction sequence.

For local experiments, `RUSTFLAGS="-C target-cpu=native" cargo build --release` can enable the build machine’s features. Such binaries may not run on other machines. Record a deliberate fleet target before shipping. Rust’s compiler options also support explicit target features and other code-generation controls. [Rust code generation options](https://doc.rust-lang.org/rustc/codegen-options/index.html)

### Profile-guided optimization

Profile-guided optimization, or PGO, trains compiler decisions using execution profiles. It may improve layout, inlining, and other choices. It does not make an intrinsically unpredictable event sequence predictable. The Rust workflow uses an instrumented build, training runs, profile merging, and a profile-use build. Train on representative regimes, then evaluate on separate held-out replays, including difficult transitions. Follow the documented toolchain-specific steps and keep non-profile compiler settings aligned. [Rust PGO workflow](https://doc.rust-lang.org/rustc/profile-guided-optimization.html)

**Checkpoint:** Is Version B branchless? At source level it has no short-circuit decision, but the generated machine code decides the hardware answer.

## 5 Software layer latency profile

### Define exactly what your timestamp means

**Service time** measures a processing region. **Queueing time** measures time waiting to start it. **Response latency** includes the delay from a defined arrival to a defined completion. In a trading path you might measure from packet receipt to book update, or from a usable market event to a fully formed outgoing order. These boundaries are different and must be named.

```text
Event becomes available ---> enters queue ---> starts work ---> finishes
          |                       |                |             |
          +-----------------------+----------------+-------------+
                                response latency
                                  |<-- waiting -->|<-- service -->|
```

For a pure queue-to-completion measurement, use queue entry as arrival; for an earlier boundary, include the earlier transport or decode interval too. Do not subtract timestamps from unrelated clocks without a synchronization and calibration plan. Exchange event timestamps, NIC timestamps, and CPU clock readings are not automatically interchangeable.

### Percentiles and jitter

Sort `N` observations. Under the nearest-rank convention, p99.99 is the observation at rank `ceil(0.9999 * N)`, with ranks beginning at one. It describes a threshold in that measured sample; it is not a guarantee for future messages. **Jitter** means variability; choose a concrete description such as `p99.99 - p50`, along with the histogram and maximum.

**Figure 5 — Similar averages can hide different behavior**

```text
Count
  ^
  |       A: many near 20 ns
  |       *****
  |       *****      B: concentrated near 40 ns
  |       *****      ********
  |       *****      ********                     A: rare slow events
  +-----------------------------------------------------------------> time
          20           40                                  200 ns

Conceptual distributions only, not a measurement.
```

A concrete arithmetic example makes the tradeoff clearer. Suppose A takes 20 ns for 99.98% of events and 200 ns for 0.02%; its mean is 20.036 ns and its population p99.99 is 200 ns. An idealized B always takes 40 ns, so both its mean and p99.99 are 40 ns. Under a strict 100 ns objective, B meets that objective while A misses it for 0.02% of events.

That does not establish B as universally more valuable to a trading desk. Doubling typical service time may increase utilization and queueing or lose more opportunities. Choose based on the latency budget, arrival rate, failure cost, and full distribution. A general-purpose CPU also cannot promise precisely 40 ns for every observation merely because a function has fixed control flow.

### How many samples are enough

At p99.99, the upper fraction is `0.0001`. Ten thousand observations leave roughly one observation in that fraction; one million leave roughly one hundred. These are expected counts, not confidence intervals. Temporal correlation, changing regimes, and repeated input sequences reduce the value of treating samples as independent.

Collect enough independent runs and enough events in each relevant regime. Report sample count, duration, warmup, drops, timeout handling, histogram resolution, p50, p99, p99.9, p99.99, maximum, and objective exceedances. A quantile threshold without its sample size and arrival conditions is weak evidence. Preserve outliers until their cause is understood; do not quietly delete interruptions to make a percentile attractive.

### Throughput timing and message timing answer different questions

```rust
let start = std::time::Instant::now();
// Process a large batch here.
let elapsed = start.elapsed();
let amortized_ns = elapsed.as_nanos() as f64 / number_of_records as f64;
```

This measures average elapsed time per record in a batch, including effects such as pipelining and vectorization. It does not measure each event’s latency and cannot produce p99.99.

Per-event timing uses a clock around each decision. At nanosecond scales the clock itself, barriers to optimization, and sample recording can rival the work. The lab’s `samples` mode therefore labels its output **instrumented samples**, not uninstrumented decision latency. `black_box` is a best-effort optimization barrier useful in benchmarking; it is not a CPU serialization primitive, security tool, or guarantee against every optimization. [Rust black_box documentation](https://doc.rust-lang.org/std/hint/fn.black_box.html)

Measure an empty timing region as a diagnostic, but do not subtract one constant from every percentile and call the result exact. The overhead and code interactions need not be constant or additive. For very short kernels, combine throughput timing, assembly, counters, and a larger realistic event boundary.

### The queue is part of your algorithm’s outcome

If messages arrive every 50 ns and one event takes 200 ns to process, messages keep arriving during the stall. They wait. At higher load, a small service-time regression can produce a much larger response-time regression.

**Figure 6 — A delayed event creates waiting behind it**

```text
Arrival:   A       B       C       D       E
           |       |       |       |       |
Worker:    [----------- A ----------][ B ][ C ][ D ][ E ]
                   B waits ----------^
                           C waits --------^

An idle benchmark hides this buildup.
```

A **closed-loop** test sends the next event after the previous one completes. During a slowdown it sends fewer events and can hide the waiting that independent arrivals would create. This is coordinated omission. An **open-loop** replay schedules arrivals independently of completions and measures from scheduled availability, preserving the backlog. Record generator lateness too: a replay generator that falls behind must not relabel a late injection as an on-time arrival. Histogram libraries can offer correction mechanisms, but faithful arrival scheduling is preferable when available. [HdrHistogram coordinated omission API](https://github.com/HdrHistogram/HdrHistogram/blob/master/src/main/java/org/HdrHistogram/AbstractHistogram.java)

For an educational single-worker model with arrival time `a[i]` and service time `s[i]`, compute:

```text
start[i]   = max(a[i], finish[i-1])
finish[i]  = start[i] + s[i]
waiting[i] = start[i] - a[i]
latency[i] = finish[i] - a[i]
```

This model excludes networking and parallel processing but makes backlog visible. Under overload, define bounded queues and explicit drop, reject, or recovery behavior. An ever-growing queue is not a latency solution.

**Checkpoint:** A loop processes ten million orders in a second. Can you infer p99.99? No; total time gives no per-event distribution or waiting behavior.

## 6 Macro layer market state and volatility

### A market regime is an input process

A **regime** is a period with characteristic behavior, such as quiet trading, a fast price move, or recovery after a feed interruption. The hardware does not observe a label saying “volatile.” It observes instruction addresses, branch outcomes, memory addresses, and dependencies.

Volatility can alter these sequences, but price volatility does not guarantee predictor difficulty. A busy interval might repeat the same message type and be easy to predict. A quiet stream might have an irregular sequence of relevant events. The useful question is whether a particular regime changes the inputs to your code in a performance-relevant way.

| Workload | What it teaches | What it cannot establish |
|---|---|---|
| Repeated valid order | Best-case steady path | Behavior under changing outcomes |
| Alternating outcomes | Response to a simple pattern | General unpredictability |
| Pseudorandom outcomes | Sensitivity to irregular decisions | Realistic temporal market structure |
| Correlated bursts | Regime transitions and locality changes | Complete exchange semantics |
| Historical replay | Recorded event ordering and mix | Every future condition |
| Fault injection | Recovery and exceptional paths | Frequency of those events in production |

The lab provides `steady`, `mixed`, `chaotic`, and `regime` synthetic inputs. `mixed` varies one flag; `chaotic` varies several rejection causes; `regime` alternates 4096-record windows of steady and irregular approvals. Its deterministic pseudorandom generator is an experiment tool, not market simulation or a source of cryptographic randomness. Repeating a finite buffer can train predictors and caches; vary buffer length and seeds when extending the experiment.

### Preserve the structure of historical data

A real replay needs valid initialization and state transitions. If a capture begins halfway through a session, obtain a consistent snapshot or replay the required prefix. Preserve the protocol’s ordering and recovery rules; sorting all messages by an exchange timestamp can corrupt the sequence, especially across independent channels.

For a concrete feed example, Nasdaq’s TotalView-ITCH specification defines message layouts, order references, timestamps, and book-changing events. Use the specification for the exact feed version you ingest. That specification is not a replay scheduler and does not make an isolated record stream a complete live transport capture. [Nasdaq TotalView-ITCH specification](https://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/NQTVITCHSpecification_5.0.pdf)

An effective replay corpus includes quiet and busy periods, bursts of cancels and executions, large instrument sets, status changes, and captured recovery cases where available. Supplement recorded data with clearly labeled faults such as gaps, duplicates, malformed lengths, and saturation. Do not invent historical stress results when you only ran a synthetic generator.

**Figure 7 — A replay suitable for latency experiments**

```text
Capture + initial state + protocol rules
                   |
                   v
             Validate sequence
                   |
          +--------+---------+
          |                  |
          v                  v
  Untimed reference    Arrival schedule
          |                  |
          v                  v
   Expected outputs    Timed candidate
          |                  |
          +--------+---------+
                   v
     Compare state, decisions, timing, drops
```

### Separate three replay modes

**Functional replay** checks that the book, decisions, rejects, and sequence handling match a trusted implementation. It can run without timing.

**Maximum-speed replay** measures throughput and exposes compute or memory limits. Removing gaps changes the input arrival process; do not call its latency “production latency.”

**Paced replay** preserves recorded arrival gaps, or applies a declared acceleration factor, and measures waiting plus processing. If only exchange event timestamps are available, say so: they approximate a different boundary from local packet receipt. The generator and capture clocks also need attention.

Include transitions in the timed region. Do not warm up on the exact future sequence and then claim to have measured an unseen regime change. Compare both a deliberately warm steady state and the first portion after a transition. A sliding window of misses or cycles can expose adaptation that an all-day average hides.

### Avoid optimizing for the training tape

Keep tuning data separate from evaluation data. Rotate A/B execution order, use paired replay segments, and repeat. When the reference is stateful, reset both versions identically between runs. Compare a digest of all relevant state and outputs, with detailed checks when the digest differs. Approval counts alone are insufficient for a stateful trading engine: two implementations can approve the same number of orders but approve different orders.

**Checkpoint:** The branch miss rate improves on a quiet replay but worsens after a rapid event-type change. Investigate the transition window, code footprint, and event sequence before concluding that all volatility is inherently unpredictable.

## 7 Macro layer hardware platforms

### Understand the CPU limit you have actually measured

“Branch history buffer saturation” is too imprecise to serve as a migration trigger. Predictors have finite tables and history mechanisms; target prediction, indirect branches, and returns involve additional structures. Addresses can alias in prediction tables, and code footprint and execution history matter. There is no universal number of source branches beyond which a processor suddenly becomes unsuitable.

Difficulty can come from genuinely weak correlation in outcomes, predictor conflicts, frequent changes in code paths, or a long wait for branch inputs. Separately, the front end can be limited by instruction-cache or translation behavior. Measure and attribute the bottleneck instead of labeling all these effects as one full buffer. The research literature also identifies systematically difficult and rarely executed branches even with advanced predictors. [Branch prediction research](https://arxiv.org/abs/1906.08170)

Before changing architecture, try the changes supported by your evidence: simplify decisions; remove duplicate computation; keep critical state local; reduce indirect dispatch where it matters; use measured code layout or PGO; and partition work without violating ordering. A faster memory lookup or a smaller working set may matter more than eliminating one branch.

### What an FPGA changes

An **FPGA**, or field-programmable gate array, can implement a custom circuit containing comparisons, arithmetic, storage, routing, and registers. A selection can become a multiplexer, or **mux**: hardware that chooses between values. There need not be a CPU-style speculative branch predictor in that datapath.

**Figure 8 — A fixed policy as a hardware pipeline**

```text
                     +----------------+
Order -------------->| enabled check  |----+
       |             +----------------+    |
       |             +----------------+    |
       +------------>| quantity check |----+--> combine --> register --> result
       |             +----------------+    |
       |             +----------------+    |
       +------------>| price check    |----+
                     +----------------+

Checks may operate concurrently when resources and dependencies allow.
State updates, shared memory, and output readiness can still impose stalls.
```

FPGA design does not mean every branch is eliminated by executing every possible path in parallel. A designer may duplicate computations, share resources across cycles, use a state machine, or select with a mux. The right circuit depends on dependencies, area, clock timing, and memory access.

Two numbers are essential: **latency**, cycles from input to output, and **initiation interval**, cycles between accepting successive inputs. A hypothetical 250 MHz circuit with eight pipeline stages has a nominal 32 ns pipeline latency. If its initiation interval is one, it can accept a new input every 4 ns when interfaces and dependencies permit. This is an arithmetic example, not a board or product specification. Pipeline dependencies and memory constraints can increase the initiation interval. [AMD pipeline scheduling](https://docs.amd.com/r/en-US/ug1448-hls-guidance/Pipeline-Scheduling?contentId=LG3gnOOZTRoh3_P6eNRnjA), [AMD latency definition](https://docs.amd.com/r/en-US/ug1399-vitis-hls/Latency?contentId=NXxKhchuo1IXGs9Nu8WdeA)

Backpressure occurs when a downstream stage cannot accept data. It can create waiting even in a fixed pipeline. Variable memory access, shared-state hazards, clock-domain crossings, and interfaces can also prevent a constant end-to-end latency. On a hybrid system, include any host-device and NIC path in the measured boundary; device kernel time alone is not the system result.

### An evidence-based platform decision

| Consideration | CPU implementation | FPGA implementation |
|---|---|---|
| Rapidly changing rules | Usually straightforward to revise in software | Requires a suitable reconfiguration or parameter plan |
| Bounded streaming checks | Can be efficient after tuning | Often a natural pipeline candidate |
| Irregular large state | Flexible memory and software tools | Memory ports, placement, and dependencies need careful design |
| Verification | Software tests, replay, deployment checks | Also simulation, synthesis, timing, and circuit equivalence work |
| Latency evidence | Full measured path under load | Full measured path including interfaces and stalls |

The “CPU cannot meet the objective” conclusion needs a defined objective, representative traffic, and repeatable evidence after reasonable tuning. The “FPGA can meet it” conclusion needs a concrete design and end-to-end measurement. Rust remains useful for the reference model, replay generator, host integration, and output checker; ordinary Rust compilation does not automatically produce an FPGA circuit. AMD’s Vitis HLS, for example, describes a C/C++ to RTL flow. [AMD Vitis HLS](https://www.amd.com/en/products/software/adaptive-socs-and-fpgas/vitis/vitis-hls.html)

**Checkpoint:** A device advertises one input per cycle. Does that mean one-cycle latency? No. Throughput and pipeline latency are different, and interface stalls still matter.

## 8 Run the Rust lab

### Start with correctness

Extract the lab, open a terminal in its directory, and run:

```bash
cargo test
cargo run --release -- verify
```

The test compares all three policies on important boundaries and 400,000 deterministic synthetic records. It includes disabled orders, zero quantity, exact limits, just-over-limit values, and integer maxima. This is useful differential testing for the example, not a proof for every future policy change.

Before an optimization, explicitly write the semantic contract. Does it return a Boolean, a rejection reason, or a state mutation? If the original returns the first rejection reason, merely agreeing on approve/reject does not preserve that interface. If it reserves credit, the test must also compare the reservation state and failure behavior.

### Run the throughput mode

```bash
cargo build --release
./target/release/branch-prediction-lab bench branch mixed 65536 1000
./target/release/branch-prediction-lab bench eager  mixed 65536 1000
./target/release/branch-prediction-lab bench table  mixed 65536 1000
```

The final arguments mean 65,536 records per buffer and 1,000 processing passes. The timer surrounds the repeated scans, excluding data generation and warmup. Output includes elapsed time, amortized nanoseconds per record, and a checksum. Equal checksums are a basic sanity check; the separate differential test checks individual results.

The scan counts approvals, not stateful matching-engine work. The compiler may unroll or vectorize it. Each pass reuses the same buffer. The example therefore measures a batch-processing kernel and helps you discover code-generation effects; it does not reproduce one packet triggering a dependent trading decision.

The data is passed through `black_box` at each scan boundary and outputs are consumed to discourage eliminating or hoisting the measured work. The policy implementations are chosen outside the timed loop. There is no allocation, logging, random generation, or per-record implementation dispatch inside the scan.

Repeat for each profile. Change buffer size to explore locality. Run configurations in alternating order, with no compilation or unrelated benchmark running concurrently, before interpreting differences. Affinity removes migration as one variable but does not guarantee isolation from interrupts, a sibling hardware thread, or other shared resources.

### Inspect the instrumented distribution

```bash
./target/release/branch-prediction-lab samples branch mixed 100000 10
./target/release/branch-prediction-lab samples eager  mixed 100000 10
```

This makes one million instrumented observations per command. Samples are stored after each measured interval and sorted afterward. Clock overhead remains in each observation, and the clock calls and recording alter the history and cache behavior of the next event. These observations cannot be directly compared with the throughput mode as if they measured identical execution.

Use the mode to learn sample count, sorting, nearest-rank quantiles, and interruptions. Then implement an arrival-driven replay around a realistic processing boundary. Do not claim this mode measures production p99.99.

### Keep a measurement notebook

For every candidate, retain the compiler version, build flags, binary identifier, source revision, CPU model, core placement, microcode and OS context, frequency/power policy, and workload identity. Record warmup, record count, arrival method, working-set size, measurement boundary, event definitions, and profiler overhead where relevant. Record the actual results, including rejected hypotheses.

A practical comparison table looks like this:

| Candidate and regime | Correct outputs | Cycles per event | Misses per event | p99.99 response | Drops or rejects |
|---|---|---|---|---|---|
| Baseline quiet | measured result | measured result | measured result | measured result | measured result |
| Candidate quiet | measured result | measured result | measured result | measured result | measured result |
| Baseline transition | measured result | measured result | measured result | measured result | measured result |
| Candidate transition | measured result | measured result | measured result | measured result | measured result |

Do not fill this table using fabricated benchmark numbers. If a counter is unavailable, write “unavailable.” If a latency boundary was not tested, write “not measured.” The illustrative arithmetic elsewhere in this chapter is separate from empirical results.

## 9 From a fast function to production code

A production-ready hot path needs bounded behavior and correct state management as well as good machine code. The point is not to remove every check. It is to place checks deliberately and validate trusted invariants at the right boundary.

Validate lengths, identifiers, units, and sequence state before constructing internal records. Reuse established invariants inside a narrow trusted representation, while keeping invalid-state handling explicit. Preallocate where justified, initialize and touch memory before the measured steady state, and avoid surprising allocation or synchronous logging in the critical path. A preallocated buffer is still finite: specify what happens when it fills.

When using notional limits, widen before multiplying:

```rust
fn within_notional(price_ticks: u64, qty: u32, cap: u128) -> bool {
    let notional = u128::from(price_ticks) * u128::from(qty);
    notional <= cap
}
```

The product of these particular operand widths fits in `u128`. Performing the multiplication in `u64` and converting afterward would not provide that guarantee. Price units and cap units must agree. Wider arithmetic may add work, but arithmetic correctness is part of the contract being optimized.

Shared state needs a concurrency design. A branchless risk check is still wrong if another thread can consume the same credit between the check and reservation. Ownership by a single event loop can simplify reasoning for some systems; multi-threaded designs need appropriate synchronization and ordering. A collection of relaxed atomics does not automatically preserve a cross-field invariant.

Keep essential diagnostics while bounding their cost: for example, fixed-size counters and a defined telemetry queue whose overflow behavior is known. Moving logging elsewhere can reduce critical-path delay but still uses CPU, cache, and queue capacity. Test the operational configuration that will actually run.

**Figure 9 — The optimization acceptance loop**

```text
Specify outputs and timing boundary
                 |
                 v
Reference + edge and state tests
                 |
                 v
Change one measured cause
                 |
                 v
Inspect assembly + collect diagnostics
                 |
                 v
Replay multiple regimes at declared load
                 |
                 v
Correctness + latency budget + capacity pass?
       | yes                         | no
       v                             v
Review and controlled rollout    Rework or discard candidate
       |
       v
Observe real behavior with a tested rollback path
```

A reasonable acceptance gate is agreement with the reference, no unexplained regression on required regimes, a measured response-latency distribution within the objective at target load, and bounded behavior on overload and recovery. Set the numerical thresholds from your system’s requirements before choosing a winning version. Compare error and drop behavior as well as successful messages; a system that silently stops processing hard cases can look deceptively fast.

## 10 A learning sequence with exercises

### Stage 1 Predict a single branch

Write down expected outcomes for always true, long runs of true, alternating, and pseudorandom inputs. Walk each through the two-bit predictor model. Explain why that model cannot tell you exactly what a modern predictor does.

**Done when:** you can distinguish branch direction, branch target, speculation, execution, and retirement.

### Stage 2 Measure and explain a kernel

Run the policy tests, inspect all three optimized scan implementations, and measure throughput on `steady` and `mixed`. If authorized PMU access is available, collect cycles, instructions, branches, and misses. Calculate the ratios yourself.

**Done when:** you can explain a result using emitted instructions and measured work, while separating evidence from hypotheses.

### Stage 3 Change the memory problem

Increase buffer size, add realistic instrument-state access, and compare sequential and irregular access. Keep the policy semantics constant. Test a small lookup versus a larger one and record whether the bottleneck moves.

**Done when:** you can explain why branch removal can worsen a dependent memory path.

### Stage 4 Measure a distribution

Use instrumented samples to learn percentile calculation. Build the single-worker queue model using fixed arrivals, then introduce rare long service times. Compare service and response distributions and count objective exceedances.

**Done when:** you no longer describe elapsed batch time divided by record count as a tail latency measurement.

### Stage 5 Replay changing conditions

Use a correctly initialized recorded feed or a clearly labeled stateful synthetic stream. Compare functional, maximum-speed, and paced replay. Include transitions, independent evaluation data, and overload. Check final state and event-by-event decisions.

**Done when:** you can explain what your replay preserves and what it omits.

### Stage 6 Make a platform argument

Write a short engineering note that identifies the limiting CPU behavior, the target latency and throughput, remaining software options, and a proposed hardware dataflow. Include memory dependencies, interface delays, backpressure, verification, and operational change cost.

**Done when:** your hardware proposal is grounded in a measured bottleneck and an implementable design, rather than a source-level branch count.

## 11 Questions to test your understanding

1. **A branch is taken half the time. Will the predictor miss half the time?** Not necessarily. Correlation and learnable history matter.
2. **Does squashing wrong-path work restore the previous cache contents?** Not generally; speculative cache effects may survive.
3. **If branch misses decrease, must IPC rise?** No. Both instruction count and other stalls can change.
4. **Is cyclomatic complexity a hardware performance counter?** No. It describes a control-flow graph under a stated counting convention.
5. **Is eager Boolean evaluation always safe?** No. It can evaluate an invalid access, division, or unwanted side effect previously guarded by short-circuiting.
6. **Does a constant instruction path guarantee constant nanoseconds?** No. Memory, contention, scheduling, interrupts, and other effects remain.
7. **Does an FPGA remove waiting?** No. Dependencies, shared resources, interfaces, and backpressure can cause it.
8. **What is the final proof that an optimization helps?** Correct behavior and a useful improvement at the required system boundary under representative conditions, with the measurement limitations stated.

## References and further reading

The chapter links sources beside the claims they support. These references are primary documentation or original research; the experimental workflow and trading examples are engineering applications of those concepts.

- Intel, [Top-down Microarchitecture Analysis Method](https://www.intel.com/content/www/us/en/docs/vtune-profiler/cookbook/2024-0/top-down-microarchitecture-analysis-method.html).
- Intel, [Refined Speculative Execution Terminology](https://www.intel.com/content/www/us/en/developer/articles/technical/software-security-guidance/best-practices/refined-speculative-execution-terminology.html).
- Intel, [Hardware Features and Behaviors Related to Speculative Execution](https://www.intel.com/content/www/us/en/developer/articles/technical/software-security-guidance/technical-documentation/hardware-behavior-related-to-speculative-execution.html).
- NIST, [Structured Testing](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication500-235.pdf).
- Linux perf documentation, [perf stat](https://man7.org/linux/man-pages/man1/perf-stat.1.html) and [perf list](https://man7.org/linux/man-pages/man1/perf-list.1.html).
- Rust documentation, [black_box](https://doc.rust-lang.org/std/hint/fn.black_box.html), [code generation](https://doc.rust-lang.org/rustc/codegen-options/index.html), and [PGO](https://doc.rust-lang.org/rustc/profile-guided-optimization.html).
- HdrHistogram, [project documentation](https://github.com/HdrHistogram/HdrHistogram).
- Nasdaq, [TotalView-ITCH 5.0 specification](https://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/NQTVITCHSpecification_5.0.pdf).
- Lin and Tarsa, [Branch Prediction Is Not a Solved Problem](https://arxiv.org/abs/1906.08170).
- AMD, [Vitis HLS](https://www.amd.com/en/products/software/adaptive-socs-and-fpgas/vitis/vitis-hls.html), [pipeline scheduling](https://docs.amd.com/r/en-US/ug1448-hls-guidance/Pipeline-Scheduling?contentId=LG3gnOOZTRoh3_P6eNRnjA), and [latency](https://docs.amd.com/r/en-US/ug1399-vitis-hls/Latency?contentId=NXxKhchuo1IXGs9Nu8WdeA).
