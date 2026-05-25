# Cascade unification — one parent-walking hierarchy

Design and phased plan for replacing the cascade module's three fixed-depth topologies
(entity → global, panel → global, child → panel → global) with one parent-walking resolution,
together with the split that moves cascade overrides off `WorldTextStyle`, `DiegeticPanel`, and
`PanelText` into one generic override component per attribute — so cascade membership is a property of
the entity tree, never of an incidentally shared component.

## Goal

One cascade. One `Resolved<A>` per logical attribute. One rule, applied by following `ChildOf`:
*my own override, else my parent's `Resolved<A>`, else the global default at the root.* A standalone
text is depth-1 off the root, a panel is depth-1, a panel label is depth-2; deeper nesting needs no
new type.

A node declares an override by carrying `Override<A>` — one generic component per attribute. Because
each attribute has exactly one override component type, and an entity cannot hold two of the same
component, "two overrides for one attribute on one node" has no representation. The `Exclude` marker
is unnecessary by construction.

## The mechanism is attribute-agnostic

Nothing here is specific to text. `CascadeAttr`, `Override<A>`, `Resolved<A>`, the resolution pass,
and the parent-walk are generic over the attribute. Any value that should resolve *my override, else
my parent's, else a global default* plugs in the same way — a panel background color, a line height —
as a new `CascadeAttr` impl plus a field on `CascadeDefaults`. No new plugin, trait, enum, or topology.

