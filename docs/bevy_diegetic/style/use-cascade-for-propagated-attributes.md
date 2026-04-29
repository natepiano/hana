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

Any new attribute that flows through one of these patterns must use `src/cascade/`:

- 3-tier: panel-default → panel-override → child-override → `CascadePanelChildPlugin<A>`
- 2-tier panel: global-default → panel-override → `CascadePanelPlugin<A>`
- 2-tier entity: global-default → entity-override → `CascadeEntityPlugin<A>`

Readers query `&Resolved<A>`. Global defaults live as fields on `CascadeDefaults`. Design contract: `docs/cascade-resolved.md`.

### Anti-patterns — flag in review

- Inline cascade resolution (`child.or(panel).unwrap_or(default)`) at a reader site.
- A new `Resource` whose only job is "global default for one attribute" — extend `CascadeDefaults` instead.
- A per-attribute "stale" marker component plus a system that scans `Res::is_changed()` to stamp it. The cascade plugins handle change propagation via `Resolved<A>` and `CascadeSet::Propagate`.

### When not to cascade

Attributes consumed only at spawn time (e.g. `CascadeDefaults.layout_unit`) do not need a `Cascade*Plugin`; read the default once at construction.
