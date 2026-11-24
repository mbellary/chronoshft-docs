# Capability Spec: Bit math and layout validation

- **Spec ID** `CAP-01`
- **Status** Draft
- **Version** 0.1.0

## Layer A: Language-Agnostic Contract

### 1. Purpose & Core Concept

A layout assigns each field a range in a fixed-width integer. With the ranges known ahead of time, a field can be read or replaced with shifts and masks.

### 2. Structural Attributes & Memory Layout

**LAYOUT-001.** A word has width `W` with positions `0..W-1`, starting at the least significant bit. A field occupies `[start, start + len)`. A flag occupies one position. The layout describes bits in an integer value. Byte order is chosen when that value is serialized.

### 3. Construction & Validation Semantics

- **LAYOUT-002.** Require `len > 0`, `start < W`, and `len <= W - start`. Use the subtraction form or widened arithmetic to test bounds without overflowing.
- **LAYOUT-003.** Within one layout, two ranges conflict when `max(start_a, start_b) < min(end_a, end_b)`. A flag participates as a one-bit range. Adjacent fields may share an endpoint. Each tagged variant has its own payload layout and reserves the same tag range.
- **LAYOUT-004.** The low mask `M` contains `len` ones. Use the all-ones word when `len = W`. The positioned mask is `M << start`.

### 4. Behavioral Operations

| ID | Operation | Result |
| --- | --- | --- |
| LAYOUT-005 | Extract `(word, start, len)` | Shift the word right by `start`, then keep the low `len` bits. |
| LAYOUT-006 | Replace `(word, value, start, len)` | Clear the positioned mask in the word and OR in `(value & M) << start`. Bits outside the range stay unchanged. |
| LAYOUT-007 | Interpret a signed `len`-bit pattern | For unsigned extracted pattern `p`, return `p - 2^len` if its top bit is set, otherwise return `p`. The signed range is `-2^(len-1)..=2^(len-1)-1`. |

| Given | When | Then |
| --- | --- | --- |
| Eight-bit word `10110001`, field `[4,7)` | Extract | The result is `011` or 3. |
| The same word and field | Write 5 (`101`) | The word becomes `11010001`. The leading 1 and trailing `0001` stay in place. |
| Four-bit pattern `1101` | Decode as signed | The result is `13 - 16 = -3`. |
| `W=128, start=0, len=128` | Form the mask | Every bit is set, with no shift by 128. |
| `[0,4)` and `[4,8)` | Validate | The ranges are adjacent and accepted. |
| `[0,4)` and flag bit 3 | Validate | The ranges overlap and are rejected. |

### 5. Non-Functional & Performance Constraints

**LAYOUT-008.** Extraction and insertion require fixed work per field and use only integer values. Validation precedes generated shifts. Word transformations provide value semantics and carry no synchronization.

## Layer B: Rust Target Binding

### 1. Public API & Macro Syntax

`#[bit_storage(repr = u64)]` or `#[bit_storage(repr = u64, discriminant(start = 0, len = 4))]` sets the storage type and optional tag range. Use `#[field(start = N, len = M)]` or `#[flag(N)]` on named members. The parser reads positions and widths as `u32` literals. Backing types are the fixed-width signed and unsigned integers from 8 through 128 bits. Runtime descriptors are specified in [CAP-06](06-runtime-bitfields-and-flags.md).

### 2. Memory, ABI & Performance Guarantees

The macro computes layout constants during expansion. Signed primitive getters sign-extend when the field is narrower than the getter type. Otherwise the extracted pattern is cast to the declared type, which can discard high bits when the declared type is narrower than the field.

### 3. Compile-Time vs. Runtime Error Boundary

| Requirement | Trigger | Phase | Contract |
| --- | --- | --- | --- |
| LAYOUT-002 | Zero width or a field, flag, or tag outside the word | Macro expansion | Reject with the corresponding [CAP-05](05-error-types-and-diagnostics.md) diagnostic. |
| LAYOUT-003 | Two members overlap, or a member intersects the tag | Macro expansion | Reject the declaration before emitting a storage API. |

**Open gap.** The source computes several endpoints with `u32` addition before completing validation. Very large literals can overflow, and a macro panic or wrapped endpoint can precede the expected diagnostic. Runtime `BitField::new` has a related gap.

## Verification Checklist

- [ ] Add compile-fail cases for zero and oversized widths, endpoint overflow, and field/flag/tag collisions.
- [ ] Verify masks and signed boundaries for every supported width, including full-width fields.
- [ ] Verify same-word preservation for each replacement vector.
