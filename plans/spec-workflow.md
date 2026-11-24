# Capability-Based Specification Architecture (`nexus-bits`)

## 1. Overview & Philosophy

Rather than mapping specifications 1:1 to Rust source files (`error.rs`, `fields.rs`, etc.), specifications are organized by **external capability and behavioral contract**. 

* **Specs define *what* must hold true:** Domain invariants, bit-layout math, public API signatures, and failure contracts.
* **Source files are implementation details:** A single capability may span multiple internal modules, and a single module (e.g., `error.rs`) may satisfy requirements across multiple specs.
* **Agent Skills consume specs:** Reusable task skills (code generation, `trybuild` test authoring, layout auditing) read these capability specs to execute engineering workflows without coupling to internal module layout.

---

## 2. Workflow
you must follow a 4-step workflow to carve out clean, decoupled specs. 

The output of this workflow must be a `README.md` and `0X-<capability>.md` files.


The following provides the example for the crate like `nexus-bits`,


## Step 1: Identify External "Capabilities" (Not Internal Modules)

Ask yourself: *What distinct promises does `nexus-bits` make to a user who only reads the public docs?*

Ignore internal parsing helpers (`parser.rs`, `codegen.rs`, `utils.rs`). Instead, group promises into vertical slices where each slice has clear inputs, transformations, and outputs:

1. **`01-bit-math-and-layout-validation.md`** — The foundational rules (shared by structs and enums): bit range math, overlap detection, width bounds, zero-length rejection, and sign-extension rules.
2. **`02-int-enum-contract.md`** — How `#[derive(IntEnum)]` maps primitive discriminants to Rust enums, handles invalid wire values, and participates as a field inside larger bit layouts.
3. **`03-flat-bit-storage.md`** — `#[bit_storage]` on structs: `#[repr(transparent)]` newtypes, `raw()`/`from_raw()`, checked builders (`build()`), unchecked setters (`set_unchecked`), and primitive vs. `IntEnum` getters.
4. **`04-tagged-union-storage.md`** — `#[bit_storage]` on enums: discriminant placement, variant-specific layouts, validated variant newtypes (`FooBar`), `kind()`, `is_*()`, and `as_*()`.
5. **`05-error-types-and-diagnostics.md`** — Exact runtime error payloads (`FieldOverflow`, `UnknownDiscriminant`, `Overflow`) and compile-time error message contracts.

please add the capabilities to the `README.md` file.

---

## Step 2: Use a Strict Two-Layer Spec Template

**you must use the Capability Spec Template to generate the specifications.**

Give every rule in your spec a short, stable ID (e.g., `LAYOUT-001`, `SIGN-002`, `ENUM-004`).

To preserve portability without losing Rust precision, split every capability spec into two explicit sections:

### Layer A: Language-Agnostic Contract (The "What")
* **Purpose & Scope:** One sentence stating the capability.
* **Domain Model & Invariants:** Mathematical rules that must always hold (e.g., $\text{start} + \text{len} \le W$, two intervals $[s_1, s_1 + l_1)$ and $[s_2, s_2 + l_2)$ overlap iff $\max(s_1, s_2) < \min(s_1 + l_1, s_2 + l_2)$).
* **State Transitions / Operations:** Input $\rightarrow$ Output tables, edge cases, and failure conditions.
* **Acceptance Criteria (Gherkin / Given-When-Then):** Testable scenarios independent of syntax.

### Layer B: Rust Target Binding (The "How in Rust")
* **Public Signatures & Attributes:** Exact macro syntax (`#[field(start = ..., len = ...)]`), generated struct names, and trait bounds.
* **Memory & ABI Guarantees:** E.g., `#[repr(transparent)]`, zero-allocation hot paths, `Copy`/`Clone` derivations.
* **Compile-Time vs. Runtime Boundary:** Explicitly mark which invalid states must trigger `compile_error!` versus returning `Err(...)`.


---

## Step 3: Map Your Existing Files Into Capabilities (Many-to-Many)

Once the capability docs exist, treat your `.rs` files as implementation details that satisfy parts of one or more specs - example capability spec map:


| Spec File | Capability Scope | Core Contracts Covered | Primary Verification |
| :--- | :--- | :--- | :--- |
| `01-bit-math-and-layout-validation.md` | Bit-range geometry, masking, sign extension, and static layout validation | Overlap detection, container width bounds, zero-length rejection, two's complement sign extension, unchecked masking | `trybuild` compile-fail tests + bit-math unit tests |
| `02-int-enum-contract.md` | Integer-backed enum mapping and wire validation | Discriminant-to-variant mapping, fallible wire decoding (`Result`), infallible building | Unit tests + property tests (`proptest`) |
| `03-flat-bit-storage.md` | Flat packed integer newtypes (`#[bit_storage]` on structs) | `#[repr(transparent)]` ABI, `raw()`/`from_raw()`, checked builder (`build()`), unchecked setters, primitive vs. `IntEnum` getters | Runtime integration tests + layout size/align assertions |
| `04-tagged-union-storage.md` | Discriminated unions (`#[bit_storage]` on enums) | Discriminant bounds & uniqueness, discriminant-field overlap checks, validated variant newtypes (`FooBar`), `kind()`, `is_*()`, `as_*()` | `trybuild` compile-fail tests + variant dispatch tests |
| `05-error-types-and-diagnostics.md` | Runtime error payloads and compile-time diagnostic contracts | `FieldOverflow`, `UnknownDiscriminant`, `Overflow` struct shapes and exact `compile_error!` message formats | Runtime error assertions + `trybuild` `.stderr` snapshots |

Notice how `field.rs` and `error.rs` naturally feed multiple capabilities without forcing you to maintain artificial 1:1 spec files.

please add the Capability Spec Map to the `README.md` file.


---

## 4. Source-to-Capability Traceability Matrix

Internal Rust files implement slices of one or more capability specs:

| Internal Module (`src/`) | Satisfies Capabilities | Notes |
| :--- | :--- | :--- |
| `error.rs` | `03`, `04`, `05` | Provides runtime error structs (`FieldOverflow`, `UnknownDiscriminant`, `Overflow`) |
| `field.rs` | `01`, `03`, `04` | Defines field offset/width metadata, masks, and shift/sign-extend codegen |
| `validation.rs` | `01`, `04`, `05` | Enforces compile-time geometric and discriminant invariants |
| `struct_gen.rs` | `03` | Emits `#[repr(transparent)]` struct newtype and `*Builder` |
| `enum_gen.rs` | `04` | Emits parent enum newtype, `*Kind` enum, validated variant types, and variant builders |
| `int_enum.rs` | `02` | Implements `#[derive(IntEnum)]` conversion traits |

please add the Source-to-Capability Traceability Matrix to the `README.md` file.
