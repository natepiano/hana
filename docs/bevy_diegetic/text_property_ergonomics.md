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

Adding a new cascade attribute is one `cascade_attr!` line plus three one-line
wrappers in `cascade/attributes.rs`, and one `.add_plugins(CascadePlugin::<A>)`
line in the plugin:

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

Rebuild the cascade core so a new attribute is a `cascade_attr!` line plus a few
one-line wrappers, and the public surface never leaks reflection types.
Everything else sits on this, so it lands first.

**Work:**

- **Per-attribute defaults.** Replace the cascade fields of the monolithic
  `CascadeDefaults` (defaults.rs) with a `CascadeDefault<A>` resource per cascade
  attribute, each with a `Default` (so `init_resource` works). `propagate_cascade`
  reads `Res<CascadeDefault<A>>` and uses `is_changed()` for default-change
  detection, retiring the `Local<Option<A>>` sentinel (plugin.rs). Two invariants
  this design depends on: `propagate_cascade::<A>` must stay unconditional (no
  `run_if`), or the `is_changed()` edge is lost; and `init_resource` marks the
  resource added==changed on frame 1, so the first propagation fires once — which
  is correct (it seeds descendants). A non-default global is set with
  `insert_resource(CascadeDefault::<A>(v))` at startup or `ResMut<CascadeDefault<A>>`
  at runtime. Non-cascade defaults (`panel_font_unit`, `layout_unit`) stay where
  they belong.
- **Trait split.** Introduce a sealed public marker trait
  `CascadeProperty: Copy + PartialEq + Send + Sync + 'static`; keep the internal
  `CascadeAttr: CascadeProperty + FromReflect + TypePath + Typed +
  GetTypeRegistration` (resolved.rs). Public verb signatures name only the value
  type and `CascadeProperty`; the attribute type still derives `Reflect` at its
  definition, but `Reflect` never appears in the public API.
- **Shared resolve helper.** Factor `resolve<A>(world: &World, entity, default) ->
  A` that performs the ancestor walk (with the existing cycle/depth guards).
  Refactor `propagate_cascade` onto it, and have it read the per-attribute
  `CascadeDefault<A>` — the same source the propagation pass uses — so the spawn
  seed and the propagation pass can never disagree on the default. This is the
  single source of truth for resolution, reused by the self-heal in Phase 2.
- **The `cascade_attr!` macro.** An in-crate `macro_rules!` that, per attribute,
  emits: the newtype, its `derive(Reflect)`, the `CascadeProperty` + `CascadeAttr`
  impls, the `CascadeDefault<A>` resource + its `Default`, and the `register_type`
  calls. It does **not** emit the `.add_plugins(CascadePlugin::<A>::default())`
  line (that registration stays hand-written, one line per attribute in the
  plugin), nor the three verb wrappers — `macro_rules!` cannot synthesize the
  `override_text_alpha` identifier from the `TextAlpha` token. So adding an
  attribute is: one `cascade_attr!` line, three one-line wrappers, and one
  `add_plugins` line — state this accurately rather than as "one line."
- **Re-express the existing attributes.** Move `TextAlpha` and `FontUnit` into a
  new `cascade/attributes.rs`, declared through `cascade_attr!`. This module
  becomes the single home for every cascading property.

**Key files:** `cascade/{defaults,resolved,plugin,cascade_set,mod}.rs`, new
`cascade/attributes.rs`.

**Acceptance:** existing cascade tests pass; the default-change tests are
rewritten onto per-attribute `CascadeDefault<A>` resources (the old monolithic
`CascadeDefaults` + `Local<Option<A>>` sentinel pattern is fully replaced);
default-change propagation works through the per-attribute resource;
`TextAlpha`/`FontUnit` are declared via the macro; no public-API or rendering
behavior change yet. Add a test that declares a throwaway attribute through
`cascade_attr!` and asserts inheritance + default resolution, proving the
generator path.

---

## Phase 2 — Public cascade verbs

The user-facing `override_` / `inherit_` / `resolved_` surface for cascade
attributes, built on Phase 1.

**Work:**

- **Internal insert helper.** `apply_cascade_override<A>(entity, value)` is the
  one place that inserts `Override<A>`; both the runtime command and the
  panel-reconcile spawn path (`reconcile.rs`, the label-alpha read) call it so
  they cannot drift.
- **Named `EntityCommand`s + extension trait.** `override_x` (insert
  `Override<A>`) and `inherit_x` (remove it) as `EntityCommands` methods. Reachable
  from systems, exclusive systems, and `&mut World` tests via `world.commands()`.
- **Self-heal.** `override_x` inserts `Resolved<A>` when absent (via the Phase-1
  `resolve` helper) so a same-frame-as-spawn override never shows a one-frame
  default, guarded by the same `current != new` inequality as `propagate_cascade`
  so a redundant insert never fires a spurious `Changed`. Self-heal fixes only the
  overridden entity itself; its descendants re-resolve through the propagation
  pass's subtree walk when the parent's `Override<A>` changes — they do not need
  (and do not get) self-heal.
- **Read getter.** `resolved_x(world, e)` returns the inner value (e.g.
  `AlphaMode`), never the newtype. No public `Query` wrapper — user systems filter
  on the input they control (`Changed<WorldTextStyle>`). A `has_override_x(world, e)
  -> bool` reader (authored-vs-inherited) is intentionally deferred until a caller
  needs it — a trivial add when one appears.
- **Timing contract.** Document on the verbs: a `set` is observed same-frame when
  scheduled `.after(CascadeSet::Propagate)`, else next frame. `propagate_cascade`
  runs in `Update`; the render systems already run after it in `PostUpdate`.
