---
spec_id: SPEC-04
title: Tagged Union Storage
crate: nexus-bits
status: Draft
version: 0.1.0
satisfies:
  - nexus-bits-derive/src/lib.rs
  - nexus-bits/src/int_enum.rs
  - nexus-bits/src/error.rs
verified_by:
  - nexus-bits/tests/derive_enum.rs
---

# SPEC-04: Tagged Union Storage

---

## Layer A: Language-Agnostic Contract (The "What")

### 1. Purpose & Scope

Store a variant identifier and its payload in one integer, with the identifier determining how to interpret the payload bits.

### 2. Domain Model & Invariants

#### 2.1 Definitions

- The **parent** holds a backing word that may contain any bit pattern.
- The **tag**, also called the discriminant, is the unsigned code that identifies the variant. All variants use the same tag positions.
- The **payload** is the set of fields and flags defined for a particular variant. A **typed variant** exposes that variant's payload getters.
- **Projection** checks a parent word against a requested variant and, on success, returns a typed variant holding the same word.

#### 2.2 Layout and Construction Invariants

| Req ID | Rule | Invariant |
| --- | --- | --- |
| **TAG-001** | Shared tag, separate payload layouts | One word holds the tag and payload. In each variant, payload members fit the word and are disjoint from one another and from the tag under [SPEC-01](01-bit-math-and-layout-validation.md). Different variants may reuse payload positions. |
| **TAG-002** | Tag width and codes | The tag width is 1 through 64 bits and fits in the backing word. Each variant has a unique code between zero and the all-ones value for that width. Gaps are allowed. |
| **TAG-003** | Raw construction and projection | Raw construction preserves every backing bit. Projection checks the tag first, then checks enum-typed payload members in declaration order. On success it preserves the full word. |
| **TAG-004** | Builder and typed payload | The builder starts from zero, inserts the variant tag, and packs assigned members under the defaults and field rules in [SPEC-03](03-flat-bit-storage.md). The draft invariant requires a typed variant's enum getters to decode its stored codes. Layer B documents where current builders fall short. |
| **TAG-009** | Work by operation | Testing one tag takes fixed work. Projection checks only the requested variant's enum members after a matching tag. The generated storage operations use local values without heap allocation. |

### 3. Operations & State Transitions

| Req ID | Operation | Result |
| --- | --- | --- |
| **TAG-003** | Wrap a raw word | Return a parent holding that exact word, including unused bits and unassigned codes. |
| **TAG-004** | Build a variant | Validate assigned values in declaration order, then pack the tag and accepted payload. Return the first field overflow if a check fails. |
| **TAG-005** | Identify kind | Extract the tag and return the corresponding variant name, or report an unknown tag. |
| **TAG-006** | Test for a variant | Compare the extracted tag to the requested variant code. Return a Boolean independent of payload validity. |
| **TAG-007** | Project a variant | Report a tag mismatch before touching the payload. With a matching tag, report the first unassigned enum payload code. Return a typed variant on success. |
| **TAG-008** | Read typed payload or return raw | Read a payload member using its declared type. Raw extraction and conversion to the parent preserve all bits. |

#### 3.1 Worked Example

One byte can represent a trade or a quote. The bottom two bits hold the tag. Both variants use bits 2 through 4, with the tag telling the reader what those bits mean.

| Variant | Bit 7 | Bits 6–5 | Bits 4–2 | Bits 1–0 |
| --- | --- | --- | --- | --- |
| Trade | Unused | Side code | Quantity | `01` |
| Quote | Unused | Unused | Level | `10` |

A trade with quantity 5 (`101`) and side code 1 (`01`) is `0_01_101_01` or `0x35`. A quote with level 5 is `0_00_101_10` or `0x16`. The same three payload bits hold `101` in both words. A reader checks the last two bits to decide whether the value 5 means quantity or level. If a trade contains side code 3 and only codes 0 and 1 are assigned, its tag still identifies it as a trade, but projection reports the unknown side code.

### 4. Acceptance Criteria (Gherkin)

