---
spec_id: SPEC-03
title: Flat Bit Storage
crate: nexus-bits
status: Draft
version: 0.1.0
satisfies:
  - nexus-bits-derive/src/lib.rs
  - nexus-bits/src/int_enum.rs
  - nexus-bits/src/error.rs
verified_by:
  - nexus-bits/tests/derive_struct.rs
  - nexus-bits/tests/field_elision.rs
---

# SPEC-03: Flat Bit Storage

---

## Layer A: Language-Agnostic Contract (The "What")

### 1. Purpose & Scope

Pack several named values into one integer with a fixed layout, checked construction from field assignments, and typed reads from the packed word.

### 2. Domain Model & Invariants

#### 2.1 Definitions

- A **backing word** is the single fixed-width integer that holds the record.
- A **field** is a range of bits that holds an integer or an enum code. A **flag** is one bit that holds a Boolean.
- A **builder** holds assignments until construction. Each member is either omitted or assigned a value.

#### 2.2 Layout and Construction Invariants

| Req ID | Rule | Invariant |
| --- | --- | --- |
| **FLAT-001** | Shared fixed layout | Every record of a given type uses the same field positions. Fields and flags fit the word and occupy disjoint ranges under [SPEC-01](01-bit-math-and-layout-validation.md). |
| **FLAT-002** | Raw input preserves bits | Raw construction accepts every backing word and preserves unused bits and unassigned enum codes. |
| **FLAT-003** | Last assignment and zero defaults | Each assignment replaces the previous assignment for that member. Omitted fields, false or omitted flags, and unused positions contribute zero to a freshly built word. |
| **FLAT-004** | Checked construction | The draft contract checks assigned values against their field bounds, in declaration order, and reports the first overflow. Enum fields use their integer code. Layer B documents the current cast-before-check limitations. |
| **FLAT-009** | Value semantics | A finished record owns one word. Primitive and flag reads do fixed work per field. The generated packing machinery uses local values without heap allocation. |

### 3. Operations & State Transitions

#### 3.1 Construction and Packing

| Req ID | Operation | Input | Result |
| --- | --- | --- | --- |
| **FLAT-002** | Wrap a raw word | Any backing-word bit pattern | A record holding that exact pattern. |
| **FLAT-003** | Assign a member | Builder, member, and value | The same assignments with that member replaced. |
| **FLAT-004**, **FLAT-005** | Build | Member assignments | Validate before packing. Start from zero, mask each accepted value to its field width, and shift it into position. Set the bits for flags assigned true. A failed check returns field overflow. |

#### 3.2 Reading

| Req ID | Operation | Result |
| --- | --- | --- |
| **FLAT-006** | Read a primitive field or flag | Extract and convert the field, extending the sign bit for a narrow signed field. For a flag, test its bit. |
| **FLAT-007** | Read an enum field | Decode its extracted code under [SPEC-02](02-int-enum-contract.md). When decoding reports absence, return the member name and extracted code. |
| **FLAT-008** | Return the raw word | Return every backing bit unchanged. |

#### 3.3 Worked Example

An order record holds an active flag, a mode from 0 to 7, and a retry count from 0 to 7. Bit 3 is unused.

| Positions | Member | Assigned value | Bits |
| --- | --- | --- | --- |
| 7 | active | true | `1` |
| 6–4 | mode | 5 | `101` |
| 3 | Unused | Zero on build | `0` |
| 2–0 | retries | 1 | `001` |

The builder combines the ranges into `1_101_0_001`, or `0xD1`. The mode getter shifts the word right by four places and keeps the low three bits, giving `101` or 5. A raw word of `0xD9` has the same field values with unused bit 3 set. Reading the raw word still returns `0xD9`.

### 4. Acceptance Criteria (Gherkin)

