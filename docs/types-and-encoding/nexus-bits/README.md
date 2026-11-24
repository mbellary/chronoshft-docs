# nexus-bits capability specifications

These Draft 0.1.0 specifications describe the public contract of `nexus-bits` 0.4.4 and the macros supplied by `nexus-bits-derive` through the default `derive` feature. The source links are pinned to the reviewed Nexus revision. The local checkout also contains working changes to the flag implementation and tests, which were included in the review. The runtime descriptors and the `IntEnum` trait are available with default features disabled.

## Capabilities and verification map

| Specification | Public contract | Primary verification |
| --- | --- | --- |
| [01 Bit math and layout validation](01-bit-math-and-layout-validation.md) | Bit numbering, bounds, overlaps, masks, and signed decoding | Boundary vectors and macro compile-fail cases |
| [02 IntEnum contract](02-int-enum-contract.md) | Typed values and integer codes | Round trips, unknown codes, and derive rejection cases |
| [03 Flat bit storage](03-flat-bit-storage.md) | Struct layouts, builders, accessors, and raw values | Builder and getter integration tests, size and alignment assertions |
| [04 Tagged union storage](04-tagged-union-storage.md) | Tags, variant-specific payloads, and conversion to variant types | Dispatch, payload validation, malformed layouts, and ABI assertions |
| [05 Error types and diagnostics](05-error-types-and-diagnostics.md) | Structured failures and compile-time diagnostics | Exact payload and message assertions |
| [06 Runtime bit fields and flags](06-runtime-bitfields-and-flags.md) | Reusable descriptors for reading and editing caller-owned words | Const evaluation, boundary checks, and preservation properties |

## Source-to-capability traceability matrix

| Source | Specifications | Responsibility |
| --- | --- | --- |
| [nexus-bits/src/lib.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/lib.rs) | 02–06 | Public exports and macro feature gate |
| [nexus-bits/src/field.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/field.rs) | 01, 05, 06 | Runtime range construction, extraction, and replacement |
| [nexus-bits/src/flag.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/flag.rs) | 01, 05, 06 | Runtime single-bit operations |
| [nexus-bits/src/int_enum.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/int_enum.rs) | 02–04 | Shared integer conversion trait |
| [nexus-bits/src/error.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits/src/error.rs) | 03–06 | Error values and display formatting |
| [nexus-bits-derive/src/lib.rs](https://github.com/mbellary/nexus/blob/5135dde04ff0e7396423cb3a57be3e6c4e421dbc/nexus-bits-derive/src/lib.rs) | 01–05 | Attribute parsing, layout validation, diagnostics, and generated APIs |

The checkout's `nexus-bits/spec/bitfield.md` and `nexus-bits/spec/flags.md` are prior references for CAP-06. This suite uses the current Rust declarations and implementations to resolve conflicting descriptions. Open gaps describe cases where the implementation falls short of a proposed invariant. The verification checklists track coverage specific to these specifications.
