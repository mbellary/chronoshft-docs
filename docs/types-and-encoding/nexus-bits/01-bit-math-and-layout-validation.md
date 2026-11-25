---
spec_id: SPEC-01
title: Bit Math and Layout Validation
crate: nexus-bits
status: Draft
version: 0.1.0
satisfies:
  - nexus-bits-derive/src/lib.rs
  - nexus-bits/src/field.rs
  - nexus-bits/src/flag.rs
verified_by:
  - nexus-bits/tests/derive_struct.rs
  - nexus-bits/tests/derive_enum.rs
  - nexus-bits/tests/tests.rs
  - nexus-bits/tests/flag_contracts.rs
---

# SPEC-01 · Bit math and layout validation

**Put a value into a fixed group of bits and preserve the bits around it.**

---

## Layer A · Language-agnostic contract

**The contract defines where fields fit and how to read, replace, or clear them.**

### 1. Purpose and scope

**Use this contract when several values share one fixed-width integer.**

Define the bit ranges, layout checks, and arithmetic needed to pack and unpack fields in a fixed-width integer.

---

### 2. Domain model and invariants

**A layout assigns each field a valid range of positions within the same word.**

#### 2.1 Definitions

- **Container `C`.** This is a word of width `W`, where `W` is 8, 16, 32, 64, or 128. Bit 0 is the least significant bit, with value 1. Each position to its left has twice the value of the previous position. A container is treated as an unsigned bit pattern for raw extraction. The byte format determines the order when the word is written to or read from bytes.
- **Field `F`.** It occupies the half-open interval `[s, e)`. The start `s` is included, the length `l` counts the occupied positions, and the end `e = s + l` is excluded. For example, `[4, 7)` occupies bits 4, 5, and 6.
- **Low mask `L`.** This is `2^l − 1`. It has `l` ones starting at bit 0. The positioned field mask `M = L << s` moves those ones to the field. For an 8-bit word and `[4, 7)`, `L = 0b0000_0111` and `M = 0b0111_0000`. For a full-width field, both masks are the all-ones word.

#### 2.2 Layout invariants

| Req ID | Rule | Mathematical invariant |
| --- | --- | --- |
| **LAYOUT-001** | Nonzero length | A field must occupy at least one bit. Require `l >= 1`. |
| **LAYOUT-002** | Container bounds | Require `0 <= s < W` and `l <= W − s`. Evaluate the bounds without overflowing before computing the endpoint. |
| **LAYOUT-003** | Non-overlap within a layout | Distinct fields in one layout are disjoint when `max(s1, s2) >= min(e1, e2)`. In a tagged layout, include the tag range when validating each payload. Payloads of different variants may reuse positions. |
| **LAYOUT-004** | Primitive type width | For a field read as a scalar integer of width `T`, require `l <= T` so that reading a raw field preserves every field bit. |
| **LAYOUT-005** | Minimum signed width | An ordinary signed primitive field requires `l >= 2`. A one-bit signed field would require an explicit sign-only contract. |
| **LAYOUT-006** | Boolean width | A flag occupies one bit. The code 0 represents false and 1 represents true. |

LAYOUT-004 and LAYOUT-005 describe the draft target from the sample. Layer B records the behavior of the current Rust implementation.

---

### 3. Operations and state changes

**Select the field with a mask and use shifts to move its bits into or out of position.**

#### 3.1 Value bounds and encoding

| Req ID | Operation | Input | Valid domain | Transformation | Failure condition |
| --- | --- | --- | --- | --- | --- |
| **MATH-001** | Check and encode unsigned | Value `v`, length `l` | `0 <= v <= 2^l − 1` | Encode `v` unchanged. | Reject a value above the upper bound before packing. |
| **MATH-002** | Check and encode signed | Value `v`, length `l` | `−2^(l−1) <= v <= 2^(l−1) − 1` | Encode the low `l` bits of the two's complement value, equivalent to `v mod 2^l` with a nonnegative remainder. | Reject a value outside either bound before packing. |

#### 3.2 Bitwise insertion and extraction

The formulas use fixed-width words. `&` is bitwise AND, `OR` is bitwise OR, `XOR` is exclusive OR, and `~` flips every bit of a word. `<<` moves bits left and `>>` moves bits right.

