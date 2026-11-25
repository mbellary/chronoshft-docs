---
spec_id: SPEC-05
title: Error Types and Diagnostics
crate: nexus-bits
status: Draft
version: 0.1.0
satisfies:
  - nexus-bits/src/error.rs
  - nexus-bits/src/field.rs
  - nexus-bits/src/flag.rs
  - nexus-bits-derive/src/lib.rs
verified_by:
  - nexus-bits/tests/tests.rs
  - nexus-bits/tests/derive_struct.rs
  - nexus-bits/tests/derive_enum.rs
  - nexus-bits/tests/flag_contracts.rs
---

# SPEC-05 · Error types and diagnostics

**Report invalid declarations during compilation and bad values where callers can handle them.**

---

## Layer A · Language-agnostic contract

**Failure reports distinguish an invalid layout, a value that does not fit, and an unassigned code.**

### 1. Purpose and scope

**Use the failure information to find the rejected value or declaration rule.**

Describe how invalid layouts, out-of-range values, and unrecognized codes are reported so callers can act on the failure.

### 2. Domain model and invariants

**An error carries the rejected number and any field or tag context the operation provides.**

#### 2.1 Definitions

- **Overflow.** It reports the rejected value and a numeric maximum in the operation's backing type.
- **Field overflow.** It adds the member name to an overflow.
- **Unknown discriminant.** It reports an extracted code and its context. The context can identify a payload field or the tag itself.
- **Declaration diagnostic.** It identifies an invalid static layout or enum declaration during compilation.

#### 2.2 Failure invariants

| Req ID | Rule | Invariant |
| --- | --- | --- |
| **ERR-001** | Overflow context | A standalone checked insert reports value and maximum. A builder adds the first failing member's name. The reported builder maximum is the field mask expressed in the backing type. For a signed field, it can differ from the numeric upper bound used by the check. |
| **ERR-002** | Decoding context | For a failed field decode, report the field name and the extracted code. For an unknown or mismatched tag, report the tag context and extracted tag. |
| **ERR-003** | Failure at the validating operation | Reject invalid declarations during compilation. Checked operations return structured errors where specified by the owning capability. Constructor assertions and violated typed-variant invariants have separate failure paths in Layer B. |
| **ERR-006** | Structured reporting | Construct errors from values and a reference to the declared name where applicable. Text rendering occurs when the caller requests it. |

### 3. Operations and state changes

**An operation reports its failure first, and the caller can format the error on demand.**

| Req ID | Operation | Input | Result |
| --- | --- | --- | --- |
| **ERR-001** | Report overflow | Rejected value, reported maximum, and optional member context | Numeric overflow details, including the member name for a builder failure. |
| **ERR-002** | Report an unknown code | Extracted tag or field code and context | Structured decoding failure with the supplied code and context. |
| **ERR-003** | Diagnose an invalid declaration | Layout or enum declaration | Reject compilation with the diagnostic for the failing rule. |
| **ERR-004** | Render overflow | Structured overflow | Write the value and maximum, with a field prefix for field overflow. |
| **ERR-005** | Render an unknown code | Structured decoding failure | Write the code, with a context prefix when the name is nonempty. |

#### 3.1 Worked example

A three-bit unsigned mode field accepts 0 through 7. A builder receives 8 and returns the field name `mode`, value 8, and maximum 7. A caller can inspect the maximum and handle the failure directly. If the caller renders the same error, it reads `field 'mode': value 8 exceeds max 7`.

```text
[Builder receives mode = 8]
              |
     [Check against 7]
              |
[Error holds name, value, maximum]
              |
  [Caller inspects or formats it]
```

### 4. Acceptance criteria

**Check the returned values, field names, and diagnostic text.**

```gherkin
Feature: Error types and diagnostics

  Scenario: Read structured details from builder overflow (ERR-001)
    Given a three-bit unsigned mode field
    When a record is built with mode set to 8
    Then the error identifies mode with value 8 and maximum 7

  Scenario: Report the extracted tag (ERR-002)
    Given a word with nonzero payload bits and an unassigned tag 3
    When the kind of the word is queried
    Then the error identifies the tag context and code 3

  Scenario: Reject a zero-width declaration (ERR-003)
    Given a packed field declared with width zero
    When the declaration is compiled
    Then compilation fails with the zero-length diagnostic

  Scenario: Render an overflow with its field (ERR-004, ERR-006)
    Given a field overflow for mode with value 8 and maximum 7
    When the error is formatted
    Then the output is "field 'mode': value 8 exceeds max 7"

  Scenario: Render an unknown code with no context (ERR-005)
    Given an unknown-code error with code 3 and an empty context
    When the error is formatted
    Then the output is "unknown discriminant: 3"
```

---

## Layer B · Rust target binding

**Rust uses structured error values, compiler diagnostics, and documented panic paths.**

### 1. Public signatures and attributes

**Read an error's fields to handle it in code, or format it for a person.**

#### 1.1 Error values

