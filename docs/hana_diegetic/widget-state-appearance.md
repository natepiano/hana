# Widget State Appearance

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Extends per-state widget appearance from the widget's own root box to every element a widget owns, replaces the sixteen flat state builders with four bundle builders, adds a content-color property so text and lines can change with state, and cascades the bundles from a global default through panel, widget, and part with per-property merge at every hop. Removes `Slider::disabled_color`, the one blunt "paint everything one color" switch that exists because children were unreachable.

## Delegation Context

<!-- Shared across all phases. /plan:delegate prepends this to every dispatch. -->

- **Project:**
  - `hana_diegetic` (`crates/hana_diegetic`) — diegetic UI layout engine for Bevy; in-world panels driven by a Clay-inspired layout algorithm. This is the primary package for every phase.
  - `bevy_kana` (`crates/bevy_kana`, `0.2.0-dev`) — generic cascade/attribute infrastructure. Touched by **Phase 8 only**, which adds a `combine` function pointer to `CascadePlugin<A>` and turns the two resolve walks into a fold. **`CascadeAttribute` is not changed** — it carries a blanket impl (`cascade.rs:179`), so no type can override a method on it. `hana_diegetic` depends on it (`bevy_kana = { workspace = true, features = ["input"] }`); `bevy_kana`'s dev-deps pull `hana_diegetic` back in (dev-only, no cycle).

- **Stack:** Rust edition 2024. Bevy `0.19.0` pinned at the workspace root (all `bevy_*` subcrates likewise `0.19.0`). `thiserror 2.0.18`, `trybuild 1`, `bevy_enhanced_input 0.26.0`, `parley 0.9.0`, `smallvec 1`, `bitflags 2`. No `bevy_ui`.

- **Layout** (only the dirs the phases touch):
  - `crates/hana_diegetic/src/layout/` — builder typestate (`builder.rs`), element storage + build-time validation (`element.rs`), draw primitives (`draw.rs`, `line.rs`), `engine/`.
  - `crates/hana_diegetic/src/widgets/` — `appearance.rs` (state bundle), `visual.rs` (override storage + dispatch), `id.rs` (computed widget records), `reify.rs` (spawn/update), `slider.rs` / `button.rs` / `editable.rs` (the three presentation systems), `interactivity.rs` (the cascade precedent every step mirrors), `mod.rs` (`WidgetsPlugin`, `WidgetSystems`).
  - `crates/hana_diegetic/src/cascade/` — diegetic-side cascade wiring: `mod.rs` (re-exports, `cascade_plugin` helper), `attributes.rs` (`CascadeEntityCommandsExt`), `constants.rs` (`CASCADE_ATTRIBUTE_BYTES`), `resolved.rs` (`cascade_attribute!` macro, `SdfMaterial` / `TextMaterial` / `ShapeMaterial`), `defaults.rs` (`PanelDefaults`).
  - `crates/hana_diegetic/src/panel/` — `builder.rs` (`BuilderData`, `PanelBuildError`), `diegetic_panel.rs` (seeding, precompose), `lifecycle.rs` (ownership observers, teardown), `mod.rs` (`HeadlessLayoutPlugin`).
  - `crates/hana_diegetic/src/render/` — retained-batching consumers of `VisualSlotOverride`: `fill_batch.rs`, `panel_text/`, `panel_shapes/`, `analytic_paths/`, `batch_store.rs`.
  - `crates/hana_diegetic/src/ime/` — `editor.rs`, the generated editor content tree (text, selection, caret, validation).
  - `crates/hana_diegetic/examples/` — flat files; `widgets.rs` is the canonical widget example (auto-discovered, no `[[example]]` entry).
  - `crates/hana_diegetic/tests/` — `headless_widgets.rs` (external-client integration test), `trybuild.rs` (driver), `trybuild/{pass,fail}/` (fixtures).
  - `crates/bevy_kana/src/` — `cascade.rs` (generic `Cascade<T>`, `CascadeAttribute`, propagation).

- **Key files** (line refs re-verified after Phase 2; files neither Phase 1 nor Phase 2 touched still carry `HEAD` = `64f8bdc0` refs):
  - `src/layout/builder.rs` (1332 lines) — `El<L, Role>`; sealed `ElementRole` (`:104`), `Widget` (`:114`), `HasPressedState` (`:158`); `WidgetDeclaration` (`:129`), `WidgetRootSlot` (`:135`); `El::background` (`:602`), `El::border` (`:608`); `El::editable_field` (`:715`); the four state verbs `hovered` (`:791`), `focused` (`:801`), `disabled` (`:811`) on `El<L, WidgetElement<W>>` and `pressed` (`:830`) on the `HasPressedState` block; `El::disabled_color` (`:886`); `LayoutBuilder::with_root` (`:1153`), `with` (`:1185`), `text` (`:1215`), `image` (`:1247`); `Text::layout` (`:265`).
  - `src/layout/element.rs` — `CommonEl`/`Element`, `appearance` field (`:148`); `WidgetContainsInteractiveDescendant` (`:788`); `validate_tree`'s stack walk (`:785-836`), the **only** appearance-reachable walk that returns `Result<_, PanelBuildError>`, calling `validated_element_widget_owner` at `:820`; `computed_widget_records` (`:838`, returns `Vec<ComputedWidgetRecord>` — **no `Result`**) and its owning-record walk (`:895`) calling `record_owned_widget_element` (`:1356`) and `element_visual_capabilities` (`:1332`); `set_field_editing_content` (`:1022`); `validated_element_widget_owner` (`:1263`); `validated_element_appearance` (`:1304`) with the four `any` calls (`:1311-1326`); `set_element_state_appearance` (`:461`, `#[cfg(test)]`).
  - `src/layout/draw.rs:11` — `PanelDraw`. `src/layout/line.rs:42` — `PanelShape` enum; `PanelCircle` struct at `:64`.
  - `src/ime/editor.rs` (1968 lines) — `inline_editor_content_tree` **definition at `:1132`** (the earlier `:665` / later `:1184` sites are callers/helpers, not the def).
  - `src/widgets/appearance.rs` — `VisualChange<T>` (`:26`) with `VisualChange::layer_onto` (`:49`), `Appearance` (`:113`) with `Appearance::layer_onto` (`:172`), the four `Widget*Appearance` wrappers and their size assertions (`:214` region), `StateAppearance` (`:284`, **not a `Component`**) with `cascades()` (`:293`); `WidgetStateCascades<'a>` (`:299`) with `new` (`:308`), `any_overridden` (`:323`), `layer` (`:330`), `any` (`:352`), `resolve` (`:367`); `WidgetState` (`:386`), `LAYER_ORDER` (`:400`). **`layer`/`any`/`resolve` live on `WidgetStateCascades`, not on `StateAppearance`.**
  - `src/widgets/visual.rs` — `VisualSlotOverride` (`:168`) with the generic `color` field (`:170`), `WidgetVisualSlots.elements` / `.part_appearances` (`:83`) with `with_elements` (`:97`) / `with_part_appearances` (`:106`) / `elements()` (`:116`) / `part_appearances()` (`:119`, **`#[cfg(test)]`**), `WidgetVisualOverrides` (`:255`), `subtree_color` field (`:256`) / `set_subtree_color` (`:262`) / getter (`:267`), `write_widget_overrides` (`:314`, replaces the whole component), `write_slot_override` (`:348`, one slot only), `VisualOverrideIndex` (`:413`), `dispatch_visual_overrides` (`:463`) with its widget filter (`:471`), existing `HashMap<usize, VisualSlotOverride>` (`:491`), and the subtree seeding loop (`:492`).
  - `src/widgets/button.rs` (`presentation_inputs_changed` `:134` — **filters `With<WidgetOf>` only, no `WidgetKind` filter, and its removal drains are unfiltered**; `present_button_state` writes one slot via `write_slot_override` `:241`), `src/widgets/slider.rs` (`presentation_inputs_changed` `:1138` — filters `WidgetKind::Slider`; `present_slider_state` `:1194`, writes the whole component via `write_widget_overrides` `:1288`, subtree seeding `:1242`, `disabled_color` field `:172` / default `:191` / builder `:233` / crate-internal setter `:255`, test `disabled_color_recolors_every_slider_element_and_suppresses_focus_border` `:5267`), `src/widgets/editable.rs` (`presentation_inputs_changed` `:29` — filters `WidgetKind::EditableField` on both the changed query and the removal drains; `present_editable_state` writes one slot via `write_slot_override` `:122`).
  - `src/widgets/id.rs` — `WidgetKind` (`:98`), `VisualElementCapabilities` bitflags (`:115`, one `CONTENT` bit covering text **and** image **and** non-empty `PanelDraw` together), `ComputedWidgetRecord` (`:138`) with `appearance` field (`:143`) and `part_appearances` (`:144`), `appearance()` (`:188`), `push_visual_element` (`:208`), `part_appearances()` (`:216`), `push_part_appearance` (`:218`).
  - `src/widgets/reify.rs` — `reify_widgets` (`:184`, gated on `Changed<ComputedDiegeticPanel>` at `:194`), its existing-widget query (`:196-211`), `spawn_widget` (`:296`), `update_widget` (`:352`) with the `WidgetVisualSlots` inequality guard (`:445`), `update_widget_appearance` (`:482`).
  - `src/widgets/mod.rs` — `WidgetSystems` enum (`:143`), ordering `Reify → ReifyCommandsApplied → ResolveInteractivity → InteractivityCommandsApplied → Focus → SemanticInput → FocusCommandsApplied → PresentationCommandsApplied`; `WidgetsPlugin` (`impl Plugin` `:223`) with `add_plugins` (`:233-237`) including `cascade::cascade_plugin::<WidgetInteractivity>()` (`:234`), `configure_sets` (`:238-267`), `add_systems` (`:299-313`) where the three presenters attach their run conditions (`:300` button, `:304` editable, `:308` slider); `mod appearance;` stays **private** (`:1`) — the public surface comes from the `pub use appearance::…` re-exports, so no phase needs `pub mod` here.
  - `src/cascade/mod.rs:44` — `cascade_plugin<A: CascadeRoot>()`.
  - `src/widgets/interactivity.rs` (529 lines) — `Cascade<WidgetInteractivity>`, the pattern every cascade step mirrors.
  - `src/cascade/attributes.rs` (353 lines) — `CascadeEntityCommandsExt` (`:30`), `resolved_*` fns (`:223-322`), `apply_cascade_override` (`:326`), `remove_cascade_override` (`:336`), `resolved_cascade` (`:345`). `src/cascade/constants.rs:7` — `CASCADE_ATTRIBUTE_BYTES: usize = 32`. `src/cascade/resolved.rs` (177 lines) — `cascade_attribute!` (`:20`), `SdfMaterial`/`TextMaterial`/`ShapeMaterial` (`:112`/`:125`/`:138`) with their per-attribute size assertions at `:118`/`:131`/`:144`, `CascadeRoot` (`:175`).
  - `crates/bevy_kana/src/cascade.rs` (676 lines) — `Cascade<T>` (`:23`); `resolve_cascade` (`:146`) and `resolve_cascade_ref` (`:161`), unbounded-generic public helpers with **no `hana_diegetic` call site** (only the `:502` unit test and the `lib.rs:41-42` / `prelude.rs:36-37` re-exports); **`CascadeAttribute` trait def (`:174`) with a blanket impl over its bounds (`:179`) — this is why a per-type method override is impossible**; `CascadeFrom` (`:197`), `CascadeDefault<A>` (`:237`, `#[reflect(Resource)]`), `Resolved<A>` (`:242`), `CascadeSet` (`:252`) with `Propagate` (`:254`), `CascadePlugin<A>` (`:258`) with `new` (`:265`) and `Plugin::build` (`:276`) registering `resolve_inserted_cascade` (`:283`, observer body `:339`), `resolve_entity_cascade` (`:332`), `propagate_cascade` (`:361`, calls the resolver at `:399`), `resolve_from_queries` (`:419`, first-override early return at `:433`), `resolve_from_world` (`:446`).
  - `src/panel/builder.rs` (1325 lines) — `PanelBuildError` (`:45`), `BuilderData` (`:200`). `src/panel/diegetic_panel.rs` (2432 lines) — `replace_from_precompose_helper` (`:451`), `seed_panel_overrides` (`:1566`). `src/panel/lifecycle.rs` (2089 lines) — `PanelCascadeOwnership` (`:122`), `teardown_owned_shared_state` (`:775`). `src/panel/mod.rs` (321 lines) — `HeadlessLayoutPlugin` (`:192`, `impl Plugin` `:194`), which registers the attribute cascades explicitly because `RenderPlugin` is absent.
  - `src/render/fill_batch.rs` (5616 lines) — `apply_sdf_visual_override` (`:1359`), which reads `fill_color.or(color)` and `border_color.or(color)`. `src/render/panel_text/batching.rs` (2888 lines) — cascade-resolution block (`:288`), `apply_routed_text_run_update` (`:435`). `src/render/batch_store.rs` — `BatchStore::upsert` (`:201`). `src/render/analytic_paths/batching.rs` — `TextRunBatch::rebuild` (`:314`).
  - `src/lib.rs` — crate-root `pub use widgets::*` block (`:339-403`). Phase 1 added `Appearance` (`:339`) and the four `Widget{Hovered,Pressed,Focused,Disabled}Appearance` wrappers; a later phase adding a public widget symbol extends this block.
  - `examples/widgets.rs` (1691 lines) — `.disabled_color` use (`:1162`), `add_slider` (`:1200`), `apply_state_appearance` (`:1450`).
  - `tests/headless_widgets.rs` (131 lines) — external-client integration test; no state-appearance coverage today.
  - `tests/trybuild.rs` — the driver, and the **only** place a fixture becomes reachable. It declares `typestate_helper_signatures_compile` (`#[ignore]` by default, covering the `overlay_*` fail glob and `pass/typestate_helpers.rs`) and `tooltip_typestate_signatures_compile` (covering the `tooltip_*` and `editable_widget_*` fail globs, `pass/tooltip_typestate.rs`, and `pass/widget_state_appearance.rs`). **A fixture whose filename matches no existing glob is never compiled and its acceptance-gate line is vacuous** — any phase adding fixtures must list `tests/trybuild.rs` in its **Files** and add or widen a glob. `tests/trybuild/pass/` — `tooltip_typestate.rs`, `typestate_helpers.rs`, `widget_state_appearance.rs`. `tests/trybuild/fail/` — 14 fixtures; `editable_widget_has_no_pressed_state.{rs,stderr}` proves an editable field has no `pressed` verb (`.rs:12` calls `.pressed(…)`; `.stderr:1` reports `error[E0599]: the method 'pressed' exists for struct 'El<hana_diegetic::Row, WidgetElement<ImeEditableFieldSpec>>', but its trait bounds were not satisfied`).

