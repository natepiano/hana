---
date_created: '[[2026-04-29]]'
date_modified: '[[2026-04-29]]'
tags:
- rust
- bevy
- bevy_diegetic
- cascade
mechanism: llm
---
## Use the cascade framework for propagated attributes

Any new attribute that inherits through the entity tree must use `src/cascade/`:

- global default → parent override → child override → `CascadePlugin<A>`
- global default → entity override → `CascadePlugin<A>`

Readers query `&Resolved<A>` internally or use the typed public `resolved_*`
reader. Global defaults live in per-attribute `CascadeDefault<A>` resources, not
as fields on `PanelDefaults`.

### Anti-patterns — flag in review

- Inline cascade resolution (`child.or(panel).unwrap_or(default)`) at a reader site.
- A cascade-owned standalone `TextStyle` setter or builder. Use typed
  `EntityCommands` verbs such as `override_text_alpha` / `inherit_text_alpha`.
- A per-attribute "stale" marker component plus a system that scans `Res::is_changed()` to stamp it. The cascade plugins handle change propagation via `Resolved<A>` and `CascadeSet::Propagate`.

### When not to cascade

Attributes consumed only at construction time (for example `layout_unit` and
panel-construction defaults) do not need `CascadePlugin<A>`; read the default
once at construction.
