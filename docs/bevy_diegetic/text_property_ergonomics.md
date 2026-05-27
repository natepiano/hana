# Text property ergonomics — implementation plan

## Goal

Give every text property an ergonomic per-entity runtime mutation path while
keeping the existing cascade (parent→child inheritance for attributes like alpha
and font unit). Two property kinds, two surfaces:

- **Plain fields** (size, weight, color, wrap, align, …) — per-entity only, no
  inheritance. Mutated by direct `&mut` setters on the component.
- **Cascade attributes** (text alpha, font unit, and a growing set) — inherit
  through the panel/entity tree. Mutated by commands that set a per-entity
  override and let the propagation pass re-resolve.

The split is explicit and named so the call site reads the difference.

## Design model

The cascade is the CSS model with the selector/specificity layer removed:

| CSS | here |
| --- | --- |
| specified value (inline style) | `Override<A>` — per-node authored value |
| computed value (`getComputedStyle`) | `Resolved<A>` — what the renderer reads |
| inheritance (absent declaration) | absent `Override<A>` → walk ancestors |
| UA-stylesheet default | per-attribute `CascadeDefault<A>` resource |
| `el.style.x = v` | `override_x` command |
| `inherit` keyword | `inherit_x` command (removes the override) |

Precedence is the trivial chain `{ global default < inherited-from-parent <
per-entity Override }`. `Override<A>` / `Resolved<A>` stay `pub(crate)`; the
public surface is a typed facade that never names them.

`WorldTextStyle` and `LayoutTextStyle` are two aliases of one generic
`TextProps<C>` (markers `ForStandalone` / `ForLayout`). `WorldTextStyle`
(`ForStandalone`) is the runtime component for standalone text and panel labels;
`LayoutTextStyle` (`ForLayout`) is the layout engine's build-time input that the
panel builder takes (`builder.text(text, config)`).

## Public API surface

Plain field (per-entity, immediate):

```rust
WorldTextStyle::new(24.0).with_weight(Bold) // spawn
style.set_size(18.0);                        // runtime, direct &mut
```

Cascade attribute (deferred command; read via &World):

```rust
commands.spawn(WorldText::new("hi"))
        .override_text_alpha(AlphaMode::Add);   // spawn-time = same verb
commands.entity(e).override_text_alpha(Add);    // runtime set
commands.entity(e).inherit_text_alpha();        // drop override → inherit/default
let m: AlphaMode = resolved_text_alpha(world, e); // computed value
```

Adding a new cascade attribute is one macro line plus three one-line wrappers,
all in `cascade/attributes.rs`:

```rust
cascade_attr!(TextAlpha(AlphaMode), default = AlphaMode::Blend);

pub fn override_text_alpha(e: &mut EntityCommands, m: AlphaMode) { e.override_cascade(TextAlpha(m)); }
pub fn inherit_text_alpha(e: &mut EntityCommands)                { e.inherit_cascade::<TextAlpha>(); }
pub fn resolved_text_alpha(w: &World, e: Entity) -> AlphaMode    { resolved_cascade::<TextAlpha>(w, e).0 }
```

## Non-goals

No CSS specificity, selector matching, or multi-origin cascade
(UA/user/author/`!important`). Keep the three-level precedence chain. No
published proc-macro crate — the generator is an in-crate `macro_rules!`.

---

## Phase 1 — Cascade foundation and generator

Rebuild the cascade core so a new attribute is a one-line declaration and the
public surface never leaks reflection types. Everything else sits on this, so it
lands first.

**Work:**

- **Per-attribute defaults.** Replace the cascade fields of the monolithic
  `CascadeDefaults` (defaults.rs) with a `CascadeDefault<A>` resource per cascade
  attribute. `propagate_cascade` reads `Res<CascadeDefault<A>>` and uses
  `is_changed()` for default-change detection, retiring the `Local<Option<A>>`
  sentinel (plugin.rs). Non-cascade defaults (`panel_font_unit`, `layout_unit`)
  stay where they belong.