```gherkin
Feature: Tagged union storage

  Scenario: Reuse payload positions across variants (TAG-001)
    Given a two-bit tag at bit 0 and two variants with payloads at bits 2 through 4
    When the layout is validated
    Then both payload ranges are accepted

  Scenario: Reject payload overlap with the tag (TAG-001)
    Given a tag occupying bits 0 and 1 and a payload field starting at bit 1
    When the layout is validated
    Then the layout is rejected for overlap with the tag

  Scenario: Reject duplicate tag codes (TAG-002)
    Given two variants both assigned tag 1
    When the declaration is validated
    Then the second assignment is rejected

  Scenario: Pack a trade and read its quantity (TAG-004, TAG-008)
    Given the example trade layout and side codes 0 and 1
    When a trade is built with quantity 5 and side code 1
    Then the raw word is 0x35
    And its typed quantity getter returns 5

  Scenario: Read an unassigned tag (TAG-005, TAG-006)
    Given the only assigned tags are 1 and 2
    When the tag 3 is identified and tested against variant 1
    Then identification reports unknown tag 3 and the test returns false

  Scenario: Distinguish a matching tag from a valid payload (TAG-006, TAG-007)
    Given a trade tag with unassigned side code 3
    When it is tested and projected as a trade
    Then the tag test returns true
    And projection reports side code 3 for the side field

  Scenario: Reject a mismatched tag before decoding the payload (TAG-007)
    Given a quote whose bits would contain an invalid side code if read as a trade
    When it is projected as a trade
    Then projection reports the quote tag as the mismatch

  Scenario: Preserve unused bits through projection (TAG-003, TAG-008)
    Given the example trade word 0x35 with unused bit 7 set to form 0xB5
    When it is projected as a trade and converted back to the parent
    Then the resulting raw word is 0xB5
```

---

## Layer B: Rust Target Binding (The "How in Rust")

### 1. Public Signatures & Attributes

#### 1.1 Tagged Layout Declaration

Use `#[bit_storage(repr = R, discriminant(start = S, len = L))]` with the default `derive` feature. The backing primitive `R` may be signed or unsigned, with a width from 8 through 128 bits. Positions and lengths are parsed as `u32` literals and `#[variant(N)]` codes as `u64` literals. Variants have named fields or are unit variants. Payload members use `#[field(start = S, len = L)]` or `#[flag(N)]`.

```rust
use nexus_bits::{IntEnum, bit_storage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntEnum)]
#[repr(u8)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

#[bit_storage(repr = u8, discriminant(start = 0, len = 2))]
pub enum Event {
    #[variant(1)]
    Trade {
        #[field(start = 2, len = 3)]
        qty: u8,
        #[field(start = 5, len = 2)]
        side: Side,
    },
    #[variant(2)]
    Quote {
        #[field(start = 2, len = 3)]
        level: u8,
    },
}
```

#### 1.2 Generated Types and Methods

The table shows public methods for the example. `R` denotes the backing type, and errors are exported by `nexus_bits`.

| Generated type | Methods and conversions |
| --- | --- |
| `Event` | `const fn from_raw(raw: R) -> Self`, `const fn raw(self) -> R`, `fn kind(&self) -> Result<EventKind, UnknownDiscriminant<R>>` |
| `Event`, per variant | `fn is_trade(&self) -> bool`, `fn as_trade(&self) -> Result<EventTrade, UnknownDiscriminant<R>>`, `fn trade() -> EventTradeBuilder` |
| `EventTrade` | `fn builder() -> EventTradeBuilder`, `const fn raw(self) -> R`, `const fn as_parent(self) -> Event`, `impl From<EventTrade> for Event` |
| `EventTrade`, per member | Primitive `const fn qty(&self) -> u8`, flag `const fn member(&self) -> bool`, enum `fn side(&self) -> Side` |
| `EventTradeBuilder` | `fn member(self, val: T) -> Self`, `fn build(self) -> Result<EventTrade, FieldOverflow<R>>`, `fn build_parent(self) -> Result<Event, FieldOverflow<R>>` |

Generated types inherit the input visibility. The parent tuple field also inherits it. Typed variant tuple fields are private. Variant names form suffixes on generated types. In method suffixes, the generator lowercases uppercase letters and inserts an underscore before each uppercase character after the first. `EventKind` contains the variant names and uses Rust's default representation. Its own discriminant values do not carry the assigned tag codes.

### 2. Memory & Performance Guarantees

#### 2.1 What Each Record Stores

The parent and each typed variant are `#[repr(transparent)]` over `R` with its size, alignment, and ABI. The trade and quote in the example both take **one byte**, including their tag. The tag bits live inside the word. Payloads share positions, and offsets and masks are constants in generated code. A typed variant holds the whole packed word by value, so returning one from projection needs no separate payload allocation or reference to the parent.

For the same example, a byte-oriented record with one byte for the tag and a two-byte payload buffer takes three bytes. The buffer has room for separate quantity and side bytes in a trade and is reused for a quote. This comparison uses the explicit `ByteEvent` layout below.

| Stored layout | Bytes per event | Bytes for 1,024 events |
| --- | ---: | ---: |
| Explicit byte tag and two-byte payload buffer | 3 | 3,072 |
| Packed `Event` or one of its typed variants | 1 | 1,024 |

Parent, variants, and kind derive `Debug, Clone, Copy, PartialEq, Eq, Hash`. The packed types compare and hash their whole backing word. Builders hold optional assignments and derive `Debug, Clone, Copy, Default`. The builder's size and layout are separate from the packed representation and have no fixed ABI contract.

