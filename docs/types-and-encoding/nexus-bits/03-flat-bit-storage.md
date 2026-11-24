# Capability Spec: Flat bit storage

- **Spec ID** `CAP-03`
- **Status** Draft
- **Version** 0.1.0

## Layer A: Language-Agnostic Contract

### 1. Purpose & Core Concept

A flat layout packs several named values into one integer. The declaration fixes each field's position so the packed value needs only the backing word.

### 2. Structural Attributes & Memory Layout

**FLAT-001.** One value owns one word. Fields and flags occupy disjoint ranges as specified by [CAP-01](01-bit-math-and-layout-validation.md). A builder holds optional input values separately from the packed representation.

### 3. Construction & Validation Semantics

- **FLAT-002.** Raw construction accepts every backing word and preserves every bit. Validation of enum codes occurs when reading their fields.
- **FLAT-003.** A builder starts empty. Each assignment replaces the prior assignment for that member. A build starts from a zero word. Omitted fields, false flags, and unassigned bits contribute zero.
- **FLAT-004.** Validate assigned values in declaration order and return the first field overflow. For primitive fields, the intended value bounds are the unsigned or signed range of the field and input types. Enum fields use their integer code. See the Rust binding for current limitations.

### 4. Behavioral Operations

| ID | Operation | Result |
| --- | --- | --- |
| FLAT-005 | Build from assignments | Mask each accepted field value before placing it. Set flags assigned true. Return the packed word. |
| FLAT-006 | Read a primitive or flag | Extract the field and convert to its declared type under CAP-01, or test the flag bit. |
| FLAT-007 | Read an enum field | Decode the extracted code and return the variant or an error with the field name and extracted value. |
| FLAT-008 | Return raw storage | Return the backing word unchanged. |

| Given | When | Then |
| --- | --- | --- |
| An unsigned 3-bit field at bit 4 and a flag at bit 7 in a `u8` | Build with field 5, flag true | Raw storage is `0xD0`. |
| The same layout | Build with field 8 | Return overflow with value 8 and max 7. |
| An enum field whose mapping only contains codes 1 and 2 | Build with the field omitted and read it | The word contains 0 and the getter reports an unknown code. |
| Raw storage contains set bits in unused positions | Read the raw word | The unused bits are preserved. |

### 5. Non-Functional & Performance Constraints

**FLAT-009.** Primitive and flag access uses fixed work per field. The generated builder validates and packs by member without dynamic allocation. Packed values own their data and have no synchronization semantics.

## Layer B: Rust Target Binding

### 1. Public API & Macro Syntax

```rust
use nexus_bits::bit_storage;

#[bit_storage(repr = u8)]
pub struct Order {
    #[field(start = 4, len = 3)]
    mode: u8,
    #[flag(7)]
    active: bool,
}

let order = Order::builder().mode(5).active(true).build().unwrap();
assert_eq!(order.raw(), 0xD0);
```

| Receiver | Generated signatures |
| --- | --- |
| `Order` | `pub const fn from_raw(raw: R) -> Self`, `pub const fn raw(self) -> R`, `pub fn builder() -> OrderBuilder` |
| `Order` | Primitive or flag `pub const fn member(&self) -> T` (`bool` for flags). Enum `pub fn member(&self) -> Result<T, nexus_bits::UnknownDiscriminant<R>>`. |
| `OrderBuilder` | `pub fn member(self, val: T) -> Self`, `pub fn build(self) -> Result<Order, nexus_bits::FieldOverflow<R>>` |

`R` is the storage integer and `T` is the member type. The primitive field syntax uses bare fixed-width integer type names. Other field types take the `IntEnum` path. The macro generates the tuple field with the storage type's visibility as declared on the input struct. The truncating `set_unchecked` method belongs to `BitField` in CAP-06.

### 2. Memory, ABI & Performance Guarantees

The packed type is `#[repr(transparent)]` over `R`, sharing its size, alignment, and ABI. It derives `Debug, Clone, Copy, PartialEq, Eq, Hash`. The builder derives `Debug, Clone, Copy, Default` and stores one option per member. Its layout is unspecified.

### 3. Compile-Time vs. Runtime Error Boundary

| Requirement | Trigger | Phase | Contract |
| --- | --- | --- | --- |
| FLAT-001 | Invalid member layout or struct shape | Macro expansion | CAP-05 diagnostic. |
| FLAT-004 | `IntEnum::Repr` wider than the backing word | Rust const evaluation | Width assertion documented in CAP-05. |
| FLAT-004 | First assigned value fails the generated check | Runtime | `FieldOverflow<R>` with field name, value cast to `R`, and low mask as `R`. |
| FLAT-007 | Decoding returns absence | Runtime | `UnknownDiscriminant<R>` with field name and extracted storage value. |

**Open gap.** Narrow signed primitive inputs are checked against signed bounds before casting. Unsigned inputs and enum codes are compared after casting to `R`, using its signedness. Casting can truncate or reinterpret the value before validation. Primitive checks are skipped when the field width covers the input type. Enum decoding casts to `T::Repr`, so extra extracted bits may be discarded. Full-width signed backing can also make an all-ones comparison mask negative. The checked builder therefore has no universal lossless-round-trip guarantee for these combinations.

## Verification Checklist

- [ ] Verify zero defaults, last assignment, first overflow, sign extension, and arbitrary raw round trips.
- [ ] Exercise wider inputs, signed backing, and enum conversion boundaries.
- [ ] Assert size and alignment against the backing type.