| Req ID | Operation | Formula | Postcondition |
| --- | --- | --- | --- |
| **MATH-003** | Extract raw | `raw = (C >> s) & L`, with a logical right shift | The field starts at position 0 and `0 <= raw <= L`. |
| **MATH-004** | Sign extend | Let `S = 2^(l−1)`. Interpret `(raw XOR S) − S` as a mathematical integer. | The result represents the same signed value in a destination of width `T >= l`. The new high bits copy the field's sign bit. |
| **MATH-005** | Insert masked | `C' = (C & ~M) OR ((v & L) << s)` | The selected positions hold the low `l` bits of `v`. All other positions preserve their original values. |
| **MATH-006** | Clear field | `C' = C & ~M` | Every selected bit is zero. All other positions preserve their original values. |

#### 3.3 Worked example

An eight-bit word starts as `0b1011_0001`. Its field occupies bits 4 through 6. The bit at position 7 and the low four bits belong to the rest of the word.

```text
Bit position      7 | 6 5 4 | 3 2 1 0
Before            1 | 0 1 1 | 0 0 0 1
Mask              0 | 1 1 1 | 0 0 0 0
After clearing    1 | 0 0 0 | 0 0 0 1
After inserting   1 | 1 0 1 | 0 0 0 1
```

| Action | Bits 7 · 6–4 · 3–0 | Result |
| --- | --- | --- |
| Read the field | `1 · 011 · 0001` | `011` is 3 because the 2 and 1 positions are set. |
| Replace it with 5 | `1 · 101 · 0001` | `101` sets the 4 and 1 positions. The word becomes `0xD1`. |
| Clear the field | `1 · 000 · 0001` | The word becomes `0x81`. |

---

### 4. Acceptance criteria

**Check boundary layouts, signed reads, and preservation of neighboring bits.**

```gherkin
Feature: Bit-field layout validation and arithmetic

  Scenario: Reject an empty field (LAYOUT-001)
    Given an 8-bit container and a field at start 0 with length 0
    When the layout is validated
    Then validation reports an invalid length

  Scenario: Reject a field past the end of the word (LAYOUT-002)
    Given a 16-bit container and a field at start 12 with length 5
    When the layout is validated
    Then validation reports that the exclusive endpoint 17 exceeds 16

  Scenario: Reject an overlapping flag (LAYOUT-003, LAYOUT-006)
    Given a field A occupying [0, 8) and a flag B at bit 7
    When their shared layout is validated
    Then validation reports an overlap identifying A and B

  Scenario: Allow adjacent fields (LAYOUT-003)
    Given fields occupying [0, 8) and [8, 16) in a 16-bit container
    When the layout is validated
    Then validation succeeds

  Scenario: Reject incompatible primitive widths (LAYOUT-004, LAYOUT-005)
    Given a 9-bit field read as an 8-bit unsigned integer
    And a separate 1-bit field declared as an ordinary signed integer
    When each layout is validated
    Then each declaration is rejected by its respective width rule

  Scenario: Check an unsigned value before packing (MATH-001)
    Given a field with length 3
    When 7 and 8 are checked separately
    Then 7 is accepted and 8 is rejected

  Scenario: Check both signed boundaries (MATH-002)
    Given a signed field with length 4
    When -9, -8, 7, and 8 are checked separately
    Then -8 and 7 are accepted, and -9 and 8 are rejected

  Scenario: Sign extend a negative field (MATH-003, MATH-004)
    Given a 4-bit field containing 0b1101
    When the bits are extracted raw and then interpreted as signed
    Then the raw value is 13 and the signed value is -3
    And the signed 8-bit pattern is 0b1111_1101

  Scenario: Truncate an insertion and preserve neighbors (MATH-005, MATH-006)
    Given a 16-bit container set to 0xFFFF and a field occupying [4, 8)
    When 0x10 is inserted into the field with masking
    Then the word is 0xFF0F
    And clearing that field in the original word also yields 0xFF0F

  Scenario: Handle a full-width field (LAYOUT-002, MATH-003)
    Given a 128-bit field at position 0 in a 128-bit container
    When the field mask is formed and the field is extracted
    Then the mask has 128 one bits and extraction returns the full word
```

---

## Layer B · Rust target binding

**Rust checks declared layouts during macro expansion and provides descriptors for individual fields.**

### 1. Public signatures and attributes

**Declare field positions with attributes or construct a reusable `BitField`.**

#### 1.1 Field attribute syntax

Use `#[bit_storage(repr = R)]` for a struct and `#[bit_storage(repr = R, discriminant(start = S, len = L))]` for a tagged enum. The metavariables `S`, `L`, and `N` denote integer literals parsed as `u32`.

