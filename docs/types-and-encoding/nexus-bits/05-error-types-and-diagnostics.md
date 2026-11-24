# Capability Spec: Error types and diagnostics

- **Spec ID** `CAP-05`
- **Status** Draft
- **Version** 0.1.0

## Layer A: Language-Agnostic Contract

### 1. Purpose & Core Concept

Report which layout rule, field value, or integer code prevented an operation from succeeding.

### 2. Structural Attributes & Memory Layout

- **ERR-001.** Overflow carries the reported input and maximum. A field overflow adds the member name.
- **ERR-002.** Unknown discriminant carries the extracted code and a context name. A tagged storage failure uses the tag context for an unrecognized or mismatched tag and the member name for a payload failure.

### 3. Construction & Validation Semantics

**ERR-003.** Reject malformed static declarations during compilation. Dynamic range and code errors return structured values at the operation that validates the input. CAP-03 and CAP-04 define which fields are checked and in what order.

### 4. Behavioral Operations

| ID | Operation | Result |
| --- | --- | --- |
| ERR-004 | Format an overflow | Render the input and maximum, adding the field prefix when present. |
| ERR-005 | Format a decoding failure | Render the code, adding the context prefix when the name is nonempty. |

| Given | When | Then |
| --- | --- | --- |
| Field `mode` has width 3 and the input is 8 | Build in unsigned storage | Return a field overflow with `value=8` and `max=7`. |
| A tagged word has unmapped tag 3 | Query its kind | Return unknown discriminant with field `__discriminant` and value 3. |
| An empty context and unknown code 3 | Format the error | Render `unknown discriminant: 3`. |

### 5. Non-Functional & Performance Constraints

**ERR-006.** Creating an error value uses the payload and a static context string. Formatting writes to the caller's formatter.

## Layer B: Rust Target Binding

### 1. Public API & Macro Syntax

```rust
pub struct Overflow<T> {
    pub value: T,
    pub max: T,
}

pub struct FieldOverflow<T> {
    pub field: &'static str,
    pub overflow: Overflow<T>,
}

pub struct UnknownDiscriminant<T> {
    pub field: &'static str,
    pub value: T,
}
```

Each derives `Debug, Clone, Copy, PartialEq, Eq` subject to the corresponding bounds on `T`. Each implements `core::fmt::Display` when `T: Display`.

| Error | Exact display pattern |
| --- | --- |
| `Overflow` | `value {value} exceeds max {max}` |
| `FieldOverflow` | `field '{field}': {overflow}` |
| `UnknownDiscriminant` with empty field | `unknown discriminant: {value}` |
| `UnknownDiscriminant` with a field | `field '{field}': unknown discriminant {value}` |

Builder payloads are stored as the backing type. For signed primitive builder overflow, `max` is the low mask cast to the backing type. The upper signed bound used by the comparison can differ from that value.

### 2. Memory, ABI & Performance Guarantees

These types use the default Rust representation. A field name borrows static storage. The error types have no `core::error::Error` implementation in this baseline.

### 3. Compile-Time vs. Runtime Error Boundary

The table records source-owned message bodies. Braces designate substituted values. Rust and the parser control error headers, spans, and syntax diagnostics.

| Requirement | Trigger | Enforcement phase | Message body |
| --- | --- | --- | --- |
| ENUM-002 | Derive on a non-enum | Macro error | `IntEnum can only be derived for enums` |
| ENUM-002 | Missing enum repr | Macro error | `IntEnum requires a #[repr(u8/u16/u32/u64/i8/i16/i32/i64)] attribute` |
| ENUM-002 | Unsupported enum repr | Macro error | `IntEnum requires a primitive integer repr (u8..u128, i8..i128)` |
| ENUM-002 | Enum variant with fields | Macro error | `IntEnum variants cannot have fields` |
| LAYOUT-002 | Unknown storage argument | Macro error | `` expected `repr` or `discriminant` `` |
| LAYOUT-002 | Missing storage repr | Macro error | `` bit_storage requires `repr = ...` `` |
| LAYOUT-002 | Unsupported storage repr | Macro error | `repr must be an integer type` |
| LAYOUT-002 | Unknown range argument | Macro error | `` expected `start` or `len` `` |
| LAYOUT-002 | Missing range input | Macro error | `` missing `start` `` or `` missing `len` `` |
| LAYOUT-002 | Zero length | Macro error | `len must be > 0` |
| LAYOUT-002 | Field outside backing word | Macro error | `field exceeds {bits} bits (start {start} + len {len} = {end})` |
| LAYOUT-002 | Flag outside backing word | Macro error | `flag bit {bit} exceeds {bits} bits` |
| LAYOUT-003 | Overlapping members | Macro error | `field '{later}' overlaps with '{earlier}'` |
| FLAT-001 | Union input | Macro error | `bit_storage cannot be applied to unions` |
| FLAT-001 | Struct without named fields | Macro error | `bit_storage requires named fields` |
| FLAT-001 | Struct with a discriminant declaration | Macro error | `discriminant is only valid for enums` |
| FLAT-001, TAG-001 | Missing member attribute | Macro error | `field requires #[field(start = N, len = M)] or #[flag(N)] attribute` |
| FLAT-004 | Wider enum representation in a flat field | Const assertion | `IntEnum repr type is wider than storage repr — values may be truncated` |
| TAG-001 | Tuple variant | Macro error | `tuple variants not supported, use named fields` |
| TAG-001 | Member overlaps tag | Macro error | `field '{member}' overlaps with discriminant` |
| TAG-002 | Missing discriminant range | Macro error | `bit_storage enum requires discriminant: #[bit_storage(repr = T, discriminant(start = N, len = M))]` |
| TAG-002 | Discriminant outside backing word | Macro error | `discriminant exceeds {bits} bits (start {start} + len {len} = {end})` |
| TAG-002 | Missing variant tag | Macro error | `enum variant requires #[variant(N)] attribute` |
| TAG-002 | Tag exceeds field capacity | Macro error | `variant discriminant {value} exceeds max {max} for {len}-bit field` |
| TAG-002 | Duplicate tag | Macro error | `duplicate discriminant {value}: already used by '{first_variant}'` |
| TAG-002 | Tag field wider than 64 bits with otherwise valid declaration | Macro panic | `discriminant length must be <= 64 bits (got {len})` |
| RUN-002 | Zero runtime field width | Const assertion or runtime panic | `field length must be > 0` |
| RUN-002 | Runtime field outside word with a representable endpoint | Const assertion or runtime panic | `field exceeds integer bounds` |
| RUN-002 | Runtime flag outside word | Const assertion or runtime panic | `bit position exceeds integer bounds` |

The missing-repr message predates the 128-bit support. CAP-01 and CAP-06 document endpoint overflow cases that can bypass or precede the expected diagnostic.

## Verification Checklist

- [ ] Assert field names, values, masks, and display strings, including signed and empty-context cases.
- [ ] Capture compile-fail cases for every macro-owned diagnostic and distinguish macro panics from emitted compile errors.
- [ ] Cover constant evaluation and runtime constructor failures.
