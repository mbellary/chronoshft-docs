# nexus-bits capability specifications

These Draft 0.1.0 specifications describe the public contract of `nexus-bits` 0.4.4 and the macros supplied by `nexus-bits-derive` through the default `derive` feature. The source links are pinned to the reviewed Nexus revision. The local checkout also contains working changes to the flag implementation and tests, which were included in the review. The runtime descriptors and the `IntEnum` trait are available with default features disabled.

## Capabilities and verification map

| Specification | Public contract | Primary verification |
| --- | --- | --- |
| [01 Bit math and layout validation](01-bit-math-and-layout-validation.md) | Bit numbering, bounds, overlaps, masks, and signed decoding | Boundary vectors and macro compile-fail cases |
| [02 IntEnum contract](02-int-enum-contract.md) | Typed values and integer codes | Round trips, unknown codes, and derive rejection cases |
| [03 Flat bit storage](03-flat-bit-storage.md) | Struct layouts, builders, accessors, and raw values | Builder and getter integration tests, size and alignment assertions |
| [04 Tagged union storage](04-tagged-union-storage.md) | Tags, variant-specific payloads, and conversion to variant types | Dispatch, payload validation, malformed layouts, and ABI assertions |
| [05 Error types and diagnostics](05-error-types-and-diagnostics.md) | Structured failures and compile-time diagnostics | Exact payload and message assertions |
| [06 Runtime bit fields and flags](06-runtime-bitfields-and-flags.md) | Reusable descriptors for reading and editing caller-owned words | Const evaluation, boundary checks, and preservation properties |

## Use cases

A precomputed [`BitField`](06-runtime-bitfields-and-flags.md) stores the position, width, and mask for a field. The same descriptor can read or update that field across many integer words. Each operation does a bounded amount of work and allocates no heap memory. Updates return a new word and preserve bits outside the selected field. These properties are useful in systems targeting sub-100 µs latency, where end-to-end timing still depends on the surrounding workload.

### 1. Tagged pointers in lock-free data structures

