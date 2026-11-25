# Documentation & Specification Output Rules

Write all technical documentation, specifications, and code explanations using these strict formatting and pedagogical rules:

1. **Bottom-Line Up Front (BLUF):** Start every response and major section with **one bolded sentence** stating the core answer, rule, or architectural takeaway.
2. **Beginner-First Clarity:** Assume the reader is new to low-level systems. Explain every concept in simple, everyday words first, define unavoidable jargon immediately on first use, and build intuition before showing code.
3. **Visual Illustrations:** Use clean ASCII diagrams, bit/byte memory maps, data-flow arrows (`[A] -> [B]`), and step-by-step state transitions to illustrate memory layouts, hardware boundaries, and multi-step mechanics.
4. **Zero Filler:** Omit conversational preambles, transitions, and wrap-ups. Start immediately with the technical content.
5. **Scannable Hierarchy:** Use sentence-case markdown headings, compact comparison tables (for 3+ items), and bulleted lists with **bold lead-in terms** and strictly one idea per bullet.
6. **Plain, Prescriptive Language:** Use short sentences, common verbs, and direct commands (*"Convert endianness at the boundary"*, not *"You should consider converting..."*).
7. **Systems & Hardware Grounding:** Anchor explanations in concrete execution constraints—memory layout, 64-byte cache lines, ABI guarantees, thread boundaries, `no_std`, and failure modes—using everyday analogies where helpful.
8. **Explicit Boundaries:** Pair usage guidance with anti-patterns (*When to use* vs. *When **not** to use*) and quantify limits (`O(1)`, bit widths, latency budgets) instead of vague adjectives.
9. **Copy-Paste Ready:** Keep all code blocks, schemas, and templates self-contained, minimal, and immediately usable without manual cleanup.
9. **Copy-Paste Ready:** Keep all code blocks, schemas, and templates self-contained, minimal, and immediately usable without manual cleanup.

For full formatting examples, ASCII diagram templates, and pre-publish checklists, read `guides/STYLE_GUIDE.md` when performing documentation reviews or audits.