```text
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

Each derives `Debug, Clone, Copy, PartialEq, Eq` subject to the corresponding bounds on `T`. Each implements `core::fmt::Display` when `T: Display`. The baseline provides no `core::error::Error` implementation.

#### 1.2 Display contract

| Error | Exact display pattern |
| --- | --- |
| `Overflow` | `value {value} exceeds max {max}` |
| `FieldOverflow` | `field '{field}': {overflow}` |
| `UnknownDiscriminant` with empty field | `unknown discriminant: {value}` |
| `UnknownDiscriminant` with a field | `field '{field}': unknown discriminant {value}` |

Builder payloads are stored as the backing type. For signed primitive builder overflow, `max` is the low mask cast to the backing type. The upper signed bound used by the comparison can differ from that value.

Tagged storage uses `"__discriminant"` for tag errors. An empty field is a separate formatting case. The direct `IntEnum::try_from_repr` API returns `None` for unassigned codes.

### 2. Memory and performance guarantees

**Creating an error stores small values and can reuse a static name without allocating a string.**

#### 2.1 What each error stores

| Error | Inline data | Field name storage |
| --- | --- | --- |
| `Overflow<T>` | Two values of `T` | No name |
| `FieldOverflow<T>` | Two values of `T` and an `&'static str` | A reference to the static name, with its address and length |
| `UnknownDiscriminant<T>` | One value of `T` and an `&'static str` | A reference to the static context, with its address and length |

A builder error for `mode` holds a reference to the word `mode` in static storage. Creating another error for that same field stores another reference, while both references can point to the same characters. The number of characters in the name does not change the size of the error value. No owned string or formatted message is stored. These are default Rust layouts. Padding, size, and alignment depend on `T` and the target, and the types have no fixed application binary interface (ABI) for passing the values across compiled code boundaries. The enclosing `Result` also has its own layout. An error's size alone does not establish the size of a successful result or how that result is passed.

For a concrete observation, `size_of` on the reviewed `x86_64-unknown-linux-gnu` toolchain reports the following. Bytes include padding. Both contextual `u8` errors have the same observed size even though one carries an extra byte of numeric data.

| Instantiation | Observed size | Observed alignment |
| --- | ---: | ---: |
| `Overflow<u8>` | 2 bytes | 1 byte |
| `FieldOverflow<u8>` | 24 bytes | 8 bytes |
| `UnknownDiscriminant<u8>` | 24 bytes | 8 bytes |

#### 2.2 When the work happens

| Path | Work performed |
| --- | --- |
| Successful check | The operation performs its validation and returns its success value without constructing error text. |
| Return a structured error | Populate the numeric fields and, where applicable, attach the static name reference. The crate-owned construction path creates no heap allocation. |
| Read error fields | Read the stored values directly. Inspecting an overflow limit or offending code requires no formatting. |
| Format with `Display` | Write the literal text, optional name, and formatted numeric values to the supplied formatter. There is no intermediate owned message in these implementations. A custom `T::fmt` or the caller's destination may allocate. |
| Compile-time diagnostics | Layout and declaration checking, including diagnostic construction, executes in the compiler or macro. This diagnostic work adds no runtime layout check to code generated for an accepted declaration. |

The example reads the numeric limit before rendering the error. The final line uses `format!`, which creates an owned string at the caller's request. Message cost depends on the name length, rendered values, and destination. The contract specifies these operations and allocations, while instruction counts and timings depend on the target and caller.

In a Cargo binary project, add the following line under `[dependencies]`. Save the Rust example as `src/main.rs` and run `cargo run`.

```toml
nexus-bits = "0.4.4"
```

```rust
use nexus_bits::bit_storage;

#[bit_storage(repr = u8)]
struct Order {
    #[field(start = 0, len = 3)]
    mode: u8,
}

fn main() {
    let error = Order::builder().mode(8).build().unwrap_err();
    assert_eq!(error.field, "mode");
    assert_eq!((error.overflow.value, error.overflow.max), (8, 7));
    assert_eq!(format!("{error}"), "field 'mode': value 8 exceeds max 7");
}
```

### 3. Compile-time and runtime error boundary

**The failure mode depends on the operation and whether it runs during compilation or at runtime.**

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
| LAYOUT-001 | Zero length | Macro error | `len must be > 0` |
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

The missing-repr message predates the 128-bit support. [SPEC-01](01-bit-math-and-layout-validation.md) and [the runtime descriptor specification](06-runtime-bitfields-and-flags.md) document endpoint overflow cases that can bypass or precede the expected diagnostic.

| Operation | Failure | Rust result |
| --- | --- | --- |
| Checked runtime field insertion | Input compares greater than the field maximum | `Err(Overflow<T>)` |
| Flat or tagged build | An assigned value fails its field check | `Err(FieldOverflow<R>)` |
| Flat enum getter or tagged projection | Extracted enum code is unassigned | `Err(UnknownDiscriminant<R>)` with the member name |
| Tagged kind or projection | Tag is unknown or mismatched, respectively | `Err(UnknownDiscriminant<R>)` with field `"__discriminant"` |
| Typed variant enum getter | Stored code cannot be decoded | Panic with `variant type invariant violated` in the current implementation |

### 4. Verification traceability

**Tests cover common error payloads and formatting, with specific boundary and diagnostic cases pending.**

| Requirements | Existing verification | Pending coverage |
| --- | --- | --- |
| ERR-001, ERR-002 | `tests/tests.rs`, `tests/derive_struct.rs`, and `tests/derive_enum.rs` exercise error payloads. The worked example checks a builder failure. | Additional signed mask and code narrowing cases. |
| ERR-003 | Constructor failure cases and flag compile-fail doctests cover runtime and const boundaries. | Compile-fail fixtures for the macro-owned diagnostic bodies and macro panic paths. |
| ERR-004, ERR-005 | Tests exercise overflow and decoding error formatting. | Assert both unknown-code context forms together, including an empty context. |
| ERR-006 | Source inspection of constructors and formatting, plus target-specific size measurements. | Layout measurements on other targets are outside this contract. |