- **Centralize.** All verb wrappers live alongside their `cascade_attr!`
  declaration in `cascade/attributes.rs`.

**Key files:** `cascade/attributes.rs`, `cascade/mod.rs` (public re-exports),
`cascade/plugin.rs` (observer/command registration).

**Acceptance:** tests for set / inherit / read on a standalone; self-heal on the
spawn frame (spawn + `override_x` same frame resolves to the override, no
one-frame default); a parent `override_x` same frame re-resolves an existing
child through the propagation pass; same-frame observation after
`CascadeSet::Propagate`; an alpha-only `override_text_alpha` mutates the material
without respawning the run's mesh (composes with the existing per-run rebuild
work). `Override`/`Resolved` stay `pub(crate)` — confirm no public signature
names them.

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
  `reconcile.rs` converts to `Override<TextAlpha>`. Update the `alpha_mode()` and
  unit accessor doc comments at the same time: they currently describe the
  standalone bridge reading them, which is no longer true.
- **Rework `seed_world_text_overrides`.** It no longer reads removed fields; it
  seeds `Resolved<A>` to the global default at spawn (so `Resolved` always
  exists). Non-default values come from explicit `override_x` calls.
- **Capture label authoring before conversion (compile-break otherwise).** Three
  sites read `unit`/`alpha_mode` off a `ForStandalone` today and break the moment
  the accessors are gated: `reconcile.rs` (reads label alpha *after*
  `as_standalone()`), `seed_world_text_overrides` (above), and `as_standalone()`
  itself (copies both fields). For `reconcile.rs`, read `alpha_mode()` from the
  `LayoutTextStyle` (`ForLayout`) *before* converting, then route the value
  through Phase-2's `apply_cascade_override` — do not stub the read to `None`,
  which compiles but silently drops every label's authored alpha. The regression
  test below is what distinguishes the correct fix from the silent-drop one.
- **Asymmetric conversions.** `as_standalone()` no longer carries `unit`/
  `alpha_mode` into `ForStandalone`; `as_layout_config()` produces a layout config
  whose `unit`/`alpha_mode` default to "inherit". Document each direction.
- **Plain-field runtime setters.** Add a `set_*` per plain `ForStandalone` field
  (size, weight, slant, line-height, letter/word spacing, align, anchor,
  sidedness, font_features, render_mode, shadow_mode — `set_color` already
  exists), gated to `impl TextProps<ForStandalone>` so a meaningless setter on a
  layout style is a compile error. Each is a direct `&mut` write that fires
  `Changed<WorldTextStyle>` and feeds the existing gated reconcile. `wrap` is
  excluded: `ForStandalone` hard-codes it to `TextWrap::None` and never reads it
  (wrapping is a layout-engine property), so `set_wrap` stays `ForLayout`-only.
- **Migrate callers.** Update examples (`cascade.rs`, `text_alpha.rs`,
  `units.rs`, `world_text.rs`) and the README to author standalone alpha via
  `override_text_alpha` after spawn instead of `with_alpha_mode`. `cascade.rs`
  has three standalone `with_alpha_mode` call sites; the README documents the
  standalone-authoring flow in four places.
- **Document the cascade.** Rewrite the cascade module's `//!` doc to
  match the post-Phase-1 architecture: per-attribute `CascadeDefault<A>` (no
  monolithic `CascadeDefaults`), the `CascadeProperty` / `CascadeAttr` split,
  and the `cascade_attr!` recipe. Add a "Using the cascade" section covering the
  public verbs (`override_x` / `inherit_x` / `resolved_x`), stating that the verbs
  are `EntityCommands` methods (not `TextProps` setters) and that the same
  `override_x` verb is used at spawn and at runtime. Document the
  `CascadeSet::Propagate` timing contract precisely: a reader sees a
  freshly-written value same-frame only if scheduled
  `.after(CascadeSet::Propagate)`; a reader before it sees the prior frame's
  value; spawn-time seeding is synchronous (the first `Update` after spawn already
  sees the seeded value), and panel labels are reconciled in `PostUpdate`. Drop
  the stale `WorldTextStyle.unit` / `with_alpha_mode` override-source references.
- **Document extending the cascade — two cases.** In the same module `//!` doc,
  add an "Adding a cascade attribute" section that distinguishes the two ways a
  property becomes cascading, because the cost differs sharply:
  - **A new attribute that never existed** (e.g. an outline width): the clean
    recipe — one `cascade_attr!(Name(Ty), default = …)` line plus three one-line
    `override_/inherit_/resolved_` wrappers in `cascade/attributes.rs`, one
    `.add_plugins(CascadePlugin::<Name>::default())` line in the plugin, and one
    read site that calls `resolved_name`. The value type need only be
    `Copy + PartialEq`.
  - **Promoting an existing plain field to cascading** (e.g. color, which is a
    plain `TextProps` field read at `world_text/mesh_spawning.rs` and
    `panel_text/shaping.rs`): the recipe *plus* a migration — repoint every render
    read from the struct field to `Resolved<A>`, seed `Resolved<A>` at spawn in
    `seed_world_text_overrides` and the panel-label path, capture the label's
    authored value off the `LayoutTextStyle` before `as_standalone()` and route it
    through `apply_cascade_override` (the D1 pattern), decide whether the plain
    `set_*`/`with_*` accessors are deleted or kept as sugar that calls
    `override_x`, and migrate callers. This is the exact pattern Phase 3 runs for
    `alpha_mode` — point the doc at the alpha migration as the worked example.
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
