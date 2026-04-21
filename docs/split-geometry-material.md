# Split geometry from material updates in text renderers

## Status

Skeleton — not a plan yet. Captures the observation so it doesn't get lost.

## Problem

`render_world_text` (and by analogy `shape_panel_text_children` / `build_panel_batched_meshes`) treats each processed entity as a single atomic work unit: shape the text through parley, look up glyphs in the MSDF atlas, spawn mesh children with configured materials. When anything on the entity changes, the whole pipeline re-runs.

Some changes don't need the whole pipeline. After the cascade refactor (see `cascade-resolved.md`), a mutation to `CascadeDefaults.text_alpha` fires `Changed<Resolved<WorldTextAlpha>>` on affected entities. That's a precise signal — only the alpha mode changed, not the text, not the style, not the font size. But the current reader body doesn't differentiate: it despawns the mesh children, re-shapes "hello" through parley, re-looks-up every glyph, and re-spawns meshes. Wasted work — only the material's `alpha_mode` field actually needed to change.

## What it might look like

Split the reader's work into two passes:

1. **Geometry pass.** Fires on `Or<Changed<WorldText>, Changed<WorldTextStyle>>` (or the analogous "content changed" signals). Does parley shaping, atlas lookup, mesh geometry construction.
2. **Material pass.** Fires on `Or<Changed<Resolved<WorldTextAlpha>>, ...other cascaded material params...>`. Mutates material asset fields in place (or swaps handles) without touching geometry.

An entity with both change classes fires both passes in one frame.

## Open questions

- Can `MsdfTextMaterial` support in-place mutation via `materials.get_mut()` cleanly, or do we always need a new handle?
- Does `StandardMaterial` on the text path share the same constraints?
- How does this interact with `SharedMsdfMaterials` caching for panel text?
- What's the right Bevy idiom for "material params changed, geometry didn't" in 0.18?

## When to pursue

Profile first. If alpha-mode toggling at interactive rates never shows up as a frame-time problem, this optimization isn't worth the reader-system refactor complexity. Revisit if a real workload makes the reshape waste visible.

## Relationship to cascade-resolved

The cascade refactor is a prerequisite: without per-entity `Resolved<A>` components and their `Changed<T>` signals, the reader has no way to distinguish "material param changed" from "everything changed." The cascade refactor lands; this is a future follow-up.
