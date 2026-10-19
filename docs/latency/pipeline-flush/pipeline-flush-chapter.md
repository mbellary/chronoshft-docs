# Pipeline flushes in HFT: from CPU recovery to correct Rust systems

*A beginner’s chapter with diagrams, worked examples, measurement exercises, and a runnable Rust lab.*

**What you will learn.** Explain what a CPU discards after a bad prediction; distinguish that from fences, cache coherence, and network buffering; measure lost work without confusing throughput with latency; and write bounded Rust components that preserve correctness during a market-data gap or a partial network write.

**Scope.** Hardware discussion uses Intel-style x86-64 as the main example. AMD, Arm, individual CPU generations, NICs, drivers, and operating systems require their own manuals and measurements. All numerical examples below are hypothetical calculations unless explicitly identified otherwise. The supplied lab is educational code and a starting point for engineering reviews, not a complete exchange gateway or production trading system. Sources were checked on October 19, 2026.

**Companion files:** [Rust lab instructions](pipeline-flush-lab/README.md), [engine and writer source](pipeline-flush-lab/src/lib.rs), and [verification notes](verification.md). A standalone HTML edition includes scalable illustrations and a navigation sidebar.

## Learning map

Your topics naturally form five dimensions. The first three concern the processor; the last two concern application and network behavior.

| Dimension | Your topics | Question to answer before optimizing |
|---|---|---|
| 1. Control-flow recovery | Branch misprediction and pipeline refill | How much useful execution was delayed, and why? |
| 2. Ordering and serialization | LFENCE, SFENCE, MFENCE, CPUID, scheduling | Which ordering guarantee does correctness require? |
| 3. Data ownership and locality | Invalidation, RFO, false sharing | Which core owns and modifies each cache line? |
| 4. Queueing and transmission | Buffer drain, polling, Nagle, batching | Where does work wait, and can the consumer catch up? |
| 5. State recovery | Sequence gaps, snapshots, replay, rollback | When may the system safely make decisions again? |

Read sections 1–3 for vocabulary, 4–8 for the five dimensions, and 9–12 for measurement and production development. Work through the exercises after running the lab.

## 1. Replace one overloaded word with precise events

A pipeline is a sequence of stages through which work travels. Different work items can occupy different stages simultaneously. Think of a restaurant: one meal is being ordered while another is being cooked and a third is being served. A CPU does something analogous with instructions, although its execution is substantially more complex.

A CPU **pipeline flush** commonly means squashing instructions that must no longer complete architecturally, for example instructions younger than a mispredicted branch. A network **flush** usually means forwarding buffered bytes or draining queued work. A recovery operation might instead discard application state. These events have different causes, effects, and correctness rules.

A branch misprediction is routine on a speculative processor. It consumes a latency budget, but it does not automatically make a system nondeterministic or catastrophic. Two separate meanings of determinism matter:

- **Functional determinism:** the same accepted event sequence produces the same book and decisions.
- **Timing predictability:** processing time stays within a useful distribution or validated bound.

An ordinary Linux application on a general-purpose CPU typically measures a latency distribution. It should not claim a strict worst-case execution bound merely because it has run many benchmarks.

### Correct the proposed axioms before using them

| Starting claim | Engineering correction |
|---|---|
| A misprediction always costs 15–20+ cycles | This can be an illustrative scale, not an architectural constant. Branch resolution, dependencies, target availability, CPU model, and surrounding work affect observed loss. |
| Refill depth is a second fixed penalty to add | Commonly reported misprediction recovery costs already include some redirection and refill effects. Define measurement boundaries before adding anything. |
| All fences drain all stores and flush the pipeline | Fences have distinct ordering semantics. A memory-ordering barrier and a fully serializing instruction are different mechanisms. |
| A branch flush releases cache-line ownership | Squashing instructions does not generally invalidate the data cache or surrender every owned line. Speculative accesses can leave cache effects behind. |
| A subsequent read always triggers RFO | An ordinary demand read requests data. RFO concerns acquiring write ownership; the exact transaction depends on coherence state and architecture. |
| TCP_NODELAY means immediate wire transmission | It disables Nagle’s algorithm. Other queues, transport constraints, buffering, and scheduling remain. |
| Drop stale market data to get current | Only when the protocol’s semantics permit it. Discarding required incremental updates can corrupt the book. |