- **Trait split.** Introduce a sealed public marker trait
  `CascadeProperty: Copy + PartialEq + Send + Sync + 'static`; keep the internal
  `CascadeAttr: CascadeProperty + FromReflect + TypePath + Typed +
  GetTypeRegistration` (resolved.rs). Public verb signatures name only the value
  type and `CascadeProperty`; the attribute type still derives `Reflect` at its
  definition, but `Reflect` never appears in the public API.
- **Shared resolve helper.** Factor `resolve<A>(world: &World, entity, default) ->
  A` that performs the ancestor walk (with the existing cycle/depth guards).
  Refactor `propagate_cascade` onto it. This is the single source of truth for
  resolution, reused by the self-heal in Phase 2.
- **The `cascade_attr!` macro.** An in-crate `macro_rules!` that, per attribute,
  emits: the newtype, the `CascadeProperty` + `CascadeAttr` impls, the
  `CascadeDefault<A>` resource and its init, `register_type` calls, and the
  `CascadePlugin::<A>` wiring.
- **Re-express the existing attributes.** Move `TextAlpha` and `FontUnit` into a
  new `cascade/attributes.rs`, declared through `cascade_attr!`. This module
  becomes the single home for every cascading property.

**Key files:** `cascade/{defaults,resolved,plugin,cascade_set,mod}.rs`, new
`cascade/attributes.rs`.

**Acceptance:** existing cascade tests pass unchanged; default-change propagation
works through the per-attribute resource; `TextAlpha`/`FontUnit` are declared via
the macro; no public-API or rendering behavior change yet. Add a test that
declares a throwaway attribute through `cascade_attr!` and asserts inheritance +
default resolution, proving the generator path.

---

## Phase 2 — Public cascade verbs

The user-facing `override_` / `inherit_` / `resolved_` surface for cascade
attributes, built on Phase 1.

**Work:**

- **Internal insert helper.** `apply_cascade_override<A>(entity, value)` is the
  one place that inserts `Override<A>`; both the runtime command and the
  panel-reconcile spawn path call it so they cannot drift.
- **Named `EntityCommand`s + extension trait.** `override_x` (insert
  `Override<A>`) and `inherit_x` (remove it) as `EntityCommands` methods. Reachable
  from systems, exclusive systems, and `&mut World` tests via `world.commands()`.
- **Self-heal.** `override_x` inserts `Resolved<A>` when absent (via the Phase-1
  `resolve` helper) so a same-frame-as-spawn override never shows a one-frame
  default.
- **Read getter.** `resolved_x(world, e)` returns the inner value (e.g.
  `AlphaMode`), never the newtype. No public `Query` wrapper — user systems filter
  on the input they control (`Changed<WorldTextStyle>`).
- **Timing contract.** Document on the verbs: a `set` is observed same-frame when
  scheduled `.after(CascadeSet::Propagate)`, else next frame. `propagate_cascade`
  runs in `Update`; the render systems already run after it in `PostUpdate`.
- **Centralize.** All verb wrappers live alongside their `cascade_attr!`
  declaration in `cascade/attributes.rs`.

**Key files:** `cascade/attributes.rs`, `cascade/mod.rs` (public re-exports),
`cascade/plugin.rs` (observer/command registration).

**Acceptance:** tests for set / inherit / read on a standalone; self-heal on the
spawn frame; same-frame observation after `CascadeSet::Propagate`; an alpha-only
`override_text_alpha` mutates the material without respawning the run's mesh
(composes with the existing per-run rebuild work). `Override`/`Resolved` stay
`pub(crate)` — confirm no public signature names them.

---

## Phase 3 — `WorldTextStyle` runtime surface

Make the standalone component fully runtime-mutable, and remove the spawn-only
authoring fields that the cascade now owns. This is the breaking change and the
example/README migration.

**Work:**

