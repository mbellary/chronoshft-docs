# nexus-bits capability specifications

**`nexus-bits` packs several small values into one fixed-width integer and lets you read each value by its position or name.**

## 1. What the crate does

**Use `BitField` and `Flag` with a word you already own, or use `#[bit_storage]` to generate a packed Rust type.**

Consider a feed that puts four values in its first two bytes. The bytes form a 16-bit unsigned integer, or `u16`. A **field** is a fixed group of bits within that integer. Count bit positions from the right, starting at zero.

```text
Bytes in arrival order    A6                 B5
Header word               0xA6B5
Bit positions             15         8       7..5    4      3..0
Field                     sequence flags     time    side   message
                                             in force       type
Bits                      10100110           101     1      0101
Value                     166                5       true   5
```

A `BitField` remembers a field's position, width, and **mask**, a pattern of ones that selects the field's bits. A `Flag` handles one bit as true or false. Create either descriptor as a constant when the layout is known at compile time, or build it once at runtime when configuration supplies the layout. The caller's word holds the values.

The `#[bit_storage]` macro checks a declared layout at compile time and generates a packed type, named getters, and a builder. Use `IntEnum` when a protocol gives names to specific integer codes.

## 2. When to use the crate

**Pack fields when their positions and widths are fixed and the wire format or memory savings justify the extra work to extract values.**

| Situation | Decision |
| --- | --- |
| A protocol assigns four bits to message type | Use a four-bit field. It holds values from 0 to 15. Reject values above 15 when building the header. |
| A large table repeats several small values | Measure the size of the entire record, including alignment padding, and time actual reads and writes. |
| A field changes length from packet to packet | Decode it in the protocol parser. Use fixed-width fields for any fixed-width prefix. |
| Two threads update fields in the same word | Coordinate whole-word updates through a lock or a supported atomic operation. Two independent replacements can lose an update. |
| A rarely used record has no wire format or meaningful size constraint | Keep separate fields for direct access. |

Use unsigned backing such as `u16` for ordinary nonnegative codes. Set any decoding budget from measurements of the actual workload. A sub-100 µs system target covers receipt, decoding, and the work that follows.

## 3. Where to use the crate

**At the network boundary, check and load the header bytes before passing the integer to `nexus-bits`.**

```text
[Receive packet] -> [Check for two bytes] -> [Load u16] -> [Decode four fields]
                              |                                    |
                        Too short: error                   [Caller uses result
                                                            or queues it]
```

This example sends the high byte first, which is called **big-endian**. Load `[0xA6, 0xB5]` with `u16::from_be_bytes` to get `0xA6B5` on any target. The receiver borrows the packet during decoding and leaves the payload in the receive buffer. The caller owns any queue handoff between threads, any synchronized update to a shared word, and any device register access.

## 4. Why to use the crate

**The packed header takes two bytes, and decoding its fields needs no heap allocation.**

| Representation | Stored values per header | Field access |
| --- | --- | --- |
| Separate `u8` or `bool` fields | Four one-byte values | Read a value directly |
| `u16` with hand-written masks | One two-byte integer | Apply masks and shifts you maintain |
| `u16` with `nexus-bits` | One two-byte integer | Use named getters or reuse a descriptor across words |

A type generated with `#[bit_storage]` has the backing integer's size and alignment. Runtime descriptors take separate space and can be reused across many headers. The crate supports `no_std`, meaning it can run without Rust's standard library. Actual latency depends on the compiler, hardware, and surrounding workload.

## 5. How to use the crate

**Decode the same header with reusable descriptors or with named getters generated from a Rust declaration.**

### Decode with reusable descriptors

**Check for two bytes, load them in wire order, then read four fields from the resulting integer.**

In a Cargo binary project, add this line under `[dependencies]`. Disabling the default `derive` feature keeps `BitField`, `Flag`, and the `IntEnum` trait.

```toml
nexus-bits = { version = "0.4.4", default-features = false }
```

Save this as `src/main.rs` and run `cargo run`.

