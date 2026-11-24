# Capability Spec: Tagged union storage

- **Spec ID** `CAP-04`
- **Status** Draft
- **Version** 0.1.0

## Layer A: Language-Agnostic Contract

### 1. Purpose & Core Concept

A tagged layout stores a tag and a payload in one word. The tag selects the layout used to interpret the payload.

### 2. Structural Attributes & Memory Layout

**TAG-001.** The parent and each typed variant own one word. All variants share the same tag range. Payload ranges obey CAP-01 within each variant and may reuse positions across variants.

### 3. Construction & Validation Semantics

- **TAG-002.** The tag has width 1 through 64 and fits in the word. Each assigned tag is unique and fits the unsigned tag range.
- **TAG-003.** A raw parent accepts any bits. A projection checks the tag first and then checks enum-typed payload members in declaration order.
- **TAG-004.** A variant builder starts from zero, inserts its tag, then packs assigned members under the same value and default rules as CAP-03. A typed variant is intended to carry payload codes that its getters can decode.

### 4. Behavioral Operations

| ID | Operation | Result |
| --- | --- | --- |
| TAG-005 | Identify kind | Return the variant corresponding to the tag, or an unknown-tag error. |
| TAG-006 | Test for one variant | Compare only the tag and return a boolean. |
| TAG-007 | Project as a variant | Reject tag mismatch first, then reject the first unknown payload code. Preserve the entire raw word on success. |
| TAG-008 | Read typed payload, return raw, or convert to parent | Read the field using its declared type. Raw extraction and parent conversion preserve the full word. |

| Given | When | Then |
| --- | --- | --- |
| A 2-bit tag at position 0 and a 3-bit payload at position 2 | Build variant 1 with payload 5 | The word is `0x15`. |
| Tags 1 and 2 with raw tag 3 | Query kind and test for tag 1 | Kind returns an error. The test returns false. |
| Tag 1 with an unassigned enum payload code | Test and project as variant 1 | The test returns true and the projection returns a payload error. |
| The same payload positions used by two variants | Validate | Accept each layout if it is individually disjoint from the tag and its other members. |

### 5. Non-Functional & Performance Constraints

**TAG-009.** A tag test has fixed work. Projection checks the selected variant's enum members. Generated packing uses local values without requiring allocation or synchronization. User-supplied enum conversions can have their own costs.

## Layer B: Rust Target Binding

### 1. Public API & Macro Syntax

```rust
use nexus_bits::bit_storage;

#[bit_storage(repr = u8, discriminant(start = 0, len = 2))]
pub enum Event {
    #[variant(1)]
    Trade {
        #[field(start = 2, len = 3)]
        qty: u8,
    },
    #[variant(2)]
    Heartbeat,
}

let event = Event::trade().qty(5).build_parent().unwrap();
assert_eq!(event.raw(), 0x15);
assert_eq!(event.as_trade().unwrap().qty(), 5);
```

| Generated type | Public API |
| --- | --- |
| `Event` | `const fn from_raw(R) -> Self`, `const fn raw(self) -> R`, `fn kind(&self) -> Result<EventKind, UnknownDiscriminant<R>>` |
| `Event`, per variant | `fn is_trade(&self) -> bool`, `fn as_trade(&self) -> Result<EventTrade, UnknownDiscriminant<R>>`, `fn trade() -> EventTradeBuilder` |
| `EventTrade` | `fn builder() -> EventTradeBuilder`, `const fn raw(self) -> R`, `const fn as_parent(self) -> Event`, `impl From<EventTrade> for Event` |
| `EventTrade`, per member | Primitive `const fn qty(&self) -> T`, flag `const fn member(&self) -> bool`, enum `fn member(&self) -> T` |
| `EventTradeBuilder` | `fn member(self, val: T) -> Self`, `fn build(self) -> Result<EventTrade, FieldOverflow<R>>`, `fn build_parent(self) -> Result<Event, FieldOverflow<R>>` |

Variant names form suffixes on the generated types. Method names use the variant name lowercased with an underscore before each uppercase character after the first. The generated kind enum contains the variant names and has no explicit integer representation or assigned wire discriminants.

### 2. Memory, ABI & Performance Guarantees

Parent and variant types are transparent over `R` with its size, alignment, and ABI. The parent tuple field inherits the input visibility. Generated variant tuple fields are private. Parent, variants, and kind derive `Debug, Clone, Copy, PartialEq, Eq, Hash`. Builders derive `Debug, Clone, Copy, Default`.

### 3. Compile-Time vs. Runtime Error Boundary

| Requirement | Trigger | Phase | Contract |
| --- | --- | --- | --- |
| TAG-001, TAG-002 | Invalid layout, missing or duplicate tag, tag outside range | Macro expansion | CAP-05 diagnostic. |
| TAG-004 | Assigned value fails the builder check | Runtime | First `FieldOverflow<R>`. |
| TAG-005, TAG-007 | Unmapped tag or requested variant differs | Runtime | `UnknownDiscriminant { field: "__discriminant", value: tag as R }`. |
| TAG-007 | Unknown payload code | Runtime | `UnknownDiscriminant` with payload field name and extracted value. |

**Open gap.** A builder can produce a typed variant with an omitted enum field whose zero code is unassigned. That field's getter panics with `variant type invariant violated`. The variant builder also lacks the flat builder's `IntEnum::Repr` width assertion and shares its cast-before-comparison limitations. `as_*()` is the path that checks payload decodability against the current mapping.

## Verification Checklist

- [ ] Add compile-fail cases for invalid tag ranges, duplicate tags, tuple variants, and payload/tag overlap.
- [ ] Verify tag-only tests, mismatch precedence, unknown payloads, and raw round trips.
- [ ] Verify builder/getter consistency for omitted enum codes and wider enum representations.
- [ ] Assert backing size and alignment for parents and variants.
