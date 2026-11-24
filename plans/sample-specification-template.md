---
spec_id: SPEC-01
title: Bit Math and Layout Validation
crate: nexus-bits
status: Draft
version: 0.1.0
satisfies:
  - src/field.rs
  - src/validation.rs
verified_by:
  - tests/ui/
  - tests/bit_math_tests.rs
---

# SPEC-01: Bit Math and Layout Validation

---

## Layer A: Language-Agnostic Contract (The "What")

### 1. Purpose & Scope
Defines the mathematical invariants, bit-manipulation primitives, sign-extension rules, and layout validation constraints required to pack and unpack contiguous bit-fields within a fixed-width unsigned integer container.

---

### 2. Domain Model & Invariants

#### 2.1 Definitions
* **Container ($C$):** An unsigned integer backing store of fixed bit-width $W \in \{8, 16, 32, 64, 128\}$. Bits are zero-indexed from least significant bit (LSB = $0$) to most significant bit (MSB = $W - 1$).
* **Bit-Field ($F$):** A contiguous half-open interval of bits $[s, e)$ where:
  * $s \in \mathbb{Z}_{\ge 0}$ is the starting bit offset (inclusive).
  * $l \in \mathbb{Z}_{> 0}$ is the bit-length.
  * $e = s + l$ is the ending bit offset (exclusive).
* **Field Mask ($M$):** An unsigned integer of width $W$ with bits $[s, s + l)$ set to `1` and all other bits set to `0`:
  $$M = \left( (1 \ll l) - 1 \right) \ll s$$
  *(Note: When $l = W$ and $s = 0$, $M = 2^W - 1$, avoiding shift overflow).*

#### 2.2 Layout Invariants

| Req ID | Rule | Mathematical Invariant |
| :--- | :--- | :--- |
| **LAYOUT-001** | **Non-Zero Length** | Every declared field must span at least one bit: $l \ge 1$. |
| **LAYOUT-002** | **Container Upper Bound** | Every declared field must fit completely within the container width: $s + l \le W$. |
| **LAYOUT-003** | **Strict Non-Overlap** | For any two distinct fields $F_1 = [s_1, e_1)$ and $F_2 = [s_2, e_2)$ sharing the same layout scope, their intervals must be disjoint: $\max(s_1, s_2) \ge \min(e_1, e_2)$. |
| **LAYOUT-004** | **Type Width Compatibility** | If a field targets a scalar primitive of width $T$, its bit-length must not exceed the target type's capacity: $l \le T$. |
| **LAYOUT-005** | **Signed Minimum Width** | A two's complement signed field requires at least 1 sign bit and 1 magnitude bit (or explicit single-bit sign semantics): $l \ge 2$ for signed primitives. |

---

### 3. Operations & State Transitions

#### 3.1 Value Bounds & Encoding

| Req ID | Operation | Input | Valid Domain | Transformation | Failure Condition |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **MATH-001** | **Unsigned Range Check** | Unsigned value $v$, length $l$ | $0 \le v \le 2^l - 1$ | Identity ($v$) | $v > 2^l - 1 \implies \text{FieldOverflow}$ |
| **MATH-002** | **Signed Range Check** | Signed value $v$, length $l$ | $-2^{l-1} \le v \le 2^{l-1} - 1$ | Two's complement truncation: $v \land ((1 \ll l) - 1)$ | $v < -2^{l-1} \lor v > 2^{l-1} - 1 \implies \text{FieldOverflow}$ |

#### 3.2 Bitwise Insertion & Extraction

| Req ID | Operation | Formula | Post-Condition |
| :--- | :--- | :--- | :--- |
| **MATH-003** | **Extract Raw** | $v_{\text{raw}} = (C \gg s) \land ((1 \ll l) - 1)$ | $0 \le v_{\text{raw}} \le 2^l - 1$; upper bits $[l, W)$ are zeroed. |
| **MATH-004** | **Sign Extend** | Let $S = 1 \ll (l - 1)$. $v_{\text{signed}} = (v_{\text{raw}} \oplus S) - S$ | Bits $[l, T)$ are filled with the sign bit at index $l - 1$. |
| **MATH-005** | **Insert Masked** | $C' = (C \land \neg M) \lor ((v_{\text{encoded}} \ll s) \land M)$ | Bits outside $[s, s + l)$ remain strictly unchanged. |
| **MATH-006** | **Clear Field** | $C' = C \land \neg M$ | Bits in $[s, s + l)$ are `0`; all other bits unchanged. |

---