- **Build:** `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic` — and, in Phase 8 only, also `bash ~/.claude/scripts/delegate/verify.sh check bevy_kana`.
- **Test:** `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic` — and, in Phase 8 only, also `bash ~/.claude/scripts/delegate/verify.sh test bevy_kana`. Targeted targets, used **only** by phases whose Files touch them:
  - `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic trybuild`
  - `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic headless_widgets`
  - `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **Lint:** `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic` — and, in Phase 8 only, also `bash ~/.claude/scripts/delegate/verify.sh lint bevy_kana`.
- **Docs — orchestrator-run, not a delegate line.** `verify.sh` has no verb for either command below, and `check` / `test` / `lint` catch neither: `test` routes through `cargo nextest run`, which does not execute doctests, and a `pub` item whose doc links a `pub(crate)` type passes clippy and only fails the workspace doc lint. Phase 1 shipped exactly that defect and it survived every gate. Any phase that adds public API or a doc example is verified by the orchestrator running both of these before its checkpoint commit:
  - `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p hana_diegetic`
  - `cargo test -p hana_diegetic --doc`
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --scope edit --project-root /Users/natemccoy/rust/hana_diegetic_widgets`

- **Invariants:**
  - **An accepted option must reach the runtime.** No phase may ship a builder whose value is validated and then discarded; if a combination cannot present, it is gated out of the type surface or it is not offered. *Scope limit:* this binds **part-local** authoring, validated against a concrete element at panel build. A global `CascadeDefault` or a runtime entity command cannot promise a present recipient — a higher-level property with no compatible record at some element is **dormant** there, not an error.
  - **Every level merges into the one above it, property by property.** Global default → panel → widget → part. A level that names a property wins for that property; a level silent on a property takes the value from above; a property nobody names stays at the ordinary look. A global default of `{background: GRAY, content_color: DIM}` plus `.disabled(Appearance::new().border_color(RED))` on one slider resolves to gray, dim, *and* a red border. Silence means "no opinion," not "leave me alone": a level that must hold its ordinary look against an inherited bundle names the ordinary value explicitly, and `.disabled(Appearance::new())` is a no-op rather than a way to clear an inherited look.
  - **Cascade precedence and state precedence are separate axes, resolved in that order.** First resolve each of the four states independently down the levels (global → panel → widget → part). Only then layer the *active* states in `WidgetState::LAYER_ORDER` = `[Focused, Hovered, Pressed, Disabled]`. Composing active states per level and then resolving levels would let a part's local hovered bundle defeat an inherited disabled bundle.
  - **State appearance only exists inside a widget.** Hover, press, focus, and disabled are widget states; there is no text widget and no hoverable bare element. An element that authors a state look is a *widget part*, and a part is only placeable inside a widget's children.
  - **An ordinary declaration creates the retained record a state patches.** `VisualSlotOverride` patches records layout already emitted; a state layer never authors a missing role. `.background(X).disabled(Appearance::new().background(Y))` is not redundant — the ordinary call emits the fill record. The two documented escape hatches for a state-only role are `Border::all(Px(0.0), color)` and `El::new().background(Color::NONE)`.
  - **No state property may change solved layout.** Border width changes grow inward and re-key nothing.
  - **Public opaque types, not leaked private ones.** A `pub` trait whose methods mention `pub(crate)` types trips `private_interfaces` even when the methods live on a sealed trait in a private module; E0446 additionally forbids a public trait exposing a private associated type. Every type reachable from a public associated type — `WidgetBuilder`, `WidgetPart`, `EditableField`, the scope token — is a public opaque type with private fields.
  - **Presentation must not dirty `WidgetVisualOverrides` when resolved values are unchanged.** Compare through an immutable query and take `get_mut` only on inequality; comparing inside a method already reached through `Mut<_>` is too late.
  - **Workspace lints, inherited by both packages** (`[lints] workspace = true` in each `Cargo.toml`): `[lints.rust] missing_docs = "deny"` — every new public item needs a doc comment. `[lints.clippy]` denies the `all` / `cargo` / `nursery` / `pedantic` groups (`priority = -1`) plus `allow_attributes_without_reason`, `expect_used`, `panic`, `self_named_module_files`, `unreachable`, `unwrap_used`. No `.unwrap()` / `.expect()` / `panic!` in non-test code, and any `#[allow(...)]` needs a `reason = "…"`.
  - **Headless only.** No phase needs a GPU, a window, or a screenshot. Assertions are on resolved `VisualSlotOverride` values, `VisualOverrideIndex` membership, batch-key identity, and entity counts — never on rendered color. Harnesses: `HeadlessLayoutPlugin` (`panel/mod.rs:194`) for layout / reification / cascade resolution; a plain `App` with no render device for retained batching (precedent: `fill_batch.rs` 59 tests, `panel_text/batching.rs` 33, `panel_shapes/batching.rs` 31, `material_table.rs` 31); `trybuild` for typestate boundaries. Baseline: `verify.sh test hana_diegetic` reports **1107 passed / 2 skipped** at Phase 2 completion, against 1110 `#[test]` items in `crates/hana_diegetic/src` (one is feature-gated out of the default run). Measure with that command, not by counting the workspace — a phase's gate covers this package only. **No phase may land with a lower test count than it inherited.**

## Phases

### Phase 1 — `Appearance` bundle replaces the flat state builders · status: done (`3560036b`)

#### Work Order

**Goal:** `El` exposes four `Appearance`-taking state builders instead of sixteen flat ones, with per-state authored presence recorded as `Cascade`, and no observable behavior change.

**Spec:**

Make `Appearance` public with a fluent builder (`background`, `border_color`, `border_width`, `material`) and re-export it from the crate root. Its module is private today: `widgets/mod.rs:1` declares `mod appearance;` and the crate-root `pub use widgets::*` block (`src/lib.rs:339-398`) contains no `Appearance`-family symbol.

Replace the sixteen flat builders on `El<L, WidgetElement<W>>` (`builder.rs:786-939`) with `hovered`, `focused`, `disabled`, each taking an `Appearance`; keep `pressed` on the `HasPressedState` impl block. Delete the flat methods and their private `set_*` helpers.

Store each state as a `Cascade`-shaped value **from the start**. Raw defaulted fields cannot distinguish "the state method was never called" from `.hovered(Appearance::new())`, and the second must suppress an inherited bundle while the first inherits it; `CommonEl`'s outer `Option` records only that *some* state was authored, and Phase 10 cannot recover the distinction later.

```rust
#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct WidgetHoveredAppearance(Arc<Appearance>);
// … Pressed / Focused / Disabled

pub(crate) struct StateAppearance {
    hovered:  Cascade<WidgetHoveredAppearance>,
    pressed:  Cascade<WidgetPressedAppearance>,
    focused:  Cascade<WidgetFocusedAppearance>,
    disabled: Cascade<WidgetDisabledAppearance>,
}
```

Four distinct wrapper types are required because Bevy stores one component per Rust type, and Phase 9 registers four independent `CascadePlugin` channels.

The `Arc` payload is what keeps each attribute inside the per-attribute `size_of::<A>() <= CASCADE_ATTRIBUTE_BYTES` assertion — `Appearance` is 80 bytes today and 96 after Phase 7's fifth property, against a limit of 32 (`cascade/constants.rs:7`). Add **one size assertion per attribute**; the existing limit is asserted per attribute, not blanket — see the three precedents at `cascade/resolved.rs:118`, `:131`, `:144`.

Hand-implement `PartialEq` as `Arc::ptr_eq` **then** content equality. `Appearance` holds floats and cannot be `Eq`, so derived equality never takes the same-allocation shortcut and propagation would compare every property even between clones of one allocation. Content equality must be retained, not replaced: rebuilding an equal `Appearance` in a new allocation must not dirty `Resolved<A>`.

Resolve against `Appearance::default()` in this phase so behavior is unchanged — every property defaults to `Unchanged`, which matches the current `resolve` accumulator (`appearance.rs:136`), so explicit-empty and inherited bundles stay observably identical while the authored bit is retained for Phase 10.

`VisualChange<T> { Unchanged, To(T) }` (`appearance.rs:21`) stays as the per-property representation and gains no variant: `Cascade` records whether the bundle exists; `Unchanged` means the selected bundle leaves that property at its authored value or at an earlier active layer's result.

Update `examples/widgets.rs`, the library tests, and the trybuild fail fixture whose stderr names `pressed_background` — `tests/trybuild/fail/editable_widget_has_no_pressed_state.rs:11` calls `.pressed_background(Color::BLACK)` and the matching `.stderr:1` reports the `E0599`. Both must move to the `pressed` verb, and the regenerated stderr must be committed.

**Documentation.** The sixteen methods carry the property documentation and expose prerequisites through editor completion; four state verbs do not. This phase adds: an `Appearance` property table naming each property's target records and its authored prerequisite; a state matrix (button and slider reach pressed, editable fields do not, all three reach hovered/focused/disabled); state precedence and the merge rule; the two transparent-counterpart recovery forms (`Border::all(Px(0.0), color)` and `background(Color::NONE)`); and compiling examples. `missing_docs = "deny"` makes documentation on every new public item mandatory, not optional.

**Files:**
- `src/widgets/appearance.rs` — make `Appearance` public + fluent builder; add the four `Widget*Appearance` wrappers with hand-written `PartialEq` and per-attribute size assertions; reshape `StateAppearance` to four `Cascade` fields; keep `resolve` (`:136`) resolving against `Appearance::default()`.
- `src/widgets/mod.rs:1` — `mod appearance;` → `pub mod` (or targeted `pub use`).
- `src/lib.rs:339-398` — re-export `Appearance` and the four wrapper types from the crate root.
- `src/layout/builder.rs:786-939` — delete the sixteen flat builders and their `set_*` helpers; add `hovered`/`focused`/`disabled` on `El<L, WidgetElement<W>>` and `pressed` on the `HasPressedState` impl block.
- `src/widgets/button.rs`, `src/widgets/slider.rs`, `src/widgets/editable.rs`, `src/panel/builder.rs`, `src/layout/element.rs` — migrate flat-builder call sites.
- `examples/widgets.rs` — migrate call sites (`:1162`, `:1200`, `:1450` region).
- `tests/trybuild/fail/editable_widget_has_no_pressed_state.rs` + `.stderr` — migrate and regenerate.