```text
#[field(start = S, len = L)]
#[flag(N)]
```

Apply one of these attributes to each named member. A primitive field uses a bare fixed-width integer type name. A flag's generated getter and builder setter use `bool`. Other field types use the `IntEnum` binding. The current macro accepts the fixed-width signed and unsigned integer types through 128 bits as backing types. The signed backing representation still denotes a word with the same bit numbering.

Primitive and flag getters are `pub const fn member(&self) -> T`, where `T` is the field's type or `bool`. Builder validation occurs in `build(self) -> Result<Packed, nexus_bits::FieldOverflow<R>>`. For a standalone descriptor over a caller-owned word, the public truncating and clearing operations are `BitField::<R>::set_unchecked(self, word: R, value: R) -> R` and `BitField::<R>::clear(self, word: R) -> R`. The runtime descriptor's arithmetic shift and signed overflow comparison are specified in [Runtime bit fields and flags](06-runtime-bitfields-and-flags.md).

#### 1.2 Compile-time mask and shift evaluation

The macro uses the equivalent of the following constant expression, after validating the range. The full-width case uses all ones for either signed or unsigned backing. Partial masks are computed as `u128` before conversion.

```text
const LOW_MASK: Repr = if LEN == Repr::BITS {
    !0 as Repr
} else {
    ((1u128 << LEN) - 1) as Repr
};
```

A generated signed primitive getter first casts the extracted pattern to the declared type. For `LEN < T::BITS`, it shifts left by `T::BITS - LEN`, then right by the same amount, copying the sign bit into the high positions. For equal widths the cast itself delivers the signed value. The draft width rule excludes a narrower destination because the cast then drops high bits.

---

### 2. Memory and performance guarantees

**A finished packed record stores one integer, and each field read uses a fixed amount of work.**

#### 2.1 What occupies memory

The worked example in Layer A uses three parts of the same byte. Give them names: an `active` flag at bit 7, a `mode` at bits 4 through 6, and a `retries` count at bits 0 through 3. The flag needs one bit, mode needs three, and retries needs four. Together they fill one byte.

| Stored layout | Bytes per record | Bytes for 1,024 records |
| --- | ---: | ---: |
| Separate `bool`, `u8` mode, and `u8` retries fields in the example below | 3 | 3,072 |
| All three values packed in one `u8` | 1 | 1,024 |

The generated record is `#[repr(transparent)]` over its backing integer. It occupies the same bytes, has the same alignment (which addresses can hold it), and follows the same application binary interface (ABI) rules for passing values as that integer. The macro checks the field ranges and overlaps during expansion and embeds the positions and masks in the generated code. A packed record therefore carries only the word, with no per-record offsets or masks. The table counts completed records. Builders hold assignments separately, and a reusable `BitField` descriptor holds its start, length, and mask separately from the word it operates on.

In a Cargo binary project, add the following line under `[dependencies]`. Save the Rust example as `src/main.rs` and run `cargo run`.

```toml
nexus-bits = "0.4.4"
```

```rust
use core::mem::{align_of, size_of};
use nexus_bits::{BitField, bit_storage};

#[repr(C)]
struct Separate {
    active: bool,
    mode: u8,
    retries: u8,
}

#[bit_storage(repr = u8)]
struct Control {
    #[flag(7)]
    active: bool,
    #[field(start = 4, len = 3)]
    mode: u8,
    #[field(start = 0, len = 4)]
    retries: u8,
}

const MODE: BitField<u8> = BitField::<u8>::new(4, 3);

fn main() {
    let control = Control::builder().active(true).mode(3).retries(1).build().unwrap();
    assert_eq!(size_of::<Separate>(), 3);
    assert_eq!(size_of::<Control>(), 1);
    assert_eq!(align_of::<Control>(), align_of::<u8>());
    assert_eq!(control.raw(), 0xB1);
    assert_eq!(control.mode(), 3);
    assert_eq!(MODE.set_unchecked(control.raw(), 5), 0xD1);
    assert_eq!(MODE.clear(control.raw()), 0x81);
}
```

#### 2.2 Work per operation

