# Capability Spec: Runtime bit fields and flags

- **Spec ID** `CAP-06`
- **Status** Draft
- **Version** 0.1.0

## Layer A: Language-Agnostic Contract

### 1. Purpose & Core Concept

A field or flag descriptor remembers where bits live so callers can reuse the same definition with many integer words. The descriptor precomputes a mask once.

### 2. Structural Attributes & Memory Layout

**RUN-001.** The descriptor is a copyable value. A field stores a start position, width, and positioned mask. A flag stores a bit position and one-bit mask. The caller supplies the word for each operation.

### 3. Construction & Validation Semantics

**RUN-002.** Support constant and runtime construction. A field uses the nonempty bounds of CAP-01. A flag requires `0 <= bit < W`. Independently constructed descriptors may overlap and can be applied in sequence.

### 4. Behavioral Operations

| ID | Operation | Result |
| --- | --- | --- |
| RUN-003 | Read position, width, or mask | Return the corresponding descriptor metadata. |
| RUN-004 | Extract field or get maximum | Extract `(word & mask) >> start`. For maximum use `mask >> start`. Right shift is arithmetic for signed backing, including when the field reaches the sign bit. |
| RUN-005 | Checked insertion | If the input is greater than the returned maximum in the backing type's ordering, return overflow with input and maximum. Otherwise perform truncating insertion. |
| RUN-006 | Truncating insertion or clear | Replace only the selected bits with the low input bits or zero, respectively. |
| RUN-007 | Test, set, clear, toggle, or assign a flag | Test the masked bit. Return the word with the bit set, cleared, inverted, or assigned from a boolean. Preserve the other bits. |

| Given | When | Then |
| --- | --- | --- |
| A `u8` field starting at 4, width 3, and word `0xB1` | Read, then replace with 5 | Read 3, return `0xD1`. |
| The same field and word | Insert 8 with checked insertion | Return `Overflow { value: 8, max: 7 }`. |
| An `i8` field starting at 0, width 4 | Insert −3 into zero, then extract | Insertion returns 13 (`00001101`), and extraction returns 13. |
| An `i8` field starting at 4, width 4 | Query the maximum, then attempt checked insertion of 0 | Maximum is −1. Insertion returns overflow for 0. |
| A flag at bit 7 on an eight-bit word | Toggle twice | Return the original word. |

### 5. Non-Functional & Performance Constraints

**RUN-008.** Each operation has fixed work for a supported integer width. Construction and operations use no heap allocation and return values without modifying caller storage. Shared state updates require caller synchronization.

## Layer B: Rust Target Binding

### 1. Public API & Macro Syntax

Each of the following methods is `pub const fn`, specialized for `T` in `u8/u16/u32/u64/u128/i8/i16/i32/i64/i128`.

| Type | Signatures |
| --- | --- |
| `BitField<T>` | `new(start: u32, len: u32) -> Self`, `start(self) -> u32`, `len(self) -> u32`, `mask(self) -> T`, `max_value(self) -> T` |
| `BitField<T>` | `get(self, val: T) -> T`, `set(self, val: T, field_val: T) -> Result<T, Overflow<T>>`, `set_unchecked(self, val: T, field_val: T) -> T`, `clear(self, val: T) -> T` |
| `Flag<T>` | `new(bit: u32) -> Self`, `bit(self) -> u32`, `mask(self) -> T`, `is_set(self, val: T) -> bool` |
| `Flag<T>` | `set(self, val: T) -> T`, `clear(self, val: T) -> T`, `toggle(self, val: T) -> T`, `set_to(self, val: T, enabled: bool) -> T` |

### 2. Memory, ABI & Performance Guarantees

Both types derive `Clone, Copy, Debug, PartialEq, Eq`. The descriptors use Rust's default representation. The all-ones mask path supports full-width fields without shifting by the word width. These APIs are available in `no_std` with default features disabled.

### 3. Compile-Time vs. Runtime Error Boundary

| Requirement | Trigger | Phase | Contract |
| --- | --- | --- | --- |
| RUN-002 | Invalid position or field width | Constant evaluation or runtime | Constant evaluation fails, or the constructor panics at runtime. See CAP-05 for messages. |
| RUN-005 | Field input compares greater than the maximum | Runtime or constant evaluation | Return `Err(Overflow { value: field_val, max })`. |

**Open gap.** The field constructor adds two `u32` operands before checking their sum. Inputs whose mathematical sum exceeds `u32::MAX` can trigger arithmetic overflow or bypass the intended bounds assertion if overflow checks are disabled.

## Verification Checklist

- [ ] Verify valid and invalid const constructors and runtime boundary cases.
- [ ] Exercise unsigned and signed extraction and insertion, including signed top-bit and full-width cases.
- [ ] Verify mask preservation and flag identities across backing widths.
