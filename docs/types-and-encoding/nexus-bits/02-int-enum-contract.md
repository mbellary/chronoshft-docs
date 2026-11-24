# Capability Spec: IntEnum contract

- **Spec ID** `CAP-02`
- **Status** Draft
- **Version** 0.1.0

## Layer A: Language-Agnostic Contract

### 1. Purpose & Core Concept

An integer-backed enum gives names to selected values of a fixed-width integer and supports conversion in both directions.

### 2. Structural Attributes & Memory Layout

**ENUM-001.** Every named variant has a unique integer code in the representation's range. Codes may be sparse. A copyable enum value carries the selected variant and has no associated payload.

### 3. Construction & Validation Semantics

**ENUM-002.** Validate the representation and unit-variant shape when declaring the mapping. Signed representations allow negative codes. The representation width and the width of a packed field are independent.

### 4. Behavioral Operations

| ID | Operation | Result |
| --- | --- | --- |
| ENUM-003 | Encode a variant | Return its assigned code. |
| ENUM-004 | Decode a code | Return the corresponding variant if present, otherwise report absence. |
| ENUM-005 | Embed a mapping in a packed field | Apply the enclosing storage capability's validation and error rules. |

| Given | When | Then |
| --- | --- | --- |
| Codes 0 and 3 | Decode 3, then encode | The result is 3. |
| Codes 0 and 3 | Decode 2 | The result is absent. |
| Signed code −1 | Encode and decode | The result is the original variant. |

### 5. Non-Functional & Performance Constraints

**ENUM-006.** Derived conversion uses integer comparisons and value returns without heap allocation. The generated decoder can compare against each declared code.

## Layer B: Rust Target Binding

### 1. Public API & Macro Syntax

```rust
pub trait IntEnum: Copy {
    type Repr;
    fn into_repr(self) -> Self::Repr;
    fn try_from_repr(repr: Self::Repr) -> Option<Self>;
}
```

Apply `#[derive(Clone, Copy, IntEnum)]` and a single integer `#[repr(T)]` to a unit-variant enum. The derive accepts `u8/u16/u32/u64/u128` and `i8/i16/i32/i64/i128`. Explicit and implicit Rust discriminants are supported. With a manual implementation, the trait itself places no bound on `Repr`. Packed storage additionally requires the generated casts and associated conversions to type-check. A manual mapping must round-trip to serve as a packed field.

### 2. Memory, ABI & Performance Guarantees

The derive emits a trait implementation. The enum's own representation attribute governs its ABI. The derive supplies neither `Copy` nor an additional wrapper.

### 3. Compile-Time vs. Runtime Error Boundary

| Requirement | Trigger | Phase | Contract |
| --- | --- | --- | --- |
| ENUM-002 | Wrong input kind, missing or unsupported representation, or a payload variant | Macro expansion | [CAP-05](05-error-types-and-diagnostics.md) diagnostics. |
| ENUM-001 | Duplicate or out-of-range codes, or missing `Copy` | Rust compilation | Compiler diagnostics. |
| ENUM-004 | An unassigned code | Runtime | `None` directly. Packed readers may return `UnknownDiscriminant` as specified by CAP-03 and CAP-04. |

## Verification Checklist

- [ ] Cover sparse, implicit, signed, and 128-bit codes and unknown-code boundaries.
- [ ] Add compile-fail coverage for declaration rules.
- [ ] Verify embedded conversion through both generated storage forms.