| Operation | Work expressed by the implementation |
| --- | --- |
| Read `control.mode()` | Shift right by 4 and keep the low three bits with mask `0b111`. The getter uses the declared positions directly. |
| Read a narrow signed primitive field | Extract the pattern, then use a left shift and an arithmetic right shift to propagate its sign bit into the higher bits of the result. |
| Replace mode with `MODE.set_unchecked(word, 5)` | Clear the old mode bits with the complement of mask `0x70`, then shift and mask the new value and combine it with the preserved bits. |
| Clear mode with `MODE.clear(word)` | AND the word with the complement of mask `0x70`. The flag and retry bits are preserved. |
| Replace mode with `MODE.set(word, value)` | Derive the maximum from the cached mask and compare the input with it. Return overflow on failure, or perform the same masked replacement on success. |
| Build `Control` | Check the assigned values that require range validation, then combine their masked bits into one word. The work grows with the number of members in the layout. |

Primitive reads, replacements, and clears use a fixed amount of integer work per field and allocate no heap memory. Reading a packed field adds extraction work to the access, while the smaller record reduces the number of bytes stored. These are source-level properties. The compiler and target determine the final instructions and timing.

Full-width masks use a dedicated all-ones case. For example, a 128-bit field at offset zero uses `u128::MAX` as its unsigned mask. Computing that mask takes no shift by 128. A field ending exactly at the word boundary still starts at a position smaller than the word width, so its position is a valid shift count.

---

### 3. Compile-time and runtime boundary

**Macro layout checks run during compilation, while builders check supplied values when called.**

| Req ID | Condition | Enforcement phase | Mechanism |
| --- | --- | --- | --- |
| **LAYOUT-001** | `len == 0` | Macro expansion | Emits `len must be > 0`. |
| **LAYOUT-002** | Field exceeds the backing word, with a representable endpoint | Macro expansion | Emits `field exceeds {bits} bits (start {start} + len {len} = {end})`. Tag and flag diagnostics are listed in [Error types and diagnostics](05-error-types-and-diagnostics.md). |
| **LAYOUT-003** | Members overlap or a payload member intersects the tag | Macro expansion | Emits `field '{later}' overlaps with '{earlier}'` or `field '{member}' overlaps with discriminant`. |
| **LAYOUT-004** | `len > T::BITS` | Draft target at macro expansion | Current macro permits the declaration and the getter can truncate the extracted bits. |
| **LAYOUT-005** | Signed primitive field with `len == 1` | Draft target at macro expansion | Current macro permits the declaration and decodes the two patterns as 0 and −1. |
| **LAYOUT-006** | Declaring a Boolean flag | Macro expansion and Rust type checking | Use `#[flag(N)]` for a one-bit range. A `bool` annotated with `#[field(...)]` takes the `IntEnum` path and fails type checking. |
| **MATH-001**, **MATH-002** | Builder rejects an assigned field value | Builder execution | Returns `Err(FieldOverflow { field, overflow: Overflow { value, max } })`. The payload holds the value cast to the backing type and the low mask as `max`. |
| **MATH-005**, **MATH-006** | Replacing without a range check or clearing | Constant evaluation or runtime | `BitField::set_unchecked` truncates and `BitField::clear` zeroes the field. Both return the resulting word directly. |

Two implementation gaps affect enforcement. The macro and `BitField::new` compute endpoints using `u32` addition in some validation paths, so extreme inputs can panic or wrap before the intended bounds check. Generated builders compare unsigned inputs after casting to the backing type in some layouts. A narrowing cast or signed comparison can defeat the MATH-001 bound. Narrow signed primitive fields are checked against their signed bounds before casting.

### 4. Verification traceability

**The tests cover basic packing and preservation, with overflow and invalid layout cases still pending.**

| Requirements | Existing verification source | Pending coverage |
| --- | --- | --- |
| LAYOUT-001–LAYOUT-006 | Flag constructor compile-fail doctests in `nexus-bits/src/flag.rs` cover invalid flag positions. | Macro compile-fail fixtures for overlap, invalid endpoints including arithmetic overflow, and the draft primitive width rules. |
| MATH-001–MATH-004 | `nexus-bits/tests/derive_struct.rs` and `nexus-bits/tests/derive_enum.rs` exercise generated packing, overflow, and signed decoding. | Cross-width range checks, including unsigned inputs cast to narrower or signed backing types. |
| MATH-005–MATH-006 | `nexus-bits/tests/tests.rs` exercises runtime field replacement and clearing. `nexus-bits/tests/flag_contracts.rs` covers single-bit preservation in the reviewed checkout. | Verify preservation and full-width masks across all supported backing widths. |
