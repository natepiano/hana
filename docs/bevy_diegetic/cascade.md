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
  `DiegeticPanel::screen().text_alpha_mode(x)` records the field; a spawn-time bridge observer inserts
  the component — see [Implementation phases](#implementation-phases) for why emission is a bridge, not a
  `build()` return) and **always** carries `Override<FontUnit>` (the bridge inserts it from
  `CascadeDefaults::panel_font_unit`, or the value the builder was given); children inherit both via the
  parent-walk.
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

Each phase compiles green and commits as a unit. The original single "move every cascade field into
`Override<A>`" step is split in two — panel-side first, standalone-side second — for cross-fire
isolation, *not* a difference in authoring mechanism. **Both sides feed `Override<A>` the same way:
through an `On<Add>` authoring-bridge observer.** The cascade-source fields stay on their structs
(`DiegeticPanel`, `WorldTextStyle`) as authoring inputs; the bridge reads them at spawn and inserts
`Override<A>` only when the field is `Some`. The fields stop being a *cascade source* — the cascade
reads `Override<A>` — but they are not deleted (`build()` and `WorldTextStyle::new()` are unchanged).

- The **panel** move introduces `Override<A>` on panel entities only. The standalone cascades still read
  `WorldTextStyle`, so each `Override<A>` is observed by exactly one cascade — no cross-fire.
  Self-contained.
- The **standalone** move makes `Override<A>` shared by the standalone *and* panel cascades while the
  per-role plugins still run, so each is then observed by both. Splitting it into its own phase confines
  that shared-component coexistence to one phase, which the Phase-3 collapse removes.

**Why a spawn bridge and not "`build()` emits the components".** `build()` returns one `DiegeticPanel`
value (and `WorldTextStyle::new()` one `WorldTextStyle`) that callers drop into a *by-value* spawn tuple
— `commands.spawn((Marker, panel, Transform))`, sometimes `.observe(..)`. A returned value has one
compile-time type, so it cannot carry "component present on this call, absent on that call": the only
type that means *maybe a component* is `Option<Override<A>>`, and Bevy does **not** implement `Bundle`
for `Option<C>` (checked in the local `bevy_ecs` tree). Making `build()` carry the on/off decision would
therefore require an `Option` stuffed into a transient carrier component that an observer collapses to
presence/absence — pure plumbing around the value-return. The bridge avoids it: the `Option` stays where
it already lives (the struct field), the observer reads it at spawn, and `build()`,
`WorldTextStyle::new()`, and every call site stay untouched. The conditionality is required by the design
itself, not just these phases — Phase 3 has `.text_alpha_mode(x)` insert `Override<TextAlpha>` only when
called — and the bridge is doubly necessary for `FontUnit`: the Phase-3 always-on panel seed comes from
`CascadeDefaults::panel_font_unit`, a resource the pure `build()` cannot read but the observer can.

### Phase 1 — panel-side: cascade reads `Override<A>`, fed by a spawn bridge ✅ complete