The initial attributes are two (see [Registered attributes](#registered-attributes)): `TextAlpha` and
`FontUnit`.

## Attribute value types and the override / resolved pair

The cascade is three generic pieces plus one pure value type per attribute:

```rust
// Pure value types — the cascade attributes. Wrapped in Override<A> / Resolved<A>;
// never inserted bare, so they are not Components themselves.
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct TextAlpha(pub AlphaMode);
#[derive(Clone, Copy, PartialEq, Debug, Reflect)]
pub(crate) struct FontUnit(pub Unit);

/// A node's own override for attribute `A`. The type parameter says *what* is
/// overridden; the value is the data. Exactly one component type per attribute, and
/// an entity holds at most one of any component — so "two sources for one attribute"
/// cannot be written down.
#[derive(Component, Clone, Copy, Reflect)]
pub(crate) struct Override<A: CascadeAttr>(pub A);

/// The cached resolved value for `A`. Exactly one per attribute per entity.
#[derive(Component, Clone, Copy, Reflect)]
pub(crate) struct Resolved<A: CascadeAttr>(pub A);
```

`Override<A>` is the input, `Resolved<A>` is the output — a matched generic pair. The value types stay
pure values, with no double duty as both the override component and the resolved payload.

### One attribute per logical value — including font unit

Both `TextAlpha` and `FontUnit` are single attributes. Standalone world text and panel labels are
drawn by different render systems but cascade the same value type; node *kind* selects the renderer,
not the attribute.

Font unit looked like it needed two types because standalone text and panels want different *defaults*
— world text in `Meters`, panel text in `Points`. It does not. The context difference is carried by the
cascade itself: the single global default (`CascadeDefaults::font_unit`, `Meters`) is the standalone
default, and the panel builder seeds `Override<FontUnit>(Points)` on every panel so everything under a
panel inherits `Points` through the parent-walk. This is the same way a panel seeds `Override<TextAlpha>`
for its subtree.

A consequence, by design: a panel is depth-1 with no `Resolved`-carrying ancestor, so the only way it
gets `Points` is to carry its own `Override<FontUnit>` — the builder therefore seeds it on **every**
panel, unconditionally. So no panel, existing or new, ever reads the cascade global; `font_unit`
governs standalone world text only. Changing `font_unit` at runtime re-resolves standalone text and
does **not** reach panels; a caller who wants panels retuned changes them explicitly (per panel, or the
builder seed). The seed value comes from `CascadeDefaults::panel_font_unit` (`Points`), read once at
panel construction — a construction-time seed, not a cascade global, the same role `layout_unit`
already plays.

## The override accessor

A node's own override is read generically — no per-kind source list, no enum, no match to keep
exhaustive:

```rust
fn override_at<A: CascadeAttr>(node: EntityRef<'_>) -> Option<A> {
    node.get::<Override<A>>().map(|o| o.0)
}
```

Every node kind — standalone, panel, label — declares an override the same way: by carrying
`Override<A>`.

## The split — overrides leave `WorldTextStyle`, `DiegeticPanel`, and `PanelText`

Today the overrides live as fields: alpha and unit on `WorldTextStyle` (`TextProps<ForStandalone>`),
`text_alpha_mode` and `font_unit` on `DiegeticPanel`, and a per-run `alpha_mode` on `PanelText`. The
cascade-source fields move to `Override<A>`:

- A standalone overrides alpha/unit by carrying `Override<TextAlpha>` / `Override<FontUnit>`, not by
  setting a `WorldTextStyle` field.
- A panel sets the default for the text under it by carrying `Override<TextAlpha>` (its builder
  `DiegeticPanel::screen().text_alpha_mode(x)` inserts the component) and **always** carries
  `Override<FontUnit>` (seeded from `CascadeDefaults::panel_font_unit` at construction, or the value
  the builder was given); children inherit both via the parent-walk.
- A label's per-run override is an `Override<TextAlpha>` on the label entity.

Because the cascade no longer reads `WorldTextStyle` / `DiegeticPanel` / `PanelText`, none of those is
a cascade source — the cascade reads only `Override<A>`. That is what makes membership a property of
the tree rather than of an incidentally shared component (the original crash was `WorldTextStyle`
shared between standalone text and panel labels).

Two constraints on the split:

- **`unit` has a second consumer.** The layout engine reads `config.unit()` (`layout/element.rs`) for
  point scale, so `unit` stays on `TextProps<ForLayout>` for measurement; on `TextProps<ForStandalone>`
  the field is documented as layout-measurement-only — its cascade-override role is now
  `Override<FontUnit>`, and render-time unit comes from `Resolved<FontUnit>`.
- **`world_scale` stays put.** A non-cascade field on `WorldTextStyle`, applied by the renderer as a
  post-cascade bypass; `Resolved<FontUnit>` encodes the `Unit` tier only.

## Membership — `Exclude` is gone, by construction

For any attribute `A` there is exactly one override component type, `Override<A>`, and an entity holds
at most one of any component. So "a node carrying two overrides for the same attribute" cannot be
written, and there is no shared multi-role component to mis-enroll. Node *kind* (standalone / panel /
label) is carried by the `WorldText` / `DiegeticPanel` / `PanelChild` markers and matters only for
which render system draws the entity — it is orthogonal to the cascade. `Exclude` / `ExcludeNone` are
deleted.

## Resolution — spawn observer + propagation pass

`Resolved<A>` is cached per entity. One resolver, with a spawn-time path and a change-time path.

- **Spawn.** An `On<Add>` observer computes a node's initial `Resolved<A>` by walking up `ChildOf`
  through ancestors' `Override<A>` to the global default — correct at any depth regardless of spawn
  order, because ancestors' `Override<A>` components are already present at command flush even before
  their own `Resolved<A>` exists. The observer inserts via a deferred command that applies within the
  same flush, before `CascadeSet::Propagate` runs. That is safe because the observer inserts onto the
  just-spawned (live) entity, and overrides now live on a non-shared component (the membership split):
  the insert cannot land on an entity a HUD rebuild freed the same frame. The split plus
  observer-before-system ordering is what closes the original crash. The cascade never runs an
  `On<Remove>` handler that defers an insert; removal is handled read-side, in the propagation pass.

- **Change.** A system in `CascadeSet::Propagate` re-resolves a node when any of these fire: its own
  `Override<A>` changes (`Changed<Override<A>>`, which an in-place `get_mut` edit triggers); its
  `Override<A>` is removed (`RemovedComponents<Override<A>>`, returning the node to inheriting); its
  `ChildOf` changes (`Changed<ChildOf>`, reparenting); its parent's `Resolved<A>` changes (flowed
  roots-first through `Children` / `iter_descendants`, reading the parent's cached value); or
  `CascadeDefaults` changes. It re-resolves through `query.get(entity)` on the live cascade query and
  inserts `Resolved<A>` only on a changed value — a despawned entity returns `Err` and is skipped, so a
  removal or reparent mid-frame can never write to a freed id. Removing an *ancestor's* override
  re-resolves the ancestor, whose `Resolved<A>` change then flows to descendants — no separate trigger
  needed. (This caching is the behavior the split would otherwise drop: today's readers re-read
  `WorldTextStyle` every frame, so nothing currently caches the override.)

  Two constraints follow from `RemovedComponents`, which is a per-read, double-buffered Bevy system
  param: the `CascadeSet::Propagate` system must run **every frame** — no run-condition that can skip
  it, or a frame's removals are cleared unread and the re-resolve is missed — and exactly **one** system
  per attribute reads `RemovedComponents<Override<A>>` (it is consumed on read); any secondary logic
  keys off the `Changed<Resolved<A>>` that system writes.

- **Bounded walk.** The parent walk is iterative with an explicit depth cap (`const CASCADE_DEPTH_CAP`,
  far above the real maximum of ~4) that terminates at the global default on exceed, plus a debug-only
  visited check; on exceed it `warn!`-logs in both debug and release so a malformed hierarchy is
  visible. A self-parent, a `ChildOf` cycle, a parentless node, or a dangling `ChildOf` after a parent
  despawns (Bevy does not clear it) all terminate at the global default — never a hang or panic.

- **No despawn in the pass.** The propagation pass never despawns entities (a module invariant tied to
  the original crash; documented, not enforced by a runtime assert) and the walk tolerates a dangling
  `ChildOf`, so a same-frame reconcile despawn cannot land an insert on a freed or reused id.

- **Ordering.** Observers fire during command flush, before `CascadeSet::Propagate`, so a same-batch
  spawn carries `Resolved<A>` before propagation runs. A code-level comment pins this invariant so a
  schedule reorder cannot silently break it.

Standalone world text and panels have no cascade parent, so they resolve to their own `Override<A>`
else the global default; the propagation pass re-runs them only when their override, their `ChildOf`,
or the default changes.

## Read side — entity-selection filters stay

`render_world_text` filters `Without<PanelChild>`; the panel-text systems filter `With<PanelChild>`.
These select which entities a render system draws (standalone vs panel text are drawn by different
systems) and are orthogonal to the cascade — keep them. Standalone text reads `Resolved<TextAlpha>` and
`Resolved<FontUnit>`; panel labels read `Resolved<TextAlpha>`; panel layout reads the panel's
`Resolved<FontUnit>`. A panel-side `Override<TextAlpha>` change bumps `Changed<Resolved<TextAlpha>>` and
can wake the standalone render query to a zero-entity no-op (its `Without<PanelChild>` filter) —
acceptable; revisit only if it shows on a profile.

## The `CascadeAttr` trait

```rust
pub(crate) trait CascadeAttr:
    Copy + PartialEq + Send + Sync + FromReflect + TypePath + Typed + GetTypeRegistration + 'static
{
    fn global_default(defaults: &CascadeDefaults) -> Self;
}
```

`Override<A>` and `Resolved<A>` are generic over it, and a blanket impl covers every value type meeting
the bounds. The reflection bounds (`FromReflect`, `TypePath`, `Typed`, `GetTypeRegistration`) are what
`register_type::<Override<A>>()` / `register_type::<Resolved<A>>()` require; `FromReflect: Reflect` is a
supertrait, so an explicit `Reflect` bound is redundant and the generic derives need no
`#[reflect(where …)]`. The value types are pure values (`Clone, Copy, PartialEq, Debug, Reflect`) and
are not `Component` — only `Override<A>` and `Resolved<A>` are. Reflection of the wrapped value requires
the value type itself to be registered (`register_type::<TextAlpha>()`, `register_type::<FontUnit>()`),
alongside `Override<A>` / `Resolved<A>`.

`global_default` takes only `&CascadeDefaults` and no context, by design: it keeps `CascadeDefaults` the
single resource the cascade reads. A per-context default (panels wanting `Points`) is expressed by a
seeded override on the panel, not by a second attribute — see [the font-unit
note](#one-attribute-per-logical-value--including-font-unit).

## Registered attributes

One manifest of every cascade, mirrored by a module-header table in code:

| Attribute   | Global default                | Resolution                                      | Override source today |
| ---         | ---                           | ---                                             | --- |
| `TextAlpha` | `CascadeDefaults::text_alpha` | panel → label inherit; standalone own → global  | `WorldTextStyle.alpha_mode`, `DiegeticPanel.text_alpha_mode`, `PanelText.alpha_mode` |
| `FontUnit`  | `CascadeDefaults::font_unit` (standalone) | standalone own → global; panels carry a seeded override (`panel_font_unit`) that children inherit | `WorldTextStyle.unit` (cascade role), `DiegeticPanel.font_unit` |

`CascadeDefaults::panel_font_unit` is the panel builder's construction-time seed for `Override<FontUnit>`
— not a cascade global. `CascadeDefaults::layout_unit` is likewise read once at panel construction and
not cascade-propagated.

## Implementation phases

Each phase compiles green and commits as a unit.

### Phase 1 — move the cascade-source fields into `Override<A>`

- Add the value types (`TextAlpha`, `FontUnit`) and the generic `Override<A>`.
- Repoint each existing cascade reader to source from `Override<A>` by changing its `type Override` to
  the matching `Override<A>` and unwrapping (`override_value` / `panel_value` become `o.0`). The fixed
  topologies keep running; only the override *source* changes from a field to a component, so behavior
  is preserved.
- Move the cascade-source fields into components: `WorldTextStyle.alpha_mode` → `Override<TextAlpha>`,
  `WorldTextStyle.unit` (cascade role) → `Override<FontUnit>`, `DiegeticPanel.text_alpha_mode` →
  `Override<TextAlpha>`, `DiegeticPanel.font_unit` → `Override<FontUnit>`. Keep `unit` on
  `TextProps<ForLayout>` for measurement; keep `world_scale`. Retire `with_alpha_mode` / `with_unit`;
  update the panel builder to insert the components.
- `PanelText.alpha_mode` is **not** a cascade source — it is a per-run transient set in shaping from
  `config.alpha_mode()` and read as the label's tier-1 override. It stays through Phase 1; the label's
  per-run override moves to `Override<TextAlpha>` in Phase 2 (below), where the same readers are
  already being rewritten.
- Sweep every caller. `rg "with_alpha_mode|with_unit|text_alpha_mode|as_standalone|as_layout_config"`
  plus the field accessors, including examples and doctests. Both `TextProps::as_standalone()`
  (`layout/text_props.rs:598`) and `TextProps::as_layout_config()` (`:741`) copy the removed fields and
  must stop; their callers `render/panel_text/reconcile.rs`, `render/panel_text/shaping.rs`, and
  `render/world_text/shaping.rs` depend on the fixed methods. `panel/builder.rs` and
  `examples/text_alpha.rs` call `.text_alpha_mode()` on the *builder* (a builder-local field) — no
  component change there.

Commits together: the data move and every reader of the old override path.

### Phase 2 — collapse the three topologies into one parent-walking cascade

- Rename the existing `CascadeAttribute` trait to `CascadeAttr` (it already carries the reflection
  bounds above). Add the generic `Resolved<A>` and one hierarchical cascade plugin: a spawn-time
  `On<Add>` observer for initial resolution + a roots-first propagation pass in `CascadeSet::Propagate`
  gated on `Changed<Override<A>>`, `RemovedComponents<Override<A>>`, `Changed<ChildOf>`, parent
  `Changed<Resolved<A>>`, and `CascadeDefaults` changes; bounded walk (`CASCADE_DEPTH_CAP` + debug-only
  visited check + warn-on-exceed). Pair each attribute with `register_type` for `Override<A>`,
  `Resolved<A>`, and the value type.
- Unify the two font-unit cascades into one `FontUnit` attribute: one cascade global
  (`CascadeDefaults::font_unit`, the standalone default), and the panel builder always seeds
  `Override<FontUnit>` from `CascadeDefaults::panel_font_unit` at construction so panel subtrees inherit
  `Points` via the parent-walk. `panel_font_unit` becomes a construction-time seed (no longer a cascade
  global).
- Repoint every reader/filter from the per-role `Resolved<…>` types to `Resolved<TextAlpha>` /
  `Resolved<FontUnit>`: `world_text/mod.rs`, `rendering.rs` (`ChangedWorldTextQuery` and the tier-1
  re-resolve helpers), `panel_text/shaping.rs`, `panel_text/alpha.rs`, and the panel font-unit read.
  Run `rg "Resolved<(World|Panel)(TextAlpha|FontUnit)>"` to confirm none are missed. **Keep** the
  `With` / `Without<PanelChild>` entity-selection filters.
- Move the label's per-run override: delete `PanelText.alpha_mode`, insert `Override<TextAlpha>` on the
  label at reconcile time, and read it through the parent-walk like any other override.
- Delete the per-role types (`WorldTextAlpha` / `PanelTextAlpha` / `WorldFontUnit` / `PanelFontUnit`),
  the three plugins (`CascadeEntityPlugin` / `CascadePanelPlugin` / `CascadePanelChildPlugin`), and
  `Exclude` / `ExcludeNone` plus their test impls.

Commit atomically: the new plugin, the reader repoint, and the old-type deletions are mutually
dependent — they cannot land separately and stay green, because old and new both writing `Resolved` in
one frame would clobber. That same atomic deletion is what guarantees a single writer: once the old
per-role types and plugins are gone, two writers for one attribute is unrepresentable — no runtime guard
or plugin-count assert is needed (an accidental double-add of the same generic plugin is caught by
Bevy's built-in plugin uniqueness).

### Phase 3 — verification + cascade example

- Verify: cross-enrollment is impossible by construction (a standalone and a panel label resolve
  independently); a same-command-batch panel+child spawn resolves in one frame; an in-place `get_mut`
  edit of an `Override<A>` re-resolves; removing an `Override<A>` returns the node to inheriting;
  reparenting a child re-resolves against the new parent; a panel always carries `Override<FontUnit>`
  and a runtime `CascadeDefaults::font_unit` change re-resolves standalone text but not panels; cycling
  all alpha modes in `text_alpha.rs` stays correct; a `ChildOf(self)` self-parent and a two-node cycle
  terminate at the global default with no hang or panic.
- Reflection sweep: `rg` for lingering `Resolved<World…>` / `Resolved<Panel…>` references; confirm
  every attribute registers `Override<A>`, `Resolved<A>`, and its value type.
- Add a cascade demonstration example: one scene showing `TextAlpha` resolving at each of three tiers —
  global default → panel override → per-run label override — and `FontUnit` resolving global default
  (standalone) vs. panel-seeded override inherited by a label, with on-screen labels showing which tier
  won.

## Changed names

| Was | Now | Location |
| --- | --- | --- |
| `PanelSlugTextRun` | `PanelText` | `render/panel_text/mod.rs` |
| `render/text_renderer/` module | `render/panel_text/` | — |
| `PanelTextChild` (marker) | `PanelChild` | `render/world_text/mod.rs` (next to `WorldText`) |
| `CascadeAttribute` (trait) | `CascadeAttr` | `cascade/resolved.rs` |
| alpha/unit fields on `WorldTextStyle` / `DiegeticPanel` / `PanelText` | `Override<TextAlpha>` / `Override<FontUnit>` components | the override is a generic component, not a field |
| per-role `Resolved<WorldTextAlpha>` / `Resolved<PanelTextAlpha>` | `Resolved<TextAlpha>` | one resolved type per attribute |
| per-role `Resolved<WorldFontUnit>` / `Resolved<PanelFontUnit>` | `Resolved<FontUnit>` | one font-unit attribute; panel `Points` is a seeded override, not a second type |
| `CascadeDefaults::world_font_unit` | `CascadeDefaults::font_unit` | sole cascade global for `FontUnit` (standalone default) |
| `CascadeDefaults::panel_font_unit` (live global) | construction-time seed for the panel's `Override<FontUnit>` | no longer cascade-propagated |
| `WorldTextStyle` = `TextProps<ForStandalone>` | unchanged; **loses** `alpha_mode` / `unit` as cascade overrides (`unit` stays for layout measurement) | `layout/text_props.rs` |
