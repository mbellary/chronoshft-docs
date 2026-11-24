---
spec_id: SPEC-06
title: Runtime Bit Fields and Flags
crate: nexus-bits
status: Draft
version: 0.1.0
satisfies:
  - nexus-bits/src/field.rs
  - nexus-bits/src/flag.rs
  - nexus-bits/src/error.rs
verified_by:
  - nexus-bits/tests/tests.rs
  - nexus-bits/tests/flag_contracts.rs
---

# SPEC-06: Runtime Bit Fields and Flags

---

## Layer A: Language-Agnostic Contract (The "What")

### 1. Purpose & Scope

Define reusable descriptors for reading or changing selected bits in a caller-supplied integer, with identical operation semantics for a valid descriptor built as a constant or at runtime.

### 2. Domain Model & Invariants

#### 2.1 Definitions

- A **word** is a signed or unsigned integer of width `W`. Position 0 is its least significant bit.
- A **field descriptor** holds a start, a width, and a mask with ones at the selected positions.
- A **flag descriptor** holds one bit position and a mask with a single one at that position.
- A **mask** selects positions in a word. For example, the mask `0b0111_0000` selects bits 4, 5, and 6.

#### 2.2 Descriptor Invariants

| Req ID | Rule | Invariant |
| --- | --- | --- |
| **RUN-001** | Reusable value | A descriptor is copyable and keeps only its layout data. Each operation receives the word to read or change. |
| **RUN-002** | Valid geometry | For a field require `0 <= start < W` and `1 <= len <= W − start`. For a flag require `0 <= bit < W`. Validate the range without overflowing. Independently constructed descriptors may select overlapping positions. |
| **RUN-008** | Word returned by value | An update returns the resulting word. The caller owns any assignment or synchronization needed to update shared storage. Work for each operation is bounded for a supported integer width. |

### 3. Operations & State Transitions

#### 3.1 Field Operations

Let `M` be the positioned mask. A right shift fills high bits with zeros for unsigned backing and extends the word's sign for signed backing.

| Req ID | Operation | Formula or result |
| --- | --- | --- |
| **RUN-003** | Read layout | Return the stored start, length, or mask. |
| **RUN-004** | Extract | Return `(word & M) >> start`. This shifts an already masked word. For signed backing, the resulting sign depends on whether the selected range includes the backing word's sign bit and that bit is set. |
| **RUN-004** | Get the comparison maximum | Return `M >> start` in the backing type. This is the unsigned field maximum for unsigned backing. With signed backing, a mask that includes the sign bit shifts to −1. |
| **RUN-005** | Insert with overflow check | Compare the input with the returned maximum using the backing type's ordering. On greater input, return the input and maximum as an overflow. Otherwise perform truncating insertion. |
| **RUN-006** | Insert with truncation | Clear the positions in `M`, shift the input left by `start`, mask it with `M`, and combine the two parts. The field holds the low `len` input bits and all other positions are preserved. |
| **RUN-006** | Clear | Return `word & ~M`. This zeroes the field and preserves every other bit. |

For example, placing −3 in a four-bit field at position 0 of an eight-bit signed word keeps `1101`. The backing sign bit is outside that field, so the result has a clear sign bit and extraction returns 13. If the field instead occupies bits 4 through 7, its mask reaches the sign bit and the returned comparison maximum is −1. Checked insertion uses that maximum, so a value of zero is rejected. These are the signed backing compatibility semantics.

#### 3.2 Flag Operations

In this table `^` means exclusive OR, `&` means bitwise AND, and `~` flips all bits. Each update leaves every bit outside the flag position unchanged.

| Req ID | Operation | Formula or result |
| --- | --- | --- |
| **RUN-003** | Read layout | Return the stored position or mask. |
| **RUN-007** | Test | Return whether `(word & M) != 0`. |
| **RUN-007** | Set, clear, or toggle | Set by OR-ing the word with `M`, clear with `word & ~M`, or toggle with `word ^ M`. |
| **RUN-007** | Assign from Boolean | Apply set for true and clear for false. |

#### 3.3 Worked Example

In `0xB1`, the three-bit mode starts at position 4 and the active flag is at position 7.

