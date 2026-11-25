---
spec_id: SPEC-02
title: IntEnum Contract
crate: nexus-bits
status: Draft
version: 0.1.0
satisfies:
  - nexus-bits/src/int_enum.rs
  - nexus-bits-derive/src/lib.rs
verified_by:
  - nexus-bits/tests/int_enum.rs
  - nexus-bits/tests/derive_struct.rs
  - nexus-bits/tests/derive_enum.rs
---

# SPEC-02 · IntEnum contract

**Use `IntEnum` to give integer codes names and detect codes with no assigned name.**

---

## Layer A · Language-agnostic contract

**Each named choice has one unique integer code within the range of its representation.**

### 1. Purpose and scope

**Use the mapping for integer codes stored on their own or inside packed fields.**

Give names to selected integer codes and define how to convert between a named variant and its code, including how to handle an unassigned code.

---

### 2. Domain model and invariants

**The integer representation sets the range, and each variant names one code in that range.**

#### 2.1 Definitions

- **Variant.** This is a named choice with no associated payload.
- **Representation.** This is the integer type that carries a code. Its width and signedness determine the available integer range. For example, an unsigned eight-bit representation permits codes 0 through 255.
- **Discriminant.** This is the integer code assigned to a variant. For example, a mapping may assign `Regular = 0` and `Auction = 3`, leaving codes 1 and 2 unassigned.

#### 2.2 Mapping invariants

| Req ID | Rule | Invariant |
| --- | --- | --- |
| **ENUM-001** | Unique codes | Each variant has exactly one code in the representation's range. Distinct variants have distinct codes. Gaps are allowed. |
| **ENUM-002** | Declaration shape | A mapping derived from an enum declaration requires an integer representation and variants with no payload. |
| **ENUM-005** | Packed field integration | A packed field has its own width and validation rules. A code valid for the enum can exceed the field's capacity. |
| **ENUM-006** | Value semantics | Derived conversions return values without allocating memory or modifying the input. |

---

### 3. Operations and state changes

**Encoding returns a code, and decoding looks up the choice assigned to that code.**

#### 3.1 Direct conversion

| Req ID | Operation | Input | Output | Failure condition |
| --- | --- | --- | --- | --- |
| **ENUM-003** | Encode | A declared variant `v` | The assigned integer code of `v`. | Every declared variant has a code. |
| **ENUM-004** | Decode | An integer in the representation's range | The matching variant if the code is assigned. | Return absence for an unassigned code. |

Decoding an encoded variant returns that same variant. Encoding the result of a successful decode returns the original code. For the mapping above, encoding `Auction` yields 3 and decoding 3 yields `Auction`. Decoding 2 yields absence.

#### 3.2 Conversion in packed storage

| Req ID | Operation | Result |
| --- | --- | --- |
| **ENUM-005** | Pack a named variant | Obtain its code and apply the builder's field validation and packing rules. |
| **ENUM-005** | Read a flat field | Decode the extracted code. An unassigned code produces a field-specific error. |
| **ENUM-005** | Project a tagged variant | Check the tag, then decode its enum payload fields. An unassigned payload code produces a field-specific error. |

```text
[Two field bits] -> [Integer code] -> [Named choice or error]
       11                 3                    Auction
       10                 2                    Unknown code
```

For example, code 3 fits a two-bit unsigned field. Reading `0b11` from that field yields `Auction`. Reading `0b10` yields an unknown-code error because the mapping leaves 2 unassigned.

---

### 4. Acceptance criteria

**Check that assigned codes round-trip and unassigned codes produce the expected failure.**

```gherkin
Feature: Integer enum conversion

  Scenario: Reject ambiguous codes (ENUM-001)
    Given two named variants in the same mapping
    When both are assigned code 3
    Then the declaration is rejected

  Scenario: Reject a payload variant in a derived mapping (ENUM-002)
    Given a variant that carries a payload
    When an integer mapping is derived from its declaration
    Then the declaration is rejected

  Scenario: Round-trip a sparse mapping (ENUM-003, ENUM-004)
    Given Regular is assigned 0 and Auction is assigned 3
    When Auction is encoded and the resulting integer is decoded
    Then encoding returns 3 and decoding returns Auction

  Scenario: Decode a gap (ENUM-004)
    Given the only assigned codes are 0 and 3
    When code 2 is decoded
    Then decoding returns absence

  Scenario: Preserve a negative code (ENUM-003, ENUM-004)
    Given a signed representation with Reverse assigned -1
    When Reverse is encoded and the code is decoded
    Then encoding returns -1 and decoding returns Reverse

  Scenario: Reject a valid enum code that exceeds an unsigned packed field (ENUM-005)
    Given an enum variant assigned code 4
    And a two-bit enum field in an unsigned backing word
    When the variant is assigned to that field and the word is built
    Then the build fails for that field with input 4 and maximum 3

  Scenario: Report an unknown code in a flat packed field (ENUM-005)
    Given a two-bit field named mode whose assigned codes are 0 and 3
    When the raw field pattern 0b10 is read
    Then the read fails with field name mode and code 2

  Scenario: Reject an unknown payload code during tagged projection (ENUM-005)
    Given a word with a matching tag and a payload field named mode
    And the payload field contains an unassigned code 2
    When the word is projected as the matching variant
    Then projection fails with field name mode and code 2
```