### 4. Acceptance Criteria (Gherkin)

```gherkin
Feature: Bit-Field Layout Validation and Arithmetic

  Scenario: Reject overlapping fields (LAYOUT-003)
    Given a 32-bit container
    And field A occupies bits [0, 8)
    And field B occupies bits [7, 16)
    When layout validation runs
    Then validation fails with an overlap error identifying fields A and B

  Scenario: Allow adjacent fields (LAYOUT-003)
    Given a 32-bit container
    And field A occupies bits [0, 8)
    And field B occupies bits [8, 16)
    When layout validation runs
    Then validation succeeds

  Scenario: Reject field exceeding container width (LAYOUT-002)
    Given a 16-bit container
    And field A is declared at start = 12, len = 5
    When layout validation runs
    Then validation fails with a bounds error (17 > 16)

  Scenario: Sign-extend negative value from sub-byte field (MATH-004)
    Given a 4-bit signed field containing raw bit pattern 0b1101
    When the field is extracted as a signed 8-bit integer
    Then the resulting value is -3 (0b1111_1101)

  Scenario: Preserve neighboring bits during insertion (MATH-005)
    Given a 16-bit container initialized to 0xFFFF
    And a field at start = 4, len = 4
    When the value 0x0 is inserted into the field
    Then the container value becomes 0xFF0F
```

---

## Layer B: Rust Target Binding (The "How in Rust")

### 1. Public Signatures & Attributes

#### 1.1 Field Attribute Syntax
Fields inside `#[bit_storage]` structs or enum variants are annotated with:
```rust
#[field(start = <usize_lit>, len = <usize_lit>)]
```
* `start` and `len` must evaluate to non-negative integer literals at macro expansion time.
* Supported container backing types (`repr`): `u8`, `u16`, `u32`, `u64`, `u128`.
* Supported scalar field types: `bool` (requires `len == 1`), `u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`, `i32`, `i64`, `i128`.

#### 1.2 Compile-Time Mask & Shift Evaluation
All masks and shift constants emitted by codegen must be `const`-evaluatable expressions without panicking on full-width shifts (`len == W`):
```rust
const MASK: Repr = if LEN == Repr::BITS as usize {
    Repr::MAX
} else {
    ((1 as Repr) << LEN) - 1
};
```

---

### 2. Memory & Performance Guarantees
* **Zero Runtime Layout Overhead:** All overlap, width, and type-fit checks execute strictly during procedural macro expansion. No layout metadata is stored at runtime.
* **Branchless Hot-Path Arithmetic:** Extraction (`MATH-003`), sign extension (`MATH-004`), and unchecked insertion (`MATH-005`) compile down to constant shifts, bitwise `AND`/`OR`/`XOR`, and integer subtraction without branches.
* **Overflow Safety in Debug/Release:** Shift operations must never trigger debug-mode arithmetic overflow panics when `len == W` or `start + len == W`.

---

### 3. Compile-Time vs. Runtime Boundary

| Req ID | Condition | Enforcement Phase | Mechanism |
| :--- | :--- | :--- | :--- |
| **LAYOUT-001** | `len == 0` | **Compile-Time** | `syn::Error` / `compile_error!("field length must be at least 1")` |
| **LAYOUT-002** | `start + len > W` | **Compile-Time** | `syn::Error` / `compile_error!("field exceeds container bit width")` |
| **LAYOUT-003** | Overlapping `[start, start + len)` | **Compile-Time** | `syn::Error` / `compile_error!("field overlaps with ...")` |
| **LAYOUT-004** | `len > TargetType::BITS` | **Compile-Time** | `syn::Error` / `compile_error!("field length exceeds target type width")` |
| **LAYOUT-005** | Signed type with `len < 2` | **Compile-Time** | `syn::Error` / `compile_error!("signed field requires at least 2 bits")` |
| **LAYOUT-006** | `bool` type with `len != 1` | **Compile-Time** | `syn::Error` / `compile_error!("bool field must have length 1")` |
| **MATH-001** | Unsigned input $> 2^{\text{len}} - 1$ | **Runtime** (Checked API) | Returns `Err(BitError::FieldOverflow { .. })` |
| **MATH-002** | Signed input outside $[-2^{\text{len}-1}, 2^{\text{len}-1}-1]$ | **Runtime** (Checked API) | Returns `Err(BitError::FieldOverflow { .. })` |
| **MATH-005** | Unchecked setter (`set_*_unchecked`) | **Runtime** (Unchecked API) | Masks input with `MASK` silently; no error returned |