- **Remove cascade-owned authoring from `ForStandalone`.** `unit` and
  `alpha_mode` become `ForLayout`-only: gate `with_alpha_mode`, `alpha_mode()`,
  and the unit authoring accessors to `impl TextProps<ForLayout>` (matching the
  marker-gating pattern), and stop the standalone seed/render paths from reading
  them. A standalone's alpha and font unit come solely from the cascade
  (`Override`/`Resolved`). `LayoutTextStyle` keeps both fields — `unit` for
  measurement (element.rs), `alpha_mode` as the panel-label build-time input that
  `reconcile.rs` converts to `Override<TextAlpha>`.
- **Rework `seed_world_text_overrides`.** It no longer reads removed fields; it
  seeds `Resolved<A>` to the global default at spawn (so `Resolved` always
  exists). Non-default values come from explicit `override_x` calls.
- **Asymmetric conversions.** `as_standalone()` no longer carries `unit`/
  `alpha_mode` into `ForStandalone`; `as_layout_config()` produces a layout config
  whose `unit`/`alpha_mode` default to "inherit". Document each direction.
- **Plain-field runtime setters.** Add a `set_*` per plain `ForStandalone` field
  (size, weight, slant, line-height, letter/word spacing, wrap, align, anchor,
  sidedness, font_features, render_mode, shadow_mode — `set_color` already
  exists), gated to `impl TextProps<ForStandalone>` so a meaningless setter on a
  layout style is a compile error. Each is a direct `&mut` write that fires
  `Changed<WorldTextStyle>` and feeds the existing gated reconcile.
- **Migrate callers.** Update examples (`text_alpha.rs`, `units.rs`,
  `world_text.rs`, …) and the README to author standalone alpha via
  `override_text_alpha` after spawn instead of `with_alpha_mode`.
- **Document the cascade.** Rewrite the `cascade/mod.rs` `//!` module doc to
  match the post-Phase-1 architecture: per-attribute `CascadeDefault<A>` (no
  monolithic `CascadeDefaults`), the `CascadeProperty` / `CascadeAttr` split,
  and the `cascade_attr!` one-line recipe for adding an attribute. Add a "Using
  the cascade" section covering the public verbs (`override_x` / `inherit_x` /
  `resolved_x`), when to reach for a plain `set_*` setter versus a cascade
  command, and the `CascadeSet::Propagate` timing contract (same-frame iff
  scheduled `.after(CascadeSet::Propagate)`, else next frame). Drop the stale
  `WorldTextStyle.unit` / `with_alpha_mode` override-source references.
- **Reflection note.** Document in the `Override`/`Resolved` doc comments that
  they are `pub(crate)` and so cannot be named by external inspectors/tests;
  revisit if they later go public.

**Key files:** `layout/text_props.rs`, `render/world_text/{mod,rendering}.rs`,
`render/panel_text/{reconcile,alpha}.rs` (verify panel-label path unchanged),
examples, README.

**Acceptance:** standalone spawn → `Resolved` at the global default;
`override_text_alpha` after spawn takes effect (self-heal); a `set_size` rebuilds
only the changed run; panel labels still resolve their authored alpha correctly
(regression test: author a label via the panel builder, assert its `Resolved<
TextAlpha>` and rendered alpha on the first frame); calling a removed standalone
accessor fails to compile; examples and README build and run; the
`cascade/mod.rs` module doc renders on docs.rs with no stale reference to
`CascadeDefaults` or `with_alpha_mode` and a worked example of each public verb.

## Cross-references

- `panel_perf.md` — the per-run rebuild work that makes a single-property change
  cheap (an `override_text_alpha` is a material mutation, a `set_size` is one
  run's rebuild).
- `crates/bevy_diegetic/src/cascade/` — `Override`/`Resolved`/`propagate_cascade`/
  `CascadeDefaults`.
- `crates/bevy_diegetic/src/layout/text_props.rs` — `TextProps<C>`, the markers,
  the `with_*`/`set_*` builders, the conversions.
- `crates/bevy_diegetic/src/layout/builder.rs` — `builder.text(text, config)`.