---

## Layer B · Rust target binding

**Rust exposes a conversion trait and a derive macro that generates it for supported enums.**

### 1. Public signatures and attributes

**Convert with `into_repr` and `try_from_repr`, or let generated packed accessors call them.**

#### 1.1 Trait and derive syntax

```text
pub trait IntEnum: Copy {
    type Repr;
    fn into_repr(self) -> Self::Repr;
    fn try_from_repr(repr: Self::Repr) -> Option<Self>;
}
```

The trait is exported as `nexus_bits::IntEnum`. The derive macro of the same name is available with the default `derive` feature. Apply the derive to an enum whose variants carry no extra data with a single integer `#[repr(T)]`. Supported representations are `u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`, `i32`, `i64`, and `i128`. Rust determines explicit and implicit discriminants. An initial implicit discriminant is 0, and each subsequent implicit discriminant is one greater than the previous one.

In a Cargo binary project, add the following line under `[dependencies]`. Copy any complete Rust example on this page to `src/main.rs` and run `cargo run`.

```toml
nexus-bits = "0.4.4"
```

```rust
use nexus_bits::IntEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntEnum)]
#[repr(u8)]
enum Mode {
    Regular,
    Auction = 3,
}

fn main() {
    assert_eq!(Mode::Regular.into_repr(), 0);
    assert_eq!(Mode::Auction.into_repr(), 3);
    assert_eq!(Mode::try_from_repr(3), Some(Mode::Auction));
    assert_eq!(Mode::try_from_repr(2), None);
}
```

#### 1.2 Generated conversion and packed binding

The derive sets `Self::Repr` to the declared primitive type. `into_repr` casts the variant to that type. `try_from_repr` compares the input with declared variant codes and returns the matching variant or `None`. Both methods generated by the derive are ordinary, non-const trait methods.

A manual implementation must satisfy `Copy`. The trait declaration leaves `Repr` unconstrained. To embed a manual mapping in generated storage, its conversions must obey the round-trip rules and the generated casts must type-check.

Under `#[bit_storage]`, builders use `into_repr` and packed readers use `try_from_repr`. The current code casts to the backing type before checking the field limit and casts the extracted pattern to the enum's representation before decoding. Full-width signed codes can round-trip through a compatible field. Narrow fields use raw masked extraction for enum codes without the sign extension used by signed primitive getters. See [Flat bit storage](03-flat-bit-storage.md) and [Tagged union storage](04-tagged-union-storage.md) for width checks, builder limitations, and the typed variant getter's invariant.

---

### 2. Memory and performance guarantees

**A packed word can hold several short codes, while decoding a code may require comparisons.**

#### 2.1 What occupies memory

A unit-variant enum with a primitive `#[repr(T)]` has the same size and alignment as `T` on the target. For the `Mode` enum above, `#[repr(u8)]` gives each value a size and alignment of one byte. Changing the representation to `u16` gives each value a size of two bytes and the target's `u16` alignment. The derive adds methods only. Each value contains its discriminant, with no per-value lookup table, heap allocation, or pointer. These guarantees apply to the enum itself. A containing struct may add padding, and the layout of the decoder's `Option<Mode>` result is outside this contract.

A field can use fewer bits than a standalone enum. `Mode` uses codes 0 and 3, so two bits per field are enough to store either code. Four standalone `Mode` values in an array occupy four bytes. Four two-bit fields in a packed `u8` occupy one byte in total. Across 1,024 groups of four modes, the stored values take 4,096 bytes as arrays or 1,024 bytes as packed records. These counts cover the stored values only.

| Storage | Bits used per mode | Bytes per group of four |
| --- | ---: | ---: |
| `[Mode; 4]` with `#[repr(u8)]` | 8 | 4 |
| One `u8` with four two-bit fields | 2 | 1 |