**How it works.** In a queue or ring buffer, pack a node reference and a version counter or state flags into one `u64`. For example, an application that controls its reference encoding can use bits 0–47 for a pool-relative node index and bits 48–63 for a version counter. Encoding an actual pointer requires a layout supported by the target ABI and address configuration. See [x86-64 five-level paging](https://docs.kernel.org/next/x86/x86_64/5level-paging.html) and [AArch64 tagged addresses](https://docs.kernel.org/next/arch/arm64/tagged-pointers.html).

**Why it matters.** On targets with `AtomicU64`, one successful compare-and-swap (CAS) can publish the reference and its tag together. The version can distinguish a reused reference while its tag differs from the reader's snapshot. The algorithm must account for counter wraparound, memory ordering, and safe node reclamation. See [Rust's atomic comparison contract](https://doc.rust-lang.org/stable/core/sync/atomic/type.AtomicU64.html#method.compare_exchange).

### 2. Zero-copy parsing of binary protocol headers

**How it works.** For a custom UDP multicast feed, define constant descriptors for a two-byte header containing a four-bit `message_type`, a one-bit `side`, a three-bit `time_in_force`, and eight `sequence_flags` bits. Check that the receive buffer contains both header bytes, load the word in the protocol's byte order, and extract the fields.

**Why it matters.** The parser can borrow the packet in the receive buffer and decode its header from a scalar word without allocating a decoded header or copying the payload. The wire layout determines the field positions and widths.

### 3. Hardware control and status registers

**How it works.** Define descriptors for fields such as `DMA_BURST_SIZE` at bits 4–7, `IRQ_ENABLE` at bit 12, and `ERROR_CODE` at bits 16–23 of a 32-bit register image. Use the device's I/O interface to read the register, then extract its status fields or compute a new control value.

**Why it matters.** A field update preserves adjacent bits in the computed word. A driver can write that word back when the device permits read-modify-write. The register specification determines how to handle reserved bits, write-one-to-clear flags, access ordering, and concurrent device updates. The same field operations can decode packed NIC descriptors and NVMe queue entries using their documented layouts.

### 4. High-density order and event identifiers

**How it works.** Divide a 64-bit order or event ID into an eight-bit `shard_id`, a 32-bit `timestamp_delta`, a four-bit `source_engine`, and a 20-bit `monotonic_sequence`. Define the timestamp unit and epoch, and specify how IDs stay unique when the timestamp or sequence reaches its field limit.

**Why it matters.** Once a consumer has the ID, it can extract the shard or timestamp directly for routing or staleness checks without a separate metadata lookup.

### 5. Compact state machines and flags

**How it works.** Pack a three-bit `connection_state`, a four-bit `retry_count`, a two-bit `priority_level`, and a two-bit `circuit_breaker_status` into one `u16`. An event loop can read the packed word, validate a transition, and compute its replacement. For state shared across threads, the caller can use compare-and-swap on a supported atomic word and revalidate the transition if the snapshot changes.

**Why it matters.** A smaller hot-state record can fit more entries in cache. Measure the containing struct's size and account for alignment and placement. Frequently updated records owned by different cores can still contend when they share a cache line.

### 6. Custom allocator and slab page headers

**How it works.** Pack a six-bit `size_class_index`, a ten-bit `allocated_slot_count`, a four-bit `numa_node_id`, and four `page_flags` bits into a single `u32`. During allocation or deallocation, read the size class and occupancy from that word, then update the count with a checked insertion. Size the field for the maximum page occupancy, including the full state.

**Why it matters.** A four-byte descriptor keeps per-page metadata compact and gives the allocator bounded-cost access to its counters and flags. The allocator's free list or bitmap determines the cost of finding an available slot.

## Source-to-capability traceability matrix

| Source | Specifications | Responsibility |
| --- | --- | --- |
| [nexus-bits/src/lib.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/lib.rs) | 02–06 | Public exports and macro feature gate |
| [nexus-bits/src/field.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/field.rs) | 01, 05, 06 | Runtime range construction, extraction, and replacement |
| [nexus-bits/src/flag.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/flag.rs) | 01, 05, 06 | Runtime single-bit operations |
| [nexus-bits/src/int_enum.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/int_enum.rs) | 02–04 | Shared integer conversion trait |
| [nexus-bits/src/error.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/error.rs) | 03–06 | Error values and display formatting |
| [nexus-bits-derive/src/lib.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits-derive/src/lib.rs) | 01–05 | Attribute parsing, layout validation, diagnostics, and generated APIs |

The checkout's `nexus-bits/spec/bitfield.md` and `nexus-bits/spec/flags.md` are prior references for CAP-06. This suite uses the current Rust declarations and implementations to resolve conflicting descriptions. Open gaps describe cases where the implementation falls short of a proposed invariant. The verification checklists track coverage specific to these specifications.

## Use case-to-capability matrix

**Direct** marks capabilities used by the example as described. **Optional** marks capabilities that apply if the design adds the behavior named in the cell. A dash means the layout has no variant-specific payload.

| Use case | [01 Layout validation](01-bit-math-and-layout-validation.md) | [02 IntEnum](02-int-enum-contract.md) | [03 Flat storage](03-flat-bit-storage.md) | [04 Tagged unions](04-tagged-union-storage.md) | [05 Errors and diagnostics](05-error-types-and-diagnostics.md) | [06 BitField and Flag](06-runtime-bitfields-and-flags.md) |
| --- | --- | --- | --- | --- | --- | --- |
| Tagged pointers | **Direct** · Fit the index and version into one word | **Optional** · Decode node state codes | **Optional** · Generate accessors for the fixed index and tag layout | — | **Optional** · Report an index that exceeds its field width | **Direct** · Read the index and replace the version in the candidate word for CAS |
| Binary protocol headers | **Direct** · Fit field widths and positions to the wire layout | **Optional** · Decode message type, side, or time-in-force codes | **Optional** · Generate accessors for the fixed header word | **Optional** · Decode a packed word whose message tag selects a different payload layout | **Optional** · Report invalid field values through typed accessors | **Direct** · Extract fields from the loaded header word |
| Hardware registers | **Direct** · Match field positions to the device layout | **Optional** · Decode named hardware status codes | **Optional** · Generate accessors for a fixed register image | — | **Optional** · Report a control value that exceeds its field width | **Direct** · Read status fields, update control fields, and represent single-bit controls with `Flag` |
| Order and event IDs | **Direct** · Partition the 64-bit ID into fixed fields | **Optional** · Decode named source engine codes | **Optional** · Generate builders and accessors for the fixed ID layout | — | **Optional** · Report a value that exceeds its ID field width | **Direct** · Pack ID fields and extract routing or timestamp metadata |
| Compact state machines | **Direct** · Fit states and counters into one word | **Optional** · Decode connection or circuit breaker state codes | **Optional** · Generate accessors for the shared layout | **Optional** · Give each state its own packed payload layout | **Optional** · Report a retry count that exceeds its field width | **Direct** · Read fields and compute a replacement state word |
| Allocator and slab headers | **Direct** · Size the occupancy field to represent a full page | **Optional** · Decode named size class codes | **Optional** · Generate accessors for the fixed page header | — | **Direct** · Report occupancy overflow from checked insertion | **Direct** · Extract the size class and occupancy, then update the count with a checked insertion |
