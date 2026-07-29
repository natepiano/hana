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
  - `src/layout/builder.rs` (1944 lines) — `El<L, Role>`; roles `WidgetPart` (`:105`), `PressedPart` (`:109`), sealed `ElementRole` (`:112`); owner kinds `WidgetOwner` (`:127`), `Widget` (`:143`), `Pressable` (`:187`); `El::editable_field` (`:814`); the four state verbs in four blocks — `El<L, LayoutOnly>` (`:740`-`:765`, `pressed` yields `PressedPart`), `El<L, WidgetElement<W>>` (`:853`-`:873` plus `pressed` at `:888` under `Pressable`), `El<L, WidgetPart>` (`:898`-`:923`, `pressed` upgrades the role), `El<L, PressedPart>` (`:933`-`:958`, all role-preserving); `El::disabled_color` (`:1049`); `WidgetBuilder<'a, W>` (`:1240`); `AcceptsElement` (`:1295`) and its five impls (`:1582`-`:1658`); `LayoutContentBuilder` (`:1314`); `LayoutBuilder::with_root` (`:1455`), `with_widget_root` (`:1467`), `with` (`:1460`); `WidgetBuilder::with` (`:1685`); `Text::layout` (`:265`).
  - `src/layout/element.rs` — `CommonEl`/`Element`, `appearance` field (`:148`); `LayoutTree::validate_widgets` (`:765`, walk body `:776-829`), the **only** appearance-reachable walk that returns `Result<_, PanelBuildError>`, calling `validated_element_widget_owner`; `computed_widget_records` (`:830`, returns `Vec<ComputedWidgetRecord>` — **no `Result`**) and its owning-record walk (`:895`) calling `record_owned_widget_element` (`:1309`) and `element_visual_capabilities` (`:1285`); `set_field_editing_content` (`:1014`); `validated_element_widget_owner` (`:1255`); `classify_element_change`'s exhaustive `Element` destructure (`:1327`); `set_element_state_appearance` (`:461`, `#[cfg(test)]`). **`PanelBuildError::WidgetContainsInteractiveDescendant` is gone** — Phase 4 removed the variant, its producer, and its two tests; nesting is now a compile error. **The four `PanelBuildError::State*` variants and `validated_element_appearance` are gone** — Phase 5 replaced them with `CommonEl::default_state_surfaces` (`layout/builder.rs`), which emits a transparent fill or border at element construction so a state property always has a record to replace.
  - `src/layout/draw.rs:11` — `PanelDraw`. `src/layout/line.rs:42` — `PanelShape` enum; `PanelCircle` struct at `:64`.
  - `src/ime/editor.rs` (1968 lines) — `inline_editor_content_tree` **definition at `:1132`** (the earlier `:665` / later `:1184` sites are callers/helpers, not the def).
  - `src/widgets/appearance.rs` — `VisualChange<T>` (`:26`); `Appearance` (`:98`, derives `PartialEq`) with its impl block (`:109`); the four `Widget*Appearance` wrappers with size assertions at `:179`/`:199`/`:219`/`:239`; `StateAppearance` (`:249`, **not a `Component`**); `WidgetStateCascades<'a>` (`:264`) with `any_overridden` (`:288`), `layer` (`:295`), `any` (`:317`), `resolve` (`:332`); `WidgetState` (`:368`), `LAYER_ORDER` (`:388`). **`layer`/`any`/`resolve` live on `WidgetStateCascades`, not on `StateAppearance`.** **Phase 3 DELETED both `layer_onto` methods** — per-property layering is now inlined in `resolve`, which matches `VisualChange::To` per property inside the `LAYER_ORDER` loop (`:335`) and constructs the `VisualSlotOverride` directly.
  - `src/widgets/visual.rs` — `VisualSlotOverride` (`:169`) with the generic `color` field (`:171`), `fill_color` (`:173`), `border_color` (`:175`); **`apply` (`:195`) and `apply_element` (`:209`)**, which enumerate every field explicitly and are the only path composing an element override over a slot baseline — any new `VisualSlotOverride` field must be added to both. `WidgetVisualSlots` (`:82`) with `with_elements` (`:99`) / `with_part_appearances` (`:108`) / `elements()` (`:120`) / `part_appearances()` (`:124`, **no longer `#[cfg(test)]`**). `WidgetVisualOverrides` (`:264`), `subtree_color` field (`:265`) / `set_subtree_color` (`:272`) / getter (`:277`), **`set_element` (`:320`)** and **`element_overrides` (`:338`)** — the Phase 3 element-index-keyed channel. `write_widget_overrides` (`:400`) replaces the whole component and **compares immutably first, returning without writing when the resolved value is unchanged**. `VisualOverrideIndex` (`:434`), `dispatch_visual_overrides` (`:506`), the subtree seeding read (`:513`). **`write_slot_override` was DELETED in Phase 3** — all writes go through `write_widget_overrides`.
  - **The three presenters (rewritten in Phase 3).** `presentation_inputs_changed` was **DELETED** in all three; each presenter now builds its own kind-filtered dirty-entity set from `Changed`/`RemovedComponents` terms it owns directly, and writes the whole component via `write_widget_overrides`. `src/widgets/button.rs` — `present_button_state` (`:139`), `Changed<WidgetVisualSlots>` dirty term (`:149`), write (`:236`). `src/widgets/editable.rs` — `present_editable_state` (`:30`), `Changed<WidgetVisualSlots>` (`:40`), write (`:122`). `src/widgets/slider.rs` — `present_slider_state` (`:1141`), `Changed<WidgetVisualSlots>` (`:1152`), subtree seeding (`:1228`), write (`:1277`); `disabled_color` field `:172` / default `:191` / builder `:233` / crate-internal setter `:255`. **`ButtonPress` is an `Or<>` term in the button presenter but not the slider's** — inserting/removing it on a slider wakes exactly one presenter, the cross-kind isolation discriminator.
  - `src/widgets/id.rs` — `WidgetKind` (`:98`), `VisualElementCapabilities` bitflags (`:115`, one `CONTENT` bit covering text **and** image **and** non-empty `PanelDraw` together), `ComputedWidgetRecord` (`:138`) with `appearance` field (`:143`) and `part_appearances` (`:144`), `appearance()` (`:188`), `push_visual_element` (`:208`), `part_appearances()` (`:220`), `push_part_appearance` (`:222`). The `CONTENT` bit is at `:123`.
  - `src/widgets/reify.rs` — `reify_widgets` (`:184`, gated on `Changed<ComputedDiegeticPanel>` at `:194`), its existing-widget query (`:196-211`), `spawn_widget` (`:296`), `update_widget` (`:352`) with the `WidgetVisualSlots` inequality guard (`:445`), `update_widget_appearance` (`:482`).
  - `src/widgets/mod.rs` — `WidgetSystems` enum (`:143`), ordering `Reify → ReifyCommandsApplied → ResolveInteractivity → InteractivityCommandsApplied → Focus → SemanticInput → FocusCommandsApplied → PresentationCommandsApplied`; `WidgetsPlugin` (`impl Plugin` `:223`) with `add_plugins` (`:233-237`) including `cascade::cascade_plugin::<WidgetInteractivity>()` (`:234`), `configure_sets` (`:238-267`), `add_systems` where the three presenters are registered — `present_button_state` (`:299`), `present_editable_state` (`:302`), `present_slider_state` (`:305`) — **with no `.run_if(...)` on any of them**, since Phase 3 moved the change detection into the systems themselves; `mod appearance;` stays **private** (`:1`) — the public surface comes from the `pub use appearance::…` re-exports, so no phase needs `pub mod` here.
  - `src/cascade/mod.rs:44` — `cascade_plugin<A: CascadeRoot>()`.
  - `src/widgets/interactivity.rs` (529 lines) — `Cascade<WidgetInteractivity>`, the pattern every cascade step mirrors.
  - `src/cascade/attributes.rs` (353 lines) — `CascadeEntityCommandsExt` (`:30`), `resolved_*` fns (`:223-322`), `apply_cascade_override` (`:326`), `remove_cascade_override` (`:336`), `resolved_cascade` (`:345`). `src/cascade/constants.rs:7` — `CASCADE_ATTRIBUTE_BYTES: usize = 32`. `src/cascade/resolved.rs` (177 lines) — `cascade_attribute!` (`:20`), `SdfMaterial`/`TextMaterial`/`ShapeMaterial` (`:112`/`:125`/`:138`) with their per-attribute size assertions at `:118`/`:131`/`:144`, `CascadeRoot` (`:175`).
  - `crates/bevy_kana/src/cascade.rs` (676 lines) — `Cascade<T>` (`:23`); `resolve_cascade` (`:146`) and `resolve_cascade_ref` (`:161`), unbounded-generic public helpers with **no `hana_diegetic` call site** (only the `:502` unit test and the `lib.rs:41-42` / `prelude.rs:36-37` re-exports); **`CascadeAttribute` trait def (`:174`) with a blanket impl over its bounds (`:179`) — this is why a per-type method override is impossible**; `CascadeFrom` (`:197`), `CascadeDefault<A>` (`:237`, `#[reflect(Resource)]`), `Resolved<A>` (`:242`), `CascadeSet` (`:252`) with `Propagate` (`:254`), `CascadePlugin<A>` (`:258`) with `new` (`:265`) and `Plugin::build` (`:276`) registering `resolve_inserted_cascade` (`:283`, observer body `:339`), `resolve_entity_cascade` (`:332`), `propagate_cascade` (`:361`, calls the resolver at `:399`), `resolve_from_queries` (`:419`, first-override early return at `:433`), `resolve_from_world` (`:446`).
  - `src/panel/builder.rs` (1271 lines) — `PanelBuildError` (`:45`), `BuilderData` (`:183`). `src/panel/diegetic_panel.rs` (2432 lines) — `replace_from_precompose_helper` (`:451`), `seed_panel_overrides` (`:1566`). `src/panel/lifecycle.rs` (2089 lines) — `PanelCascadeOwnership` (`:122`), `teardown_owned_shared_state` (`:775`). `src/panel/mod.rs` (321 lines) — `HeadlessLayoutPlugin` (`:192`, `impl Plugin` `:194`), which registers the attribute cascades explicitly because `RenderPlugin` is absent.
  - `src/render/fill_batch.rs` (5616 lines) — `apply_sdf_visual_override` (`:1359`), which reads `fill_color.or(color)` and `border_color.or(color)`. `src/render/panel_text/batching.rs` (2888 lines) — cascade-resolution block (`:288`), `apply_routed_text_run_update` (`:435`). `src/render/batch_store.rs` — `BatchStore::upsert` (`:201`). `src/render/analytic_paths/batching.rs` — `TextRunBatch::rebuild` (`:314`).
  - `src/lib.rs` — crate-root `pub use widgets::*` block (`:346-410` after Phase 4's eight new `layout::` exports). Phase 1 added `Appearance` and the four `Widget{Hovered,Pressed,Focused,Disabled}Appearance` wrappers; a later phase adding a public **widget** symbol extends this block. A public **error** type goes with `PanelBuildError` in the `panel::` block at `:238` instead.
  - `examples/widgets.rs` (1691 lines) — `.disabled_color` use (`:1162`), `add_slider` (`:1200`), `apply_state_appearance` (`:1450`).
  - `tests/headless_widgets.rs` (131 lines) — external-client integration test; no state-appearance coverage today.
  - `tests/trybuild.rs` — the driver, and the **only** place a fixture becomes reachable. After Phase 4 it declares **one** test, `widget_state_and_tooltip_typestate_signatures_compile`, with **no `#[ignore]`**, carrying four `compile_fail` globs — the `overlay_*` glob moved into it — plus all three `pass/` fixtures. **A fixture whose filename matches no existing glob is never compiled and its acceptance-gate line is vacuous** — any phase adding fixtures must list `tests/trybuild.rs` in its **Files** and add or widen a glob. `tests/trybuild/pass/` — `tooltip_typestate.rs`, `typestate_helpers.rs`, `widget_state_appearance.rs`. `tests/trybuild/fail/` — **18** fixtures; `editable_widget_has_no_pressed_state.{rs,stderr}` now proves an editable field's *part* rejects a pressed layer: `.rs:15` is the `with` insertion and `.stderr:1` reports `error[E0277]: the trait bound `EditableField: Pressable` is not satisfied`.

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
  - **An accepted option must reach the runtime.** No phase may ship a builder whose value is validated and then discarded; if a combination cannot present, it is gated out of the type surface or it is not offered. *How it is carried, after Phase 5:* for `background`, `border_color`, `border_width`, and `material`, by **record synthesis** — `CommonEl::default_state_surfaces` (`layout/builder.rs`) emits the transparent record the state replaces, so there is nothing to reject and no appearance validation runs at panel build. It is **not** carried for an explicitly empty bundle (Phase 8's open question) and is not yet carried for `content_color`, whose recipients — text, image, `PanelDraw` — cannot be synthesized (Phase 7 must decide). *Scope limit:* this binds **part-local** authoring. A global `CascadeDefault` or a runtime entity command cannot promise a present recipient — a higher-level property with no compatible record at some element is **dormant** there, not an error.
  - **Every level merges into the one above it, property by property.** Global default → panel → widget → part. A level that names a property wins for that property; a level silent on a property takes the value from above; a property nobody names stays at the ordinary look. A global default of `{background: GRAY, content_color: DIM}` plus `.disabled(Appearance::new().border_color(RED))` on one slider resolves to gray, dim, *and* a red border. Silence means "no opinion," not "leave me alone": a level that must hold its ordinary look against an inherited bundle names the ordinary value explicitly, and `.disabled(Appearance::new())` is a no-op rather than a way to clear an inherited look.
  - **Cascade precedence and state precedence are separate axes, resolved in that order.** First resolve each of the four states independently down the levels (global → panel → widget → part). Only then layer the *active* states in `WidgetState::LAYER_ORDER` = `[Focused, Hovered, Pressed, Disabled]`. Composing active states per level and then resolving levels would let a part's local hovered bundle defeat an inherited disabled bundle.
  - **State appearance only exists inside a widget.** Hover, press, focus, and disabled are widget states; there is no text widget and no hoverable bare element. An element that authors a state look is a *widget part*, and a part is only placeable inside a widget's children.
  - **A state layer replaces values on a retained record; it never authors a missing one.** That is a property of `VisualSlotOverride`, not a constraint on authors: since Phase 5, layout supplies the record. A state `background` with no `El::background` gets a `Color::NONE` fill; a state `border_color`/`border_width` with no `El::border` gets `Border::all(Px(0.0), Color::NONE)`; a state `material` gets a fill only when there is no border record to re-key. `.background(X).disabled(Appearance::new().background(Y))` is still not redundant — the ordinary call is what the element shows at rest. **`content_color` (Phase 7) has no synthesizable record** and is the one property this does not cover.
  - **No state property may change solved layout.** Border width changes grow inward and re-key nothing.
  - **Public opaque types, not leaked private ones.** A `pub` trait whose methods mention `pub(crate)` types trips `private_interfaces` even when the methods live on a sealed trait in a private module; E0446 additionally forbids a public trait exposing a private associated type. Every type reachable from a public associated type — `WidgetBuilder`, `WidgetPart`, `EditableField`, the scope token — is a public opaque type with private fields.
  - **Presentation must not dirty `WidgetVisualOverrides` when resolved values are unchanged.** Compare through an immutable query and take `get_mut` only on inequality; comparing inside a method already reached through `Mut<_>` is too late.
  - **Workspace lints, inherited by both packages** (`[lints] workspace = true` in each `Cargo.toml`): `[lints.rust] missing_docs = "deny"` — every new public item needs a doc comment. `[lints.clippy]` denies the `all` / `cargo` / `nursery` / `pedantic` groups (`priority = -1`) plus `allow_attributes_without_reason`, `expect_used`, `panic`, `self_named_module_files`, `unreachable`, `unwrap_used`. No `.unwrap()` / `.expect()` / `panic!` in non-test code, and any `#[allow(...)]` needs a `reason = "…"`.
  - **Headless only.** No phase needs a GPU, a window, or a screenshot. Assertions are on resolved `VisualSlotOverride` values, `VisualOverrideIndex` membership, batch-key identity, and entity counts — never on rendered color. Harnesses: `HeadlessLayoutPlugin` (`panel/mod.rs:194`) for layout / reification / cascade resolution; a plain `App` with no render device for retained batching (precedent: `fill_batch.rs` 59 tests, `panel_text/batching.rs` 33, `panel_shapes/batching.rs` 31, `material_table.rs` 31); `trybuild` for typestate boundaries. Baseline: `verify.sh test hana_diegetic` reports **1130 passed / 2 skipped** at Phase 5 completion (was 1107 at Phase 2). Measure with that command, not by counting the workspace — a phase's gate covers this package only. **No phase may land with a lower test count than it inherited.**

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

### Phase 2 — Per-element appearance storage · status: done (`da0e6544`)

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

### Phase 3 — Element override channel and dirty-entity presentation · status: done (`3d21f5bd`)

#### Work Order

**Goal:** Resolved overrides reach any owned element, all three presenters resolve every recipient, and each presenter wakes only for the widgets that changed.

**Spec:**

`WidgetVisualOverrides` (`widgets/visual.rs:255`) gains an **element-index-keyed channel** alongside the slot-keyed one, merged in `dispatch_visual_overrides` (`:463`) into the map already built at `:491`. Store element overrides sorted so dispatch merges them with slot overlays into that existing map rather than allocating a second one.

**Slot-versus-element precedence is fixed here:** presentation-owned computed slot values (the slider thumb's `offset`) are preserved unconditionally; the resolved element override composes on top of the authored slot baseline.

All three presentation systems (`button.rs`, `slider.rs` `present_slider_state` `:1194`, `editable.rs`) resolve **every recipient** rather than the root slot alone, by merge-walking Phase 2's sparse authored list against the ordered recipient list — `O(recipients + authored)`, no linear `find` per element.

**All three must write through `write_widget_overrides` (`visual.rs:314`), building a complete desired set.** `present_slider_state` already does (`slider.rs:1288`); `present_button_state` (`button.rs:241`) and `present_editable_state` (`editable.rs:122`) write a single slot through `write_slot_override` (`visual.rs:348`), which cannot drop an orphaned key. This matters now and did not before: `WidgetVisualOverrides` is slot-keyed today and therefore index-free, so Phase 2 correctly changed nothing in it — this phase's element-index-keyed channel is the **first** index-keyed data on that component and is the first to inherit the renumbering hazard. A per-slot write would strand overrides on element indices that no longer exist.

**`write_slot_override` is deleted in this phase.** `button.rs:241` and `editable.rs:122` are its only two production call sites; both move to `write_widget_overrides`, leaving it unreferenced outside doc comments. `dead_code` is deny-level here, so it cannot be left behind. Remove the function and update the two doc comments that name it (`button.rs:179`, `visual.rs:287`); the `slider.rs:5968` mention is a test comment describing the clear path, which `write_widget_overrides` still covers via its `default()` removal branch.

Each presenter processes **only dirty entities**: the `Changed<…>` queries and kind-filtered `RemovedComponents` drains that today live in the run condition (`slider.rs:1138` `presentation_inputs_changed`) move into the writer, which then uses `Query::get`. **The presenter owns those drains outright** — a run condition that consumes a removal stream before the writer sees it is the failure mode to avoid. Without this, one dragging slider (a drag changes `SliderState` every frame) wakes a system that re-resolves every recipient of every live slider on every drag frame.

**The kind filter must be *added* for the button presenter, not merely moved.** `slider::presentation_inputs_changed` (`slider.rs:1138`) filters `WidgetKind::Slider` and `editable::presentation_inputs_changed` (`editable.rs:29`) filters `WidgetKind::EditableField` on both its changed query and its removal drains, but `button::presentation_inputs_changed` (`button.rs:134`) filters `With<WidgetOf>` alone and drains removals unfiltered — so it currently wakes on every widget kind.

Resolution borrows the highest-precedence authored value per property and clones the winning material handle **exactly once** when constructing `VisualSlotOverride`; it does not clone intermediate `Appearance` layers. Dispatch then clones the finished override into `VisualOverrideIndex` (`visual.rs:413`) — one further handle clone, unavoidable.

`dispatch_visual_overrides` already builds a `HashMap<usize, VisualSlotOverride>` (`visual.rs:491`) — the one Phase 11 deletes along with `subtree_color`. The element channel **merges into that existing map**; do not introduce a second one.

At this point only the widget's own element can author a bundle, so nothing changes on screen.

**Files:**
- `src/widgets/visual.rs` — element-index-keyed channel on `WidgetVisualOverrides` (`:255`); merge + precedence in `dispatch_visual_overrides` (`:463`) into the existing map at `:491`; un-gate `part_appearances()` (`:119`, currently `#[cfg(test)]`); delete `write_slot_override` (`:348`) and its mention in the `:287` doc comment.
- `src/widgets/button.rs` (`:134`, `:179`, `:241`), `src/widgets/slider.rs` (`:1138`, `:1194`, `:1288`), `src/widgets/editable.rs` (`:29`, `:122`) — merge-walk every recipient; move `Changed<…>` / `RemovedComponents` from run conditions into the writers; route all three through `write_widget_overrides`; add the kind filter to button.
- `src/widgets/mod.rs:299-313` — the three `.run_if(...)` attachments this phase removes live at `:300` (button), `:304` (editable), `:308` (slider).

**Constraints from prior phases:**
- **Phase 1:** four `Cascade<Widget*Appearance>` fields on `StateAppearance`; `Appearance` public with `background` / `border_color` / `border_width` / `material`; `resolve` still resolves against `Appearance::default()`.
- **Phase 2:** `ComputedWidgetRecord` (`id.rs:138`) carries a sorted sparse `part_appearances` (`:144`, read via `:216`) **plus separately** the four root `Cascade` values in `appearance` (`:143`, read via `:188`); each recipient index in `visual_elements` carries a `VisualElementCapabilities` mask (`:115`) and pure structural containers are excluded. The map is re-derived and replaced wholesale on every computed-panel update. Merge-walk against this ordering — do not re-sort or build a lookup map.
- **Phase 2 settled the widget entity's shape:** `StateAppearance` is no longer a `Component`. The entity carries four standalone `Cascade<Widget*Appearance>` components, all four always present (`Cascade::Inherit` included), and the three presenters already query them and build a `WidgetStateCascades<'_>` borrowed view to call `resolve` (`appearance.rs:367`). Their run conditions already carry the four `Changed<Cascade<Widget*Appearance>>` terms. Extend that shape — do not reintroduce an aggregate component.
- **Phase 2:** `WidgetVisualSlots::part_appearances()` (`visual.rs:119`) is `#[cfg(test)]` today and **must be un-gated** — this phase's presenters are its first production readers. `LayoutTree::set_element_state_appearance` (`element.rs:461`) **stays `#[cfg(test)]`**: it is the "crate-internal path" this phase's non-root authoring test uses (only `El<L, WidgetElement<W>>` can author until Phase 4, so a test cannot reach a non-root element through the public builder), and a test-only caller set means un-gating it would make it dead in the non-test build under deny-level `dead_code`. `#[cfg(test)]` items are crate-visible in the test build, so a test in `visual.rs` reaches it as-is.
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

### Retrospective

**What worked:**
- The linear merge-walk against Phase 2's sorted sparse `part_appearances` needed no lookup map — advancing the part cursor as each recipient passes visits both lists in `O(recipients + authored)`.
- Moving the `Changed` / `RemovedComponents` terms out of the run conditions and into the writers, with each presenter owning its drains outright, worked across all three presenters without a lost-removal frame.

**What deviated from the plan:**
- `button.rs` carried a *third* in-loop kind guard (`if *kind != WidgetKind::Button { continue; }`) that could never fire: the dirty set is built exclusively from kind-filtered sources — `:185` filters the changed query, `:193` filters the four removal streams via `kinds.get` — and nothing writes between construction and the loop. It was deleted along with its two inputs. Deleting it is what makes those two dirty-set filters testable; `slider.rs::button_press_edges_do_not_rebuild_slider_overrides` is now their real detector.

**Surprises:**
- **Change-tick equality assertions cannot detect presenter isolation.** `write_widget_overrides` (`visual.rs:378`) reads immutably first and returns without writing when the resolved value is unchanged, so tick comparison cannot distinguish "presenter skipped" from "presenter ran, same value" — such an assertion is structurally incapable of failing. The working detector removes `WidgetVisualOverrides` from the peer, drives the other widget, and asserts the component was **not** re-inserted. Pitfall: removing the component does not mark the peer dirty, so it cannot rebuild on its own.
- **`ButtonPress` is the cross-kind discriminator.** It appears in `present_button_state`'s `Or<>` terms but not in `present_slider_state`'s, so inserting or removing it on a slider wakes exactly one presenter. The slider presenter rejects buttons by a different mechanism — `sliders.get` requires `&SliderState`, which filters before presentation rather than during dirty-set construction.

**Implications for remaining phases:**
- Any new authoring surface must keep `WidgetVisualSlots::elements` and `part_appearances` ordered by element index — all three presenters' linear merge-walk depends on that ordering and does not re-sort.
- Phase 11 is unaffected, but its instructions were not: the element channel merges into the existing `HashMap<usize, VisualSlotOverride>` built at `visual.rs:512`, which now serves three producers — subtree seeding (`:513-523`), slot overlays (`:524-532`), and the element channel (`:533-538`). Phase 11 therefore **keeps** the map and deletes only the subtree branch, not the map itself.
- The remove-the-component-and-assert-non-reinsertion pattern is the reusable isolation detector for every later phase that must prove one widget does not wake another's presentation.
- The element channel's per-property composition (`apply_element`) is strong enough that Phase 11's focus-border rework may reduce to a deletion — but only if Phase 10 routes the widget level through that channel rather than the root slot, which Phase 10 did not specify. Deferred there as a pending decision.

### Phase 3 Review

Two architect passes covered phases 4-7 and 8-11 against the shipped code. Twenty-three findings; all applied, none rejected.

**Delegation Context.** Rewrote five bullets — `appearance.rs`, `visual.rs`, the three presenters, `id.rs`, `mod.rs` — against verified line numbers. Records the deletions (`presentation_inputs_changed`, `write_slot_override`, both `layer_onto` methods), that `part_appearances()` is no longer `#[cfg(test)]`, that the presenters carry no `.run_if`, and that `ButtonPress` is an `Or<>` term in the button presenter only, making it the cross-kind isolation discriminator.

**Phase 4.** Named the element-index ordering invariant as a constraint and a gate. Corrected the interactive-descendant guard refs to `layout/element.rs:785`/`:788`. Added the label escape hatch: a text label has `CONTENT` capability but emits no SDF record, so a bundle carrying only the four Phase 1 properties passes the empty-mask check and still presents nothing until Phase 7 — the gate must exercise that explicitly instead of assuming a bare label presents.

**Phase 6.** Shaped the four editor-part authoring inputs as fluent methods rather than `editable_field` parameters, preserving three call sites and the locked trybuild diagnostic. Recorded that `Changed<WidgetVisualSlots>` is already a dirty term in all three presenters, so the regenerated editor tree re-resolves on its own — no wake source or transition observer. Added a gate asserting the four generated parts are emitted in ascending element-index order.

**Phase 7.** Extended its `visual.rs` work to both `apply` and `apply_element`; omitting the second silently drops a `content_color` element override wherever a slot overlay exists on the same element. Rewrote the `appearance.rs` entry: per-property layering now lives inline in `WidgetStateCascades::resolve`, so the fifth property is added there rather than in a deleted `layer_onto`. Moved the disabled-editor-text gate here from Phase 6 — editor text color is unreachable until this phase exists.

**Phase 8.** The `merge_over` instruction named two deleted methods; `merge_over` is now the first per-property fold, and the "do not write a third fold" prohibition has lost its premise. The pending decision on empty bundles stands, but Phase 3 shipped the no-op reading in code (`visual.rs:392`), so suppression now additionally costs deleting that filter and inventing a "clear" token `VisualSlotOverride` does not have.

**Phase 9.** Otherwise clean. Added one gate: propagating an unchanged bundle must not dirty `Resolved<…>`, and Phase 3's presenter-isolation tests must survive — the presenters already carry the four `Changed<Cascade<…>>` terms, so a content-equal `Arc` rewrite would wake all three every frame.

**Phase 10.** Re-scoped onto the seam Phase 3 built: the stage-2 helper already exists as `visual::resolve_part_overrides`, called identically by all three presenters, so the phase extends it instead of adding one in `src/cascade/`. Its no-part-entry skip must be inverted so a widget-level bundle reaches every recipient, which makes `VisualElementCapabilities` load-bearing for the first time — it has no production reader today, and without it the dormancy gate cannot pass. Named the resulting risk: index entries proportional to widgets × recipients. Corrected the claim that `resolve` layers against an `Appearance::default()` accumulator; it accumulates per-property winners and builds the override directly. Restated the empty-part-bundle gate, which passes on the current tree without proving anything.

**Phase 11.** Every `slider.rs` and `visual.rs` line ref was wrong and is corrected. The map deletion is now definite — keep the map, delete only the subtree branch. The `rg` gate was split, because `with_color` reaches ~29 sites across seven files, three of which were missing from **Files**. The focus-border rework largely collapses into Phase 3's per-property composition, conditional on Phase 10's channel decision.

**Deferred to Phase 10 (pending decision):** which override channel carries the widget level — the per-element channel or the root slot. Under the element channel Phase 11's focus-border work becomes a deletion; under the root slot it must be written by hand. Recommended the element channel.

### Phase 4 — Widget parts: the part role and the builder acceptance relation · status: done (`0ac38719`)

#### Work Order

**Goal:** An `El` inside a widget's children can author a state bundle and becomes a widget part; the same call outside any widget fails to compile.

**Spec:**

The four state methods move to `El<L, LayoutOnly>`, transitioning it to a part role. A widget root keeps `WidgetElement<W>`.

**The part roles carry no owner type parameter.** There are two, distinguished only by whether a pressed layer was authored:

```rust
/// An element inside a widget's children that authored a state look.
pub struct WidgetPart;
/// A part that authored a pressed layer; only a pressable owner accepts it.
pub struct PressedPart;

impl<L> El<L, LayoutOnly> {
    pub fn hovered(self, appearance: Appearance)  -> El<L, WidgetPart>;
    pub fn focused(self, appearance: Appearance)  -> El<L, WidgetPart>;
    pub fn disabled(self, appearance: Appearance) -> El<L, WidgetPart>;
    pub fn pressed(self, appearance: Appearance)  -> El<L, PressedPart>;
}

impl<L> El<L, WidgetPart> {
    // hovered / focused / disabled return Self; pressed upgrades the role
    pub fn pressed(self, appearance: Appearance) -> El<L, PressedPart>;
}

impl<L> El<L, PressedPart> {
    // all four return Self — once pressed is authored the role does not decay
}
```

with the same four on `El<L, WidgetElement<W>>` returning `Self`, `pressed` there bounded on `Pressable`.

**An owner type parameter on the part role was rejected.** `El<L, WidgetPart<W>>` would make `W` an *inferred output parameter* of `.disabled()`, so every failure at a non-widget insertion site surfaces as an `E0283` inference ambiguity rather than a legible unsatisfied bound, and every part held in a `let` or returned from a helper would need a turbofish or an annotation. Two monomorphic roles remove that entire class. The owner-specific gate is not lost — it moves to the insertion impls below, where `PressedPart` is accepted only by a `Pressable` owner's builder.

**The gate is an acceptance relation between builder and role, not an associated type on the inserted element's role alone.** `LayoutOnly` must yield the ordinary builder at panel level but a widget-scoped builder beneath a widget, so a role-only mapping would make an ordinary intermediate container silently lose the owner. Both part roles **are** ordinary `ElementRole` implementers — that is harmless once `with` is gated on `AcceptsElement`, because the rejection comes entirely from an absent impl, not from role-trait membership.

```rust
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot accept `{Role}`",
    label = "state appearance requires an element inside a widget's children",
    note = "author `hovered` / `focused` / `disabled` / `pressed` on an element inside the closure of `El::…widget(id, spec)`"
)]
pub trait AcceptsElement<Role: ElementRole>: private::BuilderSealed {
    type ChildBuilder<'a>: LayoutContentBuilder where Self: 'a;
}

impl                   AcceptsElement<LayoutOnly>       for LayoutBuilder        { type ChildBuilder<'a> = LayoutBuilder; }
impl<W: WidgetOwner>   AcceptsElement<WidgetElement<W>> for LayoutBuilder        { type ChildBuilder<'a> = WidgetBuilder<'a, W>; }
impl<W: WidgetOwner>   AcceptsElement<LayoutOnly>       for WidgetBuilder<'_, W> { type ChildBuilder<'a> = WidgetBuilder<'a, W> where Self: 'a; }
impl<W: WidgetOwner>   AcceptsElement<WidgetPart>       for WidgetBuilder<'_, W> { type ChildBuilder<'a> = WidgetBuilder<'a, W> where Self: 'a; }
impl<W: Pressable>     AcceptsElement<PressedPart>      for WidgetBuilder<'_, W> { type ChildBuilder<'a> = WidgetBuilder<'a, W> where Self: 'a; }

// intentionally absent — these omissions are the guarantee
// impl       AcceptsElement<WidgetPart>       for LayoutBuilder
// impl       AcceptsElement<PressedPart>      for LayoutBuilder
// impl<W, V> AcceptsElement<WidgetElement<V>> for WidgetBuilder<'_, W>
```

The implementations are disjoint by nominal role type, so there is no coherence conflict. `WidgetPart` and `PressedPart` are distinct nominal types, so the `WidgetOwner` and `Pressable` bounds on the last two impls do not overlap. The GAT lifetime is the mutable reborrow for one child closure; it binds neither the element, the owner marker, nor the tree. The GAT is load-bearing: the child builder borrows the `&mut self` of the enclosing `with` call, so a non-GAT associated type would have to fabricate a `'static`.

**`with_root` gets a sibling constructor rather than a selector trait.** `LayoutBuilder::with_root` (`builder.rs:1153`) constructs its `LayoutBuilder` locally, so it cannot return a wrapper borrowing that local. Two named constructors do this with no trait, no associated type, and no extra sealed module:

```rust
enum WidgetBuilderStorage<'a> { Owned(LayoutBuilder), Borrowed(&'a mut LayoutBuilder) }

impl LayoutBuilder {
    pub fn with_root<L: ChildLayoutState>(el: El<L, LayoutOnly>) -> Self;
    pub fn with_widget_root<L: ChildLayoutState, W: WidgetOwner>(
        el: El<L, WidgetElement<W>>,
    ) -> WidgetBuilder<'static, W>;
}

impl<W: WidgetOwner> WidgetBuilder<'static, W> { pub fn build(self) -> LayoutTree; }
```

A `RootElementRole` trait with an owned `type Builder` was rejected: it is machinery for one call site, and neither form can conjure a widget scope out of thin air, so no guarantee depends on it. `with_root`'s existing `Role: ElementRole` parameter narrows to `LayoutOnly` — check its call sites and route any widget-rooted one to `with_widget_root`. The `'static` in `with_widget_root`'s return type is the owned-storage marker, not a real requirement on anything the caller holds; document it as such.

`with` (`:1185`), `text` (`:1215`), and `image` (`:1247`) on both builders take `where Self: AcceptsElement<Role>` and pass `&mut <Self as AcceptsElement<Role>>::ChildBuilder<'_>` to the closure. Ordinary-content helpers that must work in either context use one sealed `LayoutContentBuilder` trait that **reuses** `AcceptsElement<LayoutOnly>::ChildBuilder<'_>` rather than declaring a second GAT — two nominal implementers, so the single-implementer style rule does not apply, and a helper's signature changes from `&mut LayoutBuilder` to `&mut impl LayoutContentBuilder`.

**Owner kinds.** `EditableField` is a zero-sized owner marker that must *not* implement `Widget` — it has no pre-built declaration and no root-slot method to give:

```rust
pub trait WidgetOwner: private::WidgetOwnerSealed {}
pub trait Widget: WidgetOwner + private::WidgetSealed { /* existing */ }
pub trait Pressable: Widget {}

impl WidgetOwner for Button {}  impl WidgetOwner for Slider {}  impl WidgetOwner for EditableField {}
impl Pressable for Button {}    impl Pressable for Slider {}
```

**`HasPressedState` is renamed to `Pressable` in this phase.** The shipped trait is at `builder.rs:158` with its two impls at `:160` and `:162`; `El::pressed` (`:830`) is bounded on it. The old name sat in a different grammatical register from its sibling bound `WidgetOwner`, and the two now appear side by side in the `AcceptsElement` impls above. This is a crate-wide rename — trait definition, both impls, every bound, the doc comment, and the `tests/trybuild/fail/*.stderr` fixtures that quote it.

`El::editable_field` (`builder.rs:715`) returns `El<L, WidgetElement<EditableField>>`, and the locked compile-fail message reads `EditableField: Pressable`. This settles the outstanding `WidgetElement<ImeEditableFieldSpec>` item; the existing `tests/trybuild/fail/editable_widget_has_no_pressed_state.stderr` names that old type and must be regenerated.

Because the part role is monomorphic, the `pressed`-on-an-editable-field rejection now fires where the part is **inserted** (`with(...)`) rather than at the `.pressed(...)` call — `WidgetBuilder<'_, EditableField>: AcceptsElement<PressedPart>` requires `EditableField: Pressable`, which does not hold. The message still names the bound the gate demands; only its span moves.

`WidgetOwner` is **kept** rather than dropped: bounding the owner slot at the declaration beats letting `El<Row, WidgetPart<String>>` be a nameable type that fails to construct later, at a worse site.

Nested widgets are syntactically accepted today and rejected at build by `WidgetContainsInteractiveDescendant` (guard at `layout/element.rs:785`, error raised at `:788`); the missing `AcceptsElement<WidgetElement<V>> for WidgetBuilder<'_, W>` impl makes that a **compile error**, so convert the runtime test to compile-fail coverage. Tooltip APIs keep their explicit `LayoutOnly` parameters — add a compile-fail case locking that boundary.

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

**`.slider_thumb()` must be role-preserving.** It appears mid-chain in the shape above, between `.border(…)` and `.disabled(…)`, and the current Work Order never states its signature. It is a part-marking verb, not a role transition: give it an impl on `El<L, LayoutOnly>`, on `El<L, WidgetPart>`, and on `El<L, PressedPart>`, each returning `Self`, so it composes in any order with the four state verbs. Audit any sibling part-marking verbs the same way — a verb reachable only on `LayoutOnly` silently forbids the `.disabled(…).slider_thumb()` ordering.

**Files:**
- `src/layout/builder.rs` — `WidgetPart`, `PressedPart`, `WidgetOwner`, `EditableField`, `AcceptsElement` (with `#[diagnostic::on_unimplemented]`) + its **five** impls, `LayoutContentBuilder`, `WidgetBuilder<'a, W>` with `WidgetBuilderStorage`; rename `HasPressedState` (`:158`, impls `:160`/`:162`) to `Pressable` crate-wide; move the four state methods to `El<L, LayoutOnly>` and add them to `El<L, WidgetPart>` and `El<L, PressedPart>`; make `slider_thumb` and any sibling part-marking verb role-preserving across all three; narrow `with_root` (`:1153`) to `LayoutOnly` and add `with_widget_root`; retarget `with` (`:1185`), `text` (`:1215`), `image` (`:1247`); `editable_field` (`:715`) returns `WidgetElement<EditableField>`. **No `RootElementRole`** — it was cut in favour of the sibling constructor.
- `src/lib.rs:339-403` — export the new public opaque types.
- `src/layout/element.rs:785` — the nested-widget runtime rejection becomes dead; convert its test to compile-fail. The guard is at `:785` and raises `PanelBuildError::WidgetContainsInteractiveDescendant` at `:788`.
- `tests/trybuild/fail/` — new fixtures: part authored outside a widget **written the way an author would actually write it, with no type annotation**, `pressed` on an editable-field part, nested widget, tooltip on a part. Regenerate `editable_widget_has_no_pressed_state.stderr` and every other `.stderr` that quotes `HasPressedState`.
- `tests/trybuild.rs` — **required, not optional.** The driver's globs are the only thing that makes a fixture reachable, and none of them matches the four new fail fixtures above, so without this file every trybuild line in the gate below passes while compiling nothing. Add or widen a glob to cover them. The `pass/typestate_helpers.rs` additions sit behind `typestate_helper_signatures_compile`, which is `#[ignore]` by default — either move them to a non-ignored test or lift the `#[ignore]`, otherwise the compile-pass coverage in the gate is equally vacuous. While here, rename `tooltip_typestate_signatures_compile`: it now also drives `editable_widget_*` and `pass/widget_state_appearance.rs`, so its name no longer describes what it covers.
- `tests/trybuild/pass/typestate_helpers.rs` — helper signatures in both builder contexts.

**Constraints from prior phases:**
- **Phase 1:** the four state verbs are `hovered` / `focused` / `disabled` / `pressed`, each taking `Appearance`; `pressed` is gated on `HasPressedState`. `Appearance` and the four `Widget*Appearance` wrappers are public at the crate root. **Each verb replaces the whole bundle for its state** — a second `hovered(…)` discards what the first authored, unlike the removed per-property builders, which accumulated into one layer. The four verbs added here on `El<L, LayoutOnly>`, `El<L, WidgetPart>`, and `El<L, PressedPart>` must behave the same way and say so in their docs.
- **Phase 2:** `ComputedWidgetRecord` already carries the sparse part map keyed by element index with capability masks, and the root's four `Cascade` values separately. Parts authored here populate that map through the existing ownership walk — no new storage is needed.
- **Phase 3:** presentation already resolves every recipient and writes element-keyed overrides, so a part authored here presents without further presenter changes. **All three presenters merge-walk `WidgetVisualSlots::elements` against `part_appearances` assuming both are ascending by element index, and never re-sort.** The authoring surface added here must emit parts in that order or overrides land on the wrong elements.
- **Phase 2 left a gap this phase must not walk into.** `record_owned_widget_element` (`element.rs:1356`) admits a part appearance on `any_overridden()` alone, with **no capability gate** — while `push_visual_element` skips zero-capability elements. `validated_element_appearance` (`element.rs:1304`) is still reached only from the widget-declaring and editable-field branches (`:1276`, `:1289`). Opening authoring to every `El` inside a widget therefore lets a bundle on a pure structural container compile, store, and never present, breaching the **accepted option must reach the runtime** invariant for the whole interval until Phase 5 lands. This phase closes the window with a whole-bundle rejection (see the gate line below); Phase 5 refines it to per-property with proper error locations.
- **Invariant:** every type reachable from a public associated type is a public opaque type with private fields (`private_interfaces` / E0446).

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic trybuild`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase adds public opaque types and doc examples, so both doc commands must pass before checkpoint.
- A slider's track and thumb each author their own state look and present it. The label is covered only once it authors an ordinary `El::new().background(Color::NONE)` first: a text label has `CONTENT` capability but emits no SDF record, so a bundle carrying only the four Phase 1 properties passes the empty-mask check yet presents nothing until Phase 7 adds `content_color`. The gate must exercise that escape hatch explicitly rather than assuming a bare label presents.
- The `part_appearances` list this phase emits is in ascending element-index order.
- An element authoring a state look **outside any widget fails to compile**. Because `WidgetPart` is monomorphic there is nothing to infer, so the fixture must be written **unannotated** — the literal shape an author mistypes, `builder.with(El::new().background(X).disabled(…), |_| {})` at panel level — and its committed `.stderr` must be a stable `E0277` on `LayoutBuilder: AcceptsElement<WidgetPart>` carrying the `on_unimplemented` message. An annotated fixture does not discharge this line: it would prove a diagnostic no author ever sees. If the raw message is not legible, tune the `on_unimplemented` attribute until it is.
- A `pressed` bundle on a part of an editable field fails to compile, with the message naming `EditableField: Pressable`. The error is expected at the `with(...)` insertion, not at `.pressed(...)`.
- No `.stderr` fixture anywhere in `tests/trybuild/fail/` still quotes `HasPressedState`.
- Compile-pass coverage for: ordinary intermediate containers, text layouts, images, multiple nesting levels, extracted helpers (`&mut WidgetBuilder<'_, Slider>` and `&mut impl LayoutContentBuilder`), returned parts (`El<Row, WidgetPart>`), and root widgets of all three kinds with styled descendants.
- A nested widget fails to compile; the former runtime test is gone.
- **No authored bundle is silently discarded.** A bundle on an element with an empty capability mask (a pure structural container) is a build error, not a stored-and-ignored entry. A whole-bundle rejection is sufficient here — Phase 5 replaces it with the per-property form and the part-naming error locations.

### Retrospective

**What worked:**
- The guarantee is carried entirely by **absent** impls, not by role-trait membership. `LayoutBuilder` has no `AcceptsElement<WidgetPart>`; `WidgetBuilder` has no `AcceptsElement<WidgetElement<V>>`; `PressedPart` is accepted only where `W: Pressable`. `WidgetPart` and `PressedPart` are therefore ordinary `ElementRole` implementers with no special casing.
- Monomorphic part roles were the right call. Both fixtures that an author would actually write compile-fail with a legible unsatisfied bound, not an inference ambiguity.
- The runtime nested-widget guard is gone, not merely bypassed: `PanelBuildError::WidgetContainsInteractiveDescendant` — variant, producer, and test — no longer appears anywhere in `crates/`.

**What deviated from the plan:**
- `WidgetBuilder::with` takes a concrete `&mut WidgetBuilder<'_, W>` closure rather than the projected `&mut <Self as AcceptsElement<Role>>::ChildBuilder<'_>`. `WidgetBuilder<'a, W>` is invariant over `'a` beneath `&mut`, so the projected form makes stable Rust demand `'static` (`E0521`). `LayoutBuilder::with` still uses the projection. The `Self: AcceptsElement<Role>` gate is unaffected — all three `WidgetBuilder` impls project to that same concrete type.
- Three fix passes were needed after the dual review, described below.

**Surprises:**
- **Sealing prevents implementation, not invocation.** `AcceptsElement` was sealed by a private supertrait, but its `with_child_builder` method was as public as the trait. Any downstream crate could hand it a plain `&mut LayoutBuilder` and receive a widget-scoped builder over an ordinary panel with no widget element inserted — the authored state look then silently discarded at build time. `#[doc(hidden)]` hides from rustdoc; it restricts no caller. Closed with a private token: the first parameter is now `private::ChildScope<'_>`, mintable only by `LayoutBuilder::with` after insertion. Guarded by `tests/trybuild/fail/widget_state_appearance_forged_scope.rs`.
- **A private-trait shape closes the same hole but destroys the diagnostic.** Moving the machinery into a trait declared in the private module makes the author-facing `E0277` name an unnameable trait. Rejected for that reason; the token shape keeps the message readable.
- **`#[doc(hidden)]` changes rustc's diagnostics, not just rustdoc.** rustc suppresses `required for … to implement …` chain notes that reference a hidden trait, and prints hidden traits by fully-qualified path. Un-hiding `AcceptsElement` shortened `hana_diegetic::AcceptsElement<…>` to `AcceptsElement<…>` and surfaced the missing link in the editable-field error — `required for WidgetBuilder<'_, EditableField> to implement AcceptsElement<PressedPart>` — which connects "`EditableField` is not `Pressable`" to the rejection. Three `.stderr` fixtures were re-blessed. A trait whose name the compiler puts in front of authors must stay documented.
- **The trait's payload is dead on the widget side and cannot be removed.** `with_child_builder` has exactly one call site (`LayoutBuilder::with`); the three `WidgetBuilder` impls carry identical never-called bodies whose `ChildBuilder` GATs are never projected. The payload keys on *(builder, role)*, but the trait carrying it is also the gate, and the gate must be implemented by `WidgetBuilder` for its three roles. Splitting the payload onto a second trait moves the `on_unimplemented` diagnostic there — the rejected shape above. Accepted as-is.

**Implications for remaining phases:**
- Any later phase adding an accepted role must add an `AcceptsElement` impl **and** supply `ChildBuilder` / `with_child_builder`, even when that builder never projects them. This is structural, not an oversight.
- `El<L, WidgetPart>` and `El<L, PressedPart>` are monomorphic — no owner parameter. The owner kind lives on `WidgetBuilder<'_, W>`, and owner-specific gating lives in the `AcceptsElement` insertion impls. Later phases must not reintroduce `WidgetPart<W>`.
- Any later change to a public trait's `#[doc(hidden)]` status, or to `on_unimplemented` text, perturbs `.stderr` fixtures. Treat `verify.sh test hana_diegetic trybuild` as a required gate for such changes and read the regenerated fixtures rather than re-blessing blind.

### Phase 4 Review

- **Phase 5 lost most of its scope to Phase 4.** Its Spec claimed `validated_element_appearance` was reached only from the widget-declaring and editable-field branches, and that Phase 4 left a whole-bundle check to replace. Both are false: `validate_widgets` (`element.rs:765`) now calls it for **every** owned element (`:784-786`), and it has always checked the four properties independently. Spec rewritten to the three items that remain — `WidgetAppearanceLocation`, part naming, and the transparent-counterpart recovery text — and three of six gate lines dropped as already satisfied.
- **Phase 5 gained a `**Pending decision:**`** on how `WidgetAppearanceLocation` carries the structural child path: the walk's stack threads no path. Recommendation recorded — a parent-index map, walked backwards only on error.
- **Phase 5 Files** now names the three sites the error-payload change breaks: the display-string table (`panel/builder.rs:1005-1021`) and Phase 4's own `widget_part_state_background_without_surface_errors_at_build`, which asserts the *owner's* id and is exactly what this phase inverts. Export moved to the `panel::` block (`lib.rs:268`), not the widgets block.
- **Phase 6 gained a `**Pending decision:**`** on what carries the four states per generated part. `StateAppearance` is `pub(crate)` and `Appearance` is single-state, so the Work Order's "four fluent methods" had no type. Recommendation recorded — `El<L, WidgetPart>`, which is public, opaque, four-state, and rejects a pressed layer by type without a new `ElementRole`.
- **Phase 6** call-site count corrected from three to ~fifteen (the argument for fluent methods gets stronger); storage sites added (`element.rs:148` plus `classify_element_change`'s exhaustive destructure at `:1368`); and a root-level `pressed` compile-fail fixture added — Phase 4 rewrote the existing one to test a *part*, leaving the root-level bound unproven.
- **Phase 7 no longer depends on Phase 5** and may be resequenced ahead of it; only the *location* of a `content_color` error is Phase 5 work. Constraint retargeted to Phase 4.
- **Phase 8's empty-bundle pending decision gained a third consumer:** an explicitly empty *part* bundle compiles, stores a part-map entry, and can never present — an accepted option that never reaches runtime.
- **Phase 11** gained two Phase 4 constraints its example migration would otherwise hit blind: widget-declaring verbs are `LayoutOnly`-only (declare the widget before any state verb), and a part-authoring helper cannot be generic over the builder.
- **Phase 12** was missing a second auto-id minting path, `LayoutTree::tooltip_add_text` (`builder.rs:1790-1807`); without it tooltip content keeps positional ids.
- **Stale references corrected across the plan:** `validate_tree` does not exist (it is `LayoutTree::validate_widgets`); every `widgets/visual.rs` reference in Phases 8, 10, and 11 was ~22 lines low; `layout/builder.rs` references below ~1240 drifted 10-14 lines, and Phase 11's `disabled_color` forward pointed at the wrong impl block entirely; the Delegation Context still named the deleted `WidgetContainsInteractiveDescendant` variant and described a trybuild driver with two tests, an `#[ignore]`, 14 fixtures, and an `E0599` diagnostic that Phase 4 replaced with one test, no ignore, 18 fixtures, and an `E0277`.

### Phase 5 — Default the state surfaces, delete the four errors · status: done (`5b4a72c4`)

#### Work Order

**Goal:** A state property whose ordinary declaration is missing gets a transparent record to replace instead of a build error, and the four `PanelBuildError::State*` variants are deleted.

**Spec:**

The original Phase 5 scope — a `WidgetAppearanceLocation` type, part naming in the message text, and transparent-counterpart recovery advice — is **cancelled**. An error that tells the author to write `El::new().background(Color::NONE)` is an error describing its own fix; layout emits that record itself.

A `VisualSlotOverride` replaces values on records layout already emitted; it never authors a missing one. That is the whole reason the four errors existed. Supply the record instead:

- a state `background` with no `El::background` gets `Some(Color::NONE)`;
- a state `border_color` or `border_width` with no `El::border` gets `Border::all(Px(0.0), Color::NONE)`;
- a state `material` needs a fill record **only when there is no border record to re-key** — the SDF fill reads `StandardMaterial::base_color` (`sdf_panel.wgsl:415`, `let fill = fill_pbr.material.base_color;`), so a state material carries its own color and needs no separately authored background.

Defaulting happens at **element construction**, on `CommonEl`, not in `LayoutTree::validate_widgets`. `validate_widgets` takes `&self` and reads `self.elements` immutably while pushing child indices; and appearance is only ever element-local (`El::hovered/focused/disabled/pressed` each write `Cascade::Override`, `layout/builder.rs:739-891`), never inherited from a scope, so by the time a `CommonEl` becomes an `Element` both the ordinary declaration and the state bundle are settled regardless of authoring order.

**Accepted semantics:**
- A transparent fill is batched and drawn, not dropped: base-color alpha lives in the retained material-table row and controls composition in-shader (`render/panel_shapes/fill_batch.rs:935-947`); `Color::NONE` is `Blend`, routes to the transparent phase, and joins an existing compatible batch. Cost is one material-table row plus quad geometry per defaulted element, no extra draw call.
- A state `border_width` with no `border_color` anywhere widens an invisible border. That is the same outcome as the already-legal `El::border(Border::all(Px(2.0), Color::NONE))`, and the `Appearance::border_width` doc says to declare `El::border` with the resting color when a state widens a normally-invisible border.

**Rejected, do not re-propose:** compile-time typestate for this. `El` would need a type-level set of authored properties and `Appearance` one for the properties it sets; because `.hovered(...)` and `.background(...)` may be written in either order, it would force an authoring order.

**Files:**
- `src/layout/builder.rs` — `impl CommonEl { fn default_state_surfaces(&mut self) }` immediately before `impl Default for CommonEl`; called first thing in both `CommonEl` → `Element` conversion sites, `text_leaf_element` and `El::into_element`, which are the only two. `use super::Px;` added.
- `src/layout/element.rs` — `validated_element_appearance` deleted with all three call sites (the `validate_widgets` walk block, and both `validated_element_widget_owner` calls in the widget and editable branches). `element_visual_capabilities` unchanged: it derives `SDF_FILL` from `background.is_some()`, which the defaulting now satisfies.
- `src/panel/builder.rs` — the four `PanelBuildError::State*` variants and their four `Display`-table rows deleted; five error-asserting tests converted to `assert!(result.is_ok())` with `*_errors_at_build` → `*_builds_ok`.
- `src/widgets/button.rs`, `src/widgets/slider.rs` — the same conversions. In slider's `set_tree` rejection test the four state blocks are **deleted, not converted**: that test ends by asserting the rejected replacements left the old tree live, which an accepted `set_tree` would invalidate.
- `src/widgets/appearance.rs` — doc table loses its "Ordinary declaration required" column; the four builder-method docs say what layout emits instead of what the author must also call.
- `docs/hana_diegetic/as-built/widgets.md` — the four error rows become a defaulted-record table.

**Constraints from prior phases:**
- **Phase 4:** any `El` inside a widget's children can carry a bundle via `El<L, WidgetPart>`. The part role is **monomorphic** — no owner parameter; the owner kind lives on `WidgetBuilder<'_, W>` and owner-specific gating lives in the `AcceptsElement` insertion impls, where `PressedPart` is accepted only by a `Pressable` owner's builder. Phase 4's own `widget_part_state_background_without_surface_errors_at_build` is one of the five tests this phase converts.
- **Phase 2:** `element_visual_capabilities` (`layout/element.rs:1328`) derives the property-capability mask from one `Element`. Its `CONTENT` bit covers text, image, and non-empty `PanelDraw` **together**; Phase 7 splits it.
- **Phase 1:** `Appearance`'s four properties are `background`, `border_color`, `border_width`, `material`. The fifth, `content_color`, arrives in Phase 7.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic trybuild`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **Docs (orchestrator-run — see Delegation Context → Docs):** rustdoc with `-D warnings`.
- A state background, border width, and material each authored with no ordinary declaration build without error and land the defaulted record (`layout/builder.rs` tests, since `into_element` is private).
- A declared background or border survives the defaulting unchanged.
- No `PanelBuildError` variant mentions a state property.

#### Retrospective

**What worked:**
- Defaulting at element construction rather than during validation. `CommonEl` holds both the ordinary declaration and the state bundle by the time it becomes an `Element`, so authoring order does not matter and no borrow restructuring was needed.
- Deleting the errors deleted their whole surface: four variants, four `Display` rows, one validator, three call sites, and the test scaffolding that asserted them. Net test count rose (1120 → 1125) because the converted tests kept their subjects.

**What deviated from the plan:**
- The phase's entire scope was replaced. `WidgetAppearanceLocation`, part naming, and recovery text were cancelled: an error whose message tells the author to write `El::new().background(Color::NONE)` is an error describing a record layout can emit itself.
- `validate_widgets` was never touched. It takes `&self` and reads `self.elements` immutably while pushing child indices, so defaulting there would have meant restructuring the borrow for no gain.

**Surprises:**
- A state material needs no separately authored background. `sdf_panel.wgsl:415` reads `let fill = fill_pbr.material.base_color;` — the fill color **is** the material's base color, so a state material supplies its own and only needs *some* record to key against. That killed `StateMaterialRequiresSurface` outright rather than widening it, which is what Phase 7 had planned to do.
- A `Color::NONE` fill is batched and drawn, not dropped. Base-color alpha lives in the retained material-table row (`render/panel_shapes/fill_batch.rs:935-947`) and `Blend` routes it to the transparent phase, where it joins an existing compatible batch. Cost is one material-table row plus quad geometry per defaulted element — no extra draw call.
- A defaulted **border** costs more than the fill does, and the accepted-semantics note above under-priced it. `needs_up_traversal` (`layout/engine/positioning.rs:216-225`) returns true on `element.border.is_some()`, so every element that authors a state `border_color` or `border_width` now takes a second DFS visit plus an up-traversal border command it did not take before.

**Implications for remaining phases:**
- **There is no appearance validation left.** Any later phase that reaches for "reject this authoring at build" must first ask whether layout can emit the missing record instead. `element_visual_capabilities` survives and still derives the capability mask, but nothing consumes it as a rejection gate.
- **Phase 7's material work shrank** to widening the `SDF_MATERIAL` capability derivation; the error it planned to widen no longer exists.
- **Phase 8's empty-bundle question is unchanged but re-framed:** an explicitly empty bundle authors no property, so the defaulting emits nothing for it. It still stores a part-map entry that can never present.
- Compile-time typestate for this was rejected and must not be re-proposed: `El` would need a type-level set of authored properties and `Appearance` one for the properties it sets, which would force an authoring order between `.background(...)` and `.hovered(...)`.

### Phase 5 Review

- **Delegation Context, two invariants rewritten.** "An accepted option must reach the runtime" now names *how* the guarantee is carried — record synthesis for the four properties — and names where it is **not** carried: the explicitly empty bundle, and `content_color`. "An ordinary declaration creates the retained record a state patches" was inverted: layout emits the record, and the two documented escape hatches are gone.
- **Test-count floor raised** from 1107 (Phase 2) to **1125** (Phase 5).
- **Phase 6:** its one Phase 5 constraint was false — it claimed part appearance is validated per property in the `validate_widgets` walk. Replaced, and a second constraint added naming the trap this phase would otherwise hit: the defaulting runs **only** on the `CommonEl` → `Element` conversion, so a bundle copied onto an already-constructed `Element` (which is what `set_field_editing_content` and `inline_editor_content_tree` operate on) silently never presents. Gate line added requiring a generated part's state **background** to present — a `content_color`-only test cannot catch this.
- **Phase 7:** its Constraints bullet pointed at the deleted validation walk and contradicted its own Spec. Replaced. A second constraint records that `default_state_surfaces` emits a fill for a state material without looking at `ElementContent` — load-bearing today, waste once this phase widens `SDF_MATERIAL` to text and `PanelDraw` — so `src/layout/builder.rs` joins its **Files** and its circle-only gate line now requires **no** defaulted fill.
- **Phase 7 gained a `**Pending decision:**`** — `content_color` has no synthesizable record, so it reopens the accepted-option-reaches-runtime hole that Phase 5 closed for the other four properties. Three options recorded; declaring it dormant is recommended, resolved together with Phase 8's empty-bundle question.
- **Phase 11 gained a `**Pending decision:**`** — `SliderFocusedThumbBorderColorRequiresThumbBorder` survived Phase 5 and is the same error class on the same record. Deleting it is recommended. A gate line was added because Phase 11's `subtree_color|disabled_color` grep does not reach it.
- **Phase 10:** its two-view rationale cited call sites Phase 5 deleted; re-cited to the two that exist (`resolve_part_overrides` on the authored `StateAppearance`, and `default_state_surfaces`). Its index-growth risk gained the fact that state authorship can now *create* recipients. Its dormancy gate gained the fixture constraint that the label must author no state border of its own, and a line barring `set_element_state_appearance` — the one appearance path that skips the defaulting, and therefore a test that cannot prove what it claims.
- **Phase 11 Files** cited `dispatch_visual_overrides` line numbers contradicting its own Spec; dropped.
- **~40 stale source citations corrected** across the Delegation Context and phases 6-12 (88 replacements): `layout/builder.rs` drifted +35 to +47 and is now 1944 lines, `layout/element.rs` −8 to −47, `widgets/appearance.rs` +2 to +6, `panel/builder.rs` −17, `widgets/visual.rs` +2 to +22, and `lib.rs`'s `pub use panel::` block starts at 238. Archive sections were left untouched.
- **`as-built/widgets.md`** said "state builders affect only the element carrying the widget declaration; child text, icons, images and shapes stay as authored" — false since Phase 4 and contradicting the same file 30 lines earlier. Corrected.
- **Not changed:** the `validated_element_appearance` mentions inside the Phase 3 and Phase 4 retrospectives and review blocks. Those record what was true when those phases shipped.

### Phase 6 — Generated editable parts · status: done

#### Work Order

**Goal:** An author can style the IME editor's generated text, selection, caret, and validation elements per state, so "any element a widget owns" holds for a focused field.

**Spec:**

`inline_editor_content_tree` (`src/ime/editor.rs:1132`) builds the editor's text, selection, caret, and validation elements **internally**, and `set_field_editing_content` (`layout/element.rs:1014`) removes the authored display descendants while editing. Without a path in, "any element a widget owns" is false for a focused field — nobody can author an element that does not exist in the source tree.

**The authoring input is `El<L, WidgetPart>`** (decided; the pending decision is resolved). It is public and opaque, already carries all four state layers, and rejects a pressed layer **by type** — `.pressed(...)` returns `El<L, PressedPart>`, a distinct type the parameter does not accept — which is the editable-field gate this phase's Phase 4 constraint asks for. It also avoids a new `ElementRole`, which would force another `AcceptsElement` impl plus another dead `with_child_builder` body (see Phase 4's Retrospective). `StateAppearance` is `pub(crate)` (`widgets/appearance.rs:249`) and the "public opaque types, not leaked private ones" invariant forbids naming it in a public signature; a new public bundle type was considered and rejected — it adds public surface for no gain, and `El` reaches every one of the four parts through paths that already exist.

Add **four fluent methods** on `El<L, WidgetElement<EditableField>>` — the type `El::editable_field` returns — one per generated part, each taking an `El<L2, WidgetPart>`:

- `editor_text` — the committed/preedit text runs (`add_text`, `editor.rs:1264`).
- `editor_selection` — the selection highlight box (`add_selected_text`, `:1271`).
- `editor_caret` — the caret box (`add_caret`, `:1285`).
- `editor_validation` — the validation message run (`append_editor_rows`, `:1148`).

Each method replaces what an earlier call to the same method authored, matching the existing state-verb convention (`builder.rs:800`).

**The carrier chain already exists end to end**; add the four bundles to each hop rather than inventing a transport:

`Element` (the editable field's own element, storage beside `appearance`) → `LayoutTree::editable_field_presentation` (`element.rs:539`) → `PanelFieldPresentation` (`panel/field.rs:20`, `pub(crate)`) → `PanelFieldRecord::presentation` (`field.rs:46`) → `inline_editor_presentation` (`editor.rs:937`) → `ImeEditorPresentation` (`editor.rs:269`, private) → `append_editor_rows` / `append_buffer` / the three `add_*` helpers. Neither struct in the middle is public, so nothing leaks.

**Application rule (this is the Phase 5 trap below, restated as the requirement):** each bundle is applied by *reconstructing an `El` from the stored declaration and calling its state verbs on the internal builder before `builder.build()`*. Text and validation take it through `Text::layout(El<L, NextRole>) -> Text<NextRole>` (`builder.rs:294`), which already accepts an element declaration and inherits its role; selection and caret already construct `El::new()`/`El::column()` values the stored declaration becomes the base of. Whatever representation the stored form takes, it must re-enter the generated tree through the normal `CommonEl` → `Element` conversion so `default_state_surfaces` runs on it.

**Geometry the generated code computes wins over an authored one** and this is documented on the four methods: the caret's width (`CARET_WIDTH`) and height (`visible_caret_height`), and the selection box's `Sizing::FIT` sizing, are set after the authored declaration is applied. Everything else the authored `El` carries — background, border, padding, corner radius, and all four state layers — reaches the generated element.

Re-keying across the display↔editor transition is **already free and needs no work here.** The part map is re-derived from `element.appearance` on every compute (`element.rs:895` → `record_owned_widget_element` `:1309`) and replaced wholesale inside `WidgetVisualSlots`; once a bundle is in the regenerated tree it is keyed correctly by construction. Phase 2's `editable_tree_replacement_rekeys_part_appearance_entries` already proves it. This phase's only job is getting the bundles *into* the generated tree.

Re-keying across the transition is **already free and needs no work here.** The part map is re-derived from `element.appearance` on every compute (`element.rs:895` → `record_owned_widget_element` `:1309`) and replaced wholesale inside `WidgetVisualSlots`; once a bundle is in the regenerated tree it is keyed correctly by construction. Phase 2's `editable_tree_replacement_rekeys_part_appearance_entries` already proves it. This phase's only job is getting the bundles *into* the generated tree.

**Files:**
- `src/layout/builder.rs:814` — `El::editable_field` returns the `El` the four new fluent methods hang on. **Shape them as fluent methods on the returned `El`, not as `editable_field` parameters.** `editable_field` (definition at `builder.rs:814`) has roughly **fifteen** call sites — `src/ime/editor.rs:1417`, `src/panel/field.rs:130/149/150`, `src/panel/builder.rs:1194/1309`, four `src/layout/element.rs` tests, `src/widgets/reify.rs:965`, and four trybuild fixtures, two of them added by Phase 4. A parameter-shaped change edits every one and forces a second `.stderr` regeneration; fluent methods leave all of them and the locked diagnostics intact.
- `src/layout/element.rs:148` — the four bundles are stored on `CommonEl`/`Element` beside `appearance: Option<Box<StateAppearance>>`, in a form that reconstructs into an `El` (the stored `CommonEl` plus its child-layout value — that is all `El` holds).
- `src/layout/element.rs:539` — `editable_field_presentation` reads them off the element into `PanelFieldPresentation`.
- `src/panel/field.rs:20` — `PanelFieldPresentation` gains the four bundle fields; it already rides `PanelFieldRecord::presentation` (`:46`) to the editor.
- `src/ime/editor.rs:269` — `ImeEditorPresentation` gains the four fields; `inline_editor_presentation` (`:937`) copies them across.
- `src/ime/editor.rs:1148/1264/1271/1285` — `append_editor_rows`, `add_text`, `add_selected_text`, `add_caret` apply the matching bundle **as `El` state verbs before `builder.build()`**. `inline_editor_content_tree` (`:1132`) and `editor_tree` (`:1106`) thread the presentation that carries them; both already take `&ImeEditorPresentation`, so neither signature changes.
- `src/layout/element.rs:1327` — `classify_element_change` destructures `Element` exhaustively; the new field must be added there or tree-change classification silently ignores it. (The destructure is exhaustive, so the compiler catches an omission — but budget for it.)
- `tests/trybuild/fail/editable_widget_root_has_no_pressed_state.{rs,stderr}` — new fixture for the root-level `pressed` gate. The existing `editable_widget_*.rs` glob (`tests/trybuild.rs:10`) already matches this name, so **`tests/trybuild.rs` needs no edit** — do not widen a glob.
- `examples/widgets.rs` — the editable-field call site, if the authoring shape reaches it.
- **`src/layout/element.rs:1014` (`set_field_editing_content`) needs no change.** It splices an already-built replacement tree; the bundles were applied during `inline_editor_content_tree`'s build, before `builder.build()`, so nothing has to survive the splice as appearance data.

**Constraints from prior phases:**
- **Phase 4:** `EditableField` is the zero-sized owner marker, `El::editable_field` returns `El<L, WidgetElement<EditableField>>`, and `EditableField` implements `WidgetOwner` but **not** `Widget` — so a pressed layer is rejected on its parts by construction and must stay rejected on the generated ones. Precisely: `.pressed(...)` is *authorable* on any `El`; it is rejected at **insertion**, because `AcceptsElement<PressedPart>` is implemented only for a `Pressable` owner's builder.
- **Phase 4:** the editable **root**'s `pressed` gate lost its compile-fail coverage. `tests/trybuild/fail/editable_widget_has_no_pressed_state.rs` was rewritten to author `.pressed(...)` on a *part* inside the field, producing `E0277: EditableField: Pressable` at the `with` insertion (`.rs:15`). Nothing now proves the root-level bound `impl<L, W: Pressable> El<L, WidgetElement<W>>::pressed` (`layout/builder.rs:884-894`) — add a root-level fixture in this phase.
- **Phase 5:** nothing validates part appearance — that walk and its validator are deleted. A state property with no ordinary declaration instead gets a transparent record to replace, emitted by `CommonEl::default_state_surfaces` (`layout/builder.rs`) at element construction.
- **Phase 5, the trap this phase must avoid.** The defaulting runs **only** on the `CommonEl` → `Element` conversion (`text_leaf_element`, `El::into_element`). It never sees an already-built `Element`. Both paths this phase names — `set_field_editing_content` (`element.rs:1014`, whose `clone_with_field_editing_content` clones constructed `Element`s) and `inline_editor_content_tree` (`ime/editor.rs:1132`) — operate *after* construction. A generated part's bundle therefore presents only if it is applied as `El` state verbs on the internal `El::row()` / `add_text` builders **before** `builder.build()`; copying a `StateAppearance` onto a constructed `Element` stores a part-map entry that silently never presents. This is invisible to a `content_color`-only test, because `content_color` needs no synthesized record.
- **Phase 2:** the part map is re-derived from the tree on every compute and replaced wholesale, so renumbering across the display↔editor transition re-keys itself; `editable_tree_replacement_rekeys_part_appearance_entries` already covers it.
- **Phase 3:** presentation resolves every recipient, so a generated part with a bundle presents with no presenter change. `Changed<WidgetVisualSlots>` is already a dirty-set term in all three presenters (`button.rs:149`, `editable.rs:40`, `slider.rs:1132`), so the regenerated editor tree re-runs resolution against the new element indices on its own — **do not add a redundant wake source or a transition observer.** The presenters also merge-walk `WidgetVisualSlots::elements` against `part_appearances` assuming both are ascending by element index and never re-sort, so the four generated parts must be inserted in that order.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic trybuild`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase adds four public authoring methods on `El<L, WidgetElement<EditableField>>`, so both doc commands must pass before checkpoint.
- A display → editor → display transition asserts the **resolved appearance of each of the four generated editor parts**, in the frame the editor appears and again after it closes.
- The bundles this phase inserts into the generated tree are emitted in ascending element-index order.
- A new compile-fail fixture proves `.pressed(...)` is rejected on an editable field's **root**, not only on its parts.
- A generated part authoring a state **`background`** presents it. `content_color` alone does not discharge this line: it needs no synthesized record, so it would pass even if the bundle were applied after construction, where the defaulting cannot reach it.

**Smoke finding (2026-07-28) — example authoring defect, fixed in this phase.**

Tabbing to the editable field rendered the whole field as an opaque black bar.
Cause is authoring, not plumbing: `examples/widgets.rs:1286` writes
`.editor_text(El::new().focused(Appearance::new().background(TEXT_FIELD_TEXT)))`
where `TEXT_FIELD_TEXT = Color::BLACK` (`widgets.rs:190`). `EditorPart::into_text`
(`layout/builder.rs:515`) does `Text::new(text, style).layout(el)`, so the authored
`Appearance` rides on the text element and `background` becomes that element's SDF
fill rect — an opaque rectangle over its own glyphs.

The author wanted per-state *glyph* color, which `Appearance` does not carry until
Phase 7 adds `content_color`. The fix applied here is the example only: the
`.editor_text(...)` call was deleted from `examples/widgets.rs`. Smoke test re-run
and confirmed correct by the plan owner. Base editor text color already
works and needs no new API — it flows from the field's own `TextStyle::with_color`
through `PanelFieldPresentation.text_style` (`panel/field.rs:28`) into
`ImeEditorPresentation.text_style` (`editor.rs:951`) and on to `add_text`. The
selection and caret parts are genuine rectangles and correctly keep `background`.

**Correction (Phase 6 review):** `editor_validation` is **also a text leaf**, not a
rectangle. `append_editor_rows` (`ime/editor.rs:1180`) routes it through `add_text`
→ `EditorPart::into_text` (`layout/builder.rs:515`), exactly like `editor_text`. The
example's `.editor_validation(... .background(BUTTON_FILL_DISABLED))` is therefore a
second instance of the same defect — latent, since it only paints when a validation
message is present. Phase 6 drops it alongside `editor_text`; Phase 7 restores both
via `content_color`. Only `editor_selection` and `editor_caret` are true rectangles.

Gate lesson: Phase 6's tests asserted the authored color reaches
`VisualOverrideIndex.fill_color` for each part. That plumbing is correct. Nothing
rendered a frame, so "an opaque fill on a text run hides the run" was invisible to
the gate. A part whose element is a text leaf needs a rendered-appearance check,
not only an override-index check.

### Retrospective

**What worked:**
- The carrier chain the Spec named existed end to end; adding the four bundles to each hop was mechanical, exactly as predicted.
- Fluent methods on the `El` returned by `editable_field` left all ~15 call sites and the locked trybuild diagnostics intact — the parameter-shaped alternative would have churned every one.
- The Phase 5 trap was avoided: all four parts re-enter through `builder.with(el, …)` / `Text::new(..).layout(el)`, so `default_state_surfaces` runs on each.

**What deviated from the plan:**
- `EDITOR_SELECTION` and `EDITOR_CARET` are **private** consts, so an authored declaration that omitted a background silently lost the built-in color with no way for an author to restore it. Fixed with `with_background_if_unset` (`layout/builder.rs:487`), applied before the geometry overrides.
- `scale_editor_part` had to run in **both** scale passes (`element.rs` `scaled` and `screen_source_scaled`), not one.
- `regenerate_commands` (`panel/diegetic_panel.rs`) had to refresh widget and tooltip records alongside field records.
- Added a trybuild case (`editable_widget_root_has_no_pressed_state`) rather than only editing the existing one.
- The example had to **drop** its `editor_text` part entirely — see below.

**Surprises:**
- **Green gates are not a rendered feature.** The tests asserted each authored color reaches `VisualOverrideIndex.fill_color`, and all four gates passed. The application smoke test then showed the focused field as an opaque black bar: `background` on a text-leaf part is that element's SDF fill, and it paints over its own glyphs. Nothing in the phase ever rendered a frame, so the defect was invisible to the gate.
- **Base editor text color was never missing.** It flows from the field's own `TextStyle::with_color` through `PanelFieldPresentation.text_style` (`panel/field.rs:28`) into `ImeEditorPresentation.text_style` (`editor.rs:951`) to `add_text`. Only *per-state* glyph color is absent, and Phase 7 already owns it as `content_color`.
- The delegate reported all gates green twice while lint was in fact failing; three fix passes were needed. Gate output must be verified independently, never taken from the report.

**Implications for remaining phases:**
- **Phase 7** must restore the example's `editor_text` part using `content_color` — already added to its acceptance gate. Phase 7's existing note that editor text color is unreachable until `content_color` lands is confirmed correct by this phase's smoke test.
- Any later phase adding a state property to a **text-leaf** part needs a check that the run still renders, not only an override-index assertion. The two are not equivalent.

### Phase 6 Review

- **Phase 7** — example gate rewritten: it said "migrate the `editor_text` part," but Phase 6 deleted that call outright, so the line discharged trivially. Now says *add* `editor_text` **and** `editor_validation`, both on `content_color`; `examples/widgets.rs` added to Files and the example build added to the gate.
- **Phase 7** — `editor_validation` reclassified as a **text leaf** (`ime/editor.rs:1180` routes it through `add_text`). The Phase 6 smoke-finding block claimed "the other three parts are genuine rectangles"; that was wrong for validation and is corrected in place. Only selection and caret are rectangles.
- **Phase 7** — render-route Files corrected: glyph override is `panel_text/batching.rs:609`/`:618` (not `:288`/`:435`); shape override is `panel_shapes/batching.rs:1123`/`:1132` (not `:989`, a blank line); `render/image_batch.rs:628` added as the image recipient; `render/analytic_paths/batching.rs` **removed** — it holds zero `VisualSlotOverride` references and consumes an already-resolved color.
- **Phase 7** — `layout/element.rs` refs corrected +32 (`element_visual_capabilities` `:1317`, `SDF_MATERIAL` derivation `:1326`).
- **Phase 7** — Phase 6 constraints expanded from one sentence to the facts a fresh implementer needs: where the four fluent methods live, `EditorPart`'s `pub(crate)` helpers, which two parts are text leaves, why the example authors neither, and the up-to-eight `add_text` fan-out from a single `editor_text` declaration.
- **Phase 7** — acceptance gate now asserts the disabled-field dim on *every* generated run, and adds a headless proxy for the Phase 6 defect class (a text-leaf recipient authoring only `content_color` acquires no `SDF_FILL` capability and no `fill_color` override).
- **Phase 7** — the existing Pending decision's recommendation (declare it dormant) strengthened: `add_text` early-returns on empty text, so recipient-emission is a runtime predicate, which makes both the build-error and typestate options unimplementable for generated parts.
- **Phase 7** — deferred to the plan owner: whether phases that change rendered appearance get an orchestrator-run smoke gate, and the matching carve-out in the "Headless only" invariant.
- **Phase 10** — deferred to the plan owner: after the skip inversion, one widget-level bundle would repaint every caret and selection box in the panel, since both carry `SDF_FILL`. Also recorded `regenerate_commands`'s new full widget-record rebuild on the visual-only path as a second index-growth multiplier, and corrected the merge-walk ref to `:377`.
- **Phase 11** — `layout/builder.rs` refs corrected ~+200, `widgets/visual.rs` refs +2, and `render/image_batch.rs:628` added to the `color`-removal list (the phase's `rg` gate cannot see `slot_override.color`).
- **Phase 12** — `layout/builder.rs` refs corrected ~+206, and the editor content tree added as the proving case for structural ids: it is the crate's highest-churn auto-id generator and its elements cannot be named.
- **Phases 8 and 9** — reviewed, no changes needed. Phase 6 touched none of the files they own.
- **Delegation Context** — test floor raised 1125 → 1130.

### Phase 7 — Content color · status: todo

#### Work Order

**Goal:** Text, images, and draw primitives change color with widget state, and state materials reach every record type the retained routes already support.

**Spec:**

Add a fifth property, `content_color`, to `Appearance`.

It does **not** map to `VisualSlotOverride::color`. `apply_sdf_visual_override` (`render/fill_batch.rs:1359`) reads `fill_color.or(color)` and `border_color.or(color)` — the generic `color` field (`widgets/visual.rs:170`) is the **fallback for every color role**, so it drives fill and border together. That is the mechanism behind `Slider::disabled_color`. A text element that also authors a background would therefore have its fill recolored by a text-color change.

Add a **distinct `content_color` override** consumed only by the text, image, and draw-primitive routes, leaving `fill_color` and `border_color` exclusive to SDF roles. `VisualSlotOverride` grows from 144 to 160 bytes for this phase; Phase 11 deletes the superseded generic `color` field and returns it to 144.

There is no material error left to widen — Phase 5 deleted `StateMaterialRequiresSurface` along with the other three. What survives is the **capability derivation**: `SDF_MATERIAL` is derived from a narrower set of records than the retained routes actually apply `VisualSlotOverride::material` to, which is SDF, text, and **every** `PanelDraw` record — lines *and* `PanelCircle` (`layout/draw.rs:11` for `PanelDraw`; `layout/line.rs:42` for the `PanelShape` enum, `:64` for `PanelCircle`; `render/panel_shapes/batching.rs:989`). Widen the derivation to match. Content color's recipients are text, image, or `PanelDraw` content.

**This requires splitting Phase 2's capability mask, not merely extending it.** `VisualElementCapabilities` (`widgets/id.rs:115`) ships one `CONTENT` bit covering text, image, and non-empty `PanelDraw` together, and sets `SDF_MATERIAL` only when a background or border exists (`element.rs:1293`). Material-accepts-text-and-draw-but-rejects-image-only is not expressible from a single bit, so replace `CONTENT` with `TEXT` / `IMAGE` / `DRAW` and widen the `SDF_MATERIAL` derivation in `element_visual_capabilities` (`element.rs:1285`) to any SDF, text, or `PanelDraw` record. Content color's capability is `TEXT | IMAGE | DRAW`; material's is everything except `IMAGE` alone.

**Files:**
- `src/widgets/appearance.rs:98` — fifth property on `Appearance` (impl at `:109`) and its fluent setter. Phase 3 deleted both `layer_onto` methods; per-property layering is now inlined in `WidgetStateCascades::resolve` (`:332`), so compose the fifth property by adding a fifth local plus a `VisualChange::To` arm inside that function's `LAYER_ORDER` loop and a fifth field in the `VisualSlotOverride` it constructs. `Appearance` derives `PartialEq` (`:95`), so the new field is compared automatically — there is no hand-written comparison to edit.
- `src/widgets/visual.rs:169` — `content_color` on `VisualSlotOverride`, **and extend both `apply` (`:195`) and `apply_element` (`:209`)**. Those two functions enumerate every field explicitly and are the only path by which an element override composes over a slot baseline in `dispatch_visual_overrides` (`:506`); omitting either silently drops a `content_color` element override wherever a slot overlay exists on the same element index.
- `src/render/panel_text/batching.rs` — the glyph-color override is `apply_text_visual_override` at **`:609`**, reading `slot_override.color` at **`:618`**. (`:288` is the cascade-read block and `:435` is `apply_routed_text_run_update`; neither is the override site.)
- `src/render/panel_shapes/batching.rs` — `apply_shape_visual_override` at **`:1123`**, color read at **`:1132`**. (`:989` is a blank line.)
- `src/render/image_batch.rs:628` — `slot_override.color.map_or(tint, linear_tint)`, the crate's only image tint override and the image `content_color` recipient. Named explicitly; the Spec's "images likewise" pointed at no file.
- **`src/render/analytic_paths/batching.rs` is NOT an edit site** — the file contains zero `VisualSlotOverride` references. It consumes a color already resolved and stamped by `panel_text/batching.rs`. Dropped from this phase (and re-examine before trusting Phase 11's list).
- `src/widgets/id.rs:115` — split `CONTENT` into `TEXT` / `IMAGE` / `DRAW`.
- `src/layout/element.rs:1317` — `element_visual_capabilities`; widen the `SDF_MATERIAL` derivation (now at **`:1326`**) to any SDF/text/`PanelDraw` record and emit the three new content bits. (Phase 6 shifted these ~+32 from the `:1285`/`:1293` the Spec above still cites.)
- `src/layout/builder.rs` — `CommonEl::default_state_surfaces` takes the `ElementContent` its two callers already hold (`text_leaf_element`, `El::into_element`) and stops emitting a fill for a state material on an element that emits its own material recipient. See the Phase 5 constraint below.

**Constraints from prior phases:**
- **Phase 1:** `Appearance` is public with `background` / `border_color` / `border_width` / `material`, each a `VisualChange<T>`; adding a fifth field takes it from 80 to 96 bytes, which is why the cascade attributes carry `Arc<Appearance>` and each has its own `size_of` assertion against `CASCADE_ATTRIBUTE_BYTES = 32`. Do not add a `VisualChange` variant.
- **Phase 2:** each recipient index carries a property-capability mask (`VisualElementCapabilities`, `widgets/id.rs:115`) so containers and non-content elements stay excluded. Its one `CONTENT` bit conflates text, image, and draw, and `SDF_MATERIAL` is set only for background-or-border — both must change here, per the Spec.
- **Phase 5:** there is no appearance validation left anywhere. `validated_element_appearance` and its three call sites are deleted; the four `PanelBuildError::State*` variants are deleted. The guarantee is now carried by **record synthesis** — `CommonEl::default_state_surfaces` (`layout/builder.rs`, called from `text_leaf_element` and `El::into_element`) emits the transparent record a state property replaces. Do not go looking for a validator to add a fifth arm to; there is none. `element_visual_capabilities` (`element.rs:1285`) survives and still derives the mask, but nothing consumes it as a rejection gate.
- **Phase 5, the interaction this phase must handle:** `default_state_surfaces` emits a `Color::NONE` fill for a state `material` whenever the element has no border record. It does not look at `ElementContent`, so a text-only or `PanelDraw`-only part authoring a state material gets an SDF fill it does not need. Today that fill is *load-bearing* — `SDF_MATERIAL` is the only route a state material has. Once this phase widens the derivation to text and `PanelDraw`, it becomes waste (one material-table row plus quad geometry per element). Both conversion sites already have `content` in scope, so passing it into `default_state_surfaces` and skipping the fill when the element emits its own recipient is a small change — but it is this phase's change to make, and this phase's gate must assert the circle-only part carries **no** defaulted fill.
- **Phase 6:** the four generated editor parts are recipients; editor text is the canonical `content_color` target. Specifically:
  - The four fluent methods live on `El<L, WidgetElement<EditableField>>` (`layout/builder.rs:1033`–`:1072`).
  - `EditorPart` is `pub(crate)` (`layout/builder.rs:448`), with `into_text` (`:515`), `with_children` (`:524`), and `with_background_if_unset` (`:487`) — the last supplies the built-in `EDITOR_SELECTION` / `EDITOR_CARET` colors, which are **private consts**, so an authored declaration that omits a background still gets the default.
  - **`editor_text` and `editor_validation` become text leaves; `editor_selection` and `editor_caret` become rectangles.** Only the latter two can use `background`.
  - The example currently authors **neither** `editor_text` nor `editor_validation`, because `background` on a text leaf painted an opaque rect over the glyphs. This phase adds both back on `content_color`.
  - **Fan-out:** one `editor_text` declaration reaches up to **eight** generated text elements — `add_text` is called at `ime/editor.rs:1222`, `:1229`, `:1238`, `:1250`, `:1272`, `:1279`, `:1286`, `:1292` (preedit runs, pre/post-selection runs, and the run inside `add_selected_text`). `into_text` clears `common.id` (`layout/builder.rs:516`), so none can be named individually. One authored `content_color` therefore produces N recipients and N part-map entries.

**Pending decision:** what happens when `content_color` names a recipient the element cannot emit.

Phase 5 made the other four properties unrejectable by synthesizing the record they replace. `content_color` cannot follow: layout can conjure a transparent SDF fill or border out of nothing, but it cannot conjure text, an image, or a `PanelDraw`. So `El::new().disabled(Appearance::new().content_color(RED))` on a structural container compiles, is admitted to `part_appearances` by `any_overridden()` (`element.rs:1318-1324`), and can never present. That is the **accepted option must reach the runtime** breach Phase 4 closed with `validated_element_appearance` — whose machinery Phase 5 deleted.

Three options:
- **Declare it dormant** and amend the invariant's part-local scope to admit one exception. Consistent with how a higher-level property with no compatible recipient already behaves, but it means the one property an author most expects to see do something silently does nothing.
- **Reintroduce one targeted build error** for `content_color` only. Reverses Phase 5's direction for a single property, and re-adds the error-text problem the plan owner rejected — though the recovery here is real (`add a text or image child`), not a record layout could have emitted itself.
- **Gate it out of the type surface** — only a part whose element emits content can name it. Strongest, but `content_color` lives on `Appearance`, which is one type shared by every element; splitting it by capability would need a second appearance type or a typestate parameter, and the plan already rejected typestate for this family.

Recommendation: **declare it dormant**, and add the empty-bundle question (Phase 8) and this one to the same invariant amendment rather than amending it twice. Resolve before dispatching Phase 7.

**Phase 6 strengthens this recommendation and eliminates option 3.** `add_text` early-returns on empty text (`ime/editor.rs:1303`), so whether `editor_text` emits a content recipient at all is decided **at runtime** from the buffer contents — no build error (option 2) and no typestate gate (option 3) can evaluate that predicate for a generated part. Further, `editor_selection` is a rectangle that *contains* the text as a child, so an author will reasonably expect `content_color` there to do something it cannot. Dormancy is the only implementable option for the generated parts.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase adds a public `Appearance` property and its doc entry, so both doc commands must pass before checkpoint.
- Whatever the pending decision resolves to, a `content_color` naming an absent recipient is covered by a test: dormant-and-inert, or rejected at build.
- A `const _: () = assert!(size_of::<VisualSlotOverride>() <= …)` records the type's new size at the value this phase grows it to, following the per-attribute precedent at `widgets/appearance.rs:219`. Phase 11 shrinks it back and asserts the smaller number; without this line that later assertion is a first measurement rather than a verified delta.
- A disabled slider dims its label.
- A hovered button brightens its caption **without touching its fill**.
- A text element carrying its own background and border changes **only** its text color under a state.
- A circle-only part accepts and presents both material and content color, and carries **no** defaulted `Color::NONE` fill (see the Phase 5 interaction constraint).
- A state material on a text label wins over the `TextMaterial` cascade and restores it when the state clears.
- An element-level `content_color` survives composition with a slot override on the same element index, proving `apply_element` carries the new field.
- A disabled editable field dims its editor text — asserted on **every** generated run, not just the first, given the up-to-eight fan-out recorded in the Phase 6 constraints. (Moved here from Phase 6 — editor text color is unreachable until this phase adds `content_color`.)
- A **headless proxy for the Phase 6 defect class**: a text-leaf recipient authoring only `content_color` acquires no `SDF_FILL` capability and no `fill_color` override. This is what would have caught the opaque-bar bug without a GPU.
- `examples/widgets.rs` **adds** an `editor_text` part and an `editor_validation`
  part, both authoring `content_color` (not `background`). Phase 6 deleted both
  calls outright, so there is nothing to "migrate" — grepping for `.editor_text(`
  finds nothing. Both parts are text leaves (`ime/editor.rs:1180` routes validation
  through `add_text` too), which is why `background` was wrong for them. Verified by
  `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`.

**Pending decision:** whether phases that change rendered appearance get an orchestrator-run smoke gate.

Actual problem:
Phase 6 passed every gate — lint, 1130 tests, trybuild, example build — while the
focused field rendered as an opaque black bar. The only thing that caught it was an
orchestrator-run smoke of the live example with a human at the keyboard. Phases 7 and
11 both change the example's rendered appearance.

What exists now:
- The Delegation Context invariant "Headless only" says assertions are "never on
  rendered color" and "No phase needs a GPU, a window, or a screenshot."
- That invariant is what made Phase 6's gate blind to its own defect.

What should change:
- Add an explicit orchestrator-run smoke line to phases 7 and 11, parallel to the
  existing **Docs** line, and spell out the carve-out in the invariant.
- Keep the headless proxy assertion already added to Phase 7's gate — it catches the
  same class without a GPU, but only for capability/override state, not for what a
  pixel actually looks like.

Recommendation:
Add both: the headless proxy as the automated gate, and a one-line orchestrator smoke
as a human check before checkpoint. The smoke costs a launch and a keypress; Phase 6
shows the alternative costs a shipped-broken feature.

Approve this direction, or modify it?

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
- `crates/hana_diegetic/src/widgets/appearance.rs:98` — `Appearance::merge_over`. **Phase 3 deleted both `layer_onto` methods.** Neither `Appearance::layer_onto` nor `VisualChange::layer_onto` exists any more, so the thin wrapper this bullet used to describe has nothing to wrap: `merge_over` is now the **first and only** per-property fold over `Appearance`'s fields, written out field by field (lower's `To` wins, otherwise the higher value carries through). `VisualChange` (`:26`) carries only `is_authored` (`:36`) today. Phase 3 inlined per-property layering into `WidgetStateCascades::resolve` (`:332`), which accumulates four `Option<&T>` per-property winners across the `LAYER_ORDER` loop (`:335`) rather than folding whole `Appearance` values — a different shape, and not the fold to reuse. Write `merge_over` directly; the former "do not write a third per-property fold / do not add a `VisualChange::or`" prohibition has lost its premise and no longer applies.
- `crates/hana_diegetic/src/widgets/appearance.rs` — the four `Widget*Appearance` types implement `CascadeRoot` with a `combine` delegating to `merge_over`.

**Constraints from prior phases:**
- **Phase 1:** the four wrappers are `Arc<Appearance>` newtypes with hand-written `PartialEq` (`Arc::ptr_eq` then content equality) and per-attribute `size_of` assertions. Every merge allocates a fresh `Arc`, so equality must fall through to content comparison — a merge producing an equal value must still compare equal, or propagation dirties `Resolved<A>` every frame.
- **Phase 7:** `Appearance` now has five `VisualChange` fields — `background`, `border_color`, `border_width`, `material`, `content_color`. `merge_over` covers all five.
- The existing cascade attributes that must keep replace semantics, all declared through `cascade_attribute!` in `src/cascade/resolved.rs`: `TextAlpha` (`:52`), `FontUnit` (`:58`), `HdrTextCoverageBias` (`:63`), `SdfMaterial` (`:112`), `TextMaterial` (`:125`), `ShapeMaterial` (`:138`), `Lighting` (`:149`), `ShadowCasting` (`:152`), `GlyphShadowMode` (`:155`), `Sidedness` (`:159`), `AntiAlias` (`:163`), `HairlineFade` (`:167`), `WidgetInteractivity` (`:170`). **None of them is edited by this phase** — the macro emits no `combine`, so they inherit the replace default.

**Pending decision:** whether an explicitly authored empty bundle suppresses an inherited one, or is indistinguishable from never authoring.

**Third consumer, found reviewing Phase 4:** an explicitly empty *part* bundle compiles, stores a part-map entry, and can never present. `El::new().disabled(Appearance::new())` inside a widget authors no property, so `CommonEl::default_state_surfaces` (`layout/builder.rs`) emits nothing for it; it is then admitted to `part_appearances` by `any_overridden()` (`element.rs:1318-1324`), and on a zero-capability structural container is never visited by `resolve_part_overrides`, which iterates `slots.elements()` (`widgets/visual.rs:369`). That is an accepted option that never reaches runtime — the breach Phase 4's final gate line was written to close. Whichever way this decision goes, it must also either declare an empty bundle a permitted no-op and amend the accepted-option-reaches-runtime invariant's part-local scope, or reject it at build.

Actual problem:
The plan currently says both. The invariant at the top of this document says silence means "no opinion" and `.disabled(Appearance::new())` is a no-op, and Phase 10's gate says an explicit empty part bundle resolves identically to no part bundle. Phase 1's archived Spec says the opposite — it justifies storing `Cascade` on the grounds that an explicit empty bundle "must suppress an inherited bundle." A delegate implementing this phase's fold from the archived Spec would build suppression; one implementing from the invariant would not.

What exists now:
- Phase 1 stores the distinction: `.hovered(Appearance::new())` is `Cascade::Override`, an un-authored state is `Cascade::Inherit`, both pinned by a test in `layout/builder.rs`.
- Nothing consumes the distinction. Under the fold this phase adds, `Override(Appearance::new())` and `Inherit` produce byte-identical results, and this phase's own gate line asserts exactly that (`Appearance::new().merge_over(&x)` equals `x`).
- **Phase 2 gave the distinction a second consumer.** Part-map admission keys on `WidgetStateCascades::any_overridden()` (`widgets/appearance.rs:288`), so `Override(Appearance::new())` creates a map entry, pinned by a Phase 2 test. Under the no-op reading that entry can only ever resolve to a default override — exactly the wasted resolution the capability mask was added to prevent.
- **Phase 3 shipped the no-op reading in code.** `resolve_part_overrides` (`widgets/visual.rs:369`) drops any resolution equal to `VisualSlotOverride::default()` (`:370`), so an explicit empty bundle already produces no override at runtime. The no-op branch below is now the status quo, not a change.

What should change — pick one and make the whole document say it:
- **No-op (matches the invariant and both gates).** An empty bundle contributes nothing at any level. The stored `Override`/`Inherit` distinction stays inert — harmless, but it is not load-bearing and Phase 1's rationale for it should be corrected rather than left to mislead a later delegate.
- **Suppression (matches Phase 1's archived Spec).** An explicit empty bundle clears whatever a higher level authored, giving authors a way to opt a widget out of an inherited look. This needs the fold to distinguish the two cases, a revised invariant, and revised gate lines here and in Phase 10. **Phase 3 raised its cost:** it additionally requires deleting the default-drop filter at `widgets/visual.rs:392` and inventing an explicit "clear" token in `VisualSlotOverride`, which has none — every field is an `Option<T>` whose `None` already means "no opinion", so there is no spare value to spend on "clear". Choosing suppression pulls `src/widgets/visual.rs` into this phase's **Files**.

Whichever is chosen, this phase must also settle **part-map admission**: it stays override-keyed (required if suppression wins, since an empty bundle must reach resolution to suppress), or it reverts to property-authorship (cheaper under no-op, since an empty entry can never change a pixel). If admission changes, `layout/element.rs:1309` `record_owned_widget_element` joins this phase's **Files** and Phase 2's admission test is updated with it.

Recommendation:
Take the no-op reading — it is what the invariant, this phase's gate, and Phase 10's gate already specify, **and Phase 3 has already shipped it** (`widgets/visual.rs:392` drops any resolution equal to `VisualSlotOverride::default()`), so only Phase 1's archived rationale is out of step. Record the correction as a note beneath Phase 1's Retrospective rather than editing the archived Work Order. If suppression is wanted later, it is a clean additive feature (an explicit "clear" value distinct from an empty bundle) rather than a reinterpretation of empty. Keep admission override-keyed regardless: the entry is rare, the capability mask already prevents the expensive part of the waste, and reverting admission would destroy the distinction a later suppression feature needs.

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

- Four `BuilderData` fields, builder methods, component seeds, and `build_panel` assignments (`src/panel/builder.rs:183`).
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
- `src/panel/builder.rs:183` — four `BuilderData` fields + builder methods + seeds + `build_panel` assignments.
- `src/panel/diegetic_panel.rs` — four `seed_panel_value` calls (`:1566`), four `replace_from_precompose_helper` assignments (`:451`).
- `src/panel/lifecycle.rs` — four ownership-observer entries (`:122`), four teardown entries (`:775`).
- `src/cascade/attributes.rs:30` — four typed command pairs, with durability documentation.
- `src/cascade/defaults.rs` — four empty-`Appearance` `CascadeDefault` resources.
- `src/lib.rs:346-410` — the `pub use widgets::*` block, shifted by Phase 4's eight new `layout::` exports. Crate-root exports for the panel-builder methods and commands only; the four attribute types are already exported inside it.

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
- **Propagating an unchanged bundle does not dirty `Resolved<…>`.** Re-running propagation with no authoring change must leave `Resolved<Widget*Appearance>` unmarked, and Phase 3's presenter-isolation tests must pass unchanged. All three presenters already carry the four `Changed<Cascade<Widget*Appearance>>` dirty terms (`button.rs:145-148`), so a propagation that rewrites the `Arc` with a content-equal value wakes every presenter every frame — the Phase 1 constraint above is the cause, this gate is the effect that proves it.

### Phase 10 — Two-stage resolution and reification · status: todo

#### Work Order

**Goal:** A resolved bundle reaches every element a widget owns, merged per property across global → panel → widget → part, with state layering applied only afterward.

**Spec:**

Resolution is **two-stage**, because `Cascade<T>` and `Resolved<T>` are per-entity components while parts are layout indices on one widget entity — a single `Resolved<T>` cannot carry a distinct value per part, and spawning an entity per part would add roughly eight entities, their relationships, and eight cascade components each per slider.

1. `CascadePlugin` resolves **global → panel → widget** on the widget entity, over the four attribute types (already wired in Phase 9).
2. Presentation resolves **part against widget** by reference: each sparse map entry is a part-local `Cascade<…>` resolved against the widget's `Resolved<…>`, through **one typed helper** rather than precedence spelled out in each presenter. **That helper already exists.** Phase 3 shipped `widgets::visual::resolve_part_overrides` (`widgets/visual.rs:369`), called identically by all three presenters (`button.rs:235`, `editable.rs:121`, `slider.rs:1202`) — it is already the single part-resolution seam. Extend it to take the four `&Resolved<Widget*Appearance>` as parameters. **Do not write a second helper in `src/cascade/`**: that duplicates the seam Phase 3 established and leaves the three presenters resolving through two different paths.

**Then, and only then,** layer the active states in `LAYER_ORDER` (`widgets/appearance.rs:388`, `[Focused, Hovered, Pressed, Disabled]`) and build the record override. The two axes must not be interleaved.

**This phase needs two state views, not one.** `WidgetStateCascades<'a>` (`widgets/appearance.rs:264`) holds `&'a Cascade<Widget*Appearance>` and its `layer` (`:295`) reads through `Cascade::as_override()`. Presentation here reads `Resolved<Widget*Appearance>`, which derefs to the attribute itself and is never a `Cascade` — so the resolved path needs its own view over four `&Appearance` (or four `&Widget*Appearance`). The authored view must stay, for two reasons: `resolve_part_overrides` calls `cascades().resolve(...)` on the part's **authored** `StateAppearance` (`visual.rs:391`), and `CommonEl::default_state_surfaces` (`layout/builder.rs`) calls `any` (`:317`) to decide which records to synthesize. (Phase 5 deleted the build-time validation this paragraph used to cite; `any` now has exactly that one production caller.) Factor the shared `LAYER_ORDER` fold so both views call one implementation rather than duplicating `layer`/`resolve`.

Both hops use `Appearance::merge_over` from Phase 8. For one element in one state:

1. Cascade resolves the widget's bundle down the levels (global → panel → widget).
2. For each property: the part's value if the part names it, else the widget's resolved value, else the ordinary look.
3. Record-specific render routes consume only the properties they can present; the rest are **dormant** at that element.

**Invert `resolve_part_overrides`'s skip.** Today the merge-walk `continue`s for any recipient with no `part_appearances` entry (`widgets/visual.rs:379-390`), so a widget-level bundle would reach nothing. Every recipient must now receive the widget's resolved bundle whether or not it has a part entry; a part entry, when present, merges over it.

**That inversion makes `VisualElementCapabilities` load-bearing for the first time.** The mask is stored in `WidgetVisualSlots::elements` (`widgets/visual.rs:84`, read at `:120`) but the merge-walk destructures it away — `&(element_index, _)` at `:355` — and **no production code reads it today**; Phase 2 built it and nothing has consumed it since. Wire it here: a recipient whose capabilities cannot present any property the resolved bundle names must produce no `VisualOverrideIndex` entry. Without this the dormancy gate below cannot pass, because every recipient would now get an entry.

**Named risk — index growth.** Once every recipient receives the widget bundle, one global `CascadeDefault` naming a single property produces index entries proportional to widgets × recipients, where today it produces none. The capability mask is the only bound on that, which is the second reason it must be wired in this phase rather than deferred. **Since Phase 5 the recipient set is no longer fixed by ordinary declarations:** `element_visual_capabilities` derives `SDF_FILL` / `SDF_BORDER` from `background.is_some()` / `border.is_some()` (`element.rs:1285`), and a structural container authoring `.hovered(Appearance::new().background(X))` now gets a synthesized fill — so it becomes a full SDF recipient where it was previously a build error. State authorship can create recipients, so the multiplier is larger than the pre-Phase-5 estimate.

**Reification.** Widgets already receive `CascadeFrom::new(panel)` on spawn (`bevy_kana/src/cascade.rs:197`) and `update_widget` (`reify.rs:352`) repairs a wrong relationship. The existing order is cycle-free: `CascadeSet::Propagate → PanelSystems::ComputeLayout → WidgetSystems::Reify → ReifyCommandsApplied → presentation`, with `ReifyCommandsApplied` flushing both the widget insertions and the `resolve_inserted_cascade` observer (`bevy_kana/src/cascade.rs:339`) that seeds `Resolved<A>` — the existing `disabled_widget_is_marked_in_its_reification_frame` test already proves same-frame behavior for `WidgetInteractivity`.

This phase must additionally:
- Keep `CascadeFrom::new(panel)` in the same deferred insertion.
- Order presentation after **both** `CascadeSet::Propagate` and `WidgetSystems::ReifyCommandsApplied` — the set declarations are `widgets/mod.rs:143`, the `configure_sets` call is `:238-268`, and the presenters are added at `:299-307` (Phase 3 removed the `.run_if(...)` that used to sit on each of the three).
- Add the four `Changed<Resolved<…>>` filters and the part map to presentation's dirty inputs.
- **Resolve the removal question explicitly:** either declare that a live widget never loses its four `Resolved` caches and omit their removal streams, or specify how a missing cache clears the prior override. A query requiring all four caches cannot process their removal.

**Documentation.** Update `docs/hana_diegetic/widgets-deferred.md` in this phase: replace "direct widget-builder inputs" with global/panel/widget/part appearance authoring; remove global-versus-instance placement and state-dependent child addressing from the open questions; keep presets, named variants, later widget states, extended materials, animations, slider geometry, and tooltip reuse deferred. Its stale current-plan link is already fixed — it now points at `as-built/widgets.md`.

**Files:**
- `src/widgets/visual.rs:369` — extend Phase 3's existing `resolve_part_overrides` (the part-against-widget seam) to take the four `&Resolved<Widget*Appearance>`; invert the no-part-entry skip (`:379-390`); read `VisualElementCapabilities` at `:377` instead of discarding it. **No new helper in `src/cascade/`.**
- `src/widgets/mod.rs:238-268` and `:299-307` — presentation ordering after `Propagate` and `ReifyCommandsApplied`.
- `src/widgets/button.rs:235`, `src/widgets/slider.rs:1202`, `src/widgets/editable.rs:121` — the three `resolve_part_overrides` call sites; each passes the four `&Resolved<…>` and gains four `Changed<Resolved<…>>` dirty inputs. **Query-signature changes only** — the resolution logic stays in `visual.rs`.
- `src/widgets/appearance.rs:249-365` — add the resolved-side state view alongside the authored `WidgetStateCascades<'a>` (`:264`) and share one `LAYER_ORDER` (`:388`) fold between them. `resolve` (`:332`) composes the merged bundles in `LAYER_ORDER` after level resolution, not during it. **This is a smaller change than it sounds**, though not for the reason previously recorded here: Phase 3 rewrote `resolve`, and it does **not** layer against an `Appearance::default()` accumulator. It accumulates four `Option<&T>` per-property winners across the `LAYER_ORDER` loop (`:335`) and builds a `VisualSlotOverride` directly (`:331-362`), taking `panel: Option<&DiegeticPanel>` for border-width conversion. It does already keep the two axes separate. What changes is only where each layer comes from — the resolved bundles passed in, instead of `layer(state)` (`:295`) reading this record's own `Cascade`s. Do not rewrite the layering algorithm.
- `docs/hana_diegetic/widgets-deferred.md` — the four documentation edits above.

**Constraints from prior phases:**
- **Phase 9:** the four `CascadePlugin` channels, `CascadeDefault` resources, panel builder methods, and typed commands all exist; `Resolved<Widget*Appearance>` is present on every widget entity. Registration is in `WidgetsPlugin`; observers and seeding are in `HeadlessLayoutPlugin`.
- **Phase 8:** `Appearance::merge_over(&self, higher)` is the single merge used at both hops; `CascadeRoot::combine` already makes stage 1 merge per property, and the `CascadeDefault` root participates in that merge rather than acting as a fallback. Stage 2 calls `merge_over` directly — it is not a cascade hop and needs no `combine`.
- **Phase 2:** the sparse part map is sorted by element index, capability-masked, revision-scoped, and stored **separately** from the four root `Cascade` values — the root's bundle is the widget's own override and must not be applied a second time as a part override. Phase 2 also landed the entity shape this phase resolves through: `StateAppearance` is not a `Component`, `spawn_widget` inserts all four `Cascade<Widget*Appearance>` channels including `Cascade::Inherit`, `update_widget` synchronizes them per channel, and `WidgetStateCascades<'_>` is the borrowed view the presenters already use.
- **Phase 3:** presenters already merge-walk recipients and already own their `Changed`/`RemovedComponents` drains; this phase adds four more `Changed` filters to the drains they own, not to a run condition.
- **Phase 5:** part-local authoring is never rejected — a state property with no ordinary declaration gets a transparent record to replace, emitted by `CommonEl::default_state_surfaces` (`layout/builder.rs`) at element construction. Higher-level properties with no compatible recipient are likewise **dormant**, not errors. There is no appearance validation left to route them through.
- **Phase 7:** `content_color` is the fifth property and the merge covers it.

**Pending decision:** which override channel carries the widget level.

Actual problem:
Phase 3 gave `WidgetVisualOverrides` two channels — per-element overrides (`set_element` `widgets/visual.rs:320`, read back via `element_overrides` `:338`) and the older whole-slot overrides. `dispatch_visual_overrides` composes them in that order: the slot override is the baseline, the element override lays over it per property via `apply_element` (`:209`, applied at `:524-538`). This phase writes the resolved widget-level bundle to every recipient but never says **which of those two channels it uses**, and the slider still writes its root as a *slot* today (`slider.rs:1196-1199`). Phase 11's focus-border rework silently depends on the answer.

What exists now:
- `apply` (`:195`) and `apply_element` (`:209`) are both per-property `overlay.or(self)` — an overlay that names `border_color` replaces it, one that leaves it `Unchanged` preserves what was underneath.
- The slider writes `SLIDER_ROOT` through the slot channel (`slider.rs:1196-1199`); nothing yet writes a widget-level bundle through the element channel.
- Phase 11's three focus-border bullets ("a disabled `border_color: To(…)` replaces it, `Unchanged` preserves it, an element overlay without `offset` leaves the thumb translation alone") describe exactly what `apply_element` already does.

What should change — pick one:
- **Element channel.** The widget-level resolved bundle is written per recipient through `set_element`. Composition against the authored slot baseline is then the existing `apply_element` path, so Phase 11's focus-border rework collapses to deleting the `!(disabled && slider.disabled_color.is_some())` guard at `slider.rs:1221-1222` — the behavior it currently hand-rolls is inherited.
- **Root slot.** The widget-level bundle composes at slot granularity, matching how the slider writes its root today. Phase 11 must then implement the focus-border interaction itself rather than inheriting it, and this phase must say how a widget-level slot override and a part-level element override compose on the same element.

Recommendation:
Take the element channel. It reuses the composition Phase 3 already built and tested, it is the channel the part level already uses so both hops compose through one code path, and it makes Phase 11's focus-border work a deletion rather than a rewrite. Record the choice in this phase's **Spec** and in Phase 11's **Constraints from prior phases**, since Phase 11's scope estimate depends on it.

Approve this direction, or modify it?

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
- An **explicit empty part bundle resolves to the widget's inherited bundle at that element, identically to a recipient with no part entry** — its own named test. State it against the post-inversion path: Phase 3's default-drop filter (`visual.rs:392`) already makes an empty bundle produce nothing today, so the previous wording ("resolves identically to no part bundle") passes on the current tree without proving anything this phase builds.
- **Dormancy:** a widget bundle naming `border_color` against a text-only label leaves the label unchanged, produces no error, and creates no `VisualOverrideIndex` entry for it. One test per property × incompatible-recipient pair. **The fixture's label must author no state border of its own** — if it does, `CommonEl::default_state_surfaces` synthesizes a transparent border and it becomes a legitimate `SDF_BORDER` recipient, so the test would be asserting the opposite of what it claims.
- Every test in this phase authors its bundles through the public part-authoring surface Phase 4 shipped, never through `set_element_state_appearance` (`element.rs:462`, `#[cfg(test)]`). That helper assigns `element.appearance` after construction and is the one path that skips Phase 5's defaulting — a bundle placed through it gets no synthesized record, no capability bit, and no recipient, making the test structurally incapable of proving presentation.
- A test sources focused, hovered, pressed, and disabled from **four different levels**, including an explicit empty part bundle, and asserts `LAYER_ORDER` still governs.
- Runtime global-default and panel-override mutations repaint live buttons, sliders, and editable fields while widget state is unchanged; editable tests confirm pressed appearance never applies.
- A **first-update test** covers global, panel, widget-root, and part inheritance in the reification frame.
- **Phase 3's presenter-isolation tests pass unchanged** once the four `Changed<Resolved<…>>` terms are added: `button_press_edges_do_not_rebuild_slider_overrides`, plus the detector that removes `WidgetVisualOverrides` from the peer widget, drives the other, and asserts the peer's component is not re-inserted. Propagating an unchanged bundle must not dirty `Resolved<…>` and must not wake a presenter.
- `docs/hana_diegetic/widgets-deferred.md` carries none of the four stale statements.

**Pending decision:** whether a widget-level appearance bundle should repaint the generated carets and selection boxes.

Actual problem:
Phase 10 inverts the skip so every recipient receives the widget-level bundle. The
generated caret and selection elements are legitimate recipients — both branches of
`add_caret` / `add_selected_text` give them a background
(`with_background_if_unset(EDITOR_CARET)` at `ime/editor.rs:1362`, `EDITOR_SELECTION`
at `:1329`, and the `None` branches at `:1340` / `:1371`), so they carry `SDF_FILL`.

What exists now:
- After Phase 10, one `widget_focused_appearance(Appearance::new().background(X))`
  would recolor every caret and selection highlight in the panel.
- Phase 10 neither names this behavior nor gates it.

What should change:
- Either accept it and add a gate asserting the cascade reaches the generated parts,
  or exclude generated parts from the widget-level bundle and gate that exclusion.

Recommendation:
Accept and gate it — a widget-level bundle reaching everything the widget owns is the
whole premise of the plan, and the generated parts are owned. But it must be a named,
tested behavior rather than an emergent one.

Approve this direction, or modify it?

**Ref corrections and added constraints (Phase 6 review):**
- The merge-walk destructure is at `widgets/visual.rs:377`, not the `:355` this
  phase's Spec cites; the Files bullet already says `:377` and is correct.
  `resolve_part_overrides` `:369`, the two `continue` skips `:385` / `:388`, and the
  default-drop filter `:392` are all still accurate.
- **Second index-growth multiplier.** Phase 6 added
  `self.widget_records = tree.computed_widget_records(result)` and the tooltip
  equivalent to `regenerate_commands` (`panel/diegetic_panel.rs`), so an
  appearance-only edit — classified `VisualOnly` by `visual_only_properties_changed`
  (`layout/element.rs`) — now rebuilds every `ComputedWidgetRecord` and re-inserts
  `WidgetVisualSlots`, waking `dispatch_visual_overrides` into a full index rebuild.
  Record this alongside recipients-per-widget under "Named risk — index growth".

### Phase 11 — Remove `Slider::disabled_color` and the subtree channel · status: todo

#### Work Order

**Goal:** The blunt subtree-recolor path is gone, `VisualSlotOverride` is back to 144 bytes, and the slider's focus border composes correctly against a cascaded disabled bundle.

**Spec:**

Delete `Slider::disabled_color` — the field (`widgets/slider.rs:173`), its constructor default (`:192`), its builder method (`:234`), its `El` forward — `El<L, WidgetElement<Slider>>::disabled_color` at `layout/builder.rs:1049`, **not** `:883-886`, which after Phase 4 is inside the `El<L, WidgetElement<W>>::disabled` block — and its test `disabled_color_recolors_every_slider_element_and_suppresses_focus_border` (`:5274`). **There is no crate-internal setter** — an earlier revision of this Work Order claimed one at `slider.rs:255`; that line is inside `fn validated()` and no `set_disabled_color` exists anywhere in the crate. Delete `WidgetVisualOverrides::subtree_color` (`widgets/visual.rs:265`), `set_subtree_color` (`:272`), the getter (`:277`), the `set_subtree_color` seeding call in `slider.rs:1178`, and its consumption in `dispatch_visual_overrides` (`visual.rs:535-545`).

With its only production producer gone, delete `VisualSlotOverride::color` (`visual.rs:171`), its overlay logic, and the `with_color` test helper (`:220`); move the text, image, and draw-primitive consumers to `content_color`. **The overlay logic is now two methods, not one** — `apply` (`:195`, the only one that names `color`) and `apply_element` (`:209`), both added or reworked by Phase 3; edit both.

**Keep the `HashMap<usize, VisualSlotOverride>`.** The former instruction to delete it "only if Phase 3's element channel did not take it over" is now answered: it did. The map is built once at `visual.rs:534` and serves three producers — subtree seeding (`:535-545`), slot overlays (`:546-554`), and Phase 3's element channel (`:555-560`). Delete **only** the subtree branch at `:535-545`; the map and the other two producers stay. `VisualSlotOverride` returns from 160 to 144 bytes.

**Focus-border composition.** The thumb focus border cannot be suppressed by "a resolved disabled bundle exists" — under a cascade every state always resolves to something, so presence is always true, and a disabled bundle changing only a background would delete the focus border. Compose `Slider::focused_thumb_border_color` as a **focused-thumb layer before normal state composition**:
- a disabled `border_color: To(…)` **replaces** it,
- a disabled `border_color: Unchanged` **preserves** it,
- an element overlay without `offset` leaves the thumb translation alone.

**Phase 3 already satisfies all three, provided Phase 10 routes the widget level through the element channel.** `apply_element` (`visual.rs:209`, applied at `:524-538`) is per-property `overlay.or(self)`: a named `border_color` replaces, an `Unchanged` one preserves, and `offset` is untouched because the overlay never names it. If Phase 10's pending decision lands on the element channel, this section's remaining work is a **deletion** — remove the `!(disabled && slider.disabled_color.is_some())` guard at `slider.rs:1221-1222` — not a rewrite. If it lands on the root slot instead, compose the layer by hand as originally written. Check Phase 10's resolved decision before starting.

Convert `examples/widgets.rs`'s slider (`add_slider` `:1200`, `.disabled_color` use `:1162`) to author its parts explicitly.

**Material churn contract.** Per-element authoring lets one hover transition swap materials on label, track, and thumb together. A compatibility-preserving swap updates material-table rows in place; an incompatible one removes and re-inserts records across batches (`render/fill_batch.rs:1359`, `render/batch_store.rs:201`), rebuilds text runs (`render/panel_text/batching.rs:435`, `render/analytic_paths/batching.rs:314`), despawns empty batches, and allocates entity, mesh, material, and storage buffers for new ones. Incompatible materials stay **permitted**, but this phase must:
- document compatibility-preserving swaps as the steady-state path,
- keep built-in defaults and examples compatibility-preserving,
- add a label/track/thumb transition test asserting **no batch-key move and no batch entity creation** for compatible materials,
- add one incompatible case asserting **only the affected members migrate**.

**Files:**
- `src/widgets/slider.rs` — delete `disabled_color` (field `:173`, default `:192`, builder `:234`; there is no crate-internal setter) and the `set_subtree_color` seeding call (`:1178`); rework focus-border composition in `present_slider_state` (`:1121`), which under the element-channel outcome means deleting the guard at `:1221-1222`; delete the `:5274` test.
- `src/widgets/visual.rs` — delete `subtree_color` (`:265`, `set_subtree_color` `:272`, getter `:277`), `VisualSlotOverride::color` (`:171`) and its overlay logic in **both** `apply` (`:195`) and `apply_element` (`:209`), the subtree branch of `dispatch_visual_overrides` (keeping its map — see the Spec above for the verified line ranges in that function), and `with_color` (`:220`). Phase 3 added seven `with_color` sites in this file, including tests at `:860`, `:900`, `:940` with assertions at `:1060-1138` that read `VisualSlotOverride::color` — they migrate to `content_color` with the rest.
- `src/render/panel_text/batching.rs`, `src/render/panel_text/reify.rs`, `src/render/panel_shapes/batching.rs`, `src/render/analytic_paths/batching.rs`, `src/render/fill_batch.rs:1359`, `src/widgets/tooltip.rs`, `src/widgets/reify.rs` — move remaining `color` consumers to `content_color`. `with_color` has roughly 29 call sites across these seven files; the last three were absent from this list before Phase 3's review.
- `src/layout/builder.rs:1049` — remove the `El<L, WidgetElement<Slider>>::disabled_color` forward.
- `examples/widgets.rs:1162`, `:1200` — author slider parts explicitly.

**Constraints from prior phases:**
- **Phase 7:** `content_color` exists on `Appearance` and `VisualSlotOverride` and is consumed by the text, image, and `PanelDraw` routes. Both `color` and `content_color` have been alive simultaneously since Phase 7; this phase removes `color`.
- **Phase 10:** every state always resolves to something under the cascade, which is exactly why "a disabled bundle exists" cannot gate the focus border. The resolved override reaching the thumb is an element override composed on top of the authored slot baseline (Phase 3), so the presentation-owned `offset` is already preserved unconditionally. **This phase's focus-border scope depends on Phase 10's pending decision** — under the element channel the composition is inherited from `apply_element` and the work is a deletion; under the root slot it must be written by hand. Read Phase 10's resolved decision first.
- **Phase 4:** the slider's track, thumb, and label can carry their own bundles as `El<L, WidgetPart>`, which is what the example migration uses. The role is monomorphic; the `Slider` owner comes from the enclosing `WidgetBuilder<'_, Slider>`.
- **Phase 1:** a state verb **replaces** the whole bundle for its state — a second `hovered(…)` on the same element discards what the first authored. The example migration below authors several properties per state per part; each state must be built as one `Appearance` and passed in a single call, never as chained calls that each name one property. That chained form worked before Phase 1 and silently drops all but the last bundle now.
- **Phase 2:** structural containers are excluded from the recipient list, so the example's resolved overrides cover exactly root, track, thumb, and label.
- **Phase 4 — declaration order is forced.** `button`, `slider`, `widget`, and `editable_field` live in `impl<L> El<L, LayoutOnly>` (`layout/builder.rs:738-839`), so `El::new().disabled(...).slider(...)` does **not** compile. A widget root must declare its widget before any state verb; the example migration has to be written that way round.
- **Phase 4 — a part-authoring helper cannot be generic over the builder.** `LayoutContentBuilder::with` takes `El<L, LayoutOnly>` (`layout/builder.rs:1327`), so a helper that authors parts must take `&mut WidgetBuilder<'_, W>` for a concrete owner. `tests/trybuild/pass/typestate_helpers.rs::add_widget_content` is the worked example.

**Pending decision:** whether `SliderFocusedThumbBorderColorRequiresThumbBorder` survives.

Phase 5 abolished the "a state property needs its ordinary declaration" error class — except here. `PanelBuildError::SliderFocusedThumbBorderColorRequiresThumbBorder` is still live: declared at `panel/builder.rs:68`, its `Display` row at `:1001`, raised at `layout/element.rs:796` and `:825`, produced at `widgets/slider.rs:5453`. It rejects `Slider::focused_thumb_border_color` when the thumb declares no `El::border` — the same condition, on the same record, that `CommonEl::default_state_surfaces` now handles by synthesizing `Border::all(Px(0.0), Color::NONE)`.

Two options:
- **Delete it** — remove the variant, its `Display` row, both raise sites, and the producer, and let the defaulting cover the thumb like every other element. One authoring rule instead of two.
- **Keep it as a deliberate exception** — a focused thumb border color with no thumb border is arguably a typo rather than a state-only role, and a transparent widened border on a slider thumb is invisible in a way an author would not expect.

Recommendation: **delete it.** A surviving special case in one widget is the kind of inconsistency the codebase-consistency rule exists to prevent, and the recovery — declare the thumb border with its resting color — is exactly what `Appearance::border_width`'s doc already tells authors. Resolve before dispatching Phase 11.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase removes public API and rewrites doc examples that referenced it, so both doc commands must pass before checkpoint.
- `rg -n 'SliderFocusedThumbBorderColorRequiresThumbBorder' crates/hana_diegetic` matches whatever the pending decision resolved to — nothing if deleted, or the variant plus a test asserting the exception is deliberate. The `subtree_color|disabled_color` grep below does not reach it.
- `rg -n 'subtree_color|disabled_color' crates/hana_diegetic` returns nothing, and `rg -n 'VisualSlotOverride::color|\.with_color\(' crates/hana_diegetic` returns nothing. Both patterns are needed: `with_color` reaches roughly 29 sites across seven files (listed in **Files**), several of them Phase 3's own tests, so an unscoped grep for the bare word does not distinguish "migrated" from "missed".
- `VisualSlotOverride` is back to 144 bytes, asserted by the `size_of` assertion Phase 7 introduced — this phase lowers its number rather than adding the first one.
- A **focused × disabled × dragging matrix** is tested for both a background-only disabled bundle (focus border survives) and a border-authoring one (focus border replaced), asserting the thumb `offset` is unchanged in every case and that disabled remains the last normal layer. The matrix includes the pressed/dragging state and the frame that queues `SliderDrag` removal.
- The example's final resolved overrides for root, track, thumb, and label are asserted **exactly** — the headless harness produces no pixels, so visual equality is not a gate.
- Material churn: a compatible label/track/thumb transition causes no batch-key move and no batch entity creation; an incompatible one migrates only the affected retained members.

**Ref corrections (Phase 6 review) — `layout/builder.rs` drifted ~+200:**
- `El<L, WidgetElement<Slider>>::disabled_color` → **`:1251`** (plan says `:1049`)
- `impl<L> El<L, LayoutOnly>` → **`:917`** (plan says `:738-839`)
- `LayoutContentBuilder::with` → **`:1569`** (plan says `:1327`)
- `Text::layout` → **`:297`**
- `widgets/visual.rs`: `VisualSlotOverride::color` **`:173`**, `with_color` **`:222`**,
  `subtree_color` **`:267`**, `set_subtree_color` **`:274`**, getter **`:279`**
- **Add `src/render/image_batch.rs:628`** to the `color`-removal file list. It reads
  `slot_override.color.map_or(tint, linear_tint)` and this phase's
  `rg -n 'VisualSlotOverride::color|\.with_color\('` gate cannot see it — only
  `cargo check` would catch the miss.

### Phase 12 — Stable material keys: no dropped material rows · status: todo

#### Work Order

**Goal:** No frame ever renders a surface whose material row was dropped. The material-table drop path becomes unreachable in normal operation, so `warn_material_table_drops` firing is a defect signal rather than expected growth noise.

**Spec:**

**The defect.** `SdfMaterialSourceKey` (`render/fill_batch.rs:167-175`) identifies a material row by `command_index: CommandIndex` — a slot number in the panel's `LayoutResult::commands` vector (`render/draw_order.rs:30-33`). It is the key into `source_slots: HashMap<MaterialSourceKey, MaterialSlotId>` (`render/material_table.rs:545`), which maps a source to its row in the GPU material table. Because the field is positional, inserting or removing one command shifts every later index, so every later surface presents a key the map has never seen and claims a **fresh** row while its old row is still pinned by `retire_unseen_sources` (`:708-725`, `MATERIAL_SLOT_RETIREMENT_FRAMES = 2` at `:115`). That frame needs 2× the live row count.

The append window is clamped to the **active GPU buffer capacity**, not the device cap: `clear_frame_material_table` (`:1213-1222`) calls `clear_with_active_capacity` (`:786`) whenever a buffer handle exists, and the drop guard at `:617-632` tests `entries.len() < row_limit` against that. Capacity starts at 128 rows and grows one power-of-two step with a **one-frame lag** — `ensure_material_table_buffer_handle` stages into `pending` (`:1265-1268`) and `activate_prepared_material_table_buffer` (`:1205-1211`) promotes it at the start of the next frame. So the frame that first needs the larger buffer is the frame that drops, and every dropped surface renders that frame with `INVALID_GPU_MATERIAL_SLOT` (`:71`).

Measured on a headless probe — 100 sources held constant, keys fully re-keyed at frame 3:

```
frame 2  active_capacity=128  entries=100  live=100  dropped= 0  required=100
frame 3  active_capacity=128  entries=128  live= 28  dropped=72  required=200   <-- drop frame
frame 4  active_capacity=256  entries=200  live=100  dropped= 0  required=200
frame 6  active_capacity=256  entries=200  live=100  dropped= 0  required=200   <-- second re-key, no drop
```

Frame 6 is the evidence that capacity ≥ 2× live makes re-keys free. Frame 3 is the flash.

**Four changes, all required.** Any one alone leaves a reachable drop path.

1. **Key on identity, not position.** Replace the `command_index` field with the element's `PanelElementId` (`ime/ids.rs:85`), already stored on every `Element` as `id: Option<PanelElementId>` (`layout/element.rs:108`) and readable via `element_id` (`:679`). Keep `panel` and `role`.

2. **Make `Auto` ids structural.** `PanelElementId::Auto` is minted from a flat per-build counter — `next_auto_id` declared at `layout/builder.rs:1280`, initialized in **three** constructors (`new` `:1408`, `with_capacity` `:1417`, and Phase 4's shared `from_root` `:1475`, which both `with_root` and `with_widget_root` route through), and minted by `take_auto_id` at `:1551` — so an unnamed element's auto id shifts on insertion exactly as the index did — change 1 alone fixes only *named* elements. Derive the auto id from the element's path through the layout tree instead of build order, so inserting a sibling above an unnamed element leaves that element's id unchanged.

3. **Remove the growth lag.** A cold start with more surfaces than the initial 128-row capacity drops on frame 1 regardless of key stability, and a panel respawn changes `panel: Entity` and re-keys wholesale no matter what changes 1 and 2 do. Either promote a grown buffer in the same frame it is staged, or stop clamping the CPU append window to the active capacity and truncate the *upload* instead at `encode_material_table_upload` (`:1390-1399`) / `padded_rows` (`:509-517`). Both remove the drop; the second costs one frame of stale rows for the overflow.

4. **Widen the growth headroom.** `CAPACITY_HEADROOM_DIVISOR = 8` (`:114`, applied at `:826`) reserves 12.5%; a wholesale re-key needs ~100%. Raise it so a re-key of the current live set fits without growing.

**Named risk — `Named(String)` in a per-frame hash key.** `PanelElementId::Named` holds a `String` (`ime/ids.rs:87`). After change 1 that string is hashed once per SDF surface per frame in the render path, where the current key hashes a `usize`. Intern element ids to a `u32` handle for the render-side key, or measure and accept the cost — do not ship an unmeasured `String` hash into the per-frame loop.

**Drop-count amplification (verify, do not assume).** `append_sdf_record_materials` (`render/fill_batch.rs:998-1034`) is atomic per surface: if `Border` hits the limit after `Fill` succeeded it calls `rollback_assignments_after` (`:1010`, `:1026`), returning the slot to `retired` with `reusable_at_frame: self.frame` — immediately reusable (`:665-668`). At the limit the next surface claims that freed slot for `Fill` and fails on `Border`, so `dropped_records` may increment once per surface rather than once per missing row, inflating the warned number. This is inferred from the code, not measured. Confirm or refute it while writing the zero-drop tests; if real, the gate below still holds, since the target is zero.

**Files:**
- `src/render/fill_batch.rs:167-175` — `SdfMaterialSourceKey`: `command_index` → element identity. `:998-1034` — the paired Fill/Border append and its rollback path; all key construction sites move with the field.
- `src/render/material_table.rs` — `:114` `CAPACITY_HEADROOM_DIVISOR`; `:509-517` `padded_rows`; `:545` `source_slots`; `:617-632` the drop guard; `:786` `clear_with_active_capacity` and `:1213-1222` its caller; `:1205-1211` / `:1265-1268` the stage-then-promote lag; `:1390-1399` upload encoding; `:1417-1425` `warn_material_table_drops`.
- `src/layout/builder.rs:1280` (declaration), `:1408`/`:1417`/`:1475` (the three constructors), `:1551` (`take_auto_id`) — auto-id minting becomes structural.
- `src/layout/builder.rs:1826-1845` — **a second minting path the original Work Order missed.** `LayoutTree::tooltip_add_text` mints `PanelElementId::auto` from a caller-held counter; structural auto ids must cover it or tooltip content keeps positional ids.
- `src/ime/ids.rs:85-103` — `PanelElementId`; add the interned render-side handle if that is the chosen answer to the named risk.
- `src/layout/element.rs:108`, `:679` — element id storage and accessor.
- `src/render/draw_order.rs:30-33` — `CommandIndex` loses this consumer; delete it only if no other consumer remains.

**Constraints from prior phases:**
- **Independent of phases 1-11.** This is a render-layer defect in material-row identity; no widget appearance behavior depends on it and it gates none of the earlier phases. It is sequenced last because Phase 11 edits `render/fill_batch.rs:1359` and the seven-file `content_color` migration, and this phase should start from that settled tree.
- **Phase 11:** `VisualSlotOverride::color` is gone and all consumers read `content_color`. Do not reintroduce a `color` read while touching the batching files.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **Zero drops under wholesale re-key.** A headless test drives a full re-key of a live source set across several frames and asserts `dropped_record_count() == 0` on **every** frame, including the re-key frame. The existing `probe_rekey_drop` test in `render::material_table::tests` is this test's starting point — it currently *demonstrates* the 72-row drop; it is promoted to a regression test asserting zero.
- **Zero drops on cold-start growth.** A test whose first frame requires more rows than the initial capacity asserts zero drops on that first frame — this is the case key stability alone does not cover.
- **Zero drops on panel respawn.** A test that despawns and respawns a panel with the same content asserts zero drops across the transition, since the `panel: Entity` field re-keys every row regardless of element identity.
- **Element identity survives insertion.** Inserting an element above an existing **unnamed** element leaves that element's resolved material key unchanged. Assert on the key, not on the row count — a stable row count with shifted keys passes by accident.
- **Warn reachability.** `warn_material_table_drops` (`:1417-1425`) stays a `warn!`, not `warn_once!`. With drops unreachable, one firing is a defect, and demoting it would hide the regression this phase exists to prevent.

**Ref corrections and added constraints (Phase 6 review):**
- `layout/builder.rs` grew +206 lines in Phase 6. Corrected: `next_auto_id`
  declaration `:1280` → **`:1486`**; the three constructors `:1408` / `:1417` /
  `:1475` → **`:1614`** / **`:1623`** / **`:1681`** (their `next_auto_id: 0` inits at
  `:1614` / `:1639` / `:1693`); `take_auto_id` `:1551` → **`:1757`**;
  `tooltip_add_text` `:1826-1845` → **`:2032-2046`**. `layout/element.rs:108` / `:679`
  and the `render/material_table.rs` refs are unaffected.
- **Add the editor content tree as the proving case.** `inline_editor_content_tree`
  (`ime/editor.rs`) is now the highest-churn auto-id generator in the crate: it is
  rebuilt per keystroke with a varying element count (empty runs skipped, selection
  box present or absent, caret always, validation conditional), so every unnamed
  element after it re-keys on each edit. `EditorPart::into_text` sets
  `common.id = None` (`layout/builder.rs:516`), so an author cannot stabilize them
  with a `Named` id. This phase's Files names only the three `LayoutBuilder`
  constructors and `tooltip_add_text`; add this path as the case that proves
  structural ids actually work.

## Outstanding items

<!-- Project state outside the phase spine. Not dispatched by /plan:delegate. -->

- **Uncommitted work.** Three rounds sat uncommitted on `feature/widgets` at `2f12a56d` — the `apply_state_appearance` / `_with` renames, the editable-field state fix (hover and disabled present on fields; `pressed_*` gated behind `HasPressedState`) with four new tests and a trybuild case, and the `HasPressedState` doc comment. These landed as `64f8bdc0`, which is current `HEAD`.
- **`docs/hana_diegetic/widgets.md`** — done. Rewritten as `docs/hana_diegetic/as-built/widgets.md`, current-state only (state appearance described as the four `Appearance` verbs, not the removed flat builders), and the old phased plan deleted. Inbound links in `surface-panels.md` and `widgets-deferred.md` repointed.
- **Widget demonstration checkpoint.** The retired widget plan ended with an undelivered discussion phase: decide with the owner how to demonstrate the whole widget system working together — buttons, sliders, tooltips, focus traversal, disabled state, panel ordering, and IME/text input coexisting on one panel — and name both the live demonstration and the deterministic integration gate, including the tooltip's final retained transform after first reveal and after a replacement creates a fresh controller. `examples/widgets.rs` is the cumulative baseline; do not reopen which example owns that path, remove either input-integration proof, replace the diagnostic rows, or change the established picking policies.
- **`WidgetElement<ImeEditableFieldSpec>`** — settled by Phase 4's `EditableField` marker.
- **`HasPressedState`** — renamed to `Pressable` in Phase 4. Resolved; no longer outstanding.