The example extends the `Mode` declaration above. With only `second` set to `Auction`, its code `11` sits at bits 3 and 2. The other fields contribute zero, so the byte is `00_00_11_00`. Reading `second` shifts those bits to the bottom of the byte, masks out the other fields, and decodes 3 as `Auction`.

```rust
use core::mem::{align_of, size_of};
use nexus_bits::{IntEnum, bit_storage};

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntEnum)]
#[repr(u8)]
enum Mode {
    Regular,
    Auction = 3,
}

#[bit_storage(repr = u8)]
struct PackedModes {
    #[field(start = 0, len = 2)]
    first: Mode,
    #[field(start = 2, len = 2)]
    second: Mode,
    #[field(start = 4, len = 2)]
    third: Mode,
    #[field(start = 6, len = 2)]
    fourth: Mode,
}

fn main() {
    let packed = PackedModes::builder().second(Mode::Auction).build().unwrap();

    assert_eq!(size_of::<Mode>(), 1);
    assert_eq!(align_of::<Mode>(), 1);
    assert_eq!(size_of::<[Mode; 4]>(), 4);
    assert_eq!(size_of::<PackedModes>(), 1);
    assert_eq!(align_of::<PackedModes>(), align_of::<u8>());
    assert_eq!(packed.raw(), 0b00_00_11_00);
    assert_eq!(packed.second().unwrap(), Mode::Auction);
}
```

#### 2.2 What conversion costs

| Operation | Work expressed by the implementation |
| --- | --- |
| `Mode::Auction.into_repr()` | One primitive cast to `u8`. The derive performs no lookup or allocation. |
| `Mode::try_from_repr(0)` | The generated decoder matches the first declared code. |
| `Mode::try_from_repr(3)` or `Mode::try_from_repr(2)` | The generated decoder can inspect both declared codes. It returns `Some(Auction)` for 3 and `None` for 2. |
| `packed.second()` | Shift, mask, and cast the extracted field before decoding it. The getter produces a structured error if decoding returns `None`. |
| Build the packed record | Convert the assigned enum values, check their field limits, then pack their bits. The builder may convert an assigned enum value during validation and again during packing. |

For an enum with `N` variants, the generated decoder has one equality guard per variant. A late match or unknown code can require up to `N` comparisons in that source-level implementation. The derived encode and decode methods allocate no heap memory. Generated code is marked `#[inline]`. The compiler chooses whether to inline and how to lower the comparisons, so the contract gives no fixed instruction count, branch count, or latency. The storage savings in the example come with extraction and decoding work each time a packed field is read.

---

### 3. Compile-time and runtime boundary

**Compilation checks the enum declaration, and each decode handles the code actually stored.**

| Req ID | Condition | Enforcement phase | Mechanism |
| --- | --- | --- | --- |
| **ENUM-001** | Duplicate code or code outside the representation range | Rust compilation | Rust rejects the enum declaration. |
| **ENUM-002** | Derive applied to a non-enum, unsupported or missing representation, or a variant with fields | Macro expansion | Emits the applicable [derive diagnostic](05-error-types-and-diagnostics.md). Malformed representation syntax can produce a parser diagnostic. |
| **ENUM-002** | Enum fails the `Copy` bound | Rust compilation | Rust rejects the generated trait implementation. |
| **ENUM-004** | Direct conversion receives an unassigned code | Runtime | `try_from_repr` returns `None`. |
| **ENUM-005** | Assigned code fails the generated field limit check | Builder execution | Returns `FieldOverflow<R>` with the field name, reported input, and mask-derived maximum. |
| **ENUM-005** | Unassigned code in a flat getter or during tagged projection after a matching tag | Runtime | Returns `UnknownDiscriminant<R>` with the payload field name and extracted value in backing type `R`. |

### 4. Verification traceability

**Existing tests cover direct mappings and packed fields, with more rejection cases still pending.**

| Requirements | Existing verification | Pending coverage |
| --- | --- | --- |
| ENUM-001, ENUM-002 | Rust declaration and trait checks. | Compile-fail fixtures for invalid derive targets, representation attributes, payload variants, duplicate codes, and missing `Copy`. |
| ENUM-003, ENUM-004 | `tests/int_enum.rs` covers round trips, sparse and unknown codes, signed codes, and 128-bit representations. | Direct assertion for implicit discriminant progression. |
| ENUM-005 | `tests/derive_struct.rs` and `tests/derive_enum.rs` exercise embedded enum fields, overflow, and unknown-code handling. | Explicit signed-code and representation-width combinations in both packed forms. |
| ENUM-006 | The generated implementation uses casts, comparisons, and value returns. The worked example asserts enum and packed size, alignment, and round-trip decoding. | — |