```rust
use nexus_bits::{BitField, Flag};

const MESSAGE_TYPE: BitField<u16> = BitField::<u16>::new(0, 4);
const SIDE: Flag<u16> = Flag::<u16>::new(4);
const TIME_IN_FORCE: BitField<u16> = BitField::<u16>::new(5, 3);
const SEQUENCE_FLAGS: BitField<u16> = BitField::<u16>::new(8, 8);

#[derive(Debug, PartialEq, Eq)]
struct ShortHeader;

fn decode(packet: &[u8]) -> Result<(u16, bool, u16, u16), ShortHeader> {
    let [high, low, ..] = packet else {
        return Err(ShortHeader);
    };
    let word = u16::from_be_bytes([*high, *low]);
    Ok((
        MESSAGE_TYPE.get(word),
        SIDE.is_set(word),
        TIME_IN_FORCE.get(word),
        SEQUENCE_FLAGS.get(word),
    ))
}

fn main() {
    assert_eq!(decode(&[0xA6, 0xB5, 0xFF]), Ok((5, true, 5, 166)));
    assert_eq!(decode(&[0xA6]), Err(ShortHeader));
}
```

### Use a generated packed type

**Enable the default `derive` feature for named getters and checked construction through a builder.**

Replace the previous dependency line and `src/main.rs` with the following, then run `cargo run` again.

```toml
nexus-bits = "0.4.4"
```

```rust
use nexus_bits::bit_storage;

#[bit_storage(repr = u16)]
struct Header {
    #[field(start = 0, len = 4)]
    message_type: u8,
    #[flag(4)]
    side: bool,
    #[field(start = 5, len = 3)]
    time_in_force: u8,
    #[field(start = 8, len = 8)]
    sequence_flags: u8,
}

fn main() {
    // This fixed array supplies two bytes by construction.
    let bytes = [0xA6, 0xB5];
    let header = Header::from_raw(u16::from_be_bytes(bytes));
    assert_eq!(header.message_type(), 5);
    assert!(header.side());
    assert_eq!(header.time_in_force(), 5);
    assert_eq!(header.sequence_flags(), 166);

    let rebuilt = Header::builder()
        .message_type(5)
        .side(true)
        .time_in_force(5)
        .sequence_flags(166)
        .build()
        .unwrap();
    assert_eq!(rebuilt.raw().to_be_bytes(), bytes);

    let Err(error) = Header::builder().message_type(16).build() else {
        panic!("16 exceeds the four-bit message type");
    };
    assert_eq!(error.field, "message_type");
}
```

### Quick reference

**Choose the API by what you need to read, change, or validate.**

| Task | API | Result or failure |
| --- | --- | --- |
| Read a field or flag | `BitField::get` or `Flag::is_set` | Returns the integer value or a Boolean. |
| Replace a field with a width check | `BitField::set` | Returns a replacement word or `Overflow<T>`. Store the returned word to apply the change. |
| Build a packed value | `Header::builder()`, then set values and call `build()` | Returns the packed value or `FieldOverflow<T>`. Omitted fields start at zero. |
| Decode a named code | `IntEnum::try_from_repr` or a generated flat enum getter | An unknown code returns `None` from the trait method or `UnknownDiscriminant<T>` from the getter. |
| Set a layout | `BitField::new`, `Flag::new`, or `#[bit_storage]` | Invalid descriptor geometry can panic at runtime or fail constant evaluation. Invalid macro layouts fail compilation. |

`from_raw` wraps the word as supplied. Validate packet length, reserved bits, and protocol rules in the decoder. Consult the [flat storage spec](03-flat-bit-storage.md) for narrowing cases in generated builders and the [runtime field spec](06-runtime-bitfields-and-flags.md) for signed storage and constructor limits.

## Capabilities and verification map

**Six draft specifications define the layouts, APIs, and failure behavior.**

These Draft 0.1.0 specifications describe the public contract of `nexus-bits` 0.4.4 and the macros supplied by `nexus-bits-derive` through the default `derive` feature. The source links are pinned to the reviewed Nexus revision. The local checkout also contains working changes to the flag implementation and tests, which were included in the review. The runtime descriptors and the `IntEnum` trait are available with default features disabled.

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

