# Domain Docs

## Before exploring

- Read root `CONTEXT.md` if it exists.
- Read relevant records in `docs/adr/`.
- If either is absent, proceed without flagging it; create domain docs when concepts or decisions are resolved.

## Layout

This is a single-context repo:

- `/CONTEXT.md` — shared domain vocabulary and concepts.
- `/docs/adr/` — architecture decisions.

## Vocabulary and decisions

Use terms defined in `CONTEXT.md` for concepts in issues, proposals, and tests. If a needed term is missing, note it as a possible domain-modeling gap.

If work conflicts with an ADR, surface the conflict rather than silently overriding it.