- Add the value types (`TextAlpha`, `FontUnit`) and the generic `Override<A>`. Register
  `Override<TextAlpha>`, `Override<FontUnit>`, and each value type — `Override<A>` is generic, so
  registration is manual, the same as the existing `register_type::<Resolved<A>>()` (see
  [the trait section](#the-cascadeattr-trait)).
- **Keep `text_alpha_mode` / `font_unit` (and their accessors) on `DiegeticPanel`** as authoring inputs.
  The builder's `.text_alpha_mode()` / `.font_unit()` setters, the `build_panel` write, and `build()`'s
  `Result<DiegeticPanel, InvalidSize>` return are all unchanged. What changes: these fields are **no
  longer a cascade source** — the cascade reads `Override<A>` instead.
- Add a **panel authoring bridge** — an `On<Add, DiegeticPanel>` observer (with `Res<CascadeDefaults>`)
  that inserts `Override<TextAlpha>` from `panel.text_alpha_mode()` *only when `Some`* and
  `Override<FontUnit>` from `panel.font_unit()` *only when `Some`*. Field absent → component absent →
  the cascade's "inherit" signal. This is the panel twin of the Phase-2 standalone bridge.
- Repoint only the panel cascades to source from the components: `PanelTextAlpha::PanelOverride =
  Override<TextAlpha>`, `PanelFontUnit::Override = Override<FontUnit>` (`panel_value` / `override_value`
  unwrap `o.0`). The 3-tier and 2-tier topologies keep running; only the override *source* changes from a
  field to a component.
- Leave the standalone cascades (`WorldTextAlpha`, `WorldFontUnit`) untouched — they still read
  `WorldTextStyle.alpha_mode` / `.unit`, so `new(Pt(..))` standalone sizing is unchanged. `Override<A>`
  lives only on panels this phase, so each is observed by exactly one cascade — no cross-firing.
- `PanelText.alpha_mode` stays (a per-run transient set in shaping from `config.alpha_mode()`); the
  label's per-run override moves to `Override<TextAlpha>` in Phase 3.

**Behavior preservation (verified against the code).**

- *No-override panel.* With no `text_alpha_mode`, the panel gets no `Override<TextAlpha>`, so
  `on_panel_added::<PanelTextAlpha>` never fires and the panel has no `Resolved<PanelTextAlpha>`. The
  reader `apply_panel_result` (`render/panel_text/shaping.rs`) already falls back to
  `PanelTextAlpha::global_default` when the panel's `Resolved` is missing, and children resolve via
  `on_panel_child_added` reading the absent parent override → global default. Same effective value as
  today.
- *Runtime global mutation.* `examples/text_alpha.rs` mutates `CascadeDefaults::text_alpha` at runtime
  (`defaults.text_alpha = state.alpha_mode`), but every panel there calls `.text_alpha_mode(Blend)`, so
  all panels carry an override and are unaffected — the runtime mutation drives standalone `WorldText`
  only. Conditional emission does not regress it.
- *Font-unit layout.* `compute_panel_layouts` re-lays-out only on `Ref<DiegeticPanel>::is_changed()` or a
  pending tree change; it does **not** watch `Changed<Resolved<PanelFontUnit>>`. So a runtime
  `panel_font_unit` change does not re-layout panels today either way, and `Override<FontUnit>` being
  conditional (this phase) vs. always-present (Phase 3) is unobservable for layout. The reader
  (`compute_layout.rs:98`) falls back to `PanelFontUnit::global_default` when `Resolved<PanelFontUnit>` is
  absent.
- *Spawn-order race.* The bridge inserts `Override<A>` via a deferred command, one flush behind the
  panel. `PanelText` children are spawned by the shaping system in a later schedule, never co-spawned with
  the panel, so `on_panel_child_added` reading the parent's `Override<TextAlpha>` is race-free.

Commits together: the value types + `Override<A>` + registration, the panel bridge observer, and the two
repointed panel-cascade readers. `DiegeticPanel`'s fields/accessors, the builder, `build()`, and all ~30
panel spawn sites are untouched.

### Retrospective

**What worked:**
- `Override<A>` mirrors `Resolved<A>`'s exact where-clause bounds; compiled first try, with manual
  `register_type` per monomorphization (`Override<TextAlpha>`, `Override<FontUnit>`) + value types as planned.
- The cascade repoint collapses to `Some(o.0.0)` for both `PanelTextAlpha::panel_value` and
  `PanelFontUnit::override_value` — component presence *is* the override signal, so the `Option` is always
  `Some`. All 158 crate tests pass; examples and `build()` callers untouched.

**What deviated from the plan:**
- The bridge observer (`seed_panel_overrides`, `panel/diegetic_panel.rs`) omits the `Res<CascadeDefaults>`
  the Phase-1 prose parenthesized. Both inserts are "only when `Some`," which read nothing from defaults; an
  unused param would fail the no-dead-code style rule. `Res<CascadeDefaults>` is added in Phase 3 when the
  FontUnit branch becomes always-on and seeds from `panel_font_unit`. The parenthetical was a forward-reference.

**Surprises:**
- The bridge had to live in `HeadlessLayoutPlugin`, not `TextRenderPlugin`: headless `PanelFontUnit`
  resolution depends on `Override<FontUnit>` being inserted even with no text renderer present. As a
  consequence `Override<TextAlpha>` is also inserted/registered in headless mode — harmless, since its cascade
  plugin (`CascadePanelChildPlugin::<PanelTextAlpha>`, in `TextRenderPlugin`) is absent there, so the component
  just sits unread.
- Non-override panels now carry *no* `Resolved<PanelTextAlpha>` at all (previously every panel got one,
  because `on_panel_added` fired on `On<Add, DiegeticPanel>`). The reader's existing `global_default` fallback
  (`apply_panel_result`, `compute_layout.rs:98`) covers them — matches the behavior-preservation analysis.

**Implications for remaining phases:**
- Phase 2's standalone bridge (`On<Add, WorldTextStyle>`) can copy `seed_panel_overrides` almost verbatim —
  same placement logic, same `Some(o.0.0)` projection.
- Phase 3 *edits* the existing `seed_panel_overrides` (adds `Res<CascadeDefaults>`, flips FontUnit to
  always-insert), rather than creating the bridge.
- Registration of `Override<TextAlpha>`/`Override<FontUnit>` + value types currently lives in
  `HeadlessLayoutPlugin`. Phase 3's unified plugin must take ownership of (or leave) this registration so no
  dangling/duplicate `register_type` remains once the per-role plugins are deleted.

### Phase 1 Review

Architect review of the remaining phases against the Phase-1 code. All findings were determinate plan
corrections (no user-facing forks) and were folded into Phases 2–4:

- **Phase 2 cross-fire mechanism corrected** — the prose claimed "the single `Exclude` marker cannot express
  not-a-panel-and-not-a-panel-child." The real wiring is `WorldTextAlpha`/`WorldFontUnit` already set
  `Exclude = PanelChild` and `PanelTextAlpha` (3-tier) has none; rewrote the paragraph to name the two actual
  spurious resolves (`Resolved<WorldTextAlpha>` on panels, `Resolved<PanelTextAlpha>` on standalones) and why
  each is unread.
- **Phase 2 reader edit-sites named** — the "tier-1 re-resolve reads the component" bullet now names
  `re_resolve_world_font_unit` / `re_resolve_world_text_alpha` and **adds** widening `ChangedWorldTextQuery`'s
  `Or<…>` with `Changed<Override<TextAlpha>>` / `Changed<Override<FontUnit>>` (else runtime alpha/unit edits
  stop re-rendering standalone).
- **Phase 3 registration de-duplicated** — the bullet double-counted Phase-1's `Override<A>`/value-type
  registration; reworded to "add `Resolved<A>`, relocate the existing registration," and added that the
  unified plugin must register in both `HeadlessLayoutPlugin` and the render path.
- **Phase 3 schedule/ordering pinned** — propagation stays in `CascadeSet::Propagate` (`Update`); the spawn
  observer covers `PostUpdate`-spawned labels the same frame; readers (`PostUpdate`) follow by schedule order.
- **Phase 3 standalone helpers deleted, not repointed** — keeping `re_resolve_world_*` alongside the cached
  unified pass would make two writers of standalone `Resolved<…>`; the bullet now says delete them.
- **Phase 3 font-unit made concrete** — flip `seed_panel_overrides` FontUnit branch to unconditional + add
  `Res<CascadeDefaults>`; rename `CascadeDefaults::world_font_unit` → `font_unit` (struct + docstrings); noted
  the `compute_layout.rs:98` fallback becomes unreachable for panels.
- **Phase 3 label override edit-site named** — the move must replace `apply_panel_result`'s hand-rolled
  child resolution (and the `build_panel_text` per-run set), not just delete `PanelText.alpha_mode`.
- **Phase 4 topology check added** — verify a label is a `ChildOf` descendant of its panel with no
  `Resolved`-bearing intermediate.

### Phase 2 — standalone-side field→component move

- Move the standalone cascade-source fields into components: `WorldTextStyle.alpha_mode` (cascade role)
  → `Override<TextAlpha>`, `WorldTextStyle.unit` (cascade role) → `Override<FontUnit>`. Keep `unit` on
  `TextProps<ForLayout>` for measurement (`layout/element.rs` reads `config.unit()`); keep `world_scale`
  as the post-cascade bypass. Retire `with_alpha_mode` / `with_unit` (no callers).
- Add a standalone authoring bridge: an `On<Add, WorldTextStyle>` observer that seeds `Override<FontUnit>`
  from `style.unit()` (when `Some`) and `Override<TextAlpha>` from `style.alpha_mode()` (when `Some`), so
  `WorldTextStyle::new(Pt(..))` keeps authoring the override and the three unit examples render
  unchanged. This bridge is permanent — the same observer-bridge pattern Phase 1 introduces for panels,
  applied to the standalone authoring struct.
- Repoint the standalone cascades to source from the components: `WorldTextAlpha::Override =
  Override<TextAlpha>`, `WorldFontUnit::Override = Override<FontUnit>` (`override_value` returns
  `Some(o.0.0)` — presence is the override). The reader-side tier-1 re-resolve lives in
  `re_resolve_world_font_unit` / `re_resolve_world_text_alpha` (`render/world_text/rendering.rs`), which
  today read `style.unit()` / `style.alpha_mode()`; repoint both to read `Override<FontUnit>` /
  `Override<TextAlpha>` through a new query param (the fields now return `None`). **Also** widen
  `ChangedWorldTextQuery` (`rendering.rs:30`): its `Or<…>` wakes `render_world_text` on
  `Changed<WorldTextStyle>`, which no longer fires for alpha/unit edits — add `Changed<Override<TextAlpha>>`
  and `Changed<Override<FontUnit>>`, or a runtime override edit stops re-rendering standalone text.
- Sweep the standalone-side `as_standalone` / `as_layout_config` callers
  (`render/panel_text/reconcile.rs`, `render/panel_text/shaping.rs`, `render/world_text/shaping.rs`):
  `TextProps::as_standalone()` (`layout/text_props.rs:598`) and `as_layout_config()` (`:741`) keep
  copying `unit` for measurement; their cascade-override role now flows through `Override<A>`.
- Transient cross-fire (removed in Phase 3): now `Override<TextAlpha>` / `Override<FontUnit>` are shared
  by the standalone and panel cascades. The standalone 2-tier attributes already set `Exclude = PanelChild`
  (`render/world_text/mod.rs:170,194`); `PanelTextAlpha` is the 3-tier panel cascade and its
  `on_panel_added` query carries no `Exclude`. So after the repoint two spurious resolves appear: a *panel*
  (which is `Without<PanelChild>`) has `on_cascade_target_added::<WorldTextAlpha>` fire on its
  `Override<TextAlpha>`, writing a spurious `Resolved<WorldTextAlpha>` onto the panel; and a *standalone*
  has the unfiltered `on_panel_added::<PanelTextAlpha>` fire on its `Override<TextAlpha>`, writing a
  spurious `Resolved<PanelTextAlpha>` onto the standalone. Both are harmless: `render_world_text` filters
  `With<WorldText>, Without<PanelChild>` and never matches a panel, and the panel-text readers filter
  `With<PanelChild>` / `With<DiegeticPanel>` and never match a standalone, so neither spurious `Resolved`
  is ever read. It disappears when Phase 3 deletes the per-role plugins.

Commits together: the standalone data move, the authoring bridge, and the two standalone-cascade readers.

### Phase 3 — collapse the three topologies into one parent-walking cascade

- Rename the existing `CascadeAttribute` trait to `CascadeAttr` (it already carries the reflection
  bounds above). Add the generic `Resolved<A>` and one hierarchical cascade plugin: a spawn-time
  `On<Add>` observer for initial resolution + a roots-first propagation pass in `CascadeSet::Propagate`
  gated on `Changed<Override<A>>`, `RemovedComponents<Override<A>>`, `Changed<ChildOf>`, parent
  `Changed<Resolved<A>>`, and `CascadeDefaults` changes; bounded walk (`CASCADE_DEPTH_CAP` + debug-only
  visited check + warn-on-exceed). `Override<A>` and the value types are **already registered** (Phase 1,
  in `HeadlessLayoutPlugin`); Phase 3 only adds `register_type::<Resolved<A>>()` and **relocates** the
  existing `Override<A>`/value-type registration onto the unified plugin, so nothing is stranded when the
  per-role plugins (each of which emits the current `register_type::<Resolved<per-role>>()`) are deleted.
- Schedule + spawn ordering (state it explicitly): keep the propagation pass in `CascadeSet::Propagate`
  (`Update`); the unified `On<Add>` observer computes a node's initial `Resolved<A>` synchronously at
  spawn, which covers panel labels spawned in `PostUpdate` by `reconcile_panel_text_children` /
  `shape_panel_text_children` — those get a correct `Resolved` the same frame from the observer (it walks
  up to the panel's already-present `Override<A>`), and any *later* parent/override/default change flows
  next frame through the `Update` pass. All readers run in `PostUpdate`, after `CascadeSet::Propagate` by
  schedule order. The unified plugin must be added in **both** `HeadlessLayoutPlugin` (headless `FontUnit`
  resolution with no text renderer) and the render path — today the panel cascade lives in
  `HeadlessLayoutPlugin`, the alpha cascades in `TextRenderPlugin`.
- Unify the two font-unit cascades into one `FontUnit` attribute: one cascade global
  (`CascadeDefaults::font_unit`, the standalone default), and the panel bridge observer **always** inserts
  `Override<FontUnit>` — from `panel.font_unit()` when set, else from `CascadeDefaults::panel_font_unit` —
  so panel subtrees inherit `Points` via the parent-walk. Concretely: flip `seed_panel_overrides`'s
  FontUnit branch (`panel/diegetic_panel.rs`) from `if let Some` to an unconditional insert and add the
  `Res<CascadeDefaults>` param Phase 1 deferred; the pure `build()` cannot read the resource. Edit the
  `CascadeDefaults` struct itself (`cascade/defaults.rs`): rename `world_font_unit` → `font_unit`, keep
  `panel_font_unit` as the construction-time seed, update both field docstrings. `panel_font_unit` becomes a
  construction-time seed (no longer a cascade global). Since every panel now carries `Override<FontUnit>`,
  no panel ever resolves `FontUnit` from the cascade global — the `global_default` fallback in the panel
  font-unit read (`compute_layout.rs:98`) becomes unreachable for panels (still correct, just dead for that
  entity kind).
- Repoint every reader/filter from the per-role `Resolved<…>` types to `Resolved<TextAlpha>` /
  `Resolved<FontUnit>`: `world_text/mod.rs`, `rendering.rs` (`ChangedWorldTextQuery`), `panel_text/shaping.rs`,
  `panel_text/alpha.rs`, and the panel font-unit read. **Delete** the standalone tier-1 re-resolve helpers
  `re_resolve_world_font_unit` / `re_resolve_world_text_alpha` (`rendering.rs`) rather than repointing them:
  the unified pass now caches `Resolved<A>` and re-resolves on `Changed<Override<A>>` (an in-place `get_mut`
  edit triggers it), so keeping the helpers would make two writers of standalone `Resolved<…>` — the exact
  double-writer clobber the atomic commit below claims to make unrepresentable. Run
  `rg "Resolved<(World|Panel)(TextAlpha|FontUnit)>"` to confirm none are missed. **Keep** the `With` /
  `Without<PanelChild>` entity-selection filters.
- Move the label's per-run override: delete `PanelText.alpha_mode`, insert `Override<TextAlpha>` on the
  label at reconcile time, and read it through the parent-walk like any other override. This **replaces**
  the hand-rolled child resolution in `apply_panel_result` (`panel_text/shaping.rs`), which today computes
  `panel_text.alpha_mode.map_or(panel_fallback, …)` against `Resolved<PanelTextAlpha>` and writes the
  child's `Resolved` itself: remove that logic and its `panel_alpha` / `existing_child_alpha` queries (and
  the per-run `alpha_mode: config.alpha_mode()` set in `build_panel_text`), letting the generic pass own the
  label's `Resolved<TextAlpha>` — otherwise the label has two competing writers.
- Delete the per-role types (`WorldTextAlpha` / `PanelTextAlpha` / `WorldFontUnit` / `PanelFontUnit`),
  the three plugins (`CascadeEntityPlugin` / `CascadePanelPlugin` / `CascadePanelChildPlugin`), and
  `Exclude` / `ExcludeNone` plus their test impls.

Commit atomically: the new plugin, the reader repoint, and the old-type deletions are mutually
dependent — they cannot land separately and stay green, because old and new both writing `Resolved` in
one frame would clobber. That same atomic deletion ends the Phase-2 cross-fire and guarantees a single
writer: once the old per-role types and plugins are gone, two writers for one attribute is
unrepresentable — no runtime guard or plugin-count assert is needed (an accidental double-add of the
same generic plugin is caught by Bevy's built-in plugin uniqueness).

### Phase 4 — verification + cascade example

- Verify: cross-enrollment is impossible by construction (a standalone and a panel label resolve
  independently); a same-command-batch panel+child spawn resolves in one frame; an in-place `get_mut`
  edit of an `Override<A>` re-resolves; removing an `Override<A>` returns the node to inheriting;
  reparenting a child re-resolves against the new parent; a panel always carries `Override<FontUnit>`
  and a runtime `CascadeDefaults::font_unit` change re-resolves standalone text but not panels; cycling
  all alpha modes in `text_alpha.rs` stays correct; a `ChildOf(self)` self-parent and a two-node cycle
  terminate at the global default with no hang or panic.
- Verify the parent-walk topology: a panel label is a `ChildOf` descendant of the panel with no
  `Resolved`-bearing intermediate entity (e.g. a glyph-mesh child) between them, so the walk resolves the
  label against the panel's subtree — `reconcile_panel_text_children` must parent labels under the panel,
  not under an intermediate.
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
| alpha/unit fields as the **cascade source** | `Override<TextAlpha>` / `Override<FontUnit>` components, inserted by an `On<Add>` bridge | fields stay on `WorldTextStyle` / `DiegeticPanel` as authoring inputs the bridge reads once at spawn; the cascade reads the component. `PanelText.alpha_mode` is removed in Phase 3 (label override moves to `Override<TextAlpha>`) |
| per-role `Resolved<WorldTextAlpha>` / `Resolved<PanelTextAlpha>` | `Resolved<TextAlpha>` | one resolved type per attribute |
| per-role `Resolved<WorldFontUnit>` / `Resolved<PanelFontUnit>` | `Resolved<FontUnit>` | one font-unit attribute; panel `Points` is a seeded override, not a second type |
| `CascadeDefaults::world_font_unit` | `CascadeDefaults::font_unit` | sole cascade global for `FontUnit` (standalone default) |
| `CascadeDefaults::panel_font_unit` (live global) | construction-time seed for the panel's `Override<FontUnit>` | no longer cascade-propagated |
| `WorldTextStyle` = `TextProps<ForStandalone>` | unchanged; **loses** `alpha_mode` / `unit` as cascade overrides (`unit` stays for layout measurement) | `layout/text_props.rs` |