**How it works.** In a queue or ring buffer, pack a node reference and a version counter or state flags into one `u64`. For example, an application that controls its reference encoding can use bits 0–47 for a pool-relative node index and bits 48–63 for a version counter. When publishing an index, use checked `BitField::set` and handle `Overflow<u64>` before attempting the atomic update. Encoding an actual pointer requires a layout supported by the target ABI and address configuration. See [x86-64 five-level paging](https://docs.kernel.org/next/x86/x86_64/5level-paging.html) and [AArch64 tagged addresses](https://docs.kernel.org/next/arch/arm64/tagged-pointers.html).

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

**How it works.** Pack a six-bit `size_class_index`, a ten-bit `allocated_slot_count`, a four-bit `numa_node_id`, and four `page_flags` bits into a single `u32`. During allocation or deallocation, read the size class and occupancy with `BitField::get`. Update the count with `BitField::set` and handle `Overflow<u32>` if the new count exceeds the field width. Size the field for the maximum page occupancy, including the full state.

**Why it matters.** A four-byte descriptor keeps per-page metadata compact and gives the allocator bounded-cost access to its counters and flags. The allocator's free list or bitmap determines the cost of finding an available slot.

## Source-to-capability traceability matrix

**Each source row links to its capability specifications and names the tests that exercise its behavior.**

Source and test links point to the same reviewed Nexus commit. Entries marked **Local only** are present in the reviewed checkout. The final column records pending checks. See individual specifications for requirement-level coverage.

| Source | Specifications | Responsibility | Existing verification | Pending verification |
| --- | --- | --- | --- | --- |
| [nexus-bits/src/lib.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/lib.rs) | [01](01-bit-math-and-layout-validation.md), [02](02-int-enum-contract.md), [03](03-flat-bit-storage.md), [04](04-tagged-union-storage.md), [05](05-error-types-and-diagnostics.md), [06](06-runtime-bitfields-and-flags.md) | Public exports and the `derive` feature gate | [Crate doctest](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/lib.rs) packs and reads an ID using public exports. [`int_enum.rs`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/int_enum.rs) exercises the default-feature macro export. | Verify macro imports are unavailable with default features disabled. |
| [nexus-bits/src/field.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/field.rs) | [01](01-bit-math-and-layout-validation.md), [05](05-error-types-and-diagnostics.md), [06](06-runtime-bitfields-and-flags.md) | Field bounds, masking, extraction, and replacement | [`bitfield_new_full_width`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/tests.rs), [`bitfield_set_preserves_other_bits`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/tests.rs), and [`bitfield_set_overflow_error`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/tests.rs). | Cover endpoint overflow and signed masks in both overflow-checking modes. |
| [nexus-bits/src/flag.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/flag.rs) | [01](01-bit-math-and-layout-validation.md), [05](05-error-types-and-diagnostics.md), [06](06-runtime-bitfields-and-flags.md) | Position checks and single-bit operations | [`flag_new_exceeds_bounds_panics`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/tests.rs) and [`flag_toggle_preserves_other_bits`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/tests.rs). **Local only** · `nexus-bits/tests/flag_contracts.rs` and the invalid-position compile-fail doctest in `nexus-bits/src/flag.rs`. | Publish the local tests before using them as evidence for a pinned revision. |
| [nexus-bits/src/int_enum.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/int_enum.rs) | [02](02-int-enum-contract.md), [03](03-flat-bit-storage.md), [04](04-tagged-union-storage.md) | Conversion trait shared by direct and packed enum reads | [`sparse_roundtrip`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/int_enum.rs) and [`try_from_repr_invalid`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/int_enum.rs). Packed paths are exercised by [`enum_fields_roundtrip`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/derive_struct.rs) and [`instrument_all_variants_roundtrip`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/derive_enum.rs). | Exercise signed enum codes across backing widths in both packed forms. |
| [nexus-bits/src/error.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/error.rs) | [02](02-int-enum-contract.md), [03](03-flat-bit-storage.md), [04](04-tagged-union-storage.md), [05](05-error-types-and-diagnostics.md), [06](06-runtime-bitfields-and-flags.md) | Overflow and unknown-code payloads and formatting | [`overflow_display`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/tests.rs), [`field_overflow_display`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/derive_struct.rs), [`unknown_discriminant_display`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/derive_struct.rs), and [`basic_enum_unknown_discriminant`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/derive_enum.rs). | Compare tagged and field error context, including empty context. |
| [nexus-bits-derive/src/lib.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits-derive/src/lib.rs) | [01](01-bit-math-and-layout-validation.md), [02](02-int-enum-contract.md), [03](03-flat-bit-storage.md), [04](04-tagged-union-storage.md), [05](05-error-types-and-diagnostics.md) | Layout checks, generated APIs, and diagnostics | [`roundtrip`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/int_enum.rs), [`basic_fields_roundtrip`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/derive_struct.rs), [`field_elision.rs`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/field_elision.rs), and [`instrument_all_variants_roundtrip`](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/tests/derive_enum.rs). | Add compile-fail fixtures for overlapping fields, duplicate tags, and invalid `IntEnum` declarations. Add narrowing and omitted-enum regression cases. |

The local `nexus-bits/spec/bitfield.md` and `nexus-bits/spec/flags.md` are prior references for SPEC-06.

## Use case-to-capability matrix

**Use this matrix to find the exact specifications needed for each implementation.**

**Direct** marks a rule or API used in the use case. **Optional** marks an alternate API or additional behavior. A dash means there is no mapping for the design shown.

| Use case | [01 Layout validation](01-bit-math-and-layout-validation.md) | [02 IntEnum](02-int-enum-contract.md) | [03 Flat storage](03-flat-bit-storage.md) | [04 Tagged unions](04-tagged-union-storage.md) | [05 Errors and diagnostics](05-error-types-and-diagnostics.md) | [06 BitField and Flag](06-runtime-bitfields-and-flags.md) |
| --- | --- | --- | --- | --- | --- | --- |
| [Tagged pointers](#1-tagged-pointers-in-lock-free-data-structures) | **Direct** · Keep the 48-bit index and 16-bit version disjoint | **Optional** · Decode node-state codes with `IntEnum::try_from_repr` | **Optional** · Generate getters with `#[bit_storage]` | — | **Direct** · Handle `Overflow<u64>` when publishing an index | **Direct** · Read the index with `BitField::get` and assemble the candidate word with `BitField::set` |
| [Binary protocol headers](#2-zero-copy-parsing-of-binary-protocol-headers) | **Direct** · Match field positions and widths to the wire format | **Optional** · Decode named message or time-in-force codes with `IntEnum::try_from_repr` | **Optional** · Use the generated `Header::from_raw` and getters shown above | **Optional** · Use a tagged `#[bit_storage]` enum if the tag selects a payload layout within the word | **Optional** · Handle `UnknownDiscriminant` from a generated enum getter or `FieldOverflow` from the alternate builder | **Direct** · Decode with `BitField::get` and `Flag::is_set` |
| [Hardware registers](#3-hardware-control-and-status-registers) | **Direct** · Match the documented register bits | **Optional** · Decode status codes with `IntEnum::try_from_repr` | **Optional** · Generate getters for a fixed register image with `#[bit_storage]` | — | **Optional** · Handle `Overflow<u32>` if control updates use checked `BitField::set` | **Direct** · Read fields with `BitField::get` and compute a control bit with `Flag::set_to` |
| [Order and event IDs](#4-high-density-order-and-event-identifiers) | **Direct** · Fit four disjoint fields into 64 bits | **Optional** · Decode engine codes with `IntEnum::try_from_repr` | **Optional** · Build a named packed ID with `#[bit_storage]` | — | **Optional** · Handle `Overflow<u64>` if packing uses checked `BitField::set` | **Direct** · Pack the fields and use `BitField::get` for routing and staleness checks |
| [Compact state machines](#5-compact-state-machines-and-flags) | **Direct** · Keep state and counter fields disjoint | **Optional** · Decode state codes with `IntEnum::try_from_repr` | **Optional** · Generate getters for the shared layout with `#[bit_storage]` | **Optional** · Use a tagged `#[bit_storage]` enum if states carry different packed fields | **Optional** · Handle `Overflow<u16>` if updating counters with checked `BitField::set` | **Direct** · Read with `BitField::get` and replace fields after validating a transition |
| [Allocator and slab headers](#6-custom-allocator-and-slab-page-headers) | **Direct** · Size occupancy to include the full-page count | **Optional** · Decode size-class codes with `IntEnum::try_from_repr` | **Optional** · Generate getters for the fixed page header with `#[bit_storage]` | — | **Direct** · Handle `Overflow<u32>` from checked occupancy insertion | **Direct** · Read with `BitField::get` and update occupancy with `BitField::set` |
