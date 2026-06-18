# Note template

Copy this into `docs/src/concepts/<concept>.md` when a ladder is finished, then
fill every section. Keep it tight — this is a *reference you'll re-read*, not a
transcript of the ladder.

```markdown
# <Concept name>

> Ladder: [`src/bin/<concept>.rs`](https://github.com/utkarsh5026/rust-scratch/blob/master/src/bin/<concept>.rs) ·
> Run: `cargo run --bin <concept>` · Phase N · M rungs

## TL;DR

<The one-paragraph mental model. If you can't write this, you're not done.>

## Why it exists

<The concrete problem this concept solves. What goes wrong without it?>

## The ladder

| # | Tier | Rung | The lesson |
|---|------|------|------------|
| 1 | foundations | <rung> | <one line> |
| … | | | |
| M | capstone | <rung> | <one line> |

## Signatures to know

<The std types / trait defs / bounds worth memorizing, in fenced rust blocks.>

## Footguns

- **<trap>.** <what bites you and the fix.>

## Explain it back

- <question you should answer cold>
- …

## See also

- <related concept notes>
```
