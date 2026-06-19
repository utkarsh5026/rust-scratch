---
description: Read a Rust bin and write a deep, first-principles doc page into the mdBook site
argument-hint: <bin-name>
allowed-tools: Read, Write, Edit, Glob, Bash(mdbook:*)
---

Turn a concept ladder (or any Rust bin) into a **readable, in-depth, first-principles
doc page** in the `docs/` knowledge base that publishes to GitHub Pages.

Target: $ARGUMENTS

## 1. Locate & read the source

- Resolve the file: `src/bin/$ARGUMENTS.rs` (accept a bare name, a `.rs` name, or a
  full path). If `$ARGUMENTS` is empty or the file doesn't exist, `Glob src/bin/*.rs`,
  list the options, and ask which one.
- **Read the WHOLE file.** Pay attention to: the top `// Concept:` / `// Ladder:`
  comment, every problem's doc-comment block (the *why*), each function body, the
  `check_N` assertions (they reveal the intended behavior), and any capstone.
- Don't skim. The doc must reflect what the code *actually does*, not a generic
  explanation of the topic.

## 2. Write the doc page

Create `docs/src/concepts/<kebab-name>.md` (kebab-case the bin name, e.g.
`cell_refcell` → `cell-refcell.md`). Write for **future-you re-reading in 6 months** —
readable, with real depth, building understanding from first principles.

Requirements for the page:

- **First principles, not hand-waving.** Don't just say *what* a thing is — explain
  *why it has to exist*: what problem it solves, what breaks without it, what the
  compiler is enforcing and why. Derive each idea from the one before it.
- **Readable & skimmable.** Short paragraphs, headings, tables, and fenced ```rust
  blocks. Lead each section with the punchline, then explain.
- **No emojis.** Keep the prose clean — convey emphasis with headings, **bold**,
  blockquote callouts, and tables, not icons.
- **Concrete code from the actual file.** Quote the real signatures, bounds, and
  tricky lines — then unpack them piece by piece. Put the wrong/naive version next
  to the right one (label them in a comment, e.g. `// WRONG` / `// OK`) where the
  ladder sets up that contrast.
- **Depth, earned step by step.** Foundations → mechanics → the subtle edge cases
  and footguns → real-world patterns → the from-scratch capstone insight. A reader
  should come away able to *re-derive* the concept, not just recognize it.

Suggested page shape (adapt to the concept — depth over rigid structure):

```markdown
# <Concept name>

> Ladder: [`src/bin/<name>.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/<name>.rs) ·
> Run: `cargo run --bin <name>` · Phase N · M rungs

## TL;DR
<the mental model in a few sentences>

## Why this exists (from first principles)
<the problem; what fails without it; what the compiler guarantees and why>

## The ladder at a glance
<table: # | tier | rung | the lesson>

## The ideas, built up
<the core sections — each a concept derived from the last, with real code unpacked,
 wrong-vs-right contrasts, and the key insights. This is the heart of the page.>

## Footguns
<the traps the ladder set, what bites, and the fix>

## Real-world patterns
<how this shows up in actual APIs / std>

## Capstone insight
<the structural "aha" from the build-it-from-scratch rung>

## Explain it back
<questions future-you should answer cold>

## See also
<related concept notes>
```

## 3. Wire it into the site

- Add the page under the correct **Phase** heading in `docs/src/SUMMARY.md`
  (a page not listed there is **not** built).
- Tick its row (or add one) in the completed-concepts table in `docs/src/intro.md`.

## 4. Verify (if mdBook is available)

- If `mdbook` is on PATH, run `mdbook build docs` and confirm it builds with no
  broken-link warnings. If it's not installed, skip silently and just say so.
- Report: the file written, and a one-line summary of what the doc covers.

Don't modify the source `.rs` file — this is read-and-document only.
