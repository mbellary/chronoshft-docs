# Memory Alignment and Padding in Low-Latency Rust

!!! note "Purpose and companion availability"
    This HFT engineering chapter complements the [general memory alignment and padding chapter](../memory-alignment-padding/memory-alignment-padding.md). Both are part of the learning path: read the general foundations first, then use this chapter to design and measure layouts for trading systems.

    The HFT-specific `alignment_lab.rs` referenced below was not included with the research documents. The existing general alignment lab is a separate companion. Verification statements in this chapter describe the original author's checks, not a new verification run.

*A beginner’s chapter through the lens of high-frequency trading and systems engineering*

**Learning goal:** move from counting bytes to designing, checking, and measuring a memory layout for a latency-sensitive application.

**Scope:** stable Rust, with Linux on x86-64 as the main deployment example. Numerical layouts assume the usual x86-64 ABI. A 64-byte cache line is an explicit target assumption. Verify assumptions when changing processors, operating systems, or compilation targets.

**Companion:** `alignment_lab.rs` is a complete, dependency-free program. The chapter develops its ideas in small excerpts; the companion contains the executable implementation and tests. The examples are teaching components, not a complete trading engine or a production queue.

## 1. Begin with the trading path

A market-data packet arrives. Your application reads a price and quantity, updates an order book, checks a trading condition, applies risk limits, and constructs an order. The work between the chosen input and output timestamps is the path you want to understand.

```text
                     One possible tick-to-trade path

NIC receive buffer
        │
        ▼
Validate and decode ──► Update book ──► Strategy ──► Risk ──► Encode order
                                                                           │
                                                                           ▼
                                                                      NIC transmit

Memory question at every step:
Which bytes are accessed? By which core? How are they laid out?
```

“Hot path” means the code executed repeatedly for time-sensitive events. A debug log written once during startup is cold. A quantity read for every price update is hot.

Latency is the time for one event. Throughput is the amount of work completed per second. A program can improve throughput by batching updates while making the first update in each batch wait longer. Always decide which outcome you are optimizing.

Jitter is variation in latency. Report a distribution: the median, p99, p99.9, and the conditions under which you measured them. A p99.9 of 10 microseconds means approximately 99.9% of the measured observations were no slower than that value; it does not impose a bound on future observations.

Memory layout can remove sources of unnecessary work. It cannot guarantee deterministic execution on a general-purpose machine. Scheduling, interrupts, cache state, queueing, contention, and device behavior still matter. Treat every claimed nanosecond saving as a hypothesis until measured on the deployment system.

## 2. Repair the six axioms before using them

Your six dimensions form a useful learning path. Several of the proposed rules need qualification.

| Dimension | Working engineering rule | Qualification |
|---|---|---|
| Primitive alignment | Respect `align_of::<T>()` for ordinary typed access. | Alignment is a type/target requirement; it is not always the type’s size. |
| Cache isolation | Separate independently hot, conflicting accesses when coherence traffic is measured. | A writer can interfere with a reader too. Padding has a footprint cost. |
| Structural density | Remove unnecessary gaps and keep related hot data together. | Descending alignment is a useful starting heuristic, not the entire optimization. |
| SIMD | Satisfy the selected instruction’s contract and handle the tail safely. | Unaligned vector loads exist; aligned allocation alone does not make a loop safe. |
| IPC and wire layout | Define offsets, byte order, validity, ownership, and versions explicitly. | Packing does not establish all these properties or make an arbitrary cast sound. |
| Pages and TLBs | Reduce translation pressure and establish memory placement before the hot path. | Crossing a page boundary does not imply a TLB miss. Huge pages need OS backing. |

We will reuse a tiny market-depth record throughout the chapter:

```text
price_ticks : signed integer price in an agreed tick unit
quantity    : number of units at this level
venue       : numeric venue identifier
side        : 0 = bid, 1 = ask, for this teaching protocol
```

An order-book level is an aggregate at one price; it is not necessarily one individual order. The simplified record leaves out sequence numbers, timestamps, instruments, and exchange-specific fields so that the byte calculations remain visible.

## 3. The vocabulary: address, size, alignment, offset, stride

Think of memory as numbered byte-sized boxes. An address identifies a box. Decimal addresses are used below to make the arithmetic easy.

```text
Address:  0   1   2   3   4   5   6   7   8   9  10  11  12  13  14  15
         [ ][ ][ ][ ][ ][ ][ ][ ][ ][ ][ ][ ][ ][ ][ ][ ]

An 8-byte object starting at address 8 occupies addresses 8 through 15.
```

Size tells you how many bytes a value occupies, including any padding inside its representation. Alignment restricts its permitted starting address. Offset is a field’s distance from the beginning of its containing record. Stride is the distance from one array element’s start to the next.

Rust exposes the relevant measurements directly:

```rust
use std::mem::{align_of, offset_of, size_of};

println!("u64 size = {}", size_of::<u64>());
println!("u64 alignment = {}", align_of::<u64>());
```