**Constraints from prior phases:** none — this is Phase 1.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic trybuild`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- `rg -n '\b(hovered|pressed|focused|disabled)_(background|border_color|border_width|material)\b' crates/hana_diegetic` **returns nothing.** At `HEAD` `64f8bdc0` this matches 89 lines across 9 files (`button.rs` 28, `slider.rs` 23, `layout/builder.rs` 16, `panel/builder.rs` 7, `examples/widgets.rs` 6, `editable.rs` 4, `layout/element.rs` 2, and 3 in the trybuild fixture pair). Re-verify the live count at dispatch; `--lib` alone does not compile the trybuild target, so a migration can pass the library suite with stale compile-fail fixtures.
- No behavior change: existing validation errors and their messages are unchanged.
- A named test proves an **explicit empty bundle is retained as an override** rather than collapsing to inherit.
- Compile-pass coverage exercises every property on every state for both a button and a slider.

### Retrospective

**What worked:**
- The four `Arc`-backed wrapper types, per-attribute size assertions, and `Arc::ptr_eq`-then-content `PartialEq` landed exactly as specified; every size assertion holds at the 32-byte limit.
- Storing per-state presence as `Cascade` from the start cost nothing this phase and is now pinned in both directions by a test: `.hovered(Appearance::new())` stores `Override`, and the three un-authored states stay `Inherit`.
- The residual scan returns nothing, and the new compile-pass fixture exercises all four properties across all four states for both a button and a slider.

**What deviated from the plan:**
- `widgets/mod.rs:1` stayed `mod appearance;` rather than becoming `pub mod`. The Files entry offered either that or targeted `pub use`; the re-exports alone are sufficient, since `widgets` is itself a private module and `pub mod` would have widened nothing.
- Clippy required `Appearance::new`, `background`, and `border_color` to be `const fn`. `border_width` and `material` cannot be.

**Surprises:**
- **The phase acceptance gate cannot catch a broken doc link.** `verify.sh` has no rustdoc verb, so a public item linking to a `pub(crate)` type passes `check`, `test`, and `lint` and only fails much later, at the workspace doc lint. Phase 1 shipped exactly that defect (`Appearance` linking to the crate-private `VisualChange::Unchanged`); the blind reviewer caught it by reading. Every remaining phase that adds public API has the same hole.
- **The removed builders accumulated; the new verbs replace.** `hovered_background(a)` followed by `hovered_border_color(b)` produced one layer carrying both, whereas a second `hovered(…)` discards the first bundle. Migrating two chained calls into two chained calls silently drops the first. Every migrated call site was merged correctly, and the four verbs now document the replacement, but the plan did not name this hazard.
- The test-count floor stated during dispatch was wrong. The package runs 1102 tests (1100 passed, 2 skipped) — see the baseline note in Delegation Context. Measure with `verify.sh test hana_diegetic`, not by counting the workspace.

**Implications for remaining phases:**
- Phases 4, 7, 9, and 11 all add public API and inherit the doc-link blind spot. Until a rustdoc step exists, treat public-item intra-doc links as review-only, or run `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p hana_diegetic` before the checkpoint.
- Phase 10 can rely on the authored-vs-absent distinction being both stored and tested; it does not need to reconstruct it.
- Phase 7's fifth property must be added to the hand-written `PartialEq` content comparison as well as to the struct — the `Arc` keeps the size assertion satisfied, so nothing else there changes.

### Phase 1 Review

- **Phase 2** re-scoped: the four root `Cascade` values it listed as work already exist as `ComputedWidgetRecord.appearance` (`id.rs:127`), populated by the ownership walk. Phase 2 now adds only the sparse part map.
- **Phase 2** inherited a `**Pending decision:**` on how the four widget-level bundles sit on the widget entity — one aggregate component or four standalone ones. Phase 1 inserts one aggregate (`reify.rs:322`), but `propagate_cascade` only sees standalone `Cascade<A>` components and strips `Resolved<A>` without them, so Phases 9 and 10 could not work as written. Raised at Phase 2 rather than Phase 9 because Phase 3 rewrites all three presenters against whichever shape wins. **Resolved 2026-07-28: dissolve the aggregate** — `StateAppearance` loses its `Component` derive and the entity carries the four channels exclusively, landed in Phase 2. Phase 10's entity-shape bullet moved here with it.
- **Phase 3** now points at the Phase 2 decision before rewriting the presenters, and its `presentation_inputs_changed` reference moved to `slider.rs:1137`.
- **Phase 4** must now list `tests/trybuild.rs` in its Files: the driver's globs are what make a fixture reachable, none of them matches Phase 4's four new fail fixtures, and its compile-pass additions sit behind an `#[ignore]`d test — so four of its acceptance-gate lines would have passed while compiling nothing.
- **Phase 4** and **Phase 11** gained the replacement-not-accumulation constraint: a state verb replaces the whole bundle, so chained single-property calls silently drop all but the last.
- **Phase 8** carries a `**Pending decision:**` on whether an explicitly authored empty bundle suppresses an inherited one. The document currently says both — Phase 1's archived Spec says suppression, the invariant and Phases 8 and 10's gates say no-op.
- **Phase 8** now says to write `merge_over` as a thin owned wrapper over the `layer_onto` fold Phase 1 shipped, and drops the suggested `VisualChange::or`, which would have been a third copy of the same per-property rule.
- **Phase 7** gained a `size_of` assertion for `VisualSlotOverride` at the size it grows the type to, so Phase 11's "back to 144 bytes" is a verified delta rather than a first measurement.
- **Phase 10**'s `resolve` entry now says what actually changes — where the four layers come from — rather than implying the layering algorithm is rewritten.
- **Delegation Context** gained a **Docs** entry: `verify.sh` has no rustdoc or doctest verb and `cargo nextest run` does not execute doctests, so a public item linking a crate-private type passes every gate. Phase 1 shipped exactly that defect. Phases 4, 5, 7, 9, and 11 now carry an orchestrator-run docs gate line.
- **Delegation Context** corrections: three claims were false after Phase 1 (no `Appearance` re-export; the trybuild fixture naming `pressed_background`; the sixteen flat builders at `builder.rs:786-939`), and line references into `builder.rs`, `appearance.rs`, `widgets/mod.rs`, `cascade/mod.rs`, `lib.rs`, and `slider.rs` were re-verified and updated across every remaining phase.

### Phase 2 — Per-element appearance storage · status: done

#### Work Order

**Goal:** `ComputedWidgetRecord` carries a revision-scoped, sparse, capability-masked map of per-owned-element appearance, populated by the existing ownership walk — with nothing reading it yet.

**Spec:**

`ComputedWidgetRecord` (`widgets/id.rs:122`) gains a per-owned-element appearance map **alongside — not merged with — the root's four authored `Cascade` values**. The root's bundle is the widget's own override and must not also be applied as a part override, so the two stay in separate fields.

The root side already exists and is **not** work for this phase: `ComputedWidgetRecord.appearance: StateAppearance` (`id.rs:127`) is exactly those four `Cascade<Widget*Appearance>` values after Phase 1, it is populated by the ownership walk (`layout/element.rs:868`), and it is read back through `ComputedWidgetRecord::appearance()` (`id.rs:170`). This phase adds only the **part** map beside it.

The map is a **sorted, sparse** `Vec<(element_index, …)>` holding only elements that authored something. It is filled by the walk that already visits every owned element (`layout/element.rs:879`, which calls `push_visual_element` at `widgets/id.rs:188`), cloning authored entries only.

The map is **scoped to the current computed tree revision** and replaced together with `WidgetVisualSlots` on every computed-panel update, with all prior keys removed before insertion. Structural tree replacement compacts and renumbers element indices, and editable fields exercise that path on every display↔editor transition (`layout/element.rs:1001` `set_field_editing_content` rebuilds a compact arena), so a stale index silently repaints the wrong element. Indices participate in equality and change detection.