The instruction semantics come from the [Intel Software Developer’s Manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html). The persistence of microarchitectural effects after speculative execution is discussed in Intel’s [hardware behavior documentation](https://www.intel.com/content/www/us/en/developer/articles/technical/software-security-guidance/technical-documentation/hardware-behavior-related-to-speculative-execution.html). Socket behavior is specified in [Linux tcp(7)](https://man7.org/linux/man-pages/man7/tcp.7.html).

## 2. A small Rust vocabulary

You do not need advanced Rust to follow the first examples.

| Rust notation | Meaning in this chapter |
|---|---|
| `u64` | An unsigned 64-bit integer; useful for a sequence number or integer quantity |
| `usize` | An unsigned integer sized for indexing this machine’s memory |
| `let mut x` | A binding whose value can change |
| `&T` / `&mut T` | A shared borrow / an exclusive mutable borrow |
| `&[Update]` | A borrowed sequence of updates; no ownership transfer or copy of the whole sequence |
| `[u64; 8]` | Eight integers stored inline, with fixed capacity |
| `Vec<T>` | A growable allocation-backed sequence; useful during setup and experiments |
| `enum` | A type with named alternatives, such as `Live` and `Recovering` |
| `Result<T, E>` | Either `Ok(value)` or `Err(error)` |
| `?` | Return an error to the caller if this operation failed |
| `match` | Handle each possible outcome explicitly |
| `checked_add` | Addition that reports overflow instead of silently assuming it cannot happen |

In these examples, quantities and prices should use documented integer units. Floating-point prices introduce a separate representation problem. The supplied miniature book stores only quantities at eight fixed level indexes so the recovery mechanism stays visible.

Rust prevents many memory-safety bugs, but does not prove that an exchange sequence is complete, that a risk check is correct, or that a deadline is met. Ownership can help enforce one writer per book; protocol correctness still requires explicit invariants.

## 3. Follow one market event through the system

```text
Exchange              Local machine                                        Exchange
   |                       |                                                   ^
   | market event          v                                                   |
   +----> NIC RX -> RX queue -> decode -> sequence check -> book -> strategy     |
                                                                  |            |
                                                               risk check      |
                                                                  |            |
                                 NIC TX <- TX queue <- encode <- order intent --+
```

**RX** means receive. **TX** means transmit. The actual system may have additional queues, sessions, routing layers, and risk components.

For a chosen local boundary:

\[
L_{\text{local}} = t_{\text{TX boundary}} - t_{\text{RX boundary}}.
\]

A measurement from application receive to socket-write return is different from NIC hardware receive to NIC hardware transmit. A successful write does not establish wire transmission, exchange acceptance, or an execution. Give every chart its actual boundary.

A useful accounting model is:

\[
L = L_{\text{queue}} + L_{\text{processing}} + L_{\text{output waiting}}.
\]

This is an accounting decomposition using non-overlapping intervals. Within processing, cache delays, execution, and recovery can overlap; summing every hardware counter’s estimated cost can double-count time.

Imagine a hypothetical 1,000 ns local budget. A 5 ns disruption is 0.5% of that budget. It can still have an outsized consequence if it delays a dependency or contributes to backlog near saturation. The correct question is how it affects measured decisions and their tails under the expected traffic, not whether a particular number of nanoseconds sounds large.

## 4. Dimension 1 — control-flow recovery and refill

### 4.1 What the processor predicts

A branch chooses where execution continues. A processor predicts the direction or target so that it can keep work in flight before the actual answer is ready.

```rust
fn accepts(qty: u64, limit: u64) -> bool {
    if qty <= limit {
        true
    } else {
        false
    }
}
```

This source-level `if` may compile to a comparison that produces a Boolean without a branch. Source code alone cannot tell you whether the processor will predict a conditional jump. Compiler optimization, target features, and context determine the machine instructions.

A conceptual pipeline looks like this:

```text
Fetch -> Decode -> Rename -> Dispatch -> Execute -> Retire
                    |                       |
            physical registers      branches resolve here

Prediction: condition is true
    older work | branch | speculative A | speculative B | speculative C
                         ---------------- discard --------------------
Actual: condition is false
    older work | branch | redirect -> fetch correct path -> useful retirement
```

**Rename** maps program-visible registers to internal storage. **Dispatch** sends work toward execution resources. **Retire** commits completed instructions in program order. The diagram is conceptual: it omits queues, issue, multiple execution units, and model-specific stages.

On a wrong prediction, younger wrong-path instructions cannot retire as program results. Older valid instructions can still complete. The CPU redirects execution and rebuilds useful work. This is not a wholesale reset of every stage, cache, or predictor. Intel describes the distinction between architectural results and speculative effects in its [speculative-execution documentation](https://www.intel.com/content/www/us/en/developer/articles/technical/software-security-guidance/technical-documentation/hardware-behavior-related-to-speculative-execution.html).

### 4.2 Cycles, frequency, and honest arithmetic

A **cycle** is one tick of a clock domain. For a constant illustrative frequency:

\[
\text{nanoseconds} = \frac{\text{cycles}}{\text{GHz}}.
\]

| Assumed penalty | At 4 GHz | At 5 GHz |
|---|---:|---:|
| 15 cycles | 3.75 ns | 3.00 ns |
| 20 cycles | 5.00 ns | 4.00 ns |
| 25 cycles | 6.25 ns | 5.00 ns |

Thus 15–20 cycles over 4–5 GHz spans **3–5 ns**, not a universal 3.5–5 ns. These are unit conversions, not measurements or promises about a current CPU.

Modern frequency changes complicate conversions. An invariant timestamp counter, when present, normally advances at a reference rate; it is not a direct count of changing core cycles. Keep elapsed time, TSC ticks, and PMU core-cycle counts distinct. Instruction-specific timing requirements are documented in the Intel manuals’ [RDTSC and RDTSCP entries](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html).

### 4.3 Why pipeline depth is not enough

A branch dependent on a cache-missing load cannot resolve until the relevant data arrives. The wrong prediction may keep useless work flowing during that interval. After redirection, an instruction-cache or instruction-translation miss can delay the new path further. Conversely, independent older work may hide some recovery cost.

Define separate intervals if an investigation requires them:

1. Branch encountered to branch resolved.
2. Resolution to correct-path work available.
3. Correct-path work available to its useful retirement.

Ordinary application timing rarely exposes those intervals directly. Use model-specific performance analysis to interpret recovery-related events. Do not infer them from a simplified pipeline diagram, and do not add a standalone “refill penalty” to a measurement that already includes refill.

### 4.4 Translate this into Rust choices

Begin with a compact common path and a clearly expressed exceptional path:

```rust
#[cold]
fn report_gap(expected: u64, got: u64) {
    // In production: record a bounded event for another component.
    // Avoid synchronous formatting or disk I/O in the packet loop.
    let _ = (expected, got);
}
```

`#[cold]` expresses that a function is expected to be called rarely. It is a compiler hint, not a command to the CPU’s predictor. C++ `[[likely]]` syntax is not Rust syntax. See Rust’s [`cold` documentation](https://doc.rust-lang.org/stable/core/attribute.cold.html).

Next investigate:

- Data layouts that reduce pointer chasing and branch-dependent cache misses.
- Predictable protocol dispatch without needless virtual calls.
- Separating detailed error handling from the common instruction stream.
- PGO trained on representative traffic, including regime changes and exceptional cases.

Profile-guided optimization collects workload information and uses it in compilation. A quiet-session profile can misrepresent an opening auction or recovery workload; compare the resulting binary on independent replays. See the [rustc PGO guide](https://doc.rust-lang.org/rustc/profile-guided-optimization.html).

**Do not remove essential sequence, bounds, or risk checks to save a branch.** A conditional move can also create dependencies or perform extra work. A branchless rewrite must win in generated code and representative measurements while preserving behavior.

### 4.5 Lab: measure a workload, not a mythical fixed penalty

Run the supplied lab from its directory:

```sh
cargo run --release --offline -- branch same
cargo run --release --offline -- branch alternating
cargo run --release --offline -- branch random
```

Each mode processes the same number of values using different low-bit patterns. Alternation can be learnable; a fixed pseudorandom stream is not guaranteed to defeat a particular predictor. The data is prepared outside the timed region. Warm-up runs precede 40 timed batches.

The executable reports **batch-average nanoseconds per item**, including loop work and `black_box` overhead. This is a throughput-oriented experiment. Its slowest batch is not the worst individual event. Rust’s [`black_box`](https://doc.rust-lang.org/std/hint/fn.black_box.html) is a best-effort optimization barrier, not a hardware fence or a guarantee about the final instruction sequence.

Inspect the optimized code:

```sh
cargo rustc --release --offline --bin pipeline-flush-lab -- --emit=asm
```

Find `branch_work` in `target/release/deps/*.s`. A conditional jump that chooses a data path matters; the loop-back jump alone does not establish a branch-prediction experiment. If the compiler uses conditional moves or vectorization, explain the observed result accordingly.

On a Linux host with permitted hardware-counter access:

```sh
perf stat -r 5 -e cycles,instructions,branches,branch-misses \
  ./target/release/pipeline-flush-lab branch random
```

Run each mode separately and repeat with a larger timing workload if setup dominates. Whole-process counters include preparation, warm-up, and reporting. For stronger attribution, bracket the processing region with a profiling harness. Compute `IPC = instructions / cycles` and `branch miss rate = branch-misses / branches`; neither directly measures a single flush penalty. See [perf-stat(1)](https://man7.org/linux/man-pages/man1/perf-stat.1.html) and the processor-specific [Intel event catalog](https://perfmon-events.intel.com/).

## 5. Dimension 2 — ordering, fences, and serialization

### 5.1 Why ordering exists

Suppose one thread builds a quote and another uses it. Seeing a flag that says “ready” must mean the payload is available too. The source-code order, compiler transformations, and hardware memory ordering must agree with that contract.

```text
Producer core                           Consumer core
write payload
publish READY with Release  ---------->  observe READY with Acquire
                                        read the published payload
```

An Acquire load must observe the corresponding Release publication, or an applicable release sequence, for that synchronization to apply. Merely placing an Acquire instruction somewhere in a loop does not make arbitrary shared data safe.

### 5.2 Keep mechanisms distinct

| Mechanism | Main purpose | What you must not assume |
|---|---|---|
| `LFENCE` | Intel load ordering; also documented execution-ordering uses | That previous stores have all become globally visible |
| `SFENCE` | Store ordering, including relevant weakly ordered store cases | That all loads and instruction execution are fully serialized |
| `MFENCE` | Ordering of prior and subsequent loads and stores as specified | That it flushes all caches or is identical to CPUID |
| `CPUID` | Identification plus serializing behavior | That its timing is fixed, especially in virtual machines |
| Context switch | OS scheduling and architecture-specific state transition | That it is a memory-fence API or always clears every cache and TLB |
| Rust atomic ordering | A language-level concurrency guarantee | That each atomic operation emits one particular x86 fence |

Instruction details, memory types, and vendor behavior matter. The table summarizes semantics, not a performance table. Consult Intel’s [instruction reference for LFENCE, SFENCE, MFENCE, and CPUID](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html) for the target platform.

Fence delays depend on outstanding work. A fence placed behind many stores can expose waiting that an empty-loop benchmark does not show. Measure the actual publication pattern rather than assuming an instruction’s isolated timing predicts the application impact.

### 5.3 A safe, one-shot Rust publication example

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

struct Published {
    qty: AtomicU64,
    ready: AtomicBool,
}

fn main() {
    let p = Published {
        qty: AtomicU64::new(0),
        ready: AtomicBool::new(false),
    };
    thread::scope(|scope| {
        let producer = &p;
        scope.spawn(move || {
            producer.qty.store(42, Ordering::Relaxed);
            producer.ready.store(true, Ordering::Release);
        });
        while !p.ready.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        assert_eq!(p.qty.load(Ordering::Relaxed), 42);
    });
}
```

The payload is atomic too, keeping the example entirely in safe Rust. The producer writes exactly once and never overwrites it. After observing `ready == true`, the consumer can observe that publication. `spin_loop` is a processor hint, not a yield, a timeout, or a progress guarantee.

This is not a reusable queue. Resetting the flag and overwriting the payload introduces a new synchronization protocol. A multi-field quote also needs a coherent snapshot protocol; separate atomic fields alone do not guarantee they all describe the same update.

The correct ordering is the weakest ordering for which you can explain and validate the complete synchronization proof. Start from clear correctness, then review assembly and measurements. Rust defines the guarantees in [`Ordering`](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html). Avoid sprinkling inline assembly fences into an otherwise unproven algorithm.

### 5.4 Where beginners should stop optimizing

Do not weaken orderings because two variants happened to pass a stress test. Tests cover executions you observed; the language model describes executions you must allow for. Use established queue implementations, concurrency-model testing, and expert review before introducing custom lock-free storage with `UnsafeCell`, raw pointers, or manual memory reclamation.

Memory-mapped device registers and DMA add another boundary: Rust atomics for ordinary shared memory are not a substitute for a NIC API’s ownership, barriers, and completion rules.

## 6. Dimension 3 — cache ownership and locality

### 6.1 The cache line is the sharing unit

A cache holds chunks of memory called cache lines. A common x86 line size is 64 bytes, but verify the actual platform. Two independent variables can occupy the same line. If separate cores repeatedly write them, coherence can move write ownership between cores even though the program sees no logical sharing. This is **false sharing**.

```text
One shared cache line
+-----------------------------+-----------------------------+
| RX thread's packet counter  | TX thread's order counter   |
+-----------------------------+-----------------------------+
          Core A writes                 Core B writes
                ownership <-----------> ownership

Separate lines
+-----------------------------+   +-----------------------------+
| RX counter + padding        |   | TX counter + padding        |
+-----------------------------+   +-----------------------------+
          mostly Core A                    mostly Core B
```

Intel documents the locality problem and padding strategy in its [false-sharing guidance](https://www.intel.com/content/dam/develop/external/us/en/documents/gdma-2-165938.pdf).

### 6.2 RFO is about writing

If a core needs write permission for a line it does not own appropriately, a coherence transaction may acquire that permission and invalidate competing copies. An ordinary read may be served from a cache, another core, or memory, depending on the state and topology. A miss does not necessarily imply a DRAM trip. Intel’s [offcore event descriptions](https://perfmon-events.intel.com/platforms/icelakex/offcore-events/offcore/) explicitly distinguish demand data reads and ownership requests, including responses from another core’s modified line.

A wrong-path load can bring a cache line into a cache. A squashed branch does not mean that line disappears. Conversely, another core’s legitimate write can invalidate a line while the local core has had no branch misprediction. Treat control-flow recovery and coherence traffic as separate hypotheses.

### 6.3 Express an ownership-friendly layout in Rust

```rust
use std::sync::atomic::AtomicU64;

#[repr(align(64))]
struct PaddedCounter {
    value: AtomicU64,
}

struct Counters {
    received: PaddedCounter,
    transmitted: PaddedCounter,
}
```

On a platform with 64-byte lines, this gives these fields separate aligned storage. The size of each aligned wrapper is rounded to its alignment. It does not cure contention when every core updates `received`, and a different platform may need a different layout.

A stronger design often keeps counters local to the owner, then publishes summaries at an explicit cadence. Frequent monitoring reads can also change cache-sharing behavior. Include telemetry in the realistic benchmark.

For the order book, prefer a single owner per partition when practical. Partition by an actual sequencing and strategy boundary: symbol partitioning is unsafe if an invariant depends on several symbols updating together. Keep NIC queues, processing cores, and allocations on the intended NUMA node, and measure the added communication cost when splitting stages across cores.

## 7. Dimension 4 — queues, draining, and transmission

### 7.1 Why a tiny stall can create a long tail

Let arrival rate be `λ` events/second and service capacity be `μ` events/second. If a consumer pauses for `s` seconds, a simple fluid model estimates extra backlog:

\[
B \approx \lambda s.
\]

After service resumes, the backlog shrinks only at the spare capacity:

\[
T_{\text{catch-up}} \approx \frac{B}{\mu - \lambda}, \qquad \mu > \lambda.
\]

These are approximations with steady arrivals, constant service capacity, and adequate buffers. They describe backlog clearance, not every individual event’s waiting time.

Worked example: `λ = 8 million/s`, `μ = 10 million/s`, and a `2 µs` pause. About 16 events accumulate. The spare capacity is 2 million/s, so clearing the extra backlog takes about 8 µs after service resumes. At `λ >= μ`, sustained backlog cannot clear without reducing arrivals or increasing effective capacity.

```text
Time -------------------------------------------------------------->
Arrivals:    | | | | | | | | | | | | | | | | | | | |
Consumer:    work work [    PAUSE    ] work work work work
Queue:       small     grows grows   shrinks only with spare capacity

A brief stall affects later events because they arrive before prior work finishes.
```

Little’s law, `average queued work = arrival rate × average queue wait`, applies to suitable stable observation boundaries. It is not a formula for p99.99 or a proof that a full queue will recover.

### 7.2 Buffer drain time is not simply queue capacity

If `N` queued packets each require average discard cost `d`, then `N × d` estimates the effort to discard that fixed set. New packets may continue arriving, so reaching a useful boundary takes longer. Actual work may involve descriptor handling, parsing, buffer recycling, and DMA-completion rules.

**Packet**, **message**, and **update** are not interchangeable units. One packet can contain multiple exchange messages. Measure both packet and message rates and follow the protocol’s sequencing unit.

DPDK and EF_VI are different APIs with different resource rules. A typical high-performance NIC PMD polls receive descriptors in user space, while DPDK also offers drivers with other data paths. DPDK’s [poll-mode guide](https://doc.dpdk.org/guides/prog_guide/ethdev/ethdev.html) describes run-to-completion and multi-stage processing. Kernel bypass changes the path; it does not remove finite queues or guarantee low tail latency.

Use bounded polling work:

```text
repeat:
    receive up to B available packets
    process every returned packet according to protocol rules
    perform bounded output work
    check deadlines, session health, and shutdown signals
```

“Up to B” means process what is available; it does not mean wait until B packets arrive. Tune B by measuring both service capacity and the delay imposed on other work. A huge burst can reduce overhead per packet while delaying timers and output service.

### 7.3 You cannot discard arbitrary incremental updates

Consider these simplified messages:

```text
seq 100: add order X, quantity 10
seq 101: execute 4 of order X
seq 102: cancel the remaining quantity of X
```

Reading only seq 102 cannot reconstruct what happened if the preceding state is unknown. Even “old” updates may be prerequisites for a correct current state.

A feed of independent complete snapshots may permit replacing an older snapshot with a newer validated one. An incremental feed usually requires complete ordered processing or an authoritative snapshot plus correctly aligned replay. Instrument timestamps alone cannot authorize coalescing; use the protocol specification and the application’s consumers.

Track oldest-event age, queue depth, gaps, buffer exhaustion, and time with trading disabled. A larger queue can reduce drops while allowing dangerously old decisions. Capacity needs a freshness policy as well as a memory budget.

### 7.4 TCP_NODELAY removes one waiting rule

TCP is a reliable ordered byte stream. Nagle’s algorithm can hold small writes while previously sent data remains unacknowledged, subject to its rules and available data. This can interact with delayed acknowledgments. Setting `TCP_NODELAY` removes that algorithm’s buffering behavior. It does not bypass congestion control, receiver flow control, socket queues, NIC queues, or retransmission recovery. Linux also has `TCP_CORK`, whose interaction must be understood. See [tcp(7)](https://man7.org/linux/man-pages/man7/tcp.7.html).

```text
Rust caller -> optional BufWriter -> socket send queue -> TCP -> NIC TX -> wire
                   flush()          NODELAY affects one TCP rule
```

For a baseline TCP connection:

```rust
use std::{io, net::TcpStream};

fn configure(stream: &TcpStream) -> io::Result<()> {
    stream.set_nodelay(true)?;
    stream.set_nonblocking(true)?;
    Ok(())
}
```

Connection setup, address resolution, and session login belong outside the latency-critical event loop. This example only configures an already connected stream. Rust exposes the options through [`TcpStream`](https://doc.rust-lang.org/std/net/struct.TcpStream.html).

`BufWriter::flush` pushes its buffered bytes to the underlying writer. It does not wait for exchange acceptance. Calling `flush` on a TCP writer is not an application-level acknowledgment or a general wire-completion primitive.

### 7.5 Partial writes are part of correctness

A nonblocking `write` may accept only a prefix of a frame. It may also return `WouldBlock`. If you retry the entire frame after a partial write, you duplicate bytes. If you discard the unsent suffix, you truncate the stream. `write_all` does not give the caller a progress offset when it returns an error, so naïve retries on a nonblocking stream are especially problematic. See Rust’s [`Write` contract](https://doc.rust-lang.org/std/io/trait.Write.html).

The lab’s `Pending<N>` owns one fixed-capacity frame and an offset:

```text
Frame:   [ a b c d e f ]
Accepted:[ a b ]
Pending:      [ c d e f ]
Next write starts at c, with the same frame still owned by Pending.
```

Usage inside a component that owns the connection:

```rust
use std::io;
use std::net::TcpStream;
use pipeline_flush_lab::Pending;

fn output_turn(pending: &mut Pending<4096>, stream: &mut TcpStream)
    -> io::Result<bool>
{
    pending.step(stream, 4) // no more than four write attempts this turn
}
```

The caller enqueues a fully encoded frame once. If `step` returns `Ok(false)`, keep the same pending state and return to the event loop. `Interrupted` consumes an attempt too, so repeated interruptions cannot create an unbounded loop inside this function. A call-count budget is not a nanosecond deadline, and the underlying stream must actually be nonblocking.

Fatal socket errors require session recovery. Some bytes may already have reached the peer. Do not blindly replay an order on a fresh connection: reconcile exchange session sequence, client identifiers, and order status using the relevant protocol.

### 7.6 Micro-batching is a policy with a deadline

Sending several already-ready messages together can amortize work. Waiting to build a batch adds intentional delay. A policy might submit when either `B` messages are ready or the oldest one reaches age `T`. It must also specify what happens if the output cannot progress.

Under ideal timer-driven behavior, a batch interval T adds a wait ranging from roughly zero to T; actual scheduling and downstream backpressure can exceed T. Measure the oldest-message age and ensure the event loop checks the deadline frequently enough. Busy polling reserves CPU time and increases power use; it does not by itself enforce the deadline.

### 7.7 Run the virtual-time queue experiment

```sh
cargo run --release --offline -- queue
```

The model schedules arrivals independently of completion. It processes 20,000 events with 30 ns service time and one 200 ns pause. One scenario has 100 ns between arrivals and recovers; the other has 20 ns between arrivals and remains overloaded. All reported values are **simulated nanoseconds**, not a machine benchmark.

The stable scenario has only a few delayed events. Its p99 may remain unchanged while its maximum grows. That is why averages and a single percentile can conceal rare disruptions. The model ignores packetization, contention, changing service time, finite queue capacity, and drops; use it to understand the mechanism before measuring real traffic.

## 8. Dimension 5 — deterministic state recovery

### 8.1 Define the invariant before the fast path

For this chapter’s simplified feed:

> The book may be used only when it represents a trusted snapshot plus every required subsequent update, in order, in the same session, through a known cutover sequence.

A real exchange protocol may sequence a whole channel, several messages inside a packet, or separate sessions. Apply the invariant at that protocol’s scope. Redundant feeds, duplicates, out-of-order packets, and resets require documented normalization before application.

```text
                       expected next update
                 +-------------------------------+
                 v                               |
             +--------+                          |
             |  LIVE  | -------------------------+
             +--------+
                 |
          gap / invalid event / session change
                 v
          +--------------+        timeout / overflow / invalid replay
          |  RECOVERING  | <----------------------------------------+
          | trading off  |                                          |
          +--------------+                                          |
                 |                                                  |
          trusted snapshot at S                                     |
                 v                                                  |
          +--------------+                                          |
          | BUILD SHADOW | -> replay S+1 ... H -> validate ------------+
          +--------------+                         |
                                            complete and valid
                                                   |
                                              publish candidate
                                                   v
                                                  LIVE
```

The drawing is an original teaching state machine. It is not an exchange specification.

A real example of snapshot plus buffered-message alignment appears in Nasdaq’s [GLIMPSE recovery FAQ](https://nasdaqtrader.com/content/technicalSupport/FAQs/Nasdaq_Glimpse_FAQ.pdf). Exact sequence interpretation and operational rules must come from the selected feed’s current specification, not from the miniature model.

### 8.2 Why panic catching is not a rollback protocol

If an exception or Rust panic occurs after half a book update, catching it does not undo mutations. Rust unwinding runs destructors; it is not a database transaction. The engine must either construct a valid replacement before publication, use an explicitly designed undo protocol, or mark the old state unusable and recover it. Recovery itself can fail and must have a defined next action.

For a small model, copying the book to a candidate is straightforward. For a real book, a second preallocated book or bounded journal can be appropriate. Avoid allocating an unbounded recovery structure in response to a burst.

### 8.3 Read the lab’s engine as a contract

`Engine` holds an eight-level quantity array, a session identifier, the next expected sequence, and a `Mode`. Its normal ingestion behavior is:

| Input while Live | Action |
|---|---|
| Same session, valid level, exactly expected sequence | Apply one quantity assignment and advance sequence |
| Same session, valid level, older sequence | Classify as duplicate in this simplified model |
| Future sequence, wrong session, invalid level, or sequence overflow | Enter Recovering without applying that event |
| Any update while Recovering | Keep trading disabled; do not mutate the book |

Duplicate handling assumes normalization has already established a well-formed feed. The model does not retain historical payloads to prove duplicate content equality. It treats updates as level-quantity assignments, not a full order-by-order exchange protocol.

`recover` accepts a trusted snapshot through S, replay data, and a required cutover H. It disables trading first, builds a candidate, ignores replay entries already included in S, and requires exact consecutive replay from S+1 through H. Replay duplicates above S and out-of-order entries are rejected in this simplified interface. Capacity is capped at 64 replay entries per call. Invalid replay leaves the old quantities intact and the engine disabled.

The final `*self = candidate` is a publication under exclusive `&mut self` ownership. It is not a hardware-atomic publication visible to unrelated readers. A concurrent application needs a separate synchronization and reclamation design. The model assumes no new events arrive during recovery; a real system must buffer concurrent arrivals, define cutover, then continue without losing them.

### The fast path in Rust

The following methods are inside `impl Engine` in the lab. Validation precedes mutation; once recovery begins, the normal path cannot re-enable trading.

```rust
    pub fn ingest(&mut self, u: Update) -> Outcome {
        if self.mode != Mode::Live {
            return Outcome::RecoveryRequired;
        }
        if u.session != self.session || u.level >= LEVELS {
            return self.stop();
        }
        if u.seq < self.next {
            return Outcome::Duplicate;
        }
        if u.seq != self.next {
            return self.stop();
        }
        let Some(next) = self.next.checked_add(1) else {
            return self.stop();
        };
        self.quantities[u.level] = u.qty;
        self.next = next;
        Outcome::Applied
    }
    #[cold]
    fn stop(&mut self) -> Outcome {
        self.mode = Mode::Recovering;
        Outcome::RecoveryRequired
    }
```

Here `self.next` is the next required sequence. `checked_add` is evaluated before assigning a quantity, so sequence overflow cannot leave a half-applied update. The `stop` method changes only eligibility; rebuilding the book is a separate operation.

### 8.4 Run a gap and recovery

```sh
cargo run --release --offline -- recovery
```

The scenario starts with a snapshot through sequence 9:

```text
receive 10 -> apply; next becomes 11
receive 12 -> gap; disable trading
receive 11 -> still recovering; do not resume from an uncertain state
snapshot through 11 + replay 12 -> validate, replace candidate; next becomes 13
```

In the tiny example, recovery is provided directly. A production recovery worker must acquire and authenticate a suitable snapshot, preserve session identity, retain relevant updates, detect overflow, and fail closed when it cannot bridge the gap.

### 8.5 Measure coherent state and freshness separately

Define timestamps:

- `t_gap`: the engine detects unusable input.
- `t_request`: it initiates recovery.
- `t_snapshot`: a complete snapshot is available.
- `t_replay_done`: required buffered updates have been applied.
- `t_eligible`: validation and operational gates permit decisions again.

Then:

\[
T_{\text{unavailable}} = t_{\text{eligible}} - t_{\text{gap}}.
\]

Report snapshot acquisition, rebuild, replay, and validation intervals separately when their boundaries do not overlap. If work runs in parallel, use a timeline rather than adding overlapping intervals.

A state can be internally coherent and still too old to trade. Add a freshness gate appropriate to the protocol and clock quality, plus session health, risk readiness, and recovery-buffer health. Market-book recovery also does not resolve outstanding-order uncertainty; the order-entry side needs its own reconciliation before returning to normal operation.

Bound local algorithm work by configured capacities. Do not promise a fixed full recovery time when snapshot delivery, retransmission, or exchange availability is external. Set a measured objective and an explicit timeout action.

## 9. Hardware versus software: a corrected comparison

| Dimension | CPU / microarchitecture | Software / network pipeline |
|---|---|---|
| Work item | Instruction or micro-operation | Packet, protocol message, book event, order frame |
| Recovery or waiting cause | Wrong prediction, dependencies, serialization, coherence contention | Backlog, partial output, transport rules, sequence loss, session recovery |
| Useful measures | Core cycles, retired instructions, IPC, branch misses, model-specific recovery/coherence events | Event age, queue depth, RX-to-TX latency, send-pending age, disabled duration |
| Scope of discard | Younger speculative instructions in a recovery event | Only data or state that the application protocol permits discarding |
| Techniques to investigate | Compact common path, PGO, locality, correct synchronization | Capacity headroom, bounded polling, measured batching, suitable network API, validated replay |
| Shared concern | Dependencies and contention can delay progress | Delays can propagate into later events |

Fences and cache ownership transfers belong in the broader stall investigation even when no instruction stream is squashed. A context switch belongs in scheduling analysis; it can affect cache locality and timing without matching either kind of application flush.

## 10. Measure tails without fooling yourself

### 10.1 Use two complementary experiments

**Microbenchmark:** isolate a kernel such as dispatch, publication, or a queue handoff. Record generated instructions, input pattern, cycles or elapsed time, and limitations. A useful result might be “this layout reduces elapsed batch time for this workload.”

**System replay:** schedule market events independently of completion, preserve bursts and protocol order, and measure through the chosen RX/TX or decision boundary. Inject gaps, backpressure, and pauses. A useful result might be “the p99.99 decision age meets the target at this load, with zero invalid decisions.”

Optimizing a microbenchmark is evidence about that kernel. The system replay determines whether the change helps the system.

### 10.2 Avoid coordinated omission

A closed-loop benchmark sends a request, waits for the response, then sends the next request. During a stall, it stops generating traffic. A real exchange may continue sending updates; the test omits those arrivals and their queue wait.

Use a generator with an independent arrival schedule. Preserve intended arrival timestamps when it falls behind. A generator’s own overload must be recorded; otherwise the test may silently reduce offered load. If testing raw service time separately, label it as such.

The lab’s queue model is open-loop in virtual time, which makes this issue visible without claiming realistic NIC timing.

### 10.3 Understand p99.99

The p99.99 latency is a value at which approximately 99.99% of the chosen sample distribution falls at or below it. State the estimator; the queue lab uses nearest rank. For N samples its rank is `ceil(0.9999 × N)` in sorted order.

At one million samples, about 100 lie above the nominal p99.99 threshold. At ten thousand samples, only about one does. A high percentile with very few tail observations is unstable. Events during a market burst are also correlated, so counting them does not create that many independent trials.

Report sample count, duration, repeated runs, time-window distributions, maximum observed value, and dropped or excluded events. An observed maximum is not a guaranteed upper bound. Keep recovery events visible even if normal-path statistics are reported separately.

### 10.4 Instrument the right clocks and boundaries

Use `Instant` for initial portable elapsed-time experiments. Its overhead can dominate an individual nanosecond-scale event, which is why the branch lab times batches. Preallocate timing storage and record outside the hot loop where possible.

For NIC-to-NIC latency, investigate hardware timestamps and clock alignment. Cross-device timestamps require a validated mapping; application timestamps cannot by themselves identify when a frame crossed the wire. Linux documents socket timestamping, hardware timestamp sources, and TX completion distinctions in its [timestamping guide](https://docs.kernel.org/networking/timestamping.html).

If moving to TSC timing, establish processor support, ordering requirements, measurement overhead, migration handling, and the conversion rate. Inline assembly needs compiler as well as processor-ordering reasoning. Do not place CPUID around every event and then attribute its cost to the event.

### 10.5 Record the environment with the result

| Area | Record |
|---|---|
| Processor | Exact model, stepping, microcode, SMT status, core placement |
| Clock behavior | Frequency policy, observed frequency, thermal state |
| OS | Kernel, scheduler setup, interrupt placement, security mitigation configuration |
| Memory | NUMA allocation and placement, page policy, working-set size |
| Network | NIC, firmware, driver/API, queue counts and sizes, link speed, offloads |
| Build | `rustc -Vv`, Cargo profile, target flags, binary identity, PGO training identity |
| Workload | Message mix, sizes, sessions, burst pattern, sustained rate, exceptional traffic |
| Instrumentation | Timestamp boundaries, histogram precision, sample and drop counts |

CPU pinning reduces migration; it does not remove interrupts, firmware activity, or shared-resource contention. Do not alter security mitigations as a casual benchmarking shortcut. Test and document the deployment configuration that will actually be used.

### 10.6 Match symptoms to evidence

| Symptom | Candidate explanation | Next evidence |
|---|---|---|
| Branch misses rise with a regime change | Different control-flow distribution | Assembly, branch-event samples, representative PGO comparison |
| IPC drops but branch misses do not rise | Dependencies, memory stalls, instruction supply, or scheduling | Model-specific profiling, cache and scheduling traces |
| Handoff latency rises when telemetry is enabled | Sharing or scheduling interference | Cache-line ownership analysis; compare local counters and publication cadence |
| Send-pending age rises | Output backpressure or an overloaded writer | Partial-write counts, socket/queue observations, NIC timestamps |
| p50 stays low while decision age spikes | Burst queueing or rare pauses | Open-loop replay, queue high-water marks, time-window tails |
| Recovery seems fast but decisions are wrong | Missing replay, bad cutover, unsafe publication | Reference-model replay and injected protocol failures |

These are hypotheses to investigate. A generic `cache-misses` event cannot establish false sharing, and an IPC drop cannot uniquely identify a flush. Event availability and meaning vary by processor; use the [Intel event catalog](https://perfmon-events.intel.com/) or the corresponding vendor documentation.

## 11. Move from the lab to production in deliberate stages

### Stage A — build a correct reference

Decode a documented synthetic protocol into integer fields. Reject truncated frames and invalid ranges before mutating state. Create a simple reference book whose output can be compared with a faster implementation after every event. Include duplicate, reset, gap, and overflow cases.

Acceptance: every accepted sequence produces the expected book; a missing prerequisite disables affected decisions.

### Stage B — bound memory and work

Define capacities for receive bursts, decoded events, recovery replay, order frames, and telemetry. Preallocate before enabling traffic. Handle exhaustion explicitly: reject new work where legal, disable affected decisions, or initiate recovery. A queue overwrite must never silently erase a required incremental update or accepted order intent.

Acceptance: every full-buffer condition has a tested outcome; common-path allocation and synchronous logging have been removed or measured within the budget.

### Stage C — give mutable state one owner

Prefer an exclusive book owner with well-defined input and output channels. Separate telemetry and recovery I/O when that helps, while accounting for cross-core communication. If multiple cores read published state, prove coherence and object lifetime; a pointer swap alone does not solve reclamation.

Acceptance: every shared field has an ownership or synchronization explanation, and overload cannot cause unbounded memory growth.

### Stage D — establish a baseline under realistic load

Measure normal operation, sustained near-capacity load, realistic bursts, long idle-to-busy transitions, gaps, and blocked output. Collect queue ages alongside elapsed times. Observe both normal-path latency and time spent unable to make decisions.

Acceptance: results are reproducible enough to compare changes and include sufficient tail observations for the chosen percentile.

### Stage E — optimize one hypothesis at a time

Try a smaller common path, a different dispatch layout, padded ownership boundaries, PGO, burst size, or reduced cross-core handoff. Compare unchanged input and correctness checks. A change that improves p50 but worsens p99.99 or recovery correctness may be unacceptable.

For PGO, use absolute profile paths, an `llvm-profdata` compatible with the compiler’s LLVM, and the same relevant build flags across generation and use. The basic sequence is instrument → train → merge → rebuild using profile → validate on held-out traffic. Follow the current [rustc PGO procedure](https://doc.rust-lang.org/rustc/profile-guided-optimization.html).

Acceptance: a measured improvement survives representative workload variation with the same invariants.

### Stage F — validate the actual deployment and failure paths

Integrate the actual exchange protocol and approved risk rules, connection recovery, session sequence tracking, duplicate-order prevention, kill controls, and operator observability. Kernel bypass requires explicit packet-buffer ownership and NIC integration; it is not a Cargo flag that turns this lab into a gateway.

Acceptance: protocol conformance, failure injection, independent review, deployment testing, and rollback procedures are complete. Production readiness is an evidence standard for the integrated system, not a property conferred by Rust or a fast benchmark.

## 12. Exercises with answers

### Exercise 1 — units

An assumed 18-cycle loss occurs at a constant 4.5 GHz. What elapsed time does the simple conversion give?

**Answer:** `18 / 4.5 = 4 ns`. This does not establish that a particular branch actually incurs 18 cycles, or that TSC ticks equal core cycles.

### Exercise 2 — double-counting

A benchmark reports 20 cycles between branch redirection and resumed useful work. A diagram suggests a 10-stage refill. Can you report 30 lost cycles?

**Answer:** No. The measured interval may already include refill, and stage count is not an independently established latency. Define non-overlapping boundaries and use the target CPU’s behavior.

### Exercise 3 — backlog

A consumer can process 12 million events/s. Events arrive at 9 million/s. A 1 µs interruption occurs. Estimate extra backlog and subsequent catch-up time.

**Answer:** About nine events accumulate. Spare capacity is 3 million/s, so the simplified model predicts about 3 µs to clear the extra backlog after processing resumes. Actual bursts and finite buffers can change the result.

### Exercise 4 — invalid “optimization”

Your book expects 5001. The next event is 5003 and is younger than your maximum-age threshold. May you apply it and trade?

**Answer:** Its freshness does not establish completeness. Disable affected decisions and bridge the sequence gap through the protocol’s recovery procedure.

### Exercise 5 — a partial write

A 100-byte frame writes 40 bytes, then returns `WouldBlock`. What should the next successful attempt contain?

**Answer:** The remaining 60 bytes, starting at offset 40. Preserve frame ownership, respect a work budget, and prevent a later frame from being interleaved incorrectly. A fatal disconnect creates a separate order-reconciliation problem.

### Exercise 6 — interpret a fast benchmark

The branch lab’s random pattern is as fast as its predictable pattern. Does that prove there are no branch mispredictions?

**Answer:** No. Inspect generated instructions, check relevant counters, and consider memory limits, predictor behavior, compiler lowering, and measurement noise. Batch throughput can hide individual delays.

### Exercise 7 — safe reuse

Can you turn the one-shot publication example into a reusable queue by repeatedly setting `ready` to false?

**Answer:** Not without a complete producer/consumer protocol. You need rules for payload ownership, observation of new generations, overwrite prevention, and progress. A Boolean alone does not distinguish every generation.

### Exercise 8 — recovery failure

A snapshot covers through 900. The required cutover is 904. Buffered updates are 901, 902, and 904. Can you publish?

**Answer:** No. Sequence 903 is missing. Keep trading disabled and obtain enough authoritative state or replay to reach a valid cutover.

## 13. Small glossary

| Term | Meaning |
|---|---|
| Architectural state | Program-visible results defined by the instruction architecture |
| Microarchitectural state | Internal implementation state, such as caches and predictors |
| Speculation | Work performed before all conditions for its use are known |
| Squash | Prevent speculative work from committing architectural results |
| Retirement | In-order commitment of completed instruction results |
| IPC | Retired instructions divided by a specified cycle count |
| RFO | A request associated with acquiring ownership needed for writing |
| False sharing | Independent variables contending because they share a coherence unit |
| NUMA | Memory-access costs depend on CPU and memory placement |
| Backpressure | A downstream component cannot accept output at the offered rate |
| Cutover | An explicitly defined state boundary used to switch from recovery to normal processing |
| Tail latency | The slow portion of a measured latency distribution |

## 14. Sources and further reading

These are primary documentation sources. Their architectural or API semantics are stronger evidence than any timing number copied from another machine. The diagrams, sample protocol, queue arithmetic, Rust lab, and engineering progression are original teaching material derived from the stated assumptions.

1. [Intel Software Developer’s Manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html): instruction semantics, serialization, memory ordering, and timestamp-counter requirements.
2. [Intel hardware behavior related to speculative execution](https://www.intel.com/content/www/us/en/developer/articles/technical/software-security-guidance/technical-documentation/hardware-behavior-related-to-speculative-execution.html): speculative execution and effects that are not simply erased by architectural rollback.
3. [Intel optimization reference, Volume 1](https://cdrdv2-public.intel.com/821612/248966-Optimization-Reference-Manual-V1-050.pdf): model-dependent optimization and performance analysis; consult the revision appropriate to the CPU.
4. [Intel PerfMon event catalog](https://perfmon-events.intel.com/): exact event definitions and model availability.
5. [Intel offcore event example](https://perfmon-events.intel.com/platforms/icelakex/offcore-events/offcore/): ownership requests and coherence responses; an example for that platform, not a portable event list.
6. [Intel false-sharing guidance](https://www.intel.com/content/dam/develop/external/us/en/documents/gdma-2-165938.pdf): cache-line sharing and padding.
7. [DPDK poll-mode driver guide](https://doc.dpdk.org/guides/prog_guide/ethdev/ethdev.html): queues, bursts, ownership assumptions, and execution models. The unversioned guide can track development releases; match deployed DPDK documentation.
8. [Linux tcp(7)](https://man7.org/linux/man-pages/man7/tcp.7.html): TCP_NODELAY, TCP_CORK, and transport options.
9. [Linux timestamping documentation](https://docs.kernel.org/networking/timestamping.html): software/hardware timestamp boundaries and reporting.
10. [Rust atomic orderings](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html): Acquire, Release, Relaxed, and SeqCst semantics.
11. [Rust TcpStream](https://doc.rust-lang.org/std/net/struct.TcpStream.html) and [Write](https://doc.rust-lang.org/std/io/trait.Write.html): configuration, nonblocking operations, partial writes, and flushing.
12. [Rust black_box](https://doc.rust-lang.org/std/hint/fn.black_box.html): optimization-barrier limits.
13. [Rust cold attribute](https://doc.rust-lang.org/stable/core/attribute.cold.html): compiler hint semantics.
14. [rustc profile-guided optimization](https://doc.rust-lang.org/rustc/profile-guided-optimization.html): profile generation and use.
15. [perf-stat manual](https://man7.org/linux/man-pages/man1/perf-stat.1.html): counter collection and result interpretation.
16. [Nasdaq GLIMPSE recovery FAQ](https://nasdaqtrader.com/content/technicalSupport/FAQs/Nasdaq_Glimpse_FAQ.pdf): an actual snapshot/replay alignment example; consult the relevant current feed specifications for implementation.