`align_of` reports the target ABI’s required minimum alignment. `size_of` is also the stride of an element in a Rust array. `offset_of!` measures a field’s byte offset; it does not itself make the layout stable across builds. [Rust: alignment](https://doc.rust-lang.org/std/mem/fn.align_of.html), [size](https://doc.rust-lang.org/std/mem/fn.size_of.html), [field offsets](https://doc.rust-lang.org/std/mem/macro.offset_of.html).

Beginner Rust translation: `u64` is an unsigned 64-bit integer; `i64` is signed; `u8` is one byte. `::<T>` supplies a type to a generic function. `&T` borrows a value, while `&[T]` borrows a sequence of values. None of these notations implies a heap allocation.

## 4. Dimension one: natural boundaries

### 4.1 The actual rule

For ordinary access to a value of type `T`:

```text
address % align_of::<T>() == 0
```

The `%` operator gives the remainder after division. If the alignment is eight, valid starting addresses include 0, 8, 16, and 24. The relevant property is alignment, not a universal “multiple of its size” rule. Rust’s primitive alignments are target-specific. [Rust Reference: type layout](https://doc.rust-lang.org/reference/type-layout.html).

On the chapter’s target, `u8`, `u16`, `u32`, and `u64` normally have alignments 1, 2, 4, and 8. Safe Rust arranges suitable storage for ordinary values. You usually need to think explicitly about alignment when designing layouts, using foreign APIs, implementing allocators, or interpreting byte buffers.

### 4.2 Language correctness and hardware cost are separate

A CPU may support an instruction that reads bytes from an unaligned address. That does not make an unaligned Rust reference valid. Undefined behavior means the program has violated the language contract; it is not merely an instruction that might be slower. `unsafe` puts the proof obligation on the programmer. [Rust Reference: undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html).

For x86 performance, distinguish these situations:

```text
64-byte line:  0 .................................................. 63

8-byte read at  8: [8.......15]       naturally aligned, one line
8-byte read at  9:  [9.......16]      unaligned, still one line
8-byte read at 60:                                     [60...63]|[64...67]
                                                               line split
```

The second case need not incur the penalty of the third. Unaligned scalar accesses within a cache line can be inexpensive on modern x86 processors. Splits across cache lines or pages are distinct cases to investigate. There is no universal rule that every unaligned read costs two memory cycles or a fixed number of nanoseconds. Consult the relevant microarchitecture guidance. [Intel optimization manual](https://cdrdv2-public.intel.com/821612/248966-Optimization-Reference-Manual-V1-050.pdf).

### 4.3 Reading protocol bytes safely

If a protocol places a quantity in bytes 9 through 12, a safe decoder can express exactly that:

```rust
let bytes = [0x07, 0x00, 0x00, 0x00];
let quantity = u32::from_le_bytes(bytes);
assert_eq!(quantity, 7);
```

“Little-endian” means the least significant byte comes first. The byte array has no requirement to begin on a four-byte boundary. The resulting integer is an ordinary Rust value. This expresses the intended conversion clearly and lets the compiler choose the machine instructions.

**Production habit:** do not introduce unsafe pointer casts just to avoid spelling out a four-byte conversion. Inspect generated code and measure before assuming that safe decoding performs unnecessary work.

**Checkpoint:** an address is divisible by four but not eight. Is it valid for `&u64`? On this target, no. Could a specific unaligned-load operation read eight valid bytes there? Possibly, subject to its separate safety conditions.

## 5. Dimension two: cache-line isolation and ownership

### 5.1 Why independent counters interfere

A cache holds recently accessed memory close to a core. For this discussion, use 64-byte cache lines. Coherence maintains a consistent view across cores at line granularity. Independently used fields on one line can therefore interfere when at least one is frequently written. This includes writer/reader sharing. [Linux kernel: false sharing](https://kernel.org/doc/html/latest/kernel-hacking/false-sharing.html).

```text
Before: two cores repeatedly update different counters on one line

           64-byte cache line
          ┌───────────┬───────────┬──────────────────────────────┐
          │ counter A │ counter B │ other bytes                  │
          └───────────┴───────────┴──────────────────────────────┘
               ▲           ▲
            Core 0       Core 1
          ownership of this line moves between cores

After: counters have separate line-sized storage

          ┌───────────┬─────────────────────────────────────────┐
line N    │ counter A │ unused space                            │  Core 0
          └───────────┴─────────────────────────────────────────┘
          ┌───────────┬─────────────────────────────────────────┐
line N+1  │ counter B │ unused space                            │  Core 1
          └───────────┴─────────────────────────────────────────┘
```

The application sees two counters. The coherence mechanism sees ownership requests for lines. That mismatch is false sharing.

True sharing occurs when both cores need the same logical data, such as the same queue cursor. Padding cannot remove the communication required by that algorithm. It can isolate unrelated communication.

### 5.2 A Rust wrapper with a clear target assumption

```rust
use std::sync::atomic::AtomicU64;

#[repr(C, align(64))]
struct CacheLine<T>(T);

#[repr(C)]
struct QueueCursors {
    producer: CacheLine<AtomicU64>,
    consumer: CacheLine<AtomicU64>,
}
```

`#[...]` is an attribute that changes compiler behavior. Here, the wrapper requests at least 64-byte alignment. For `AtomicU64` on the target, its size is rounded to 64, so the two wrapped counters occupy distinct lines. The companion checks size and offsets. Larger `T` values can occupy multiple lines. [Rustonomicon: representation modifiers](https://doc.rust-lang.org/nomicon/other-reprs.html).

Aligning only the outer `QueueCursors` would not isolate fields. It could merely place two nearby counters at the beginning of one well-aligned line. Also distinguish an aligned `Vec` header from aligned element storage: wrapping the header does not change the allocation that holds its elements.

Use this wrapper for a specific ownership boundary, not every number in a book. Expanding one million eight-byte counters to 64 bytes each changes their storage from roughly 8 MB to 64 MB. A fix for coherence traffic can create a cache-capacity problem elsewhere.

### 5.3 Alignment does not synchronize threads

```rust
use std::sync::atomic::{AtomicU64, Ordering};

let messages = AtomicU64::new(0);
messages.fetch_add(1, Ordering::Relaxed);
```

This is appropriate for a standalone statistic when no other data is published through that counter. Relaxed ordering provides atomicity for the counter but does not publish an adjacent message body. Release/acquire operations can establish publication ordering when an acquire observes the relevant release, but the surrounding ownership protocol must also be correct. [Rust: atomic ordering](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html).

For a single-producer/single-consumer queue, reason about the protocol before its padding:

```text
Producer                                     Consumer
--------                                     --------
obtain exclusive ownership of free slot
write payload
publish position with Release ───────────────► observe with Acquire
                                             read payload
                                             finish all slot accesses
observe reclamation with Acquire ◄─────────── publish reclamation with Release
reuse slot
```

This is an ownership sketch, not a queue implementation. A working queue must handle full and empty states, wraparound, slot initialization, object destruction, and prevention of concurrent conflicting payload access. Atomic counters alone do not prove those properties.

**Production habit:** write “which core owns this field, and when does ownership change?” before adding padding.

## 6. Dimension three: structural density and locality

### 6.1 Walk through an inefficient record

```rust
#[repr(C)]
struct LooseLevel {
    side: u8,
    price_ticks: i64,
    quantity: u32,
    venue: u16,
}
```

`repr(C)` gives this example a defined field order and C-style target layout; default Rust layout can reorder fields. It is not a cross-platform wire-format guarantee. [Rust Reference: representations](https://doc.rust-lang.org/reference/type-layout.html).

The target layout is:

| Field or gap | Offset | Bytes | Explanation |
|---|---:|---:|---|
| `side` | 0 | 1 | A byte can start immediately. |
| Internal padding | 1 | 7 | The next eight-byte field starts at offset 8. |
| `price_ticks` | 8 | 8 | Occupies 8–15. |
| `quantity` | 16 | 4 | Occupies 16–19. |
| `venue` | 20 | 2 | Occupies 20–21. |
| Tail padding | 22 | 2 | The next array element needs suitable alignment. |
| **Total** | | **24** | **15 bytes of fields, 9 bytes of padding.** |

```text
Byte offsets, inclusive:

0       1......7  8.............15  16.....19  20..21  22..23
┌──────┬─────────┬────────────────┬──────────┬───────┬───────┐
│ side │ padding │ price_ticks    │ quantity │ venue │ tail  │
└──────┴─────────┴────────────────┴──────────┴───────┴───────┘
```

For a simple C-style record, the calculation is:

```text
offset = 0
for each field in declaration order:
    advance offset to a multiple of the field's alignment
    place field there
    advance offset by the field's size
round the final size to a multiple of the record's alignment
```

The last rounding creates tail padding. In real allocation code, use checked layout APIs rather than unchecked rounding arithmetic. [Rust: size calculation](https://doc.rust-lang.org/std/mem/fn.size_of.html).

### 6.2 Make the record compact

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Level {
    price_ticks: i64,
    quantity: u32,
    venue: u16,
    side: u8,
    reserved: u8,
}
```

```text
0..............7  8......11  12..13  14     15
┌────────────────┬──────────┬───────┬──────┬──────────┐
│ price_ticks    │ quantity │ venue │ side │ reserved │
└────────────────┴──────────┴───────┴──────┴──────────┘
                    total: 16 bytes
```

The final byte is an explicit field, initialized to zero by the decoder. Without it, this particular layout would still be 16 bytes because of tail padding. A named field makes that byte part of the record’s defined data; it does not mean every possible Rust record is safe to export as bytes.

Validate the design instead of relying on the drawing:

```rust
use std::mem::{align_of, offset_of, size_of};

assert_eq!(size_of::<Level>(), 16);
assert_eq!(align_of::<Level>(), 8);
assert_eq!(offset_of!(Level, quantity), 8);
assert_eq!(offset_of!(Level, venue), 12);
assert_eq!(offset_of!(Level, side), 14);
```

For 1,024 levels, our measured layouts require:

```text
LooseLevel: 1,024 × 24 = 24,576 bytes
Level:      1,024 × 16 = 16,384 bytes
Saved:                    8,192 bytes, or one third
```

With a suitably line-aligned base, four 16-byte levels fit in one 64-byte line. A 24-byte stride produces a different pattern and some records straddle lines. The base address still matters: a dense record’s eight-byte type alignment does not promise a 64-byte array base.

### 6.3 Density is a means, not the objective

Descending alignment often removes gaps in simple scalar records. Actual access patterns can justify another arrangement. Ask which fields are read together and which are cold.

If the strategy only needs price and quantity, a venue name string, audit description, and diagnostic timestamp may belong elsewhere. A compact hot record can reference cold metadata by a stable identifier. That introduces another lookup when cold data is needed; decide using the actual workload.

You can also choose between an array of structures and a structure of arrays:

```text
Array of structures (AoS)
[price qty venue side][price qty venue side][price qty venue side]

Structure of arrays (SoA)
prices:      [price][price][price]...
quantities:  [ qty ][ qty ][ qty ]...
venues:      [venue][venue][venue]...
```

AoS is convenient when an update touches one complete level. SoA is attractive when a calculation scans one or two columns across many levels. SoA also makes contiguous vector loads easier to express. Neither is automatically better for sparse updates.

**Production habit:** keep per-owner working data dense; spend padding at measured ownership boundaries. Cache isolation and density address different relationships between bytes.

## 7. Dimension four: SIMD boundaries and tails

SIMD means “single instruction, multiple data.” Instead of adding one pair of integers, an instruction can add several corresponding pairs at once.

```text
Eight u32 lanes in a 256-bit vector:

        lane 0  lane 1  lane 2  lane 3  lane 4  lane 5  lane 6  lane 7
A       [  a0 ][  a1 ][  a2 ][  a3 ][  a4 ][  a5 ][  a6 ][  a7 ]
B       [  b0 ][  b1 ][  b2 ][  b3 ][  b4 ][  b5 ][  b6 ][  b7 ]
add     [a0+b0][a1+b1][a2+b2][a3+b3][a4+b4][a5+b5][a6+b6][a7+b7]
```

Common x86 vector widths include 16 bytes for SSE registers, 32 for AVX/AVX2, and 64 for AVX-512. These are register widths; the selected instruction determines the memory-access contract.

### 7.1 Aligned and unaligned instructions

For the AVX2 integer examples:

| Intrinsic | Address requirement |
|---|---|
| `_mm256_load_si256` | Source must satisfy its 32-byte alignment requirement. |
| `_mm256_loadu_si256` | Source does not need 32-byte alignment. |

Both require enough readable initialized storage. “Unaligned” does not mean “allowed to read beyond the end.” [Rust aligned load](https://doc.rust-lang.org/core/arch/x86_64/fn._mm256_load_si256.html), [Rust unaligned load](https://doc.rust-lang.org/core/arch/x86_64/fn._mm256_loadu_si256.html).

A wrapper can provide a suitably aligned fixed-size block:

```rust
#[repr(C, align(32))]
struct Lanes([u32; 8]);
```

An allocation holding `Lanes` must satisfy that type’s alignment. An ordinary `Vec<u32>` only promises what its element type requires. Even an aligned base does not make every subslice aligned: moving forward one `u32` shifts the pointer by four bytes.

### 7.2 Handle the tail explicitly

Suppose the input contains 19 quantities. A 256-bit vector holds eight `u32` values:

```text
indices: 0........7   8.......15   16..18
         [vector 1]   [vector 2]   [tail ]
```

The lab processes complete groups with `chunks_exact(8)` and finishes the remainder scalarly. Its AVX2 loop uses unaligned loads, so it works with ordinary slices and shifted subslices. It checks CPU support before calling the AVX2 function and provides a scalar fallback.

The key loop is:

```rust
let mut chunks = values.chunks_exact(8);
for chunk in &mut chunks {
    // Full intrinsic implementation is in alignment_lab.rs.
    // Each chunk contains exactly 32 bytes; load and accumulate it.
}
// Process chunks.remainder() without reading past the slice.
```

That excerpt illustrates control flow; it is not a replacement for the executable SIMD function. The companion includes the safety comments and actual instructions.

An alternative is initialized padding out to a vector-width multiple. For summation, zeros are neutral. For minimum, maximum, multiplication, or a more complex calculation, the neutral value and semantics differ. Spare vector capacity is not initialized tail data and must not be read as if it were.

### 7.3 Relate the exercise to VWAP carefully

A simplified volume-weighted average price is:

```text
VWAP = sum(price_ticks[i] × quantity[i]) / sum(quantity[i])
```

The lab sums quantities to isolate vector loading and tail handling. It intentionally uses wrapping `u32` arithmetic, making its scalar and vector results equivalent modulo 2^32. **That arithmetic is unsuitable as an implicit rule for financial totals.**

A real VWAP implementation needs an explicit price scale, a suitably wide accumulator, overflow policy, treatment of negative prices where applicable, zero-volume handling, and a rounding rule. A scalar `i128` reference can help check bounded integer inputs before introducing a vector implementation. Floating-point vector reductions may change rounding through a different addition order.

**Production habit:** begin with a correct scalar calculation, try compiler optimization, then introduce explicit intrinsics only where measurement justifies the complexity. Wider vectors also change the amount of work per invocation; that matters when processing a tiny update.

## 8. Dimension five: stable bytes, safe views, and IPC

### 8.1 Separate the wire contract from the in-memory record

Here is a deliberately small wire protocol, defined by us for the lab:

| Byte range, inclusive | Meaning | Encoding |
|---|---|---|
| 0 | Side | 0 = bid; 1 = ask; reject other values |
| 1–8 | Price | Signed 64-bit, little-endian |
| 9–12 | Quantity | Unsigned 32-bit, little-endian |
| 13–14 | Venue | Unsigned 16-bit, little-endian |

```text
Wire message: 15 bytes; no implicit gaps

0     1..............8  9......12  13..14
┌────┬────────────────┬──────────┬───────┐
│side│ price bytes    │ qty bytes│ venue │
└────┴────────────────┴──────────┴───────┘

Decoded Level: 16 bytes; different field order

0..............7  8......11  12..13  14    15
┌────────────────┬──────────┬───────┬─────┬──────────┐
│ price_ticks    │ quantity │ venue │side │ reserved │
└────────────────┴──────────┴───────┴─────┴──────────┘
```

The wire format optimizes agreement between participants. The hot record optimizes local access. They do not need to be identical.

### 8.2 A borrowed view avoids copying the whole packet

```rust
struct LevelView<'a> {
    bytes: &'a [u8; 15],
}
```

The lifetime `'a` connects the view to the borrowed packet buffer. In the lab, the constructor requires exactly 15 bytes and validates side. Accessors decode the chosen byte ranges with `from_le_bytes`. `decode()` builds a `Level` when the caller wants to retain the values.

```rust
let packet = [1, 0x29, 0x27, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 3, 0];
let view = LevelView::new(&packet)?;
let price = view.price_ticks(); // 10_025 ticks
let level = view.decode();
```

This excerpt belongs inside a function returning a compatible `Result`. The complete lab includes a runnable caller.

The view performs no whole-message copy or allocation. Returning an integer still loads bytes and produces a scalar value. “Zero-copy” should name the copy being avoided: a receive-buffer copy, a full-message decode, an IPC transfer, or something else.

Buffer ownership matters. The NIC or buffer pool must not overwrite a receive buffer while a view still reads it. A safe Rust borrow models only the accesses represented by the program’s abstractions; a foreign DMA API still needs a correct ownership integration.

### 8.3 Why a raw cast is not the default solution

Before interpreting bytes as `&T`, you would have to establish all of these:

1. Enough bytes are present and remain accessible.
2. The address meets `T`’s alignment.
3. Every interpreted field has a valid Rust bit pattern.
4. The offsets, sizes, and byte order match the protocol.
5. Lifetimes, aliasing, mutation, and publication satisfy the access contract.

For example, an arbitrary byte cannot be assumed to be a Rust `bool`, and an arbitrary integer cannot be assumed to be a valid Rust enum discriminant. Even a well-aligned byte buffer may contain invalid typed data. [Rust Reference: invalid values and aliasing](https://doc.rust-lang.org/reference/behavior-considered-undefined.html).

An outer `repr(C)` also does not change the layout of nested fields with their own default representation. Treat the schema recursively. [Rust Reference: nested layout](https://doc.rust-lang.org/reference/type-layout.html).

### 8.4 Understand packed records before touching them

```rust
#[repr(C, packed)]
struct PackedExample {
    side: u8,
    price_ticks: i64,
}
```

The price starts at offset one and may be unaligned. Do not create `&packed.price_ticks`. Even an expression that creates a reference temporarily and then casts it to a pointer has already violated the requirement. If a packed schema is unavoidable, use a raw field pointer and the appropriate unaligned operation:

```rust
let packed = PackedExample { side: 0, price_ticks: 10_025 };
let pointer = &raw const packed.price_ticks;
// SAFETY: pointer addresses the initialized live i64 field.
// read_unaligned permits the field's possible misalignment.
let price = unsafe { pointer.read_unaligned() };
assert_eq!(price, 10_025);
```

This solves the alignment part of this read. It does not convert byte order, validate arbitrary field types, or synchronize concurrent writers. [Rust: `read_unaligned` safety](https://doc.rust-lang.org/std/ptr/fn.read_unaligned.html).

Packing can be needed to match an external format, but packing all internal records can introduce repeated unaligned accesses. For our market-depth example, use a safe wire view plus a dense, naturally aligned internal record.

### 8.5 Shared memory needs a lifecycle contract

Two processes can map the same shared bytes at different virtual addresses:

```text
Process A                         Process B
mapping base 0x1000_0000           mapping base 0x7000_0000
record offset +0x80               record offset +0x80
          └────────────── same shared backing ──────────────┘
```

If a shared record stores a process-local pointer, that value may mean nothing in the other process. A common design stores a bounded offset or slot index instead. Validate offset-plus-length arithmetic and the mapping bounds before creating an access.

A production IPC schema should specify fixed-width fields, byte order, version, mapping size, offsets, alignment, initialization, publication, slot reclamation, and recovery after a process failure. Keep `Vec`, `String`, references, and process-local allocator state out of directly shared records unless a specialized framework explicitly supports them.

For atomics in shared mappings, separately establish platform support, alignment, initialization, and process-shared behavior of the complete implementation. The chapter’s thread example does not prove an IPC design sound.

Implicit padding is not serialized application data and may be uninitialized. Encode fields deliberately. Explicit reserved bytes should have a defined initial value and a compatibility policy.

Kernel bypass changes the packet-access path; it does not waive these contracts. DPDK, for example, has its own memory, NUMA, and IOVA constraints. CPU virtual alignment is not a substitute for a NIC’s DMA requirements. [DPDK EAL memory documentation](https://doc.dpdk.org/guides/prog_guide/env_abstraction_layer.html).

**Production habit:** treat wire bytes, shared-memory records, and local hot records as separate contracts, even when an implementation deliberately makes two of them coincide.

## 9. Dimension six: pages, TLBs, and startup work

### 9.1 Add address translation to the picture

A process uses virtual addresses. The MMU translates them to physical memory. A TLB caches translations, reducing the need to consult page tables. Common x86 Linux page sizes include a 4 KiB base page and, where supported and configured, 2 MiB and 1 GiB huge pages. [Linux: HugeTLB pages](https://docs.kernel.org/admin-guide/mm/hugetlbpage.html).

```text
CPU virtual address
        │
        ▼
Translation cache (TLB)
        ├── hit ───────────────► use cached translation
        │
        └── miss ──► page-table walk ──► translation
                                               │
                                               ▼
                                       data access proceeds

A page fault is a separate event: the OS must resolve the mapping/access.
```

A TLB miss may be resolved by a hardware page-table walk without a page fault. A page fault can require operating-system work even if no disk I/O occurs. Calling both events “a cache miss” hides the distinction you need for diagnosis.

### 9.2 Crossing a boundary is geometry, not a prediction

```text
Virtual page A: offsets 0 .....................................4095
Virtual page B: offsets 0 .....................................4095

8-byte access beginning at A+4092:
                         A: [4092..4095] | B: [0..3]
```

This access needs translations for both pages. If both translations are cached, crossing the boundary does not itself create a TLB miss. Conversely, an access entirely inside one page can miss in the TLB when that translation is absent. Crossing a boundary can still have additional hardware costs; do not confuse those costs with a guaranteed miss.

For a positive access size `N` and boundary size `P`, the geometric test is:

```text
(address % P) + N > P
```

Use overflow-safe arithmetic in real code; the lab implements an equivalent subtraction-based check. The helper checks geometry only. It does not inspect a TLB or discover the actual mapping size.

### 9.3 Why huge pages may help a large pool

Consider a page-aligned 64 MiB virtual region:

```text
64 MiB / 4 KiB = 16,384 base-page mappings
64 MiB / 2 MiB =     32 huge-page mappings
```

The second layout needs fewer distinct translations to cover the region. This arithmetic is a reason to test huge pages, not a predicted speedup or a promise that all translations fit in a particular TLB. Real TLB organization and workload locality matter.

Do not try to solve page backing with `#[repr(align(2097152))]`. That changes a type’s alignment requirement; it does not ask Linux to use HugeTLB or transparent huge pages. An aligned virtual address alone proves nothing about the mapping’s page size.

Linux provides explicit HugeTLB mappings and transparent huge-page mechanisms. Explicit mappings need suitable resources and can fail. Transparent promotion is policy-dependent and may occur dynamically. Choose and verify the actual mechanism instead of assuming it. [Linux: HugeTLB](https://docs.kernel.org/admin-guide/mm/hugetlbpage.html), [transparent huge pages](https://docs.kernel.org/admin-guide/mm/transhuge.html).

### 9.4 A startup sequence to evaluate

For a large latency-sensitive pool, an engineering plan might be:

1. Estimate capacity and define behavior on exhaustion.
2. Allocate or map the pool during startup.
3. Establish NUMA policy so memory placement matches its primary users.
4. Touch and initialize the memory under the intended placement policy.
5. Verify page backing and placement from the running process.
6. Warm the access paths, then measure steady-state and disturbance cases.

Preallocation removes a category of allocation work from the hot path; initialization also matters. A reservation that has never been touched may defer work until the first update. NUMA means memory location relative to CPU sockets or nodes; a pool placed near another socket may require remote accesses.

Memory locking is a separate tool. Successful `mlock` can keep pages resident, subject to operating-system limits; it does not pin their translations inside the TLB or turn them into huge pages. [Linux: `mlock`](https://man7.org/linux/man-pages/man2/mlock.2.html).

Check the relevant mapping in `/proc/<pid>/smaps`, including fields such as `AnonHugePages`, `Shared_Hugetlb`, and `Private_Hugetlb`, as applicable. Interpret these alongside the allocation method and system policy. Merely reading the configured default huge-page size is not evidence that your pool uses it.

Large pools naturally span many pages. Requiring an entire pool to fit inside one huge page is neither necessary nor generally practical. The useful design questions concern translation coverage, residency, placement, and access pattern.

**Production habit:** make memory backing an observable startup property, with explicit failure or fallback behavior. Document whether a fallback still meets the deployment’s latency objective.

## 10. Put the six dimensions together

Here is a possible design for the running example:

```text
Receive buffer owned by parser
     │
     │ validate bounded byte view; decode required fields
     ▼
Dense Level records owned by book-update core
     │
     ├── single-level updates: AoS may be convenient
     │
     └── broad scans: evaluate SoA and SIMD
     │
     ▼
Bounded handoff queue
     ├── slots follow a proven publication/reclamation protocol
     ├── independently hot cursors isolated where justified
     └── capacity/exhaustion behavior defined
     │
     ▼
Strategy/risk owner ──► explicit outbound message encoding

Large pool backing: established, initialized, and verified at startup
```

Each decision answers a different question:

| Question | Mechanism to consider | Evidence to collect |
|---|---|---|
| Is a typed access valid? | Correct alignment and valid values | Layout checks; review of unsafe preconditions |
| Do unrelated owners fight over lines? | Ownership changes, sharding, selective padding | Controlled core placement; coherence evidence |
| Does the hot working set fit better? | Dense records, hot/cold split, AoS/SoA | Bytes touched; cache misses; end-to-end latency |
| Can a scan process several values at once? | Compiler vectorization or explicit SIMD | Scalar equivalence; assembly; representative timings |
| Do participants agree on bytes and ownership? | Explicit schema, views, publication protocol | Fixtures, malformed input tests, compatibility checks |
| Is translation pressure significant? | Pool sizing, page backing, NUMA placement | Actual mappings; translation counters; tail behavior |

Avoid claiming that a mechanism solves a different row. `repr(C)` is not synchronization. `align(64)` is not huge-page backing. Huge pages do not validate a network packet. A successful layout assertion is a useful contract check, not a latency benchmark.

## 11. Run the companion lab

Place the chapter and `alignment_lab.rs` in a working directory with a Rust compiler. From that directory:

```sh
rustc --edition=2021 --test -O alignment_lab.rs -o alignment_tests
./alignment_tests

rustc --edition=2021 -O alignment_lab.rs -o alignment_lab
./alignment_lab
```

The core layout output on the chapter’s x86-64 target is:

```text
LooseLevel: size=24 align=8 offsets=[0, 8, 16, 20]
Level: size=16 align=8 offsets=[0, 8, 12, 14, 15]
SharedCounters: size=64 a=0 b=8
IsolatedCounters: size=128 a=0 b=64
Lanes: size=32 align=32
```

The five tests check layout contracts, an independent wire-format fixture and invalid inputs, signed/integer edge cases, scalar/vector agreement for different lengths and offsets, and boundary arithmetic. AVX2 execution is conditional on CPU support. Other architectures use the scalar fallback; the exact x86-64 layout assertions are target-gated.

To run the illustrative false-sharing experiment:

```sh
./alignment_lab 1000000
```

Two workers each update one atomic counter. One arrangement puts the counters on one line; the other isolates them. The operation and iteration count are otherwise the same. The experiment alternates execution order across five pairs of trials.

**Interpret this experiment narrowly.** Its workers are not pinned to selected physical cores. Its aggregate timing includes barrier and join effects. It does not report per-update percentiles, and dividing its time by the number of operations does not produce a measured tick-to-trade latency. Atomic read-modify-write traffic also differs from many single-writer queue cursor workloads.

The experiment is useful for observing a mechanism and motivating a controlled benchmark. It cannot establish a deployment speedup or a per-occurrence false-sharing penalty.

## 12. Measure like a systems engineer

### 12.1 State a falsifiable hypothesis

An example:

> When two specified physical cores repeatedly update independent counters, isolating those counters reduces the interference observed in this workload.

That is testable. “Padding makes HFT faster” is too broad to guide a useful experiment.

For the record layout, a separate hypothesis might be that the 16-byte record improves a particular depth scan at a working-set size where the larger layout creates more cache pressure. Do not assume the same result for random single-record updates.

### 12.2 Hold the right variables fixed

Record CPU model, topology, compiler, optimization flags, operating system, placement, input distribution, and load. Distinguish two physical cores from sibling hardware threads and from cores on different sockets. A process-level CPU mask allowing two cores does not by itself bind one worker to each.

Compare the same algorithm with one layout change. Warm the relevant state where the experiment calls for warm-state results. Repeat and alternate trial order. Use sufficiently large samples for the percentiles you want; a tiny sample makes extreme-tail estimates unstable.

Keep allocation, printing, histogram growth, and other measurement work out of the timed region when they are not part of the intended workload. Use a preallocated result buffer or sampling design. Measure clock-reading overhead. Prevent the compiler from removing the work and inspect optimized code when instruction selection matters.

Avoid leaving the traffic generator idle while the system is delayed if your goal is to model a continuing external arrival stream. Such a generator can hide waiting time that real arrivals would experience. Include realistic bursts and queue occupancy.

### 12.3 Connect counters to the suspected mechanism

For cache interference, Linux’s false-sharing documentation discusses tools such as `perf c2c`. For translation pressure, inspect CPU-specific translation events; for faults, inspect fault counters. Hardware event availability and interpretation vary. [Linux false-sharing investigation](https://kernel.org/doc/html/latest/kernel-hacking/false-sharing.html).

Performance counters support an explanation; they do not replace timing the actual operation. A reduced miss count can coexist with worse latency if another tradeoff dominates.

### 12.4 Define tick-to-trade timestamps

Choose exact endpoints: for example, software receive completion to software transmit submission, or comparable hardware timestamps at the NIC. These are different metrics. State clock domains and synchronization assumptions when subtracting timestamps from different sources.

A change is ready for deployment review when its correctness argument, layout contract, representative measurements, and operational failure behavior are all reviewable. No single microbenchmark supplies all four.

## 13. Exercises with worked answers

### Exercise 1: predict a layout

Assume the chapter’s usual x86-64 alignments:

```rust
#[repr(C)]
struct Example {
    a: u8,
    b: u32,
    c: u16,
}
```

**Answer:** `a` is at 0, padding occupies 1–3, `b` is at 4, `c` is at 8, and tail padding occupies 10–11. Size is 12, alignment is 4. With fields ordered `b`, `c`, `a`, the size becomes 8. Check with the compiler before relying on these numbers.

### Exercise 2: isolate two queue counters

You put `#[repr(align(64))]` on a struct containing two adjacent atomic integers. Did you isolate them?

**Answer:** no. The beginning is aligned, but both fields may still share that first line. Use appropriately aligned wrappers or explicit verified offsets, and check both the stride and the allocation’s alignment.

### Exercise 3: avoid a SIMD overread

An eight-lane loop has 19 inputs. Can it issue three unconditional full-width loads?

**Answer:** not from a slice containing only those 19 values. Two loads cover 16 inputs; handle the final three scalarly, with an appropriate masked operation, or through initialized padding whose semantics are correct for the calculation.

### Exercise 4: repair a wire decoder

A 15-byte packet has a one-byte side followed by an eight-byte price. Can you cast its beginning to `&Level`?

**Answer:** no. Our `Level` has a different field order and size; the packet does not establish its alignment or typed-access contract. Use the validated byte view. An unaligned read alone would still not fix the layout mismatch or byte order.

### Exercise 5: explain the TLB claim

An eight-byte read spans two mapped pages. Must it miss in the TLB?

**Answer:** no. It needs both translations; either, both, or neither may already be cached. A boundary test cannot tell you translation-cache state.

### Exercise 6: reject an attractive optimization

After padding every order-book record to 64 bytes, the false-sharing microbenchmark improves, but the strategy’s p99 scan latency worsens. Which result should govern the book layout?

**Answer:** the representative strategy result. Identify the book’s actual ownership and access pattern. If one core owns it, dense records may be the better design while queue metadata remains isolated. The counter benchmark exercised a different workload.

## 14. A practical progression

Work through this chapter in five passes:

1. **Count:** predict the record offsets on paper, then compare with the lab.
2. **Validate:** understand every wire-format byte and rejected input before touching unsafe code.
3. **Own:** draw which core or process owns each mutable region and how it is handed over.
4. **Measure:** test one layout hypothesis at a time under documented conditions.
5. **Integrate:** remeasure the whole path with realistic bursts, capacity limits, and failure cases.

The engineering judgment to develop is deciding when bytes should be close together, when owners should be kept apart, and what evidence would establish that the decision helped your actual system.

---

*Verification for this deliverable: compiled with Rust 1.99.0 on x86-64; all five companion tests passed; the example program ran successfully. Layout and functional checks validate the examples within that scope. No production latency improvement or huge-page allocation is claimed.*