| Action | Bits 7 · 6–4 · 3–0 | Result |
| --- | --- | --- |
| Read mode and active | `1 · 011 · 0001` | Mode is 3 and active is true. |
| Replace mode with 5 | `1 · 101 · 0001` | The returned word is `0xD1`. |
| Clear active in the returned word | `0 · 101 · 0001` | The returned word is `0x51`. Mode is still 5. |

The same mode descriptor can read a second word, `0x21`, where it returns 2. Neither word needs its own copy of the descriptor.

### 4. Acceptance Criteria (Gherkin)

```gherkin
Feature: Runtime bit fields and flags

  Scenario: Validate a descriptor at the boundary (RUN-002)
    Given an eight-bit word
    When a field is declared at start 0 with width 8
    Then its mask contains eight one bits
    And a field of width 0 or a flag at bit 8 is rejected

  Scenario: Reuse a layout across two words (RUN-001, RUN-003, RUN-004)
    Given a field at start 4 with width 3
    When it reads words 0xB1 and 0x21
    Then the mask is 0x70 and the results are 3 and 2

  Scenario: Distinguish checked and truncating insertion (RUN-005, RUN-006)
    Given that same field in the unsigned word 0xB1
    When the input 8 is inserted in each mode
    Then checked insertion reports value 8 and maximum 7
    And truncating insertion returns 0x81 with neighboring bits preserved

  Scenario: Read a truncated negative input below the sign bit (RUN-004, RUN-005)
    Given a signed eight-bit word of zero and a field occupying bits 0 through 3
    When -3 is inserted with checking and the field is read
    Then the resulting word and extracted value are both 13

  Scenario: Check a field that includes the sign bit (RUN-004, RUN-005)
    Given a signed eight-bit field occupying bits 4 through 7
    When its maximum is read and insertion of 0 is checked
    Then its maximum is -1
    And insertion returns an overflow with value 0 and maximum -1

  Scenario: Toggle a flag twice and preserve its neighbors (RUN-007, RUN-008)
    Given a flag at bit 7 and a word of 0xD1
    When the flag is toggled and the result is toggled again
    Then the first result is 0x51 and the second result is 0xD1
```

---

## Layer B: Rust Target Binding (The "How in Rust")

### 1. Public Signatures & Attributes

#### 1.1 Supported Types and Methods

The crate exports `BitField` and `Flag`. Each method below is `pub const fn`, with an implementation for `u8/u16/u32/u64/u128/i8/i16/i32/i64/i128`. The APIs are available in `no_std` with default features disabled.

| Type | Signatures |
| --- | --- |
| `BitField<T>` | `new(start: u32, len: u32) -> Self`, `start(self) -> u32`, `len(self) -> u32`, `mask(self) -> T`, `max_value(self) -> T` |
| `BitField<T>` | `get(self, val: T) -> T`, `set(self, val: T, field_val: T) -> Result<T, Overflow<T>>`, `set_unchecked(self, val: T, field_val: T) -> T`, `clear(self, val: T) -> T` |
| `Flag<T>` | `new(bit: u32) -> Self`, `bit(self) -> u32`, `mask(self) -> T`, `is_set(self, val: T) -> bool` |
| `Flag<T>` | `set(self, val: T) -> T`, `clear(self, val: T) -> T`, `toggle(self, val: T) -> T`, `set_to(self, val: T, enabled: bool) -> T` |

#### 1.2 Mask Construction

The field constructor forms the low mask and moves it to the field's start. For a full-width field it uses the all-ones value (`!0`), avoiding a shift by the word width. The flag constructor shifts a single one to the flag position. Both store their positioned masks for use by subsequent operations. In a constant initializer, Rust evaluates the constructor at compile time. A constructor called with runtime positions performs its checks and mask calculation at runtime.

### 2. Memory & Performance Guarantees

#### 2.1 What the Descriptor and Each Word Store

Both descriptor types derive `Clone, Copy, Debug, PartialEq, Eq`. A `BitField<T>` stores two `u32` values and one mask of type `T`. A `Flag<T>` stores one `u32` value and one mask of type `T`. Their default Rust representation permits target-dependent padding and alignment. There is no fixed ABI for either descriptor.

| Instantiation | Observed size on x86-64 Linux | Observed alignment |
| --- | ---: | ---: |
| `BitField<u8>` | 12 bytes | 4 bytes |
| `Flag<u8>` | 8 bytes | 4 bytes |
| `BitField<u64>` | 16 bytes | 8 bytes |
| `Flag<u64>` | 16 bytes | 8 bytes |

