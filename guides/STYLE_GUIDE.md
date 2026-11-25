# Technical Writing & Documentation Style Guide

Treat technical documentation like high-performance systems code: eliminate filler, build intuition from first principles, illustrate memory and data flow visually, and optimize for rapid scanning.

---

## 1. Core Principles

* **Bottom-Line Up Front (BLUF):** Open every document and major section with **one bolded sentence** stating the core takeaway or direct answer before explaining supporting details.
* **Beginner-First Clarity:** Assume the reader is new to low-level systems. Explain concepts in simple, everyday words first, define unavoidable jargon immediately on first use, and build mental intuition before showing code.
* **Show, Don't Just Tell (Visual Illustrations):** Use clean ASCII diagrams—bit/byte memory maps, cache-line layouts, data-flow pipelines, and state transitions—whenever explaining layout, concurrency, or multi-step mechanics.
* **Zero Conversational Filler:** Cut throat-clearing phrases (*"In this document, we will explore..."*, *"It is important to note that..."*). Start immediately with the rule, definition, or visual model.
* **Hardware & Systems Grounding:** Tie abstract rules back to physical execution realities—memory layout, 64-byte cache lines, ABI guarantees, thread boundaries, and failure modes—using everyday analogies where helpful.
* **Single Source of Truth:** Separate declarative **Specifications** (*what must hold true*) from imperative **Workflows / Skills** (*how to execute*), keeping all code and templates copy-pasteable.

---

## 2. Beginner-First Translation Rules

When introducing a low-level systems term, follow the **Intuition → Jargon → Hardware Reality** pattern:

| Instead of Writing... | Write This (Beginner-Friendly) |
| :--- | :--- |
| *"Provides a `#[repr(transparent)]` zero-cost abstraction over primitive scalars."* | *"Wraps a standard integer (like `u64`) with zero extra memory overhead—at runtime, the CPU sees the exact same raw number (`#[repr(transparent)]`)."* |
| *"Eliminates heap allocation jitter and allocator lock contention on the hot path."* | *"Reserves all required memory once at startup so the critical trading loop never pauses to ask the operating system for memory."* |
| *"Uses SPSC ring buffers with cached head/tail indices to prevent false sharing."* | *"Passes messages between one writer thread and one reader thread using a fixed circular array, keeping their counters on separate 64-byte CPU cache lines so cores do not slow each other down."* |
| *"Convert network byte order to host endianness at the framing boundary."* | *"Network packets arrive most-significant byte first (big-endian). Convert the raw bytes into a native Rust integer before extracting bit fields."* |

---

## 3. Ready-to-Copy ASCII Diagram Templates

Use these four standard diagram patterns across crate docs, specs, and Use Case Skills.

### A. Bit & Byte Memory Layout Map
Use for bit-packed structs, wire headers, tagged pointers, and binary framing (`nexus-bits`, `nexus-fix-codec`).

```text
16-Bit Packet Header (u16, Host Byte Order)
 Bit:  15               8 7     5   4   3         0
      +------------------+-------+-----+-----------+

      |    seq_delta     | flags |side | msg_type  |
      |     (8 bits)     |(3 bit)|(1b) | (4 bits)  |
      +------------------+-------+-----+-----------+
Mask:        0xFF00        0x00E0 0x0010   0x000F
```

### B. Data-Flow & Boundary Pipeline
Use for showing where a primitive sits in a system and where invariants (like bounds checks or endian swaps) occur.

```text
[Raw UDP Bytes] ──► [Bounds & Endian Check] ──► [PacketHeader(u16)] ──► [Typed Strategy Event]
  (&[u8] slice)       (u16::from_be_bytes)         (Zero-copy wrap)      (Result<MsgType, Err>)
```

### C. Cache-Line & Memory Ownership Layout
Use for allocators, intrusive collections, and lock-free queues (`nexus-slab`, `nexus-collections`, `nexus-queue`).

```text
64-Byte CPU Cache Line (Core 0 Producer)         64-Byte CPU Cache Line (Core 1 Consumer)
+---------------------------------------+        +---------------------------------------+

| head_idx (8B) | cached_tail (8B) |pad |        | tail_idx (8B) | cached_head (8B) |pad |
+---------------------------------------+        +---------------------------------------+
         │                                                ▲
         └──────────────► [ Pre-Allocated Ring Buffer ] ──┘
                            (Contiguous RAM, no malloc)
```

### D. Step-by-Step Bitwise Operation Trace
Use for illustrating precomputed mask extraction, insertion, or clearing in `nexus-bits` specs.

```text
Operation: Insert value 0b101 (5) into field [offset = 4, width = 3]

1. Original container:   0b_0000_1111_0000_0011
2. Clear target bits:    0b_0000_1000_0000_0011   (container & ~mask)
3. Shift & mask value:   0b_0000_0101_0000_0000   ((value << 4) & mask)
4. Combine (OR):         0b_0000_1101_0000_0011   (step 2 | step 3)
```

---

## 4. Structural & Formatting Rules

| Element | Standard | Rationale |
| :--- | :--- | :--- |
| **Section Openings** | 1 bolded lead-in sentence summarizing the core takeaway. | Enables 3-second comprehension during reading or agent retrieval. |
| **Headings** | Sentence case (`## Where to use the crate`), numbered for sequential workflows. | Cleaner visual hierarchy and predictable anchor links. |
| **Bullet Lists** | **Bold lead-in term** followed by one concise idea per bullet. | Prevents dense walls of text; keeps each constraint scannable. |
| **Tables & Matrices** | Use whenever comparing 3+ items across shared attributes (trade-offs, traceability maps, cheat sheets). | Compresses multi-dimensional relationships into instant lookup grids. |
| **Code Blocks** | Minimal, compiling, copy-pasteable snippets with inline comments highlighting boundary invariants. | Eliminates guesswork around imports, attributes, and error handling. |

---

## 5. Tone & Boundary Guidelines

* **Be Direct & Prescriptive:** State constraints plainly (*"Convert byte order at the framing boundary"* rather than *"You might want to consider converting endianness"*).
* **Surface Trade-Offs & Anti-Patterns:** Always pair *"When to use"* with *"When **not** to use"* so beginners learn exact operating boundaries.
* **Quantify Where Possible:** Prefer concrete bounds (`64-byte cache line`, `O(1) time`, `sub-100 µs`, `no_std`) over vague qualifiers (`fast`, `lightweight`, `scalable`).
* **Consistent Terminology:** Use exact canonical names across docs, specs, and code (`nexus-bits`, `IntEnum`, `BitField`, `#[repr(transparent)]`).

---

## 6. Pre-Publish Audit Checklist

- [ ] **BLUF Check:** Does the first sentence answer the reader's core question in bold?
- [ ] **Beginner Check:** Is every technical term introduced with simple intuition before jargon or code?
- [ ] **Visual Check:** Does every memory layout, protocol format, or multi-step flow include an ASCII diagram?
- [ ] **Scan Check:** Can a reader grasp the entire page structure solely by reading headings, bold lead-ins, diagrams, and tables?
- [ ] **One-Idea Check:** Does every bullet point express a single, self-contained thought?
- [ ] **Boundary Check:** Are invariants, error modes, and *"When **not** to use"* anti-patterns explicitly stated?
- [ ] **Copy-Paste Check:** Are all templates, schemas, and code blocks ready to run without manual cleanup?