```gherkin
Feature: Flat bit storage

  Scenario: Reject overlapping members (FLAT-001)
    Given a field occupying bits 4 through 6 and a flag at bit 6
    When the layout is declared
    Then the declaration is rejected for overlap

  Scenario: Build with repeated and omitted assignments (FLAT-003, FLAT-005)
    Given the eight-bit order layout
    When active is set to true and mode is assigned 2 and then 5
    And retries is omitted
    Then the raw word is 0xD0
    And mode is 5 and retries is 0

  Scenario: Report overflow in declaration order (FLAT-004)
    Given the three-bit mode and retries fields, declared in that order
    When retries is assigned 9 and mode is assigned 8
    And the record is built
    Then construction fails for mode with value 8 and maximum 7

  Scenario: Preserve an unused raw bit (FLAT-002, FLAT-006, FLAT-008)
    Given a record created from 0xD9 with unused bit 3 set
    When the fields and raw word are read
    Then active is true, mode is 5, and retries is 1
    And the raw word is 0xD9

  Scenario: Read a narrow signed field (FLAT-006)
    Given a four-bit signed field containing 0b1101
    When it is read as a signed eight-bit integer
    Then the result is -3

  Scenario: Read an omitted enum field (FLAT-003, FLAT-007)
    Given a two-bit field named state whose enum assigns only codes 1 and 2
    When the record is built with state omitted and state is read
    Then the build succeeds
    And the getter reports unknown code 0 for state
```

---

## Layer B: Rust Target Binding (The "How in Rust")

### 1. Public Signatures & Attributes

#### 1.1 Flat Layout Declaration

Apply `#[bit_storage(repr = R)]` to a struct with named fields. The macro is available with the default `derive` feature. `R` is a fixed-width signed or unsigned primitive integer from 8 through 128 bits. The field positions and widths are integer literals parsed as `u32`. Bare fixed-width integer field type names use primitive accessors. Other field types use `IntEnum` and must support the generated casts. The flag attribute generates a Boolean getter and setter.

```rust
use nexus_bits::bit_storage;

#[bit_storage(repr = u8)]
pub struct Order {
    #[field(start = 4, len = 3)]
    mode: u8,
    #[field(start = 0, len = 3)]
    retries: u8,
    #[flag(7)]
    active: bool,
}
```

#### 1.2 Generated Types and Methods

| Receiver | Generated signatures |
| --- | --- |
| `Order` | `pub const fn from_raw(raw: R) -> Self`, `pub const fn raw(self) -> R`, `pub fn builder() -> OrderBuilder` |
| `Order` | `pub const fn member(&self) -> T` for primitives and flags. Flags use `bool`. |
| `Order` | `pub fn member(&self) -> Result<T, nexus_bits::UnknownDiscriminant<R>>` for enum fields. |
| `OrderBuilder` | `pub fn member(self, val: T) -> Self`, `pub fn build(self) -> Result<Order, nexus_bits::FieldOverflow<R>>` |

The generated types inherit the input struct's visibility. The single tuple field also uses that visibility, so a public `Order` can be constructed as `Order(raw)` with the same acceptance behavior as `from_raw`. Generated setters operate on the builder. Truncating replacement on a raw word is provided by [the runtime descriptor API](06-runtime-bitfields-and-flags.md).

### 2. Memory & Performance Guarantees

#### 2.1 What Each Record Stores

The packed type is `#[repr(transparent)]` over `R`, with the same size, alignment, and ABI as the backing integer. The field offsets and masks appear in the generated code and occupy no space in a finished record. `Order` derives `Debug, Clone, Copy, PartialEq, Eq, Hash`. Equality and hashing use the whole backing word, so two records with the same field values can compare unequal if an unused bit differs.

| Stored representation | Bytes per record | Bytes for 1,024 records |
| --- | ---: | ---: |
| Separate `u8` mode, `u8` retries, and `bool` active fields in the example | 3 | 3,072 |
| Packed `Order` backed by `u8` | 1 | 1,024 |