These are `size_of` and `align_of` measurements for the reviewed target. The descriptors and the data have separate storage. For example, `[u8; 1024]` holds 1,024 bytes of words. In a layout containing that array and one stored descriptor of each `u8` type, the descriptors account for 20 bytes before any enclosing-struct padding. They describe all 1,024 words. The compiler may embed constant descriptor values directly in the generated code.

```rust
use nexus_bits::{BitField, Flag};

const MODE: BitField<u8> = BitField::<u8>::new(4, 3);
const ACTIVE: Flag<u8> = Flag::<u8>::new(7);

let mut words = [0xB1, 0x21];
assert_eq!(MODE.mask(), 0x70);
assert_eq!((MODE.get(words[0]), MODE.get(words[1])), (3, 2));

words[0] = MODE.set(words[0], 5).unwrap();
assert_eq!(words, [0xD1, 0x21]);
words[0] = ACTIVE.clear(words[0]);
assert_eq!(words, [0x51, 0x21]);
assert_eq!(MODE.get(words[0]), 5);
```

#### 2.2 Work at Construction and on Each Use

| Operation | Work expressed by the implementation |
| --- | --- |
| Construct a descriptor | Check the geometry, calculate the mask once, and store it with the position and, for fields, the length. |
| Read a field | AND with the stored mask, then shift right by the stored start. Mask formation stays outside the read path. |
| Insert with checking | Shift the stored mask right to obtain the comparison maximum, compare the input, then return overflow or perform the truncating insert. The maximum is computed from the mask on each call at source level. |
| Insert with truncation | Clear the old field bits, shift and mask the new bits, then combine the two parts. |
| Clear a field or flag | Invert the stored mask and AND with the word. |
| Test, set, or toggle a flag | Test uses a mask and comparison. Set uses OR, and toggle uses exclusive OR. `set_to` selects set or clear based on the input Boolean. |

The constructors and word operations allocate no heap memory. Reusing a descriptor avoids checking geometry and rebuilding the mask for each word. Methods are marked `#[inline]`, and the compiler may fold constant descriptor values and operations. The source operations define the work to be done. Instruction count, branch behavior, and latency depend on the compiler and target.

### 3. Compile-Time vs. Runtime Boundary

| Req ID | Condition | Enforcement phase | Mechanism |
| --- | --- | --- | --- |
| **RUN-002** | Zero field width or a position outside the backing word | Constant evaluation or runtime | An assertion fails during constant evaluation or panics at runtime. See [SPEC-05](05-error-types-and-diagnostics.md) for the messages. |
| **RUN-005** | Input compares greater than `max_value()` in the backing type | Constant evaluation or runtime | Return `Err(Overflow { value: field_val, max })`. |
| **RUN-006** | Input has bits above the field width and truncating insertion is used | Constant evaluation or runtime | Drop those bits and return the modified word. |

**Current implementation limits.** The field constructor adds `start + len` in `u32` before checking the bound. An extreme endpoint can panic on arithmetic overflow or wrap when overflow checks are disabled. A separate mask-construction edge case occurs for signed backing at `len = W − 1`. For example, in `i8` shifting one left by seven sets the sign bit, yielding −128. Subtracting one then overflows with checks enabled, including during constant evaluation. With overflow checks disabled, the subtraction wraps to 127 before the positioning shift. Thus the current constructor can reject a mathematically valid signed layout.

### 4. Verification Traceability

| Requirements | Existing verification | Pending coverage |
| --- | --- | --- |
| RUN-001–RUN-004 | `tests/tests.rs` and the worked example exercise masking and reuse. Flag constructor doctests cover invalid constant positions. | Endpoint overflow and signed mask construction with length one below the word width in both overflow modes. |
| RUN-005, RUN-006 | `tests/tests.rs` exercises checked insertion, truncation, and neighboring-bit preservation. | Signed top-bit and full-width field cases across backing widths. |
| RUN-007, RUN-008 | `tests/flag_contracts.rs` exercises flag bounds, operations, and preservation. The worked example shows separate descriptor and word storage. | Descriptor sizes outside the measured target are outside this contract. |