**Recipient metadata.** `visual_elements` lists every owned layout node, including structural containers that emit no retained record — the example slider owns eight elements but only three are recipients (label, track, thumb). Carry a compact property-capability mask beside each recipient index (`{usize, u8}`, 16 bytes against today's 8 bytes for a bare index) and **omit pure containers**, so Phase 3's resolution never builds and compares a 160-byte override that can only be a no-op. The mask records which of the appearance properties the element can present, derived from the ordinary roles it declared: SDF fill, border, material, and (from Phase 7) text/image/`PanelDraw` content.

At this point only the widget's own element can author a bundle, so nothing changes on screen and no presenter reads the map.

**Widget entity shape — decided, not open.** `StateAppearance` **stops being a `Component`**. It remains the authoring-time and computed-record struct (`layout/builder.rs:395`, `layout/element.rs:147`, `widgets/id.rs:127`); the widget *entity* carries the four values as four standalone components instead, exactly as `Cascade<WidgetInteractivity>` already does two lines away.

The reason is mechanical: `propagate_cascade` queries `Query<&Cascade<A>>` and *removes* `Resolved<A>` from any entity lacking a standalone `Cascade<A>` (`crates/bevy_kana/src/cascade.rs:361-400`). A value one field deep inside a `StateAppearance` component is invisible to it, so Phase 9's four channels and Phase 10's resolution would see no widget-level override on any widget. Landing the shape here — before Phase 3 rewrites all three presenters against it — avoids writing that work twice.

Concretely:

- Drop `Component` from `StateAppearance`'s derive (`widgets/appearance.rs:279`). Every other derive stays.
- Add a crate-private borrowed view over the four cascades, and move the runtime read path onto it so there is one implementation, no per-frame clone, and no lifetime threading at the call sites:
  ```rust
  pub(crate) struct WidgetStateCascades<'a> {
      hovered:  &'a Cascade<WidgetHoveredAppearance>,
      pressed:  &'a Cascade<WidgetPressedAppearance>,
      focused:  &'a Cascade<WidgetFocusedAppearance>,
      disabled: &'a Cascade<WidgetDisabledAppearance>,
  }
  ```
  It owns `layer`, `any`, and `resolve` (today `appearance.rs:287` / `:309` / `:324`); `StateAppearance` keeps a `cascades(&self) -> WidgetStateCascades<'_>` accessor plus a `WidgetStateCascades::new(&, &, &, &)` constructor for query terms. Build-time validation (`layout/element.rs:1290-1301`, four `appearance.any(…)` calls) goes through the accessor.
- `spawn_widget` (`reify.rs:291`) inserts **all four**, `Cascade::Inherit` included — a missing component is the case that makes `propagate_cascade` strip `Resolved`. `update_widget` (`reify.rs:341`) compares and inserts **per channel**, replacing the single `existing_appearance != appearance` check (`:410`); the reify query at `:199` takes the four components in place of `&StateAppearance`.
- The three presenters take the four components in their queries (`button.rs:181`, `slider.rs:1195`, `editable.rs:70`) and build a `WidgetStateCascades` to call `resolve`. Their run conditions replace `Changed<StateAppearance>` with the four `Changed<Cascade<Widget*Appearance>>` terms (`button.rs:138`, `slider.rs:1144`, `editable.rs:34`).

This is a shape change only — no resolution behavior moves in this phase, and the presentation tests must pass unchanged.

**Files:**
- `src/widgets/id.rs:122` — add the sparse part map to `ComputedWidgetRecord`; the four root `Cascade` values are already there as `appearance` (`:127`) and need no change. `push_visual_element` (`:188`) gains the capability mask and the container filter.
- `src/layout/element.rs:868` — the owning-record walk fills the map from authored entries; `:823` `computed_widget_records` propagates it. `:1290-1301` — the four `any` calls move to the borrowed-view accessor.
- `src/widgets/visual.rs:234` — `WidgetVisualOverrides` retires prior keys on computed-panel update, alongside `WidgetVisualSlots`.
- `src/widgets/appearance.rs:279` — drop the `Component` derive; add `WidgetStateCascades<'a>` with `layer` / `any` / `resolve` and the `StateAppearance::cascades` accessor.
- `src/widgets/reify.rs` — query (`:199`), `spawn_widget` (`:291`, insert all four), `update_widget` (`:341`, per-channel comparison replacing `:410`).
- `src/widgets/button.rs` (`:138`, `:181`), `src/widgets/slider.rs` (`:1144`, `:1195`), `src/widgets/editable.rs` (`:34`, `:70`) — four query terms and four `Changed` terms in place of one.

**Constraints from prior phases:**
- **Phase 1** made `Appearance` public with a fluent builder and re-exported it plus `WidgetHoveredAppearance` / `WidgetPressedAppearance` / `WidgetFocusedAppearance` / `WidgetDisabledAppearance` from the crate root. Each wrapper is `Arc<Appearance>` with hand-written `PartialEq` (`Arc::ptr_eq` then content equality) and its own `size_of` assertion against `CASCADE_ATTRIBUTE_BYTES`.
- **Phase 1** reshaped `StateAppearance` to four `Cascade<Widget*Appearance>` fields, so per-state *authored presence* is already recorded and must be carried through the map, not flattened.
- Only `El<L, WidgetElement<W>>` can author a bundle after Phase 1; parts arrive in Phase 4. Tests here reach non-root elements through a crate-internal path.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- Existing button, slider, and editable presentation tests pass unchanged; the inherited test count does not drop.
- A test asserts the map holds only authored entries, sorted by element index.
- **The example slider's structural containers produce no map entry** — 8 owned elements, 3 recipients.
- Structural reordering and an editable display → editor → display transition retarget the map correctly and retire old indices.
- A reified widget carries all four `Cascade<Widget*Appearance>` components — including the ones left `Cascade::Inherit` — and carries **no** `StateAppearance` component.
- Re-authoring exactly one state re-inserts only that channel; the other three are untouched.

### Retrospective

**What worked:**
- The entity-shape decision landed exactly as specified. `StateAppearance` lost its `Component` derive (`appearance.rs:284`), `WidgetStateCascades<'a>` (`:299`) took over `any` / `resolve`, and `spawn_widget` now inserts the four channels as a nested tuple — which Bevy treats as a `Bundle`, so four separate components reach the entity. Presentation tests passed unchanged, confirming this was a shape change only.
- Deciding the shape here rather than at Phase 9 paid off immediately: Phase 3 rewrites all three presenters, and they now already query the four channels and build the borrowed view.
- The capability mask (`VisualElementCapabilities`, `id.rs:115`) derives cleanly from the ordinary roles an element declared, and the container filter drops the example slider from 8 owned elements to 3 recipients as predicted.

**What deviated from the plan:**
- The part map lives inside `WidgetVisualSlots` (`visual.rs:83`) rather than in its own component. The plan said "replaced together with `WidgetVisualSlots`"; sharing the component achieves that for free but couples the two — see Implications.
- `WidgetVisualSlots::part_appearances()` shipped `#[cfg(test)]` (`visual.rs:119`) because nothing outside tests reads it yet. `ComputedWidgetRecord::part_appearances()` (`id.rs:216`) is not gated.
- `computed_widget_records` crossed the 100-line clippy limit once the part-appearance push and capability filter were added; the ownership-walk body was extracted to `record_owned_widget_element` (`element.rs:1356`).

**Surprises:**
- **Admission must key on override presence, not property authorship.** The first implementation admitted a part only when some inner `Appearance` named a concrete property, which made `.hovered(Appearance::new())` indistinguishable from never authoring hovered at all — destroying exactly the information Phase 8's deferred empty-bundle decision needs. Corrected to `WidgetStateCascades::any_overridden()` (`appearance.rs:323`), which tests for `Cascade::Override`.
- **A retained-record recipient is not the same as "declares a draw."** `PanelDraw::shapes([])` and `lines([])` are public and produce a draw that emits nothing (`positioning.rs:328` returns early on an empty shape list), so the CONTENT capability bit is gated on a non-empty shape list, not on the presence of a `PanelDraw`.
- **A stale-index test must drive the component that actually gets replaced.** Two tests written against `LayoutTree::computed_widget_records` could not fail on a retained stale index — one recomputed from an untouched tree, the other reordered before its only compute. Real coverage required app-level tests in `reify.rs` that push a structural change and an editor round trip through the widget entity's `WidgetVisualSlots`.

**Implications for remaining phases:**
- **Phase 3 must un-gate `WidgetVisualSlots::part_appearances()`** — it is `#[cfg(test)]` today and Phase 3 is its first non-test reader.
- **Part appearance and slot geometry share one change-detection signal, and the coupling runs one way only.** Both live in `WidgetVisualSlots`, but a slider drag does *not* dirty it: `reify_widgets` runs only under `Changed<ComputedDiegeticPanel>` (`reify.rs:194`), `update_widget` re-inserts the component only on inequality (`reify.rs:445`), and a drag changes `SliderState` without relayout (proven by `thumb_translation_tracks_applied_value_without_relayout`, `slider.rs:5182`). The real cost is the other direction: re-authoring any part dirties `WidgetVisualSlots`, which additionally wakes `dispatch_visual_overrides` (`visual.rs:463`) into a full remove-and-rebuild of that widget's `VisualOverrideIndex` entries. Phase 3 should design against that direction; if it bites, splitting the map into its own component is the fix.
- Phase 8 can still make the empty-bundle decision either way: the authored-vs-inherited distinction survives storage intact.

### Phase 2 Review

- **Phase 5** retargeted: its Spec, Files, and Phase 2 constraint all aimed validation at the ownership walk, which is `computed_widget_records` — it returns `Vec<ComputedWidgetRecord>` with no `Result` and runs on every compute, so it cannot raise a build error. Validation moves to `validate_tree`'s stack walk (`element.rs:785-836`), the only appearance-reachable walk that both knows the owner and returns `Result<_, PanelBuildError>`, and calls `element_visual_capabilities` (`:1332`) directly instead of reading a mask off a record that does not exist yet at build time.
- **Phase 4** gained a gate line closing an invariant breach it would otherwise open: Phase 2 admits a part appearance with no capability gate, and `validated_element_appearance` is still reached only from the widget and editable branches — so opening authoring to every `El` would let a bundle on a structural container compile, store, and never present for the whole interval until Phase 5 lands. Phase 4 now rejects a bundle on a zero-capability element outright; Phase 5 refines it to per-property with part-naming locations.
- **Phase 3** gained the write-path requirement: `WidgetVisualOverrides` is slot-keyed and index-free today (which is why Phase 2 correctly changed nothing in it), so Phase 3's element-index channel is the first index-keyed data on it and the first to inherit the renumbering hazard. All three presenters must build a complete desired set through `write_widget_overrides`; the button and editable presenters write one slot today and cannot drop an orphaned key.
- **Phase 3** also gained: the two `#[cfg(test)]` accessors it must un-gate, the note that only the button presenter still lacks a widget-kind filter, the existing override map at `visual.rs:491` to merge into rather than duplicate, and two gate lines covering stale-index retirement and button wake-up.
- **Phase 7** widened: its acceptance already demands material reach text and draw while image-only elements stay rejected, which a single `CONTENT` bit cannot express. It now splits the mask into `TEXT` / `IMAGE` / `DRAW` and widens the `SDF_MATERIAL` derivation, rather than "extending" the mask.
- **Phase 10** gained the two-view requirement: `WidgetStateCascades` is defined over `&Cascade<…>`, but this phase reads `Resolved<…>`, which is not a `Cascade` — and build-time validation still needs the authored view. Two views sharing one `LAYER_ORDER` fold, plus the `appearance.rs` Files entry the Work Order lacked.
- **Phase 6** shrank: re-keying across the editor transition is free, since the part map is re-derived from the tree every compute and replaced wholesale. Its re-keying gate line is dropped as already covered by a Phase 2 test.
- **Phase 9** narrowed: the four attribute types are already exported from the crate root (Phase 1), so only the panel-builder methods and the eight commands need export work.
- **Phase 8**'s pending empty-bundle decision gained a second consequence — Phase 2 keyed part-map admission on override presence, so an empty bundle creates a map entry that can never change a pixel under the no-op reading. The decision block now also asks whether admission stays override-keyed, with a recommendation to keep it.
- **Delegation Context** and every remaining phase had their file/line references re-verified against the post-Phase-2 tree; the test-count floor moved from 1100 to **1107 passed / 2 skipped**.
- **Retrospective corrected:** the claim that a slider drag marks part appearance changed was wrong. Reification is gated on `Changed<ComputedDiegeticPanel>` and re-inserts slots only on inequality, and a drag does not relayout. The coupling runs the other way — re-authoring dirties the slots component and wakes a full index rebuild.

### Phase 3 — Element override channel and dirty-entity presentation · status: todo

#### Work Order

**Goal:** Resolved overrides reach any owned element, all three presenters resolve every recipient, and each presenter wakes only for the widgets that changed.

**Spec:**

`WidgetVisualOverrides` (`widgets/visual.rs:255`) gains an **element-index-keyed channel** alongside the slot-keyed one, merged in `dispatch_visual_overrides` (`:463`) into the map already built at `:491`. Store element overrides sorted so dispatch merges them with slot overlays into that existing map rather than allocating a second one.

**Slot-versus-element precedence is fixed here:** presentation-owned computed slot values (the slider thumb's `offset`) are preserved unconditionally; the resolved element override composes on top of the authored slot baseline.

All three presentation systems (`button.rs`, `slider.rs` `present_slider_state` `:1194`, `editable.rs`) resolve **every recipient** rather than the root slot alone, by merge-walking Phase 2's sparse authored list against the ordered recipient list — `O(recipients + authored)`, no linear `find` per element.

**All three must write through `write_widget_overrides` (`visual.rs:314`), building a complete desired set.** `present_slider_state` already does (`slider.rs:1288`); `present_button_state` (`button.rs:241`) and `present_editable_state` (`editable.rs:122`) write a single slot through `write_slot_override` (`visual.rs:348`), which cannot drop an orphaned key. This matters now and did not before: `WidgetVisualOverrides` is slot-keyed today and therefore index-free, so Phase 2 correctly changed nothing in it — this phase's element-index-keyed channel is the **first** index-keyed data on that component and is the first to inherit the renumbering hazard. A per-slot write would strand overrides on element indices that no longer exist.

Each presenter processes **only dirty entities**: the `Changed<…>` queries and kind-filtered `RemovedComponents` drains that today live in the run condition (`slider.rs:1138` `presentation_inputs_changed`) move into the writer, which then uses `Query::get`. **The presenter owns those drains outright** — a run condition that consumes a removal stream before the writer sees it is the failure mode to avoid. Without this, one dragging slider (a drag changes `SliderState` every frame) wakes a system that re-resolves every recipient of every live slider on every drag frame.

**The kind filter must be *added* for the button presenter, not merely moved.** `slider::presentation_inputs_changed` (`slider.rs:1138`) filters `WidgetKind::Slider` and `editable::presentation_inputs_changed` (`editable.rs:29`) filters `WidgetKind::EditableField` on both its changed query and its removal drains, but `button::presentation_inputs_changed` (`button.rs:134`) filters `With<WidgetOf>` alone and drains removals unfiltered — so it currently wakes on every widget kind.

Resolution borrows the highest-precedence authored value per property and clones the winning material handle **exactly once** when constructing `VisualSlotOverride`; it does not clone intermediate `Appearance` layers. Dispatch then clones the finished override into `VisualOverrideIndex` (`visual.rs:413`) — one further handle clone, unavoidable.

`dispatch_visual_overrides` already builds a `HashMap<usize, VisualSlotOverride>` (`visual.rs:491`) — the one Phase 11 deletes along with `subtree_color`. The element channel **merges into that existing map**; do not introduce a second one.

At this point only the widget's own element can author a bundle, so nothing changes on screen.

**Files:**
- `src/widgets/visual.rs` — element-index-keyed channel on `WidgetVisualOverrides` (`:255`); merge + precedence in `dispatch_visual_overrides` (`:463`) into the existing map at `:491`; un-gate `part_appearances()` (`:119`, currently `#[cfg(test)]`).
- `src/widgets/button.rs` (`:134`, `:241`), `src/widgets/slider.rs` (`:1138`, `:1194`, `:1288`), `src/widgets/editable.rs` (`:29`, `:122`) — merge-walk every recipient; move `Changed<…>` / `RemovedComponents` from run conditions into the writers; route all three through `write_widget_overrides`; add the kind filter to button.
- `src/widgets/mod.rs:299-313` — the three `.run_if(...)` attachments this phase removes live at `:300` (button), `:304` (editable), `:308` (slider).

**Constraints from prior phases:**
- **Phase 1:** four `Cascade<Widget*Appearance>` fields on `StateAppearance`; `Appearance` public with `background` / `border_color` / `border_width` / `material`; `resolve` still resolves against `Appearance::default()`.
- **Phase 2:** `ComputedWidgetRecord` (`id.rs:138`) carries a sorted sparse `part_appearances` (`:144`, read via `:216`) **plus separately** the four root `Cascade` values in `appearance` (`:143`, read via `:188`); each recipient index in `visual_elements` carries a `VisualElementCapabilities` mask (`:115`) and pure structural containers are excluded. The map is re-derived and replaced wholesale on every computed-panel update. Merge-walk against this ordering — do not re-sort or build a lookup map.
- **Phase 2 settled the widget entity's shape:** `StateAppearance` is no longer a `Component`. The entity carries four standalone `Cascade<Widget*Appearance>` components, all four always present (`Cascade::Inherit` included), and the three presenters already query them and build a `WidgetStateCascades<'_>` borrowed view to call `resolve` (`appearance.rs:367`). Their run conditions already carry the four `Changed<Cascade<Widget*Appearance>>` terms. Extend that shape — do not reintroduce an aggregate component.
- **Phase 2:** two accessors this phase needs are `#[cfg(test)]` today and must be un-gated as it reaches them — `WidgetVisualSlots::part_appearances()` (`visual.rs:119`), whose first non-test reader is this phase, and `LayoutTree::set_element_state_appearance` (`element.rs:461`), which is the "crate-internal path" this phase's non-root authoring test uses (only `El<L, WidgetElement<W>>` can author until Phase 4, so a test cannot reach a non-root element through the public builder).
- **Phase 2:** a slider drag does **not** dirty `WidgetVisualSlots` — `reify_widgets` is gated on `Changed<ComputedDiegeticPanel>` (`reify.rs:194`) and `update_widget` re-inserts only on inequality (`reify.rs:445`). The coupling to design against runs the other way: re-authoring a part dirties `WidgetVisualSlots`, which wakes `dispatch_visual_overrides` (`visual.rs:463`) into a full rebuild of that widget's index entries.
- The **Presentation must not dirty `WidgetVisualOverrides`** invariant binds this phase directly: compare through an immutable query and take `get_mut` only on inequality.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- Existing button, slider, and editable presentation tests pass unchanged.
- A test authors a bundle on a **non-root owned element** through a crate-internal path and observes the override reach that element.
- Re-running presentation with identical active states leaves `Changed<WidgetVisualOverrides>` empty.
- A test asserts the slider thumb's computed `offset` survives an element override that does not name `offset`.
- A dragging slider does not wake resolution for a second, unrelated live slider.
- A dragging slider does not wake the button presenter (its run condition gains the kind filter it lacks today).
- The example slider's structural containers produce no `VisualOverrideIndex` entries.
- A structural change that renumbers element indices leaves **no** override on a stale index, for a button and an editable field as well as a slider — the two per-slot writers now build a complete desired set.

### Phase 4 — Widget parts: the part role and the builder acceptance relation · status: todo

#### Work Order

**Goal:** An `El` inside a widget's children can author a state bundle and becomes a widget part; the same call outside any widget fails to compile.

**Spec:**

The four state methods move to `El<L, LayoutOnly>`, transitioning it to the part role with the owner kind as an inferred output parameter. A widget root keeps `WidgetElement<W>`.

```rust
impl<L> El<L, LayoutOnly> {
    pub fn hovered<W: WidgetOwner>(self, appearance: Appearance) -> El<L, WidgetPart<W>>;
    pub fn focused<W: WidgetOwner>(self, appearance: Appearance) -> El<L, WidgetPart<W>>;
    pub fn disabled<W: WidgetOwner>(self, appearance: Appearance) -> El<L, WidgetPart<W>>;
    pub fn pressed<W: HasPressedState>(self, appearance: Appearance) -> El<L, WidgetPart<W>>;
}
```

with the same four on `El<L, WidgetPart<W>>` and `El<L, WidgetElement<W>>` returning `Self`, `pressed` bounded on `HasPressedState` in each. The explicit generic signatures are load-bearing: without them `pressed` ends up either unconditional or unreachable.

**The gate is an acceptance relation between builder and role, not an associated type on the inserted element's role alone.** A role-only mapping fails twice: `LayoutOnly` must yield the ordinary builder at panel level but a widget-scoped builder beneath a widget, so an ordinary intermediate container silently loses the owner; and making the part role an `ElementRole` also makes it acceptable to the ordinary builder, so the outside-a-widget rejection never fires.

```rust
#[doc(hidden)]
pub trait AcceptsElement<Role: ElementRole>: private::BuilderSealed {
    type ChildBuilder<'a>: LayoutContentBuilder where Self: 'a;
}

impl                 AcceptsElement<LayoutOnly>       for LayoutBuilder        { type ChildBuilder<'a> = LayoutBuilder; }
impl<W: WidgetOwner> AcceptsElement<WidgetElement<W>> for LayoutBuilder        { type ChildBuilder<'a> = WidgetBuilder<'a, W>; }
impl<W: WidgetOwner> AcceptsElement<LayoutOnly>       for WidgetBuilder<'_, W> { type ChildBuilder<'a> = WidgetBuilder<'a, W> where Self: 'a; }
impl<W: WidgetOwner> AcceptsElement<WidgetPart<W>>    for WidgetBuilder<'_, W> { type ChildBuilder<'a> = WidgetBuilder<'a, W> where Self: 'a; }

// intentionally absent — these two omissions are the guarantee
// impl<W>    AcceptsElement<WidgetPart<W>>    for LayoutBuilder
// impl<W, V> AcceptsElement<WidgetElement<V>> for WidgetBuilder<'_, W>
```

The implementations are disjoint by nominal role type, so there is no coherence conflict. The GAT lifetime is the mutable reborrow for one child closure; it binds neither the element, the owner marker, nor the tree.

**`with_root` needs a second, non-GAT selector.** `LayoutBuilder::with_root` (`builder.rs:1153`) constructs its `LayoutBuilder` locally, so it cannot return a wrapper borrowing that local. Add `RootElementRole` with an owned `type Builder`, and give `WidgetBuilder` owned-or-borrowed private storage:

```rust
enum WidgetBuilderStorage<'a> { Owned(LayoutBuilder), Borrowed(&'a mut LayoutBuilder) }

pub trait RootElementRole: ElementRole + private::RootElementRoleSealed { type Builder: LayoutContentBuilder; }
impl                 RootElementRole for LayoutOnly       { type Builder = LayoutBuilder; }
impl<W: WidgetOwner> RootElementRole for WidgetElement<W> { type Builder = WidgetBuilder<'static, W>; }

impl<W: WidgetOwner> WidgetBuilder<'static, W> { pub fn build(self) -> LayoutTree; }
```

`with` (`:1185`), `text` (`:1215`), and `image` (`:1247`) on both builders take `where Self: AcceptsElement<Role>` and pass `&mut <Self as AcceptsElement<Role>>::ChildBuilder<'_>` to the closure. Ordinary-content helpers that must work in either context use one sealed `LayoutContentBuilder` trait that **reuses** `AcceptsElement<LayoutOnly>::ChildBuilder<'_>` rather than declaring a second GAT — two nominal implementers, so the single-implementer style rule does not apply, and a helper's signature changes from `&mut LayoutBuilder` to `&mut impl LayoutContentBuilder`.

**Owner kinds.** `EditableField` is a zero-sized owner marker that must *not* implement `Widget` — it has no pre-built declaration and no root-slot method to give:

```rust
pub trait WidgetOwner: private::WidgetOwnerSealed {}
pub trait Widget: WidgetOwner + private::WidgetSealed { /* existing */ }
pub trait HasPressedState: Widget {}

impl WidgetOwner for Button {}  impl WidgetOwner for Slider {}  impl WidgetOwner for EditableField {}
```

`El::editable_field` (`builder.rs:715`) returns `El<L, WidgetElement<EditableField>>`, and the locked compile-fail message reads `EditableField: HasPressedState`. This settles the outstanding `WidgetElement<ImeEditableFieldSpec>` item; the existing `tests/trybuild/fail/editable_widget_has_no_pressed_state.stderr` names that old type and must be regenerated.

`WidgetOwner` is **kept** rather than dropped: bounding the owner slot at the declaration beats letting `El<Row, WidgetPart<String>>` be a nameable type that fails to construct later, at a worse site.

Nested widgets are syntactically accepted today and rejected at build by `WidgetContainsInteractiveDescendant` (`layout/element.rs:773`); the missing `AcceptsElement<WidgetElement<V>> for WidgetBuilder<'_, W>` impl makes that a **compile error**, so convert the runtime test to compile-fail coverage. Tooltip APIs keep their explicit `LayoutOnly` parameters — add a compile-fail case locking that boundary.

The target authoring shape this phase enables:

```rust
builder.with(
    El::column().widget(SLIDER_ID, slider),
    |builder| {
        builder.text(
            Text::new("LEVEL", style).layout(
                El::new().disabled(Appearance::new().content_color(LABEL_DISABLED)),
            ),
        );
        builder.with(
            El::new()
                .background(TRACK_FILL)
                .disabled(Appearance::new().background(TRACK_DISABLED)),
            |_| {},
        );
        builder.with(
            El::new()
                .background(THUMB_FILL)
                .border(Border::all(CONTROL_BORDER_WIDTH, THUMB_BORDER))
                .slider_thumb()
                .disabled(
                    Appearance::new()
                        .background(THUMB_DISABLED)
                        .border_color(THUMB_BORDER_DISABLED),
                ),
            |_| {},
        );
    },
);

// outside any widget — does not compile
builder.with(
    El::new().background(PANEL_FILL).disabled(Appearance::new().background(GRAY)),
    |_| {},
);
```

(`content_color` lands in Phase 7; use the four Phase 1 properties here.)

**Files:**
- `src/layout/builder.rs` — `WidgetPart<W>`, `WidgetOwner`, `EditableField`, `AcceptsElement` + its four impls, `LayoutContentBuilder`, `RootElementRole`, `WidgetBuilder<'a, W>` with `WidgetBuilderStorage`; move the four state methods to `El<L, LayoutOnly>` and add them to `El<L, WidgetPart<W>>`; retarget `with_root` (`:1153`), `with` (`:1185`), `text` (`:1215`), `image` (`:1247`); `editable_field` (`:715`) returns `WidgetElement<EditableField>`.
- `src/lib.rs:339-403` — export the new public opaque types.
- `src/layout/element.rs:773` — the nested-widget runtime rejection becomes dead; convert its test to compile-fail.
- `tests/trybuild/fail/` — new fixtures: part authored outside a widget, `pressed` on an editable-field part, nested widget, tooltip on a part. Regenerate `editable_widget_has_no_pressed_state.stderr`.
- `tests/trybuild.rs` — **required, not optional.** The driver's globs are the only thing that makes a fixture reachable, and none of them matches the four new fail fixtures above, so without this file every trybuild line in the gate below passes while compiling nothing. Add or widen a glob to cover them. The `pass/typestate_helpers.rs` additions sit behind `typestate_helper_signatures_compile`, which is `#[ignore]` by default — either move them to a non-ignored test or lift the `#[ignore]`, otherwise the compile-pass coverage in the gate is equally vacuous. While here, rename `tooltip_typestate_signatures_compile`: it now also drives `editable_widget_*` and `pass/widget_state_appearance.rs`, so its name no longer describes what it covers.
- `tests/trybuild/pass/typestate_helpers.rs` — helper signatures in both builder contexts.

**Constraints from prior phases:**
- **Phase 1:** the four state verbs are `hovered` / `focused` / `disabled` / `pressed`, each taking `Appearance`; `pressed` is gated on `HasPressedState`. `Appearance` and the four `Widget*Appearance` wrappers are public at the crate root. **Each verb replaces the whole bundle for its state** — a second `hovered(…)` discards what the first authored, unlike the removed per-property builders, which accumulated into one layer. The four verbs added here on `El<L, LayoutOnly>` and `El<L, WidgetPart<W>>` must behave the same way and say so in their docs.
- **Phase 2:** `ComputedWidgetRecord` already carries the sparse part map keyed by element index with capability masks, and the root's four `Cascade` values separately. Parts authored here populate that map through the existing ownership walk — no new storage is needed.
- **Phase 3:** presentation already resolves every recipient and writes element-keyed overrides, so a part authored here presents without further presenter changes.
- **Phase 2 left a gap this phase must not walk into.** `record_owned_widget_element` (`element.rs:1356`) admits a part appearance on `any_overridden()` alone, with **no capability gate** — while `push_visual_element` skips zero-capability elements. `validated_element_appearance` (`element.rs:1304`) is still reached only from the widget-declaring and editable-field branches (`:1276`, `:1289`). Opening authoring to every `El` inside a widget therefore lets a bundle on a pure structural container compile, store, and never present, breaching the **accepted option must reach the runtime** invariant for the whole interval until Phase 5 lands. This phase closes the window with a whole-bundle rejection (see the gate line below); Phase 5 refines it to per-property with proper error locations.
- **Invariant:** every type reachable from a public associated type is a public opaque type with private fields (`private_interfaces` / E0446).

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic trybuild`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase adds public opaque types and doc examples, so both doc commands must pass before checkpoint.
- A slider's track, thumb, and label each author their own state look and present it.
- An element authoring a state look **outside any widget fails to compile**. The trybuild fixture must name the owner explicitly (`let part: El<Row, WidgetPart<Slider>> = …`) so the diagnostic is a stable `E0277` on `AcceptsElement<WidgetPart<Slider>>` rather than an inference-ambiguity `E0283`.
- A `pressed` bundle on a part of an editable field fails to compile, with the message naming `EditableField: HasPressedState`.
- Compile-pass coverage for: ordinary intermediate containers, text layouts, images, multiple nesting levels, extracted helpers (`&mut WidgetBuilder<'_, Slider>` and `&mut impl LayoutContentBuilder`), returned parts (`El<Row, WidgetPart<Slider>>`), and root widgets of all three kinds with styled descendants.
- A nested widget fails to compile; the former runtime test is gone.
- **No authored bundle is silently discarded.** A bundle on an element with an empty capability mask (a pure structural container) is a build error, not a stored-and-ignored entry. A whole-bundle rejection is sufficient here — Phase 5 replaces it with the per-property form and the part-naming error locations.

### Phase 5 — Part validation and appearance error locations · status: todo

#### Work Order

**Goal:** Every appearance-authoring element is validated per property against the records it actually emits, and a failure names the failing part rather than the owning widget.

**Spec:**

`validated_element_appearance` (`layout/element.rs:1304`) is called today only from the widget-declaring and editable-field branches (`:1276`, `:1289`), so merely permitting `appearance` on descendants would leave them **accepted and ignored** — a direct violation of the accepted-values-reach-runtime invariant.

Validate every appearance-authoring element once its owner is known, and **per property**: a bundle with a usable background and an unusable material rejects the material, rather than accepting the bundle because one property has a recipient.

**Validate in `validate_tree`'s stack walk (`layout/element.rs:785-836`), not in the ownership walk.** The ownership walk is now `record_owned_widget_element` (`:1356`), reached from `computed_widget_records` (`:838`) — which returns `Vec<ComputedWidgetRecord>` with **no `Result`** and runs on every compute rather than once at panel build, so it cannot raise `PanelBuildError`. `validate_tree`'s walk is the one that both threads the owner down (via `validated_element_widget_owner`, `:820`) and returns `Result<_, PanelBuildError>`. Compute the capability mask there by calling the free function `element_visual_capabilities` (`:1332`) directly; do not try to read a mask off `ComputedWidgetRecord`, which does not exist yet at build time.

Errors need a location that is not the owner's id. Add a shared opaque `WidgetAppearanceLocation` carrying:
- the owner widget id,
- the optional authored part id,
- the structural child path,
- the element kind.

Formatted as `widget 'level' part 'thumb'` when the part is named, and `widget 'level' anonymous text part at child path 0/1` when it is not. Each message names the **transparent-counterpart recovery**: `Border::all(Px(0.0), color)` for a state-only border, `El::new().background(Color::NONE)` for a state-only fill.

This is part-local validation only. Per the scope limit on the accepted-values invariant, higher-level (global / panel / widget) properties with no compatible recipient at a given element are **dormant** there, not an error — that path arrives in Phase 10 and must not be routed through this validation.

**Files:**
- `src/layout/element.rs` — new `WidgetAppearanceLocation`; `validated_element_appearance` (`:1304`) becomes per-property and is reached from `validate_tree`'s stack walk (`:785-836`); `validated_element_widget_owner` (`:1263`) supplies the owner; `element_visual_capabilities` (`:1332`) supplies the mask.
- `src/panel/builder.rs:45` — `PanelBuildError` variants carry `WidgetAppearanceLocation`.
- `src/lib.rs:339-403` — export `WidgetAppearanceLocation`.

**Constraints from prior phases:**
- **Phase 4:** any `El` inside a widget's children can now carry a bundle via `El<L, WidgetPart<W>>`; the owner kind `W` is known at the type level, and `WidgetOwner` covers `Button`, `Slider`, `EditableField`. Phase 4 already rejects a bundle on a zero-capability element as a whole; this phase replaces that whole-bundle check with the per-property form and the part-naming locations.
- **Phase 2:** `element_visual_capabilities` (`layout/element.rs:1332`) is a free function that derives the property-capability mask from one `Element` — call it directly during validation rather than recomputing which records an element emits. Its `CONTENT` bit covers text, image, and non-empty `PanelDraw` **together**; Phase 7 splits it.
- **Phase 1:** `Appearance`'s four properties are `background`, `border_color`, `border_width`, `material`. The fifth, `content_color`, arrives in Phase 7 and widens what this validation accepts — leave the per-property structure open to a fifth arm.
- The existing `StateMaterialRequiresSurface` error still accepts only a background or border; Phase 7 widens it.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase adds a public error-location type, so both doc commands must pass before checkpoint.
- A part authoring a property with no compatible ordinary record produces a build error naming that part, not the widget.
- A named part and an anonymous part produce the two documented location formats.
- A bundle with one usable and one unusable property rejects **only** the unusable property.
- Every appearance error message names its transparent-counterpart recovery.
- No appearance-authoring element reaches presentation unvalidated: a test walks a tree with parts at several depths and asserts each was validated.

### Phase 6 — Generated editable parts · status: todo

#### Work Order

**Goal:** An author can style the IME editor's generated text, selection, caret, and validation elements per state, so "any element a widget owns" holds for a focused field.

**Spec:**

`inline_editor_content_tree` (`src/ime/editor.rs:1132`) builds the editor's text, selection, caret, and validation elements **internally**, and `set_field_editing_content` (`layout/element.rs:1022`) removes the authored display descendants while editing. Without a path in, "any element a widget owns" is false for a focused field — nobody can author an element that does not exist in the source tree.

Define **stable authoring inputs** for those four generated parts and copy their bundles into the generated tree. The inputs are authored on the editable field's declaration (they have no `El` of their own to hang on) and are carried through the display↔editor transition so the generated parts receive resolved appearance in the frame they appear.

Re-keying across the transition is **already free and needs no work here.** The part map is re-derived from `element.appearance` on every compute (`element.rs:895` → `record_owned_widget_element` `:1356`) and replaced wholesale inside `WidgetVisualSlots`; once a bundle is in the regenerated tree it is keyed correctly by construction. Phase 2's `editable_tree_replacement_rekeys_part_appearance_entries` already proves it. This phase's only job is getting the bundles *into* the generated tree.

**Files:**
- `src/ime/editor.rs:1132` — `inline_editor_content_tree` accepts and applies the four generated-part bundles.
- `src/layout/builder.rs:715` — `El::editable_field` gains the four generated-part authoring inputs.
- `src/layout/element.rs:1022` — `set_field_editing_content` carries the bundles across the transition.

**Constraints from prior phases:**
- **Phase 4:** `EditableField` is the zero-sized owner marker, `El::editable_field` returns `El<L, WidgetElement<EditableField>>`, and `EditableField` implements `WidgetOwner` but **not** `Widget` — so `pressed` is unavailable on its parts by construction and must stay unavailable on the generated ones.
- **Phase 5:** part appearance is validated per property in `validate_tree`'s stack walk against a capability mask; the four generated parts are validated the same way once their bundles land in the tree.
- **Phase 2:** the part map is re-derived from the tree on every compute and replaced wholesale, so renumbering across the display↔editor transition re-keys itself; `editable_tree_replacement_rekeys_part_appearance_entries` already covers it.
- **Phase 3:** presentation resolves every recipient, so a generated part with a bundle presents with no presenter change.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- A display → editor → display transition asserts the **resolved appearance of each of the four generated editor parts**, in the frame the editor appears and again after it closes.
- A disabled editable field dims its editor text.

### Phase 7 — Content color · status: todo

#### Work Order

**Goal:** Text, images, and draw primitives change color with widget state, and state materials reach every record type the retained routes already support.

**Spec:**

Add a fifth property, `content_color`, to `Appearance`.

It does **not** map to `VisualSlotOverride::color`. `apply_sdf_visual_override` (`render/fill_batch.rs:1359`) reads `fill_color.or(color)` and `border_color.or(color)` — the generic `color` field (`widgets/visual.rs:170`) is the **fallback for every color role**, so it drives fill and border together. That is the mechanism behind `Slider::disabled_color`. A text element that also authors a background would therefore have its fill recolored by a text-color change.

Add a **distinct `content_color` override** consumed only by the text, image, and draw-primitive routes, leaving `fill_color` and `border_color` exclusive to SDF roles. `VisualSlotOverride` grows from 144 to 160 bytes for this phase; Phase 11 deletes the superseded generic `color` field and returns it to 144.

Widen the material counterpart at the same time. `StateMaterialRequiresSurface` accepts only a background or border today, but the retained routes already apply `VisualSlotOverride::material` to SDF, text, and **every** `PanelDraw` record — lines *and* `PanelCircle` (`layout/draw.rs:11` for `PanelDraw`; `layout/line.rs:42` for the `PanelShape` enum, `:64` for `PanelCircle`; `render/panel_shapes/batching.rs:989`). The counterpart becomes any emitted SDF, text, or `PanelDraw` record; image-only elements stay rejected. Content color's counterpart is text, image, or `PanelDraw` content.

**This requires splitting Phase 2's capability mask, not merely extending it.** `VisualElementCapabilities` (`widgets/id.rs:115`) ships one `CONTENT` bit covering text, image, and non-empty `PanelDraw` together, and sets `SDF_MATERIAL` only when a background or border exists (`element.rs:1340`). Material-accepts-text-and-draw-but-rejects-image-only is not expressible from a single bit, so replace `CONTENT` with `TEXT` / `IMAGE` / `DRAW` and widen the `SDF_MATERIAL` derivation in `element_visual_capabilities` (`element.rs:1332`) to any SDF, text, or `PanelDraw` record. Content color's capability is `TEXT | IMAGE | DRAW`; material's is everything except `IMAGE` alone.

**Files:**
- `src/widgets/appearance.rs:113` — fifth property on `Appearance` and its fluent setter; `Appearance::layer_onto` (`:172`) and `WidgetStateCascades::resolve` (`:367`) compose it. Add it to the hand-written `PartialEq` content comparison as well as to the struct.
- `src/widgets/visual.rs:168` — `content_color` on `VisualSlotOverride`.
- `src/render/panel_text/batching.rs` (`:288`, `:435`), `src/render/panel_shapes/batching.rs:989`, `src/render/analytic_paths/batching.rs:314` — consume `content_color`; images likewise.
- `src/widgets/id.rs:115` — split `CONTENT` into `TEXT` / `IMAGE` / `DRAW`.
- `src/layout/element.rs:1332` — widen the `SDF_MATERIAL` derivation and emit the three new content bits; `:1304` — widen `StateMaterialRequiresSurface`'s counterpart to any SDF/text/`PanelDraw` record and add the `content_color` counterpart arm.

**Constraints from prior phases:**
- **Phase 1:** `Appearance` is public with `background` / `border_color` / `border_width` / `material`, each a `VisualChange<T>`; adding a fifth field takes it from 80 to 96 bytes, which is why the cascade attributes carry `Arc<Appearance>` and each has its own `size_of` assertion against `CASCADE_ATTRIBUTE_BYTES = 32`. Do not add a `VisualChange` variant.
- **Phase 2:** each recipient index carries a property-capability mask (`VisualElementCapabilities`, `widgets/id.rs:115`) so containers and non-content elements stay excluded. Its one `CONTENT` bit conflates text, image, and draw, and `SDF_MATERIAL` is set only for background-or-border — both must change here, per the Spec.
- **Phase 5:** appearance validation is already per property and reached from `validate_tree`'s stack walk; the fifth property adds a fifth arm there, not a new call site.
- **Phase 6:** the four generated editor parts are recipients; editor text is the canonical `content_color` target.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase adds a public `Appearance` property and its doc entry, so both doc commands must pass before checkpoint.
- A `const _: () = assert!(size_of::<VisualSlotOverride>() <= …)` records the type's new size at the value this phase grows it to, following the per-attribute precedent at `widgets/appearance.rs:214`. Phase 11 shrinks it back and asserts the smaller number; without this line that later assertion is a first measurement rather than a verified delta.
- A disabled slider dims its label.
- A hovered button brightens its caption **without touching its fill**.
- A text element carrying its own background and border changes **only** its text color under a state.
- A circle-only part accepts and presents both material and content color.
- A state material on a text label wins over the `TextMaterial` cascade and restores it when the state clears.

### Phase 8 — Per-property merge in the cascade · status: todo

#### Work Order

**Goal:** Cascade resolution folds every authored level through a per-attribute combine, `Appearance` supplies a per-property merge, and every existing attribute keeps first-override-wins with no edit to it.

**Spec:**

Stock resolution returns the first `Cascade::Override` and stops — `resolve_from_queries` (`bevy_kana/src/cascade.rs:433`) and `resolve_from_world` (`:446`) both do — and the `CascadeDefault<A>` root is a *fallback*, never combined. The design requires **per-property merge at every hop**, including into the global default.

`Appearance`'s five fields are each `VisualChange<T>`, so a bundle is already a sparse per-property record. Merging is field-by-field: **the lower level's `To(value)` wins, otherwise the higher level's field carries through.** Write one

```rust
Appearance::merge_over(&self, higher: &Self) -> Self
```

used at **both** hops — level-to-level in Phase 10's stage 1, part-against-widget in its stage 2.

**Where the replace/merge choice lives — not on `CascadeAttribute`.** That trait (`bevy_kana/src/cascade.rs:174`) carries a **blanket impl at `:179`** over its bounds (`Clone + PartialEq + Send + Sync + FromReflect + TypePath + Typed + GetTypeRegistration + 'static`). A blanket impl means a hand-written `impl CascadeAttribute for WidgetHoveredAppearance { fn combine(…) }` is a **conflicting implementation** — the compiler rejects it. Adding a defaulted method there would be inert. Leave `CascadeAttribute` and its blanket impl exactly as they are.

The choice lives on `CascadeRoot` (`hana_diegetic/src/cascade/resolved.rs:175`) — the `pub(crate)` per-attribute trait that the `cascade_attribute!` macro (`:20`) already implements for every attribute, with **no** blanket impl:

```rust
pub(crate) trait CascadeRoot: bevy_kana::CascadeAttribute {
    fn root_default() -> Self;

    /// Combines a value authored lower in the chain with one authored above it.
    /// The default replaces the higher value outright.
    fn combine(lower: Self, _higher: &Self) -> Self {
        lower
    }
}
```

Every existing attribute takes the default and needs **no edit** — the macro emits only `root_default`. The four appearance attributes override it:

```rust
impl CascadeRoot for WidgetHoveredAppearance {
    fn root_default() -> Self { Self(Arc::new(Appearance::new())) }

    fn combine(lower: Self, higher: &Self) -> Self {
        Self(Arc::new(lower.0.merge_over(&higher.0)))
    }
}
```

**How the rule reaches `bevy_kana`.** `CascadeRoot` is `hana_diegetic`-private, so the rule travels as a function pointer on the plugin:

```rust
pub struct CascadePlugin<A: CascadeAttribute> {
    root: A,
    combine: fn(A, &A) -> A,
}
```

`CascadePlugin::new` (`:265`) keeps its current signature and defaults `combine` to a replace function; a `with_combine` builder sets it. `cascade_plugin<A: CascadeRoot>()` (`hana_diegetic/src/cascade/mod.rs:44`) becomes `CascadePlugin::new(A::root_default()).with_combine(A::combine)`. `Plugin::build` (`:276`) inserts the pointer as a **non-reflected `CascadeCombine<A>` resource** beside `CascadeDefault<A>` (`:237`) — a `fn` pointer is not `Reflect`, so it must not go inside `CascadeDefault`, which is `#[reflect(Resource)]`.

**The walk becomes a fold.** Both resolvers stop short-circuiting and accumulate, combining the root in last:

```rust
// bevy_kana/src/cascade.rs:433, today
if let Ok(Cascade::Override(value)) = authored.get(current) {
    return value.clone();
}

// with the hook
if let Ok(Cascade::Override(value)) = authored.get(current) {
    acc = Some(match acc {
        None        => value.clone(),
        Some(lower) => combine(lower, value),
    });
}
// …after the walk, and on every early exit (no `CascadeFrom`, cycle, depth limit)
acc.map_or(root, |lower| combine(lower, &root))
```

With the default `combine`, that returns exactly today's answer for every existing attribute: the first override, or the root when none. It loses the short-circuit — the chain is global → panel → widget, so at most two extra hops.

`propagate_cascade` (`:361`) gains a `Res<CascadeCombine<A>>` parameter and passes the pointer to `resolve_from_queries` at `:399`. `resolve_entity_cascade` (`:332`) reads it from the world for `resolve_from_world` (`:446`).

**Not threaded:** `resolve_cascade` (`:146`) and `resolve_cascade_ref` (`:161`) are unbounded-generic public helpers with **no `hana_diegetic` call site** (only `bevy_kana`'s own test at `:502` and the re-exports). `resolve_cascade_ref` returns `&'a T` and *cannot* merge — merging produces a new value. Leave both alone.

**Rejected alternatives, do not implement:**
- **Drop the blanket impl and put `combine` on `CascadeAttribute`.** Forces an explicit `impl CascadeAttribute for X {}` on every attribute type in both crates and is a breaking change for any other `bevy_kana` consumer — to relocate a rule `CascadeRoot` already hosts for free.
- **One cascade attribute per property per state** — `Cascade<Background>`, `Cascade<BorderColor>`, … × four states = 20 components and 20 propagation systems per participant, where stock first-override-wins would supply merge with no `bevy_kana` change. Rejected because a part is a **layout index, not an entity**, so the part hop needs field-by-field merge on `Appearance` regardless; the 20-attribute form would express one rule through two different mechanisms.

This phase touches `bevy_kana`. Nothing in `hana_diegetic` consumes the merge yet beyond unit tests.

**Files:**
- `crates/bevy_kana/src/cascade.rs` — `combine` field on `CascadePlugin<A>` (`:258`) with a replace default in `new` (`:265`) and a `with_combine` builder; `CascadeCombine<A>` resource inserted in `Plugin::build` (`:276`) beside `CascadeDefault<A>` (`:237`); fold in `resolve_from_queries` (`:419`) and `resolve_from_world` (`:446`); pointer threading through `propagate_cascade` (`:361`) and `resolve_entity_cascade` (`:332`). **Do not touch `CascadeAttribute` (`:174`), its blanket impl (`:179`), `resolve_cascade` (`:146`), or `resolve_cascade_ref` (`:161`).**
- `crates/hana_diegetic/src/cascade/resolved.rs:175` — defaulted `combine` on `CascadeRoot`.
- `crates/hana_diegetic/src/cascade/mod.rs:44` — `cascade_plugin` passes `A::combine` via `with_combine`.
- `crates/hana_diegetic/src/widgets/appearance.rs:113` — `Appearance::merge_over`, written as a thin owned wrapper over the in-place fold Phase 1 already shipped: `Appearance::layer_onto` (`:172`) and the per-property `VisualChange::layer_onto` (`:49`) implement exactly this rule (lower's `To` wins, otherwise the higher value carries through) and differ only in ownership, so the body is `{ let mut out = higher.clone(); self.layer_onto(&mut out); out }`. **Do not write a third per-property fold** and do not add a `VisualChange::or` — reuse what exists.
- `crates/hana_diegetic/src/widgets/appearance.rs` — the four `Widget*Appearance` types implement `CascadeRoot` with a `combine` delegating to `merge_over`.

**Constraints from prior phases:**
- **Phase 1:** the four wrappers are `Arc<Appearance>` newtypes with hand-written `PartialEq` (`Arc::ptr_eq` then content equality) and per-attribute `size_of` assertions. Every merge allocates a fresh `Arc`, so equality must fall through to content comparison — a merge producing an equal value must still compare equal, or propagation dirties `Resolved<A>` every frame.
- **Phase 7:** `Appearance` now has five `VisualChange` fields — `background`, `border_color`, `border_width`, `material`, `content_color`. `merge_over` covers all five.
- The existing cascade attributes that must keep replace semantics, all declared through `cascade_attribute!` in `src/cascade/resolved.rs`: `TextAlpha` (`:52`), `FontUnit` (`:58`), `HdrTextCoverageBias` (`:63`), `SdfMaterial` (`:112`), `TextMaterial` (`:125`), `ShapeMaterial` (`:138`), `Lighting` (`:149`), `ShadowCasting` (`:152`), `GlyphShadowMode` (`:155`), `Sidedness` (`:159`), `AntiAlias` (`:163`), `HairlineFade` (`:167`), `WidgetInteractivity` (`:170`). **None of them is edited by this phase** — the macro emits no `combine`, so they inherit the replace default.

**Pending decision:** whether an explicitly authored empty bundle suppresses an inherited one, or is indistinguishable from never authoring.

Actual problem:
The plan currently says both. The invariant at the top of this document says silence means "no opinion" and `.disabled(Appearance::new())` is a no-op, and Phase 10's gate says an explicit empty part bundle resolves identically to no part bundle. Phase 1's archived Spec says the opposite — it justifies storing `Cascade` on the grounds that an explicit empty bundle "must suppress an inherited bundle." A delegate implementing this phase's fold from the archived Spec would build suppression; one implementing from the invariant would not.

What exists now:
- Phase 1 stores the distinction: `.hovered(Appearance::new())` is `Cascade::Override`, an un-authored state is `Cascade::Inherit`, both pinned by a test in `layout/builder.rs`.
- Nothing consumes the distinction. Under the fold this phase adds, `Override(Appearance::new())` and `Inherit` produce byte-identical results, and this phase's own gate line asserts exactly that (`Appearance::new().merge_over(&x)` equals `x`).
- **Phase 2 gave the distinction a second consumer.** Part-map admission keys on `WidgetStateCascades::any_overridden()` (`widgets/appearance.rs:323`), so `Override(Appearance::new())` creates a map entry, pinned by a Phase 2 test. Under the no-op reading that entry can only ever resolve to a default override — exactly the wasted resolution the capability mask was added to prevent.

What should change — pick one and make the whole document say it:
- **No-op (matches the invariant and both gates).** An empty bundle contributes nothing at any level. The stored `Override`/`Inherit` distinction stays inert — harmless, but it is not load-bearing and Phase 1's rationale for it should be corrected rather than left to mislead a later delegate.
- **Suppression (matches Phase 1's archived Spec).** An explicit empty bundle clears whatever a higher level authored, giving authors a way to opt a widget out of an inherited look. This needs the fold to distinguish the two cases, a revised invariant, and revised gate lines here and in Phase 10.

Whichever is chosen, this phase must also settle **part-map admission**: it stays override-keyed (required if suppression wins, since an empty bundle must reach resolution to suppress), or it reverts to property-authorship (cheaper under no-op, since an empty entry can never change a pixel). If admission changes, `layout/element.rs:1356` `record_owned_widget_element` joins this phase's **Files** and Phase 2's admission test is updated with it.

Recommendation:
Take the no-op reading — it is what the invariant, this phase's gate, and Phase 10's gate already specify, so only Phase 1's archived rationale is out of step. Record the correction as a note beneath Phase 1's Retrospective rather than editing the archived Work Order. If suppression is wanted later, it is a clean additive feature (an explicit "clear" value distinct from an empty bundle) rather than a reinterpretation of empty. Keep admission override-keyed regardless: the entry is rare, the capability mask already prevents the expensive part of the waste, and reverting admission would destroy the distinction a later suppression feature needs.

Approve this direction, or modify it?

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check bevy_kana` and `… check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test bevy_kana` and `… test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint bevy_kana` and `… lint hana_diegetic`
- **`CascadeAttribute` and its blanket impl are byte-identical to `HEAD`**, and no attribute type gains a hand-written `impl CascadeAttribute`.
- A test asserts **every existing cascade attribute still resolves first-override-wins** across a global → panel → widget chain after the fold lands, including the no-override case that returns the `CascadeDefault` root.
- A cycle, a missing `CascadeFrom`, and depth-limit exhaustion each still yield the same value as today for a replace attribute.
- `merge_over` unit tests cover all five properties for the four combinations of (higher names it / does not) × (lower names it / does not).
- `Appearance::new().merge_over(&x)` equals `x` — an empty bundle is a no-op, not a clear.
- A three-level merge test: global naming `background`, panel naming `content_color`, widget naming `background` resolves to the widget's background, the panel's content color, and `Unchanged` elsewhere.

### Phase 9 — Register the four cascades and the panel authoring surface · status: todo

#### Work Order

**Goal:** A global default and a panel-level override for each of the four states propagate to widget entities, with the full ownership/teardown lifecycle wired.

**Spec:**

Register four `CascadePlugin` channels over the Phase 1 attribute types and build out the panel authoring surface. Every item below is a **mechanical repetition of the existing `WidgetInteractivity` pattern** — mirror it exactly (`src/widgets/interactivity.rs`, registered via `cascade::cascade_plugin::<WidgetInteractivity>()` at `src/widgets/mod.rs:234`):

- Four `BuilderData` fields, builder methods, component seeds, and `build_panel` assignments (`src/panel/builder.rs:200`).
- Four `seed_panel_value` calls in `seed_panel_overrides` (`src/panel/diegetic_panel.rs:1566`).
- Four `CascadePlugin` registrations in `WidgetsPlugin`, in the `add_plugins` tuple (`src/widgets/mod.rs:233-237`).
- Four typed `override_*` / `inherit_*` command pairs on `CascadeEntityCommandsExt` (`src/cascade/attributes.rs:30`).
- Four `add_cascade_ownership_observers!` entries (`src/panel/lifecycle.rs:122`).
- Four `teardown_owned_shared_state` entries (`src/panel/lifecycle.rs:775`).
- Four assignments in `replace_from_precompose_helper` (`src/panel/diegetic_panel.rs:451`).
- Four **empty-`Appearance` `CascadeDefault` resources** — not `PanelDefaults`.

**Placement:** registration lives in `WidgetsPlugin`; panel ownership observers and construction seeding stay in `HeadlessLayoutPlugin` (`src/panel/mod.rs:194`), matching the current division. `HeadlessLayoutPlugin` registers the attribute cascades explicitly because `RenderPlugin` is absent, so the four new cascades must be registered there too or every headless test loses them.

**Beyond the checklist:**
- Command documentation matching the existing `WidgetInteractivity` **durability boundary**: a command applied directly to a derived widget entity may be replaced by reification, so durable edits belong in the panel's authored tree.

The four attribute types are **already exported** from the crate root — Phase 1 shipped them (`src/lib.rs:385`, `:390`, `:391`, `:401`). This phase's new public surface is the panel-builder methods and the eight commands, and only those need export work.

**Public names** (final, all checked against the forbidden-words list):

| Surface | Name |
|---|---|
| Cascade attributes | `WidgetHoveredAppearance`, `WidgetPressedAppearance`, `WidgetFocusedAppearance`, `WidgetDisabledAppearance` |
| Panel builder methods | `widget_hovered_appearance`, `widget_pressed_appearance`, `widget_focused_appearance`, `widget_disabled_appearance` |
| ECS commands | `override_widget_*_appearance` / `inherit_widget_*_appearance` |

The `widget_` prefix follows the existing `WidgetInteractivity` / `override_widget_interactivity` vocabulary and keeps an ancestor call site from reading as though the panel itself enters the state.

Presentation does not read `Resolved<…>` yet — that is Phase 10.

**Files:**
- `src/widgets/mod.rs:233-237` — four `CascadePlugin` registrations in the `add_plugins` tuple.
- `src/panel/mod.rs:194` — four registrations in `HeadlessLayoutPlugin`.
- `src/panel/builder.rs:200` — four `BuilderData` fields + builder methods + seeds + `build_panel` assignments.
- `src/panel/diegetic_panel.rs` — four `seed_panel_value` calls (`:1566`), four `replace_from_precompose_helper` assignments (`:451`).
- `src/panel/lifecycle.rs` — four ownership-observer entries (`:122`), four teardown entries (`:775`).
- `src/cascade/attributes.rs:30` — four typed command pairs, with durability documentation.
- `src/cascade/defaults.rs` — four empty-`Appearance` `CascadeDefault` resources.
- `src/lib.rs:339-403` — crate-root exports for the panel-builder methods and commands only; the four attribute types are already exported (`:385`, `:390`, `:391`, `:401`).

**Constraints from prior phases:**
- **Phase 1:** the four attribute types already exist as `Arc<Appearance>` newtypes with `Reflect`, hand-written `PartialEq`, and per-attribute size assertions — they satisfy `CascadeAttribute`'s bounds as-is, and they are already re-exported from the crate root.
- **Phase 2:** every widget entity already carries all four `Cascade<Widget*Appearance>` components, `Cascade::Inherit` included (`reify.rs` `spawn_widget` `:296`, synchronized per channel by `update_widget_appearance` `:482`). That is precisely what `propagate_cascade` (`bevy_kana/src/cascade.rs:361-400`) needs in order not to strip `Resolved<A>`, so these registrations work on existing entities with no reify change.
- **Phase 8:** `CascadeRoot` (`src/cascade/resolved.rs:175`) carries a defaulted `combine` that replaces; the four appearance attributes override it with `Appearance::merge_over`. `cascade_plugin::<A>()` (`src/cascade/mod.rs:44`) already forwards `A::combine` to `CascadePlugin::with_combine`, so these four registrations merge per property with no extra wiring at the call site — register them exactly like `WidgetInteractivity`. `CascadeAttribute` is unchanged.
- **Invariant:** `missing_docs = "deny"` — the four attribute types, four builder methods, and eight commands all need doc comments.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase adds the public panel authoring surface and the runtime override/inherit commands, so both doc commands must pass before checkpoint.
- A global `CascadeDefault` reaches every widget entity's `Resolved<…>` with no per-widget authoring.
- **Panel beats global** for each of the four states, asserted on `Resolved<…>`.
- Level-to-level merge holds: a panel bundle naming only `border_color` against a global default of `background` + `content_color` resolves to all three.
- Lifecycle tests cover a pre-existing application-owned `Cascade`/`Resolved` pair, precompose replacement, role removal, and role re-addition.
- Runtime `override_widget_*_appearance` / `inherit_widget_*_appearance` commands change the resolved value and restore inheritance.

### Phase 10 — Two-stage resolution and reification · status: todo

#### Work Order

**Goal:** A resolved bundle reaches every element a widget owns, merged per property across global → panel → widget → part, with state layering applied only afterward.

**Spec:**

Resolution is **two-stage**, because `Cascade<T>` and `Resolved<T>` are per-entity components while parts are layout indices on one widget entity — a single `Resolved<T>` cannot carry a distinct value per part, and spawning an entity per part would add roughly eight entities, their relationships, and eight cascade components each per slider.

1. `CascadePlugin` resolves **global → panel → widget** on the widget entity, over the four attribute types (already wired in Phase 9).
2. Presentation resolves **part against widget** by reference: each sparse map entry is a part-local `Cascade<…>` resolved against the widget's `Resolved<…>`, through **one typed helper in `src/cascade/`** rather than precedence spelled out in each presenter.

**Then, and only then,** layer the active states in `LAYER_ORDER` (`widgets/appearance.rs:400`, `[Focused, Hovered, Pressed, Disabled]`) and build the record override. The two axes must not be interleaved.

**This phase needs two state views, not one.** `WidgetStateCascades<'a>` (`widgets/appearance.rs:299`) holds `&'a Cascade<Widget*Appearance>` and its `layer` (`:330`) reads through `Cascade::as_override()`. Presentation here reads `Resolved<Widget*Appearance>`, which derefs to the attribute itself and is never a `Cascade` — so the resolved path needs its own view over four `&Appearance` (or four `&Widget*Appearance`). The authored view must stay: build-time validation still calls `any` through `StateAppearance::cascades()` (`:293`, from `element.rs:1311` and `:1368`). Factor the shared `LAYER_ORDER` fold so both views call one implementation rather than duplicating `layer`/`resolve`.

Both hops use `Appearance::merge_over` from Phase 8. For one element in one state:

1. Cascade resolves the widget's bundle down the levels (global → panel → widget).
2. For each property: the part's value if the part names it, else the widget's resolved value, else the ordinary look.
3. Record-specific render routes consume only the properties they can present; the rest are **dormant** at that element.

**Reification.** Widgets already receive `CascadeFrom::new(panel)` on spawn (`bevy_kana/src/cascade.rs:197`) and `update_widget` (`reify.rs:352`) repairs a wrong relationship. The existing order is cycle-free: `CascadeSet::Propagate → PanelSystems::ComputeLayout → WidgetSystems::Reify → ReifyCommandsApplied → presentation`, with `ReifyCommandsApplied` flushing both the widget insertions and the `resolve_inserted_cascade` observer (`bevy_kana/src/cascade.rs:339`) that seeds `Resolved<A>` — the existing `disabled_widget_is_marked_in_its_reification_frame` test already proves same-frame behavior for `WidgetInteractivity`.

This phase must additionally:
- Keep `CascadeFrom::new(panel)` in the same deferred insertion.
- Order presentation after **both** `CascadeSet::Propagate` and `WidgetSystems::ReifyCommandsApplied` — the set declarations are `widgets/mod.rs:143`, the `configure_sets` call is `:238-267`, and the presenters are added at `:299-313`.
- Add the four `Changed<Resolved<…>>` filters and the part map to presentation's dirty inputs.
- **Resolve the removal question explicitly:** either declare that a live widget never loses its four `Resolved` caches and omit their removal streams, or specify how a missing cache clears the prior override. A query requiring all four caches cannot process their removal.

**Documentation.** Update `docs/hana_diegetic/widgets-deferred.md` in this phase: replace "direct widget-builder inputs" with global/panel/widget/part appearance authoring; remove global-versus-instance placement and state-dependent child addressing from the open questions; keep presets, named variants, later widget states, extended materials, animations, slider geometry, and tooltip reuse deferred. Its stale current-plan link is already fixed — it now points at `as-built/widgets.md`.

**Files:**
- `src/cascade/attributes.rs` — the typed part-against-widget resolution helper.
- `src/widgets/mod.rs:238-267` and `:299-313` — presentation ordering after `Propagate` and `ReifyCommandsApplied`.
- `src/widgets/button.rs`, `src/widgets/slider.rs:1194`, `src/widgets/editable.rs` — stage-2 resolution via the helper; four `Changed<Resolved<…>>` dirty inputs.
- `src/widgets/appearance.rs:293-383` — add the resolved-side state view alongside the authored `WidgetStateCascades<'a>` (`:299`) and share one `LAYER_ORDER` fold between them. `resolve` (`:367`) composes the merged bundles in `LAYER_ORDER` after level resolution, not during it. **This is a smaller change than it sounds:** `resolve` already layers in `LAYER_ORDER` against an `Appearance::default()` accumulator and already keeps the two axes separate. What changes is only where each layer comes from — the resolved bundles passed in, instead of `layer(state)` (`:330`) reading this record's own `Cascade`s. Do not rewrite the layering algorithm.
- `docs/hana_diegetic/widgets-deferred.md` — the four documentation edits above.

**Constraints from prior phases:**
- **Phase 9:** the four `CascadePlugin` channels, `CascadeDefault` resources, panel builder methods, and typed commands all exist; `Resolved<Widget*Appearance>` is present on every widget entity. Registration is in `WidgetsPlugin`; observers and seeding are in `HeadlessLayoutPlugin`.
- **Phase 8:** `Appearance::merge_over(&self, higher)` is the single merge used at both hops; `CascadeRoot::combine` already makes stage 1 merge per property, and the `CascadeDefault` root participates in that merge rather than acting as a fallback. Stage 2 calls `merge_over` directly — it is not a cascade hop and needs no `combine`.
- **Phase 2:** the sparse part map is sorted by element index, capability-masked, revision-scoped, and stored **separately** from the four root `Cascade` values — the root's bundle is the widget's own override and must not be applied a second time as a part override. Phase 2 also landed the entity shape this phase resolves through: `StateAppearance` is not a `Component`, `spawn_widget` inserts all four `Cascade<Widget*Appearance>` channels including `Cascade::Inherit`, `update_widget` synchronizes them per channel, and `WidgetStateCascades<'_>` is the borrowed view the presenters already use.
- **Phase 3:** presenters already merge-walk recipients and already own their `Changed`/`RemovedComponents` drains; this phase adds four more `Changed` filters to the drains they own, not to a run condition.
- **Phase 5:** part-local authoring is validated per property at build. Higher-level properties with no compatible recipient are **dormant**, not errors — do not route them through `validated_element_appearance`.
- **Phase 7:** `content_color` is the fifth property and the merge covers it.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic headless_widgets`
- **Merge matrix, table-driven.** For each of the five properties, all four widget × part combinations, asserting the resolved value at the part:

  | widget names it | part names it | resolved at part |
  |---|---|---|
  | no | no | ordinary look |
  | yes | no | widget's value |
  | no | yes | part's value |
  | yes | yes | part's value |

  Run the matrix for all four states and at every hop: global→panel, panel→widget, widget→part.
- A part hovered bundle carrying **only** a border color keeps the widget's inherited hovered background and replaces only the border.
- A part naming the **ordinary value** for a property holds that property against a widget bundle.
- An **explicit empty part bundle resolves identically to no part bundle** — its own named test, since this is the behavior change from whole-bundle replacement.
- **Dormancy:** a widget bundle naming `border_color` against a text-only label leaves the label unchanged, produces no error, and creates no `VisualOverrideIndex` entry for it. One test per property × incompatible-recipient pair.
- A test sources focused, hovered, pressed, and disabled from **four different levels**, including an explicit empty part bundle, and asserts `LAYER_ORDER` still governs.
- Runtime global-default and panel-override mutations repaint live buttons, sliders, and editable fields while widget state is unchanged; editable tests confirm pressed appearance never applies.
- A **first-update test** covers global, panel, widget-root, and part inheritance in the reification frame.
- `docs/hana_diegetic/widgets-deferred.md` carries none of the four stale statements.

### Phase 11 — Remove `Slider::disabled_color` and the subtree channel · status: todo

#### Work Order

**Goal:** The blunt subtree-recolor path is gone, `VisualSlotOverride` is back to 144 bytes, and the slider's focus border composes correctly against a cascaded disabled bundle.

**Spec:**

Delete `Slider::disabled_color` — the field (`widgets/slider.rs:172`), its constructor default (`:191`), its builder method (`:233`), its crate-internal setter (`:255`), its `El` forward, and its test `disabled_color_recolors_every_slider_element_and_suppresses_focus_border` (`:5267`). Delete `WidgetVisualOverrides::subtree_color` (`widgets/visual.rs:256`), `set_subtree_color` (`:262`), the getter (`:267`), the seeding loop in `slider.rs:1242`, and its consumption in `dispatch_visual_overrides` (`visual.rs:492`).

With its only production producer gone, delete `VisualSlotOverride::color` (`visual.rs:170`), its overlay logic, and the `with_color` test helper; move the text, image, and draw-primitive consumers to `content_color`. Delete the `HashMap<usize, VisualSlotOverride>` that fed subtree seeding (`visual.rs:491`) only if Phase 3's element channel did not take it over — check which before removing. `VisualSlotOverride` returns from 160 to 144 bytes.

**Focus-border composition.** The thumb focus border cannot be suppressed by "a resolved disabled bundle exists" — under a cascade every state always resolves to something, so presence is always true, and a disabled bundle changing only a background would delete the focus border. Compose `Slider::focused_thumb_border_color` as a **focused-thumb layer before normal state composition**:
- a disabled `border_color: To(…)` **replaces** it,
- a disabled `border_color: Unchanged` **preserves** it,
- an element overlay without `offset` leaves the thumb translation alone.

Convert `examples/widgets.rs`'s slider (`add_slider` `:1200`, `.disabled_color` use `:1162`) to author its parts explicitly.

**Material churn contract.** Per-element authoring lets one hover transition swap materials on label, track, and thumb together. A compatibility-preserving swap updates material-table rows in place; an incompatible one removes and re-inserts records across batches (`render/fill_batch.rs:1359`, `render/batch_store.rs:201`), rebuilds text runs (`render/panel_text/batching.rs:435`, `render/analytic_paths/batching.rs:314`), despawns empty batches, and allocates entity, mesh, material, and storage buffers for new ones. Incompatible materials stay **permitted**, but this phase must:
- document compatibility-preserving swaps as the steady-state path,
- keep built-in defaults and examples compatibility-preserving,
- add a label/track/thumb transition test asserting **no batch-key move and no batch entity creation** for compatible materials,
- add one incompatible case asserting **only the affected members migrate**.

**Files:**
- `src/widgets/slider.rs` — delete `disabled_color` (`:171`, `:190`, `:232`) and the subtree seeding loop (`:1232`); rework focus-border composition in `present_slider_state` (`:1190`); delete the `:5250` test.
- `src/widgets/visual.rs` — delete `subtree_color` (`:235`, `:241`, `:246`), `VisualSlotOverride::color` (`:147`) and its overlay logic, and the seeding consumption in `dispatch_visual_overrides` (`:471`); delete `with_color`.
- `src/render/panel_text/batching.rs`, `src/render/panel_shapes/batching.rs`, `src/render/analytic_paths/batching.rs`, `src/render/fill_batch.rs:1359` — move remaining `color` consumers to `content_color`.
- `src/layout/builder.rs` — remove the `disabled_color` forward.
- `examples/widgets.rs:1162`, `:1200` — author slider parts explicitly.

**Constraints from prior phases:**
- **Phase 7:** `content_color` exists on `Appearance` and `VisualSlotOverride` and is consumed by the text, image, and `PanelDraw` routes. Both `color` and `content_color` have been alive simultaneously since Phase 7; this phase removes `color`.
- **Phase 10:** every state always resolves to something under the cascade, which is exactly why "a disabled bundle exists" cannot gate the focus border. The resolved override reaching the thumb is an element override composed on top of the authored slot baseline (Phase 3), so the presentation-owned `offset` is already preserved unconditionally.
- **Phase 4:** the slider's track, thumb, and label can carry their own bundles as `El<L, WidgetPart<Slider>>`, which is what the example migration uses.
- **Phase 1:** a state verb **replaces** the whole bundle for its state — a second `hovered(…)` on the same element discards what the first authored. The example migration below authors several properties per state per part; each state must be built as one `Appearance` and passed in a single call, never as chained calls that each name one property. That chained form worked before Phase 1 and silently drops all but the last bundle now.
- **Phase 2:** structural containers are excluded from the recipient list, so the example's resolved overrides cover exactly root, track, thumb, and label.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase removes public API and rewrites doc examples that referenced it, so both doc commands must pass before checkpoint.
- `rg -n 'subtree_color|disabled_color|with_color' crates/hana_diegetic` returns nothing.
- `VisualSlotOverride` is back to 144 bytes, asserted by the `size_of` assertion Phase 7 introduced — this phase lowers its number rather than adding the first one.
- A **focused × disabled × dragging matrix** is tested for both a background-only disabled bundle (focus border survives) and a border-authoring one (focus border replaced), asserting the thumb `offset` is unchanged in every case and that disabled remains the last normal layer. The matrix includes the pressed/dragging state and the frame that queues `SliderDrag` removal.
- The example's final resolved overrides for root, track, thumb, and label are asserted **exactly** — the headless harness produces no pixels, so visual equality is not a gate.
- Material churn: a compatible label/track/thumb transition causes no batch-key move and no batch entity creation; an incompatible one migrates only the affected retained members.

## Outstanding items

<!-- Project state outside the phase spine. Not dispatched by /plan:delegate. -->

- **Uncommitted work.** Three rounds sat uncommitted on `feature/widgets` at `2f12a56d` — the `apply_state_appearance` / `_with` renames, the editable-field state fix (hover and disabled present on fields; `pressed_*` gated behind `HasPressedState`) with four new tests and a trybuild case, and the `HasPressedState` doc comment. These landed as `64f8bdc0`, which is current `HEAD`.
- **`docs/hana_diegetic/widgets.md`** — done. Rewritten as `docs/hana_diegetic/as-built/widgets.md`, current-state only (state appearance described as the four `Appearance` verbs, not the removed flat builders), and the old phased plan deleted. Inbound links in `surface-panels.md` and `widgets-deferred.md` repointed.
- **Widget demonstration checkpoint.** The retired widget plan ended with an undelivered discussion phase: decide with the owner how to demonstrate the whole widget system working together — buttons, sliders, tooltips, focus traversal, disabled state, panel ordering, and IME/text input coexisting on one panel — and name both the live demonstration and the deterministic integration gate, including the tooltip's final retained transform after first reveal and after a replacement creates a fresh controller. `examples/widgets.rs` is the cumulative baseline; do not reopen which example owns that path, remove either input-integration proof, replace the diagnostic rows, or change the established picking policies.
- **`cargo mend`.** Never run on this branch; it is the first step of the `/clippy` workflow.
- **`WidgetElement<ImeEditableFieldSpec>`** — settled by Phase 4's `EditableField` marker.
- **`HasPressedState`** — name accepted as-is.