The counts cover finished records. The builder stores one `Option` per member and derives `Debug, Clone, Copy, Default`. It holds temporary assignments and has no fixed size or ABI contract. Its option fields do not become part of the packed word.

```rust
use core::mem::{align_of, size_of};

#[repr(C)]
struct SeparateOrder {
    mode: u8,
    retries: u8,
    active: bool,
}

assert_eq!(size_of::<SeparateOrder>(), 3);
assert_eq!(size_of::<Order>(), size_of::<u8>());
assert_eq!(align_of::<Order>(), align_of::<u8>());

let order = Order::builder().mode(5).retries(1).active(true).build().unwrap();
assert_eq!(order.raw(), 0xD1);
assert_eq!((order.mode(), order.retries(), order.active()), (5, 1, true));

let from_raw = Order::from_raw(0xD9);
assert_eq!(from_raw.raw(), 0xD9);
assert_eq!(from_raw.mode(), order.mode());
assert_ne!(from_raw, order);
```

#### 2.2 What Each Operation Costs

| Operation | Work expressed by the generated code |
| --- | --- |
| Wrap or return a raw word | Construct the single-integer wrapper or return its integer. Both operations skip field validation. |
| Read a primitive or flag | A primitive getter shifts, masks, and casts. A narrow signed getter also extends the sign bit. A flag getter masks and tests one bit. The work is independent of the number of other fields. |
| Read an enum field | Extract the selected code, cast it, and call `try_from_repr`. A derived decoder can compare the code against each declared variant before returning a match or absence. |
| Assign one member | Store the value in that member's option. The setter takes and returns the builder by value. The compiler determines whether this produces a physical copy. |
| Build | Check assigned values in declaration order, then pack the word. The generated work grows with the number of members. An assigned enum value can be converted during validation and again during packing. |

The macro checks layout geometry at expansion and emits constant masks and shifts. The generated storage machinery allocates no heap memory. Manual `IntEnum` conversions can add work or allocations of their own. The packed record reduces stored bytes in the example and adds extraction work when a field is read. The instruction sequence, branch count, and latency depend on the compiler and target.

### 3. Compile-Time vs. Runtime Boundary

| Req ID | Condition | Enforcement phase | Mechanism |
| --- | --- | --- | --- |
| **FLAT-001** | Invalid struct shape, attributes, or geometry | Macro expansion | Reject with the applicable [diagnostic](05-error-types-and-diagnostics.md). |
| **FLAT-004** | `IntEnum::Repr` wider in bytes than the backing word | Rust const evaluation | The generated size assertion fails. |
| **FLAT-004** | An assigned value fails the generated check | Builder execution | Return `FieldOverflow<R>` for the first failing member, with the value cast to `R` and the low mask as `max`. The mask can differ from the signed upper bound. |
| **FLAT-007** | Enum decoding returns absence | Getter execution | Return `UnknownDiscriminant<R>` with the member name and extracted backing value. |

**Current implementation limits.** Narrow signed primitive inputs are checked against both signed bounds before casting. Unsigned inputs and enum codes are compared after casting to `R`. This can discard high bits or reinterpret the value and mask under signed ordering. A full-width mask is −1 in signed backing. Primitive checks are skipped when the field width covers the input type. Getter casts can discard high bits when a primitive destination or an enum representation is narrower than the field. These cases limit the lossless round trips guaranteed by the current implementation.

### 4. Verification Traceability

| Requirements | Existing verification | Pending coverage |
| --- | --- | --- |
| FLAT-001 | Valid layouts in `tests/derive_struct.rs`. | Compile-fail cases for invalid declarations and overlap. |
| FLAT-002–FLAT-008 | `tests/derive_struct.rs` and `tests/field_elision.rs` exercise construction and reads. The worked example verifies raw-bit preservation. | Reverse setter order with two failures, omitted enum codes without a zero variant, and narrowing casts. |
| FLAT-009 | The worked example verifies size and alignment for `u8` backing. | Verify size and alignment across the supported backing types. |