```rust
use core::mem::{align_of, size_of};

#[repr(C)]
struct ByteEvent {
    tag: u8,
    payload: [u8; 2],
}

assert_eq!(size_of::<ByteEvent>(), 3);
assert_eq!(size_of::<Event>(), size_of::<u8>());
assert_eq!(size_of::<EventTrade>(), size_of::<u8>());
assert_eq!(size_of::<EventQuote>(), size_of::<u8>());
assert_eq!(align_of::<Event>(), align_of::<u8>());
assert_eq!(align_of::<EventTrade>(), align_of::<u8>());
assert_eq!(align_of::<EventQuote>(), align_of::<u8>());

let trade = Event::trade().qty(5).side(Side::Sell).build_parent().unwrap();
let quote = Event::quote().level(5).build_parent().unwrap();
assert_eq!(trade.raw(), 0x35);
assert_eq!(quote.raw(), 0x16);
assert_eq!(trade.kind().unwrap(), EventKind::Trade);
assert_eq!(quote.as_quote().unwrap().level(), 5);

let with_unused_bit = Event::from_raw(0xB5).as_trade().unwrap();
assert_eq!(with_unused_bit.qty(), 5);
assert_eq!(with_unused_bit.side(), Side::Sell);
assert_eq!(with_unused_bit.as_parent().raw(), 0xB5);
```

#### 2.2 What Each Operation Costs

| Operation | Work expressed by the generated code |
| --- | --- |
| `from_raw`, `raw`, and `as_parent` | Wrap, return, or rewrap one integer without scanning fields. |
| `is_trade()` | Extract the tag with a shift and mask, then compare it with 1. The work is independent of the number of variants and payload members. |
| `kind()` | Extract the tag and match it against the declared tag codes. The compiler chooses how to implement that match. |
| `as_trade()` | Check the tag. On a match, extract and decode each enum payload field, in declaration order, stopping at the first failure. The example checks `side` and skips `qty`. |
| Typed payload getters | A primitive getter extracts the field, with sign extension where needed. A flag getter masks and tests its bit. An enum getter runs the decoder again because the typed variant still stores only the word. |
| Build a trade or quote | Run the selected variant's generated checks, then pack its tag and payload. Work grows with that variant's member count. An assigned enum value can be converted during validation and again during packing. |

Geometry checks run at macro expansion. The generated runtime machinery allocates no heap memory. A manual `IntEnum` implementation may do additional work or allocate. The byte counts describe footprint. Instruction count, branch behavior, and latency depend on the compiler and target.

### 3. Compile-Time vs. Runtime Boundary

| Req ID | Condition | Enforcement phase | Mechanism |
| --- | --- | --- | --- |
| **TAG-001**, **TAG-002** | Invalid geometry, missing or duplicate tag, or tag outside its range | Macro expansion | Reject with the applicable [diagnostic](05-error-types-and-diagnostics.md). With an otherwise valid declaration, a tag field wider than 64 bits triggers a macro panic. |
| **TAG-004** | An assigned value fails a builder check | Builder execution | Return the first `FieldOverflow<R>` with the field name, value cast to `R`, and low mask as `R`. |
| **TAG-005**, **TAG-007** | An unassigned tag in `kind()` or a mismatched tag in `as_*()` | Runtime | Return `UnknownDiscriminant { field: "__discriminant", value: tag as R }`. |
| **TAG-007** | An enum payload decoder returns absence during projection | Runtime | Return `UnknownDiscriminant<R>` with the member name and extracted field value. |
| **TAG-008** | A typed variant getter fails to decode its stored enum code | Runtime | The current getter panics with `variant type invariant violated`. |

**Current implementation limits.** Builders write zero for omitted fields, so a builder can produce a typed variant whose omitted enum field has no assigned code zero. Its getter then panics. Projection validates each enum payload through the current mapping before returning the typed variant. The tagged builder compares some inputs after casting to `R` and has no enum representation-width assertion like the flat builder. A narrowing cast can therefore discard bits before validation. With a full-width tag over signed backing narrower than 64 bits, the extracted negative word can sign-extend on the cast to `u64`. A code using the sign bit can then fail dispatch even though it fits the declared tag width.

### 4. Verification Traceability

| Requirements | Existing verification | Pending coverage |
| --- | --- | --- |
| TAG-001, TAG-002 | Valid layouts in `tests/derive_enum.rs`. | Compile-fail fixtures for invalid ranges, duplicate tags, tuple variants, and payload/tag overlap. |
| TAG-003–TAG-008 | `tests/derive_enum.rs` exercises tag dispatch, builders, and payload validation. The worked example asserts raw-bit preservation and typed reads. | Verify omitted enum codes, wider enum representations, and full-width signed tag boundaries. |
| TAG-009 | The worked example asserts parent and variant size and alignment over `u8`. | Assert size and alignment across the supported backing types. |
