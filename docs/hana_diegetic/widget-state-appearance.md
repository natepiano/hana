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
  - `src/layout/builder.rs` (**2299** lines) — **every line ref in this bullet is Phase-2 vintage and superseded.** Phase 11 inserted `EditorStateColors` (`:120`) and `PressedEditorStateColors` (`:133`) near the top, shifting everything below by **+121**. Current values: sealed `ElementRole` **`:235`**; `impl<L> El<L, LayoutOnly>` **`:1065`** with the four state verbs at **`:1069`/`:1078`/`:1087`/`:1096`**; `El::editable_field` **`:1109`**, `El::button` **`:1125`**; `El<L, WidgetElement<W>>::disabled` **`:1207`**; `El::disabled_color` **`:1399`**; `AcceptsElement` **`:1696`** (which has **no `with` method**) and its five impls **`:1987`/`:2000`/`:2014`/`:2031`/`:2048`**; `LayoutContentBuilder::with` **`:1718`**; `LayoutBuilder::with` **`:1862`**; `WidgetBuilder::with` **`:2090`**; `next_auto_id` **`:1635`** with `take_auto_id` **`:1906`**; `tooltip_add_text` **`:2181`**; `EditorPart::into_text` **`:644`**. The historical inventory follows — `El<L, Role>`; roles `WidgetPart` (`:105`), `PressedPart` (`:109`), sealed `ElementRole` (`:112`); owner kinds `WidgetOwner` (`:127`), `Widget` (`:143`), `Pressable` (`:187`); `El::editable_field` (`:814`); the four state verbs in four blocks — `El<L, LayoutOnly>` (`:740`-`:765`, `pressed` yields `PressedPart`), `El<L, WidgetElement<W>>` (`:853`-`:873` plus `pressed` at `:888` under `Pressable`), `El<L, WidgetPart>` (`:898`-`:923`, `pressed` upgrades the role), `El<L, PressedPart>` (`:933`-`:958`, all role-preserving); `El::disabled_color` (`:1049`); `WidgetBuilder<'a, W>` (`:1240`); `AcceptsElement` (`:1295`) and its five impls (`:1582`-`:1658`); `LayoutContentBuilder` (`:1314`); `LayoutBuilder::with_root` (`:1455`), `with_widget_root` (`:1467`), `with` (`:1460`); `WidgetBuilder::with` (`:1685`); `Text::layout` (`:265`).
  - `src/layout/element.rs` — `CommonEl`/`Element`, `appearance` field (`:148`); `LayoutTree::validate_widgets` (`:782`, walk body `:793-846`), the **only** appearance-reachable walk that returns `Result<_, PanelBuildError>`, calling `validated_element_widget_owner`; `computed_widget_records` (`:849`, returns `Vec<ComputedWidgetRecord>` — **no `Result`**) and its owning-record walk (`:914`) calling `record_owned_widget_element` (`:1380`) and `element_visual_capabilities` (`:1319`); `set_field_editing_content` (`:1033`); `validated_element_widget_owner` (`:1289`); `classify_element_change`'s exhaustive `Element` destructure (`:1398`); `set_element_state_appearance` (`:475`, `#[cfg(test)]`). **`PanelBuildError::WidgetContainsInteractiveDescendant` is gone** — Phase 4 removed the variant, its producer, and its two tests; nesting is now a compile error. **The four `PanelBuildError::State*` variants and `validated_element_appearance` are gone** — Phase 5 replaced them with `CommonEl::default_state_surfaces` (`layout/builder.rs`), which emits a transparent fill or border at element construction so a state property always has a record to replace.
  - `src/layout/draw.rs:11` — `PanelDraw`. `src/layout/line.rs:42` — `PanelShape` enum; `PanelCircle` struct at `:64`.
  - `src/ime/editor.rs` (1972 lines) — `inline_editor_content_tree` **definition at `:1146`** (the earlier `:665` / later sites are callers/helpers, not the def).
  - `src/widgets/appearance.rs` — `VisualChange<T>` (`:26`); `Appearance` (`:107`, derives `PartialEq`, **six** `VisualChange` fields since Phase 7: `background`, `border_color`, `border_width`, `text_color`, `path_color`, `material`) with its impl block (`:119`); `merge_over` (`:140`); `impl From<Color> for Appearance` (Phase 11) = `Self::new().background(color)`, documented at `:229-231`; the four `Widget*Appearance` wrappers — each now its own `CascadeRootResource<Self>` root resource with a **public** `new` taking `impl Into<Appearance>` (`:251`/`:294`/`:337`/`:380`), a **module-private** `appearance()` accessor (`:245`/`:287`/`:329`/`:371`), and size assertions at `:270`/`:312`/`:354`/`:396`; `StateAppearance` (`:406`, **not a `Component`**), `cascades()` (`:415`); `WidgetStateCascades<'a>` (`:421`) with `any_overridden` (`:445`), `layer` (`:452`), `any` (`:474`), `resolve` (`:489`); `WidgetState` (`:540`), `LAYER_ORDER` (`:555`). **`layer`/`any`/`resolve` live on `WidgetStateCascades`, not on `StateAppearance`.** **Phase 3 DELETED both `layer_onto` methods** — per-property layering is now inlined in `resolve`, which matches `VisualChange::To` per property inside the `LAYER_ORDER` loop (`:500`) and constructs the `VisualSlotOverride` directly.
  - `src/widgets/visual.rs` — `VisualSlotOverride` (`:172`) with the generic `color` field (`:174`), plus the two Phase 7 role-scoped fields `text_color` and `path_color`, then `fill_color` / `border_color`; the size assertion is `assert!(size_of::<VisualSlotOverride>() <= 184)` (`:199`) — an **upper bound**, so it cannot catch a field being dropped. **`apply` (`:202`) is the only method that names color fields**; `apply_element` (`:218`) saves `offset`, delegates to `apply`, and restores `offset`, so a new field is added to `apply` alone. `with_color` (`:231`) is `#[cfg(test)]`. `WidgetVisualSlots` (`:82`) with `with_elements` (`:99`) / `with_part_appearances` (`:108`) / `elements()` (`:120`) / `part_appearances()` (`:124`, **no longer `#[cfg(test)]`**). `WidgetVisualOverrides` (`:287`), `subtree_color` field (`:288`) / `set_subtree_color` (`:295`) / getter (`:300`), **`set_element` (`:341`)** and **`element_overrides` (`:361`)** — the Phase 3 element-index-keyed channel. `resolve_part_overrides` (`:390`) with its two `continue`s (`:406`/`:409`) and default-drop filter (`:413`). `write_widget_overrides` replaces the whole component and **compares immutably first, returning without writing when the resolved value is unchanged**. `dispatch_visual_overrides` (`:527`); its **subtree-seeding branch (`:556-568`) is the sole writer of the generic `color`** and since Phase 7 writes `color`, `text_color`, and `path_color` together — dropping any of the three silently un-dims `Slider::disabled_color`, which is why `slider.rs:5305`/`:5336` now assert all three. **`write_slot_override` was DELETED in Phase 3** — all writes go through `write_widget_overrides`.
  - **The three presenters (rewritten in Phase 3).** `presentation_inputs_changed` was **DELETED** in all three; each presenter now builds its own kind-filtered dirty-entity set from `Changed`/`RemovedComponents` terms it owns directly, and writes the whole component via `write_widget_overrides`. `src/widgets/button.rs` — `present_button_state` (`:139`), `Changed<WidgetVisualSlots>` dirty term (`:149`), write (`:232`). `src/widgets/editable.rs` — `present_editable_state` (`:30`), `Changed<WidgetVisualSlots>` (`:40`), write (`:121`). `src/widgets/slider.rs` — `present_slider_state` (`:1141`), `Changed<WidgetVisualSlots>` (`:1152`), subtree seeding (`:1178`), write (`:1202`); `disabled_color` field `:172` / default `:191` / builder `:233` / crate-internal setter `:255`. **`ButtonPress` is an `Or<>` term in the button presenter but not the slider's** — inserting/removing it on a slider wakes exactly one presenter, the cross-kind isolation discriminator.
  - `src/widgets/id.rs` — `WidgetKind` (`:98`), `VisualElementCapabilities` bitflags (`:115`), `ComputedWidgetRecord` (`:138`) with `appearance` field (`:143`) and `part_appearances` (`:144`), `appearance()` (`:188`), `push_visual_element` (`:208`), `part_appearances()` (`:220`), `push_part_appearance` (`:222`). **Phase 7 SPLIT the single `CONTENT` bit** into three: `TEXT` (`:123`, `1 << 3`), `IMAGE` (`:125`, `1 << 4`), `DRAW` (`:127`, `1 << 5`). One bit could not express "material accepts text and draw but rejects image-only". Any phase adding a per-role property adds a capability bit alongside it — the part-local build error has nothing to test without one.
  - `src/widgets/reify.rs` — `reify_widgets` (`:184`, gated on `Changed<ComputedDiegeticPanel>` at `:194`), its existing-widget query (`:196-211`), `spawn_widget` (`:296`), `update_widget` (`:352`) with the `WidgetVisualSlots` inequality guard (`:445`), `update_widget_appearance` (`:482`).
  - `src/widgets/mod.rs` — `WidgetSystems` enum (`:143`), ordering `Reify → ReifyCommandsApplied → ResolveInteractivity → InteractivityCommandsApplied → Focus → SemanticInput → FocusCommandsApplied → PresentationCommandsApplied`; `WidgetsPlugin` (`impl Plugin` `:223`) with `add_plugins` (`:233-238`) including `cascade::cascade_plugin::<WidgetInteractivity>()` (`:234`) and, since Phase 9, the four `cascade::cascade_plugin::<Widget*Appearance>()` lines (`:235-238`), `configure_sets` (`:242`), `add_systems` where the three presenters are registered — `present_button_state` (`:303`), `present_editable_state` (`:306`), `present_slider_state` (`:309`) — **with no `.run_if(...)` on any of them**, since Phase 3 moved the change detection into the systems themselves; `mod appearance;` stays **private** (`:1`) — the public surface comes from the `pub use appearance::…` re-exports, so no phase needs `pub mod` here.
  - `src/cascade/mod.rs:44` — `cascade_plugin<A: CascadeRoot>()`.
  - `src/widgets/interactivity.rs` (529 lines) — `Cascade<WidgetInteractivity>`, the pattern every cascade step mirrors.
  - `src/cascade/attributes.rs` (353 lines) — `CascadeEntityCommandsExt` (`:30`), `resolved_*` fns (`:223-322`), `apply_cascade_override` (`:326`), `remove_cascade_override` (`:336`), `resolved_cascade` (`:345`). `src/cascade/constants.rs:7` — `CASCADE_ATTRIBUTE_BYTES: usize = 32`. `src/cascade/resolved.rs` (177 lines) — `cascade_attribute!` (`:20`), `SdfMaterial`/`TextMaterial`/`ShapeMaterial` (`:112`/`:125`/`:138`) with their per-attribute size assertions at `:118`/`:131`/`:144`, `CascadeRoot` (`:175`).
  - `crates/bevy_kana/src/cascade.rs` (676 lines) — `Cascade<T>` (`:23`); `resolve_cascade` (`:146`) and `resolve_cascade_ref` (`:161`), unbounded-generic public helpers with **no `hana_diegetic` call site** (only the `:502` unit test and the `lib.rs:41-42` / `prelude.rs:36-37` re-exports); **`CascadeAttribute` trait def (`:174`) with a blanket impl over its bounds (`:179`) — this is why a per-type method override is impossible**; `CascadeFrom` (`:197`), `CascadeRootResource<A>` (`:248` — added by the `main` merge at `43d3cba8`; `CascadeDefault<A>` still exists in `bevy_kana` but **no longer appears anywhere in `hana_diegetic`**), `Resolved<A>` (`:265`, a tuple struct — **access is `.0`, it does not `Deref`**; the only `Deref` in this file is `CascadeChildren` at `:229`), `CascadeSet` with `Propagate`, `CascadePlugin<A, R = CascadeDefault<A>>` (`:285` — **two** type params now) with `new` (`:294`), `with_combine` (`:307`), `with_root_resource` (`:314`), and `Plugin::build` (`:337`, root-resource insert `:338-340`, `CascadeCombine` insert `:341-343`, both **only when the resource is absent**), `resolve_entity_cascade` (`:398`), `resolve_inserted_cascade` (`:413`), `propagate_cascade` (`:435`, its `Resolved<A>` removal branch `:467-472`, its `resolve_from_queries` call `:474`), `resolve_from_queries` (`:500`).
  - `src/panel/builder.rs` — `PanelBuildError` (`:50`), `SliderFocusedThumbBorderColorRequiresThumbBorder` (`:73`, `Display` row `:1055`), `StateTextColorRequiresText` (`:76`), `StatePathColorRequiresDraw` (`:79`), `BuilderData` (`:194`), the four panel-level state verbs (`:422`/`:430`/`:438`/`:446`). `src/panel/diegetic_panel.rs` — `replace_from_precompose_helper` (`:481`), `seed_panel_overrides` (`:1637`). `src/panel/lifecycle.rs` — `PanelCascadeOwnership` (`:122`), `teardown_owned_shared_state` (`:782`). `src/panel/mod.rs` — `add_cascade_ownership_observers!` defined (`:175`) and invoked (`:185`) inside the private helper Phase 9 extracted to keep `build` under clippy's 100-line limit; `HeadlessLayoutPlugin` (`:219`, `impl Plugin` `:221`), which registers the five **panel** attribute cascades explicitly because `RenderPlugin` is absent. **It does not register the four `Widget*Appearance` channels** — those are `WidgetsPlugin`-only, and a second registration is a `DuplicatePlugin` panic.
  - `src/render/fill_batch.rs` — `apply_sdf_visual_override` (`:1358`), which reads `fill_color.or(color)` at `:1364` and `border_color.or(color)` at `:1369`. Those two lines plus `render/image_batch.rs:628` and `widgets/visual.rs:230` are the **only four** readers of `VisualSlotOverride::color` in the crate. `src/render/panel_text/batching.rs` — cascade-resolution block (`:288`), `apply_routed_text_run_update` (`:430`); it reads no `VisualSlotOverride` color field. `src/render/batch_store.rs` — `BatchStore::upsert` (`:201`). `src/render/analytic_paths/batching.rs` — `TextRunBatch::rebuild` (`:314`).
  - `src/lib.rs` — crate-root `pub use widgets::*` block (`:346-410` after Phase 4's eight new `layout::` exports). Phase 1 added `Appearance` and the four `Widget{Hovered,Pressed,Focused,Disabled}Appearance` wrappers; a later phase adding a public **widget** symbol extends this block. A public **error** type goes with `PanelBuildError` in the `panel::` block at `:238` instead.
  - `examples/widgets.rs` (1702 lines) — `.disabled_color` use (`:1164`), `add_slider` (`:1204`), `apply_state_appearance` (`:1458`).
  - `tests/headless_widgets.rs` (131 lines) — external-client integration test; no state-appearance coverage today.
  - `tests/trybuild.rs` — the driver, and the **only** place a fixture becomes reachable. It declares **one** test, `widget_state_and_tooltip_typestate_signatures_compile`, carrying four `compile_fail` globs — the `overlay_*` glob moved into it in Phase 4 — plus all three `pass/` fixtures. **That test is `#[ignore]`** (commit `aeb8ac55`: 89-second compile, CI runs ignored tests as a separate job), so `verify.sh test hana_diegetic trybuild` reports `0 run / 0 passed / 1 skipped` and proves nothing. Run `cargo nextest run -p hana_diegetic --test trybuild --run-ignored all` and require `1 passed`. **Do not remove the `#[ignore]`.** (An earlier revision of this line claimed the test carried no `#[ignore]` — that was wrong.) **A fixture whose filename matches no existing glob is never compiled and its acceptance-gate line is vacuous** — any phase adding fixtures must list `tests/trybuild.rs` in its **Files** and add or widen a glob. `tests/trybuild/pass/` — `tooltip_typestate.rs`, `typestate_helpers.rs`, `widget_state_appearance.rs`. `tests/trybuild/fail/` — **21** fixtures (Phase 11 added `editable_widget_editor_part_has_no_property` and `editable_widget_editor_part_rejects_pressed_colors`, both prefixed `editable_widget_` so the existing glob matches); `editable_widget_has_no_pressed_state.{rs,stderr}` now proves an editable field's *part* rejects a pressed layer: `.rs:15` is the `with` insertion and `.stderr:1` reports `error[E0277]: the trait bound `EditableField: Pressable` is not satisfied`.

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
  - **An accepted option must reach the runtime.** No phase may ship a builder whose value is validated and then discarded; if a combination cannot present, it is gated out of the type surface or it is not offered. *How it is carried, after Phase 5:* for `background`, `border_color`, `border_width`, and `material`, by **record synthesis** — `CommonEl::default_state_surfaces` (`layout/builder.rs`) emits the transparent record the state replaces, so there is nothing to reject and no appearance validation runs at panel build. It is **not** carried for an explicitly empty bundle (Phase 8's open question) For `text_color` and `path_color` it is carried by a **build error**: their recipients — text and `PanelDraw` — cannot be synthesized, so a part naming a color it structurally cannot present is rejected at panel build (Phase 7, resolved 2026-07-29). *Scope limit:* this binds **part-local** authoring. A global root-resource insert or a runtime entity command cannot promise a present recipient — a higher-level property with no compatible record at some element is **dormant** there, not an error.
  - **Every level merges into the one above it, property by property.** Global default → panel → widget → part. A level that names a property wins for that property; a level silent on a property takes the value from above; a property nobody names stays at the ordinary look. A global default of `{background: GRAY, text_color: DIM}` plus `.disabled(Appearance::new().border_color(RED))` on one slider resolves to gray, dim, *and* a red border. Silence means "no opinion," not "leave me alone": a level that must hold its ordinary look against an inherited bundle names the ordinary value explicitly, and `.disabled(Appearance::new())` is a no-op rather than a way to clear an inherited look.
  - **Four levels, and no element-tree inheritance.** "Four levels" names the levels *this plan authors*, not a hard ceiling: Phase 8 replaced first-override-wins with a fold over the whole `CascadeFrom` chain up to `CASCADE_DEPTH_LIMIT` = 64 (`bevy_kana/src/cascade.rs:13`), so any entity a panel cascades from — an application-owned source, for instance — is a real merging level with the same per-property rule. Nothing changes for a panel that cascades from nothing. Ruled 2026-07-29; the analysis is recorded in this invariant and the note below it. A property applies only to the element it is written on and to elements reached by the four named levels above. A color written on a container element does **not** flow to its child elements by tree position. This is deliberate: the widget level already reaches every owned element at unbounded depth (`layout/element.rs:857-930`), so tree inheritance would add only the intermediate-container case, at the cost of making applicability depend on position — which no typestate can check, and which this repo has already paid for twice (`Slider::disabled_color`'s hand-written focus-border suppression at `slider.rs:1221`, and the generated-caret exclusion in Phase 10). Do not reopen this in a review pass. Phase 10's ordered-slice reduction is the insurance that keeps the option cheap if it is ever revisited.

    *The prior art, recorded accurately so the wrong lesson is not drawn later:* CSS's reputation for being hard to reason about belongs to **specificity and `!important`**, not to inheritance. The CSSWG's own published list of design regrets does not name inheritance, the cascade, or specificity; State of CSS 2025 and 2026 do not rank them as top pain points; and StyleX, which banned every other form of action-at-a-distance, deliberately kept `color` inheritance. A system with no selectors and no specificity — which this is — could take inheritance without taking what made CSS unpleasant. The decision above therefore does **not** rest on "CSS is a cautionary tale." It rests on three things specific to this codebase: the widget level already reaches every owned element at unbounded depth, so tree inheritance would add only the intermediate-container case; this repo has already paid for subtree propagation twice; and position-dependent applicability cannot be checked by any typestate, which is the property the plan owner asked for.
  - **Cascade precedence and state precedence are separate axes, resolved in that order.** First resolve each of the four states independently down the levels (global → panel → widget → part). Only then layer the *active* states in `WidgetState::LAYER_ORDER` = `[Focused, Hovered, Pressed, Disabled]`. Composing active states per level and then resolving levels would let a part's local hovered bundle defeat an inherited disabled bundle.
  - **State appearance only exists inside a widget.** Hover, press, focus, and disabled are widget states; there is no text widget and no hoverable bare element. An element that authors a state look is a *widget part*, and a part is only placeable inside a widget's children.
  - **A state layer replaces values on a retained record; it never authors a missing one.** That is a property of `VisualSlotOverride`, not a constraint on authors: since Phase 5, layout supplies the record. A state `background` with no `El::background` gets a `Color::NONE` fill; a state `border_color`/`border_width` with no `El::border` gets `Border::all(Px(0.0), Color::NONE)`; a state `material` gets a fill only when there is no border record to re-key. `.background(X).disabled(Appearance::new().background(Y))` is still not redundant — the ordinary call is what the element shows at rest. **`text_color` and `path_color` (Phase 7) have no synthesizable record** and are the two properties this does not cover — which is why they are the only ones that can fail at build.
  - **No state property may change solved layout.** Border width changes grow inward and re-key nothing.
  - **Public opaque types, not leaked private ones.** A `pub` trait whose methods mention `pub(crate)` types trips `private_interfaces` even when the methods live on a sealed trait in a private module; E0446 additionally forbids a public trait exposing a private associated type. Every type reachable from a public associated type — `WidgetBuilder`, `WidgetPart`, `EditableField`, the scope token — is a public opaque type with private fields.
  - **Presentation must not dirty `WidgetVisualOverrides` when resolved values are unchanged.** Compare through an immutable query and take `get_mut` only on inequality; comparing inside a method already reached through `Mut<_>` is too late.
  - **A merge or cascade assertion spells out its expected value literally; it never derives it from the function under test.** Writing `assert_eq!(resolved, panel.merge_over(&global))` passes even when `merge_over` is wrong and even when the channel is registered with replace semantics instead of merge. Phase 9 shipped exactly that defect and it survived every gate until blind review; the fix is `widgets/appearance.rs:832`, which names all three expected colors. The same exposure applies to any authoring-equivalence assertion (`.hovered(COLOR)` vs `.hovered(Appearance::new().background(COLOR))`) — compare both against a written-out expectation, not against each other.
  - **A test must be able to fail.** Three distinct shapes of passing-but-vacuous test have now shipped in this plan and been caught only by blind review; every phase writing a delicate assertion must check for all three. (1) *Expectation derived from the function under test* — the bullet above. (2) *Fixture where the required and the forbidden algorithm agree.* Phase 10's cross-level state-order test assigned increasing state precedence at increasing cascade precedence, so "resolve levels then layer states" and the forbidden "layer states per level then resolve levels" returned the same color and the test could not fail; the fix reversed the axes (highest-precedence state at the lowest-precedence level) and named, in a comment on the assertion, the value the wrong algorithm would produce. (3) *Mutation that is not a dirty term for the system under test.* Phase 10's editable pressed-exclusion test toggled `ButtonPress`, which is not one of `present_editable_state`'s change terms (`widgets/editable.rs:27`), so the presenter never re-ran and the absence assertion passed because nothing happened; the fix mutates a real dirty input and asserts, before the absence check, that some other expected override *is* present — proving the system ran this frame. An absence assertion always needs that positive control.
  - **Workspace lints, inherited by both packages** (`[lints] workspace = true` in each `Cargo.toml`): `[lints.rust] missing_docs = "deny"` — every new public item needs a doc comment. `[lints.clippy]` denies the `all` / `cargo` / `nursery` / `pedantic` groups (`priority = -1`) plus `allow_attributes_without_reason`, `expect_used`, `panic`, `self_named_module_files`, `unreachable`, `unwrap_used`. No `.unwrap()` / `.expect()` / `panic!` in non-test code, and any `#[allow(...)]` needs a `reason = "…"`.
  - **Headless only.** No phase needs a GPU, a window, or a screenshot. Assertions are on resolved `VisualSlotOverride` values, `VisualOverrideIndex` membership, batch-key identity, and entity counts — never on rendered color. Harnesses: `HeadlessLayoutPlugin` (`panel/mod.rs:219`) **plus `WidgetsPlugin`** for anything touching the four appearance cascades — `HeadlessLayoutPlugin` alone registers only the five panel attributes, so a widget-appearance assertion under it passes vacuously (precedent: `cascade_test_app` `widgets/appearance.rs:635`, `widgets_test_app` `widgets/visual.rs:661`); a plain `App` with no render device for retained batching (precedent: `fill_batch.rs` 59 tests, `panel_text/batching.rs` 33, `panel_shapes/batching.rs` 31, `material_table.rs` 31); `trybuild` for typestate boundaries. Baseline: `verify.sh test hana_diegetic` reports **1175 passed / 2 skipped** at Phase 11 completion (was 1172 at Phase 10, 1156 at Phase 9, 1149 at Phase 8, 1146 at Phase 6, 1107 at Phase 2). A bare `cargo nextest run -p hana_diegetic` reports a different, higher number because it selects more targets — measure with the `verify.sh` line only. Measure with that command, not by counting the workspace — a phase's gate covers this package only. **No phase may land with a lower test count than it inherited.**
  - **A trybuild gate must run the ignored test explicitly.** `verify.sh test hana_diegetic trybuild` reports `0 run / 0 passed / 1 skipped` and exits "no tests to run", because the sole test in `crates/hana_diegetic/tests/trybuild.rs` carries `#[ignore]` — deliberate repo policy, since it takes 89 seconds and CI runs ignored tests separately. **Do not remove that `#[ignore]`.** Any phase whose acceptance depends on a compile-fail fixture must instead gate on `cargo nextest run -p hana_diegetic --test trybuild --run-ignored all` and require **1 passed**; the `verify.sh` spelling proves nothing. Discovered in Phase 11.
  - **Carve-out for rendered appearance (added 2026-07-29).** The automated gate stays headless; that is unchanged. But a phase whose changes alter what the example *looks like* also gets a **live smoke test run by the plan owner** before checkpoint — currently phases 7, 10, and 12, each carrying an explicit gate line. Phase 6 is the precedent: lint, 1130 tests, trybuild, and the example build all passed while the focused field rendered as an opaque black bar, because every assertion was on override state and none on pixels. The headless proxy assertion catches the capability/override half of that class without a GPU and remains required; the smoke covers the half it structurally cannot. Driving is **keyboard only** (`brp_extras_send_keys` / `brp_extras_type_text`) — no BRP mouse control while the plan owner is at the machine.

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

**Correction (recorded at Phase 8):** this Work Order's Spec justified storing `Cascade` on the grounds that an explicitly empty bundle "must suppress an inherited bundle." That rationale did not survive. The approved reading is that an empty bundle is a **no-op** — `.hovered(Appearance::new())` under a panel authoring a hover background resolves to the panel's background, and Phase 3 shipped that behavior (`widgets/visual.rs:413` drops any resolution equal to `VisualSlotOverride::default()`). The stored `Override`/`Inherit` distinction is retained anyway: it is free, and an explicit clear token would build on it. The archived Spec above is left unedited as history; this note governs.
- The residual scan returns nothing, and the new compile-pass fixture exercises all four properties across all four states for both a button and a slider.

**What deviated from the plan:**
- `widgets/mod.rs:1` stayed `mod appearance;` rather than becoming `pub mod`. The Files entry offered either that or targeted `pub use`; the re-exports alone are sufficient, since `widgets` is itself a private module and `pub mod` would have widened nothing.
- Clippy required `Appearance::new`, `background`, and `border_color` to be `const fn`. `border_width` and `material` cannot be.

**Surprises:**
- **The phase acceptance gate cannot catch a broken doc link.** `verify.sh` has no rustdoc verb, so a public item linking to a `pub(crate)` type passes `check`, `test`, and `lint` and only fails much later, at the workspace doc lint. Phase 1 shipped exactly that defect (`Appearance` linking to the crate-private `VisualChange::Unchanged`); the blind reviewer caught it by reading. Every remaining phase that adds public API has the same hole.
- **The removed builders accumulated; the new verbs replace.** `hovered_background(a)` followed by `hovered_border_color(b)` produced one layer carrying both, whereas a second `hovered(…)` discards the first bundle. Migrating two chained calls into two chained calls silently drops the first. Every migrated call site was merged correctly, and the four verbs now document the replacement, but the plan did not name this hazard.
- The test-count floor stated during dispatch was wrong. The package runs 1102 tests (1100 passed, 2 skipped) — see the baseline note in Delegation Context. Measure with `verify.sh test hana_diegetic`, not by counting the workspace.

**Implications for remaining phases:**
- Phases 4, 7, 9, 11, and 12 all add public API and inherit the doc-link blind spot. Until a rustdoc step exists, treat public-item intra-doc links as review-only, or run `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p hana_diegetic` before the checkpoint.
- Phase 10 can rely on the authored-vs-absent distinction being both stored and tested; it does not need to reconstruct it.
- Phase 7's fifth property must be added to the hand-written `PartialEq` content comparison as well as to the struct — the `Arc` keeps the size assertion satisfied, so nothing else there changes.

### Phase 1 Review

- **Phase 2** re-scoped: the four root `Cascade` values it listed as work already exist as `ComputedWidgetRecord.appearance` (`id.rs:127`), populated by the ownership walk. Phase 2 now adds only the sparse part map.
- **Phase 2** inherited a `**Pending decision:**` on how the four widget-level bundles sit on the widget entity — one aggregate component or four standalone ones. Phase 1 inserts one aggregate (`reify.rs:322`), but `propagate_cascade` only sees standalone `Cascade<A>` components and strips `Resolved<A>` without them, so Phases 9 and 10 could not work as written. Raised at Phase 2 rather than Phase 9 because Phase 3 rewrites all three presenters against whichever shape wins. **Resolved 2026-07-28: dissolve the aggregate** — `StateAppearance` loses its `Component` derive and the entity carries the four channels exclusively, landed in Phase 2. Phase 10's entity-shape bullet moved here with it.
- **Phase 3** now points at the Phase 2 decision before rewriting the presenters, and its `presentation_inputs_changed` reference moved to `slider.rs:1137`.
- **Phase 4** must now list `tests/trybuild.rs` in its Files: the driver's globs are what make a fixture reachable, none of them matches Phase 4's four new fail fixtures, and its compile-pass additions sit behind an `#[ignore]`d test — so four of its acceptance-gate lines would have passed while compiling nothing.
- **Phase 4** and **Phase 13** gained the replacement-not-accumulation constraint: a state verb replaces the whole bundle, so chained single-property calls silently drop all but the last.
- **Phase 8** carries a `**Pending decision:**` on whether an explicitly authored empty bundle suppresses an inherited one. The document currently says both — Phase 1's archived Spec says suppression, the invariant and Phases 8 and 10's gates say no-op.
- **Phase 8** now says to write `merge_over` as a thin owned wrapper over the `layer_onto` fold Phase 1 shipped, and drops the suggested `VisualChange::or`, which would have been a third copy of the same per-property rule.
- **Phase 7** gained a `size_of` assertion for `VisualSlotOverride` at the size it grows the type to, so Phase 13's "back to 144 bytes" is a verified delta rather than a first measurement.
- **Phase 10**'s `resolve` entry now says what actually changes — where the four layers come from — rather than implying the layering algorithm is rewritten.
- **Delegation Context** gained a **Docs** entry: `verify.sh` has no rustdoc or doctest verb and `cargo nextest run` does not execute doctests, so a public item linking a crate-private type passes every gate. Phase 1 shipped exactly that defect. Phases 4, 5, 7, 9, and 12 now carry an orchestrator-run docs gate line.
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

`dispatch_visual_overrides` already builds a `HashMap<usize, VisualSlotOverride>` (`visual.rs:491`) — the one Phase 13 deletes along with `subtree_color`. The element channel **merges into that existing map**; do not introduce a second one.

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
- Phase 13 is unaffected, but its instructions were not: the element channel merges into the existing `HashMap<usize, VisualSlotOverride>` built at `visual.rs:512`, which now serves three producers — subtree seeding (`:513-523`), slot overlays (`:524-532`), and the element channel (`:533-538`). Phase 13 therefore **keeps** the map and deletes only the subtree branch, not the map itself.
- The remove-the-component-and-assert-non-reinsertion pattern is the reusable isolation detector for every later phase that must prove one widget does not wake another's presentation.
- The element channel's per-property composition (`apply_element`) is strong enough that Phase 13's focus-border rework may reduce to a deletion — but only if Phase 10 routes the widget level through that channel rather than the root slot, which Phase 10 did not specify. Deferred there as a pending decision.

### Phase 3 Review

Two architect passes covered phases 4-7 and 8-10 plus 12 against the shipped code. Twenty-three findings; all applied, none rejected.

**Delegation Context.** Rewrote five bullets — `appearance.rs`, `visual.rs`, the three presenters, `id.rs`, `mod.rs` — against verified line numbers. Records the deletions (`presentation_inputs_changed`, `write_slot_override`, both `layer_onto` methods), that `part_appearances()` is no longer `#[cfg(test)]`, that the presenters carry no `.run_if`, and that `ButtonPress` is an `Or<>` term in the button presenter only, making it the cross-kind isolation discriminator.

**Phase 4.** Named the element-index ordering invariant as a constraint and a gate. Corrected the interactive-descendant guard refs to `layout/element.rs:785`/`:788`. Added the label escape hatch: a text label has `CONTENT` capability but emits no SDF record, so a bundle carrying only the four Phase 1 properties passes the empty-mask check and still presents nothing until Phase 7 — the gate must exercise that explicitly instead of assuming a bare label presents.

**Phase 6.** Shaped the four editor-part authoring inputs as fluent methods rather than `editable_field` parameters, preserving three call sites and the locked trybuild diagnostic. Recorded that `Changed<WidgetVisualSlots>` is already a dirty term in all three presenters, so the regenerated editor tree re-resolves on its own — no wake source or transition observer. Added a gate asserting the four generated parts are emitted in ascending element-index order.

**Phase 7.** Extended its `visual.rs` work to both `apply` and `apply_element`; omitting the second silently drops a `content_color` element override wherever a slot overlay exists on the same element. Rewrote the `appearance.rs` entry: per-property layering now lives inline in `WidgetStateCascades::resolve`, so the fifth property is added there rather than in a deleted `layer_onto`. Moved the disabled-editor-text gate here from Phase 6 — editor text color is unreachable until this phase exists.

**Phase 8.** The `merge_over` instruction named two deleted methods; `merge_over` is now the first per-property fold, and the "do not write a third fold" prohibition has lost its premise. The pending decision on empty bundles stands, but Phase 3 shipped the no-op reading in code (`visual.rs:392`), so suppression now additionally costs deleting that filter and inventing a "clear" token `VisualSlotOverride` does not have.

**Phase 9.** Otherwise clean. Added one gate: propagating an unchanged bundle must not dirty `Resolved<…>`, and Phase 3's presenter-isolation tests must survive — the presenters already carry the four `Changed<Cascade<…>>` terms, so a content-equal `Arc` rewrite would wake all three every frame.

**Phase 10.** Re-scoped onto the seam Phase 3 built: the stage-2 helper already exists as `visual::resolve_part_overrides`, called identically by all three presenters, so the phase extends it instead of adding one in `src/cascade/`. Its no-part-entry skip must be inverted so a widget-level bundle reaches every recipient, which makes `VisualElementCapabilities` load-bearing for the first time — it has no production reader today, and without it the dormancy gate cannot pass. Named the resulting risk: index entries proportional to widgets × recipients. Corrected the claim that `resolve` layers against an `Appearance::default()` accumulator; it accumulates per-property winners and builds the override directly. Restated the empty-part-bundle gate, which passes on the current tree without proving anything.

**Phase 13.** Every `slider.rs` and `visual.rs` line ref was wrong and is corrected. The map deletion is now definite — keep the map, delete only the subtree branch. The `rg` gate was split, because `with_color` reaches ~29 sites across seven files, three of which were missing from **Files**. The focus-border rework largely collapses into Phase 3's per-property composition, conditional on Phase 10's channel decision.

**Deferred to Phase 10 (pending decision):** which override channel carries the widget level — the per-element channel or the root slot. Under the element channel Phase 13's focus-border work becomes a deletion; under the root slot it must be written by hand. Recommended the element channel.

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
- **Phase 13** gained two Phase 4 constraints its example migration would otherwise hit blind: widget-declaring verbs are `LayoutOnly`-only (declare the widget before any state verb), and a part-authoring helper cannot be generic over the builder.
- **Phase 14** was missing a second auto-id minting path, `LayoutTree::tooltip_add_text` (`builder.rs:1790-1807`); without it tooltip content keeps positional ids.
- **Stale references corrected across the plan:** `validate_tree` does not exist (it is `LayoutTree::validate_widgets`); every `widgets/visual.rs` reference in Phases 8, 10, and 13 was ~22 lines low; `layout/builder.rs` references below ~1240 drifted 10-14 lines, and Phase 13's `disabled_color` forward pointed at the wrong impl block entirely; the Delegation Context still named the deleted `WidgetContainsInteractiveDescendant` variant and described a trybuild driver with two tests, an `#[ignore]`, 14 fixtures, and an `E0599` diagnostic that Phase 4 replaced with one test, no ignore, 18 fixtures, and an `E0277`.

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
- **Phase 13 gained a `**Pending decision:**`** — `SliderFocusedThumbBorderColorRequiresThumbBorder` survived Phase 5 and is the same error class on the same record. Deleting it is recommended. A gate line was added because Phase 13's `subtree_color|disabled_color` grep does not reach it.
- **Phase 10:** its two-view rationale cited call sites Phase 5 deleted; re-cited to the two that exist (`resolve_part_overrides` on the authored `StateAppearance`, and `default_state_surfaces`). Its index-growth risk gained the fact that state authorship can now *create* recipients. Its dormancy gate gained the fixture constraint that the label must author no state border of its own, and a line barring `set_element_state_appearance` — the one appearance path that skips the defaulting, and therefore a test that cannot prove what it claims.
- **Phase 13 Files** cited `dispatch_visual_overrides` line numbers contradicting its own Spec; dropped.
- **~40 stale source citations corrected** across the Delegation Context and phases 6-10, 12, and 13 (88 replacements): `layout/builder.rs` drifted +35 to +47 and is now 1944 lines, `layout/element.rs` −8 to −47, `widgets/appearance.rs` +2 to +6, `panel/builder.rs` −17, `widgets/visual.rs` +2 to +22, and `lib.rs`'s `pub use panel::` block starts at 238. Archive sections were left untouched.
- **`as-built/widgets.md`** said "state builders affect only the element carrying the widget declaration; child text, icons, images and shapes stay as authored" — false since Phase 4 and contradicting the same file 30 lines earlier. Corrected.
- **Not changed:** the `validated_element_appearance` mentions inside the Phase 3 and Phase 4 retrospectives and review blocks. Those record what was true when those phases shipped.

### Phase 6 — Generated editable parts · status: done (`929d442d`)

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
- **Phase 13** — `layout/builder.rs` refs corrected ~+200, `widgets/visual.rs` refs +2, and `render/image_batch.rs:628` added to the `color`-removal list (the phase's `rg` gate cannot see `slot_override.color`).
- **Phase 14** — `layout/builder.rs` refs corrected ~+206, and the editor content tree added as the proving case for structural ids: it is the crate's highest-churn auto-id generator and its elements cannot be named.
- **Phases 8 and 9** — reviewed, no changes needed. Phase 6 touched none of the files they own.
- **Delegation Context** — test floor raised 1125 → 1130.

### Phase 7 — Text and path color · status: done (`aefc0f9c`)

#### Work Order

**Goal:** Text and draw primitives change color with widget state, and state materials reach every record type the retained routes already support.

**Spec:**

Add **two** properties to `Appearance`: `text_color` (glyph color) and `path_color` (line and circle color).

**There is no `content_color`.** An earlier draft of this plan proposed one property covering text, images, and draws; it was cancelled on 2026-07-29. Two reasons. First, one name would have covered two different operations — the text route (`panel_text/batching.rs:618`) and the shape route (`panel_shapes/batching.rs:1132`) both **replace** the color via `.unwrap_or(fill_color)`, while the image route (`image_batch.rs:628`) **multiplies** (`image_batch.rs:136`: "Linear RGBA tint multiplied after texture sRGB decode"). Second, one element can carry both text content and a draw, and those must be colorable independently. Do not reintroduce `content_color`, `foreground`, or `ink` — all three were considered and rejected.

**Images are out of scope for this phase.** They get their own `.tint()` property in Phase 13, which is the phase that deletes the generic `color` field images currently read. Until then `image_batch.rs:628` keeps reading `slot_override.color` unchanged; do not edit it here.

Neither property maps to `VisualSlotOverride::color`. `apply_sdf_visual_override` (`render/fill_batch.rs:1359`) reads `fill_color.or(color)` and `border_color.or(color)` — the generic `color` field (`widgets/visual.rs:170`) is the **fallback for every color role**, so it drives fill and border together. That is the mechanism behind `Slider::disabled_color`. A text element that also authors a background would otherwise have its fill recolored by a text-color change.

Add **two distinct overrides**, `text_color` consumed only by the text route and `path_color` only by the draw-primitive route, leaving `fill_color` and `border_color` exclusive to SDF roles.

**Byte-size budget — measured 184.** The plan's 144→160 figure assumed one new field at 16 bytes; two fields predicted 176 on that basis. The measured value is **184**: `Option<Color>` is 20 bytes here, not 16. The assertion records 184. Phase 13 then removes the generic `color` and adds `tint`, a net-zero change, so Phase 13 asserts **184 as well**; its "back to 144" line was wrong and has been corrected.

There is no material error left to widen — Phase 5 deleted `StateMaterialRequiresSurface` along with the other three. What survives is the **capability derivation**: `SDF_MATERIAL` is derived from a narrower set of records than the retained routes actually apply `VisualSlotOverride::material` to, which is SDF, text, and **every** `PanelDraw` record — lines *and* `PanelCircle` (`layout/draw.rs:11` for `PanelDraw`; `layout/line.rs:42` for the `PanelShape` enum, `:64` for `PanelCircle`; `render/panel_shapes/batching.rs:989`). Widen the derivation to match. Content color's recipients are text, image, or `PanelDraw` content.

**This requires splitting Phase 2's capability mask, not merely extending it.** `VisualElementCapabilities` (`widgets/id.rs:115`) ships one `CONTENT` bit covering text, image, and non-empty `PanelDraw` together, and sets `SDF_MATERIAL` only when a background or border exists (`element.rs:1293`). Material-accepts-text-and-draw-but-rejects-image-only is not expressible from a single bit, so replace `CONTENT` with `TEXT` / `IMAGE` / `DRAW` and widen the `SDF_MATERIAL` derivation in `element_visual_capabilities` (`element.rs:1319`) to any SDF, text, or `PanelDraw` record. `text_color`'s capability is `TEXT` alone and `path_color`'s is `DRAW` alone; material's is everything except `IMAGE` alone. The `IMAGE` bit is created here even though no property in this phase names it — it exists for the material derivation, and Phase 13's `tint` will use it.

**Files:**
- `src/widgets/appearance.rs:98` — **two** new properties on `Appearance` (impl at `:109`) and their fluent setters. Phase 3 deleted both `layer_onto` methods; per-property layering is now inlined in `WidgetStateCascades::resolve` (`:332`), so compose each by adding a local plus a `VisualChange::To` arm inside that function's `LAYER_ORDER` loop and a field in the `VisualSlotOverride` it constructs — two of each, not one. `Appearance` derives `PartialEq` (`:95`), so the new fields are compared automatically; there is no hand-written comparison to edit.
- `src/widgets/visual.rs:169` — `text_color` **and** `path_color` on `VisualSlotOverride`, **and extend both `apply` (`:195`) and `apply_element` (`:209`) with both fields**. Those two functions enumerate every field explicitly and are the only path by which an element override composes over a slot baseline in `dispatch_visual_overrides` (`:506`); omitting either function, or either field in either function, silently drops that element override wherever a slot overlay exists on the same element index.
- `src/render/panel_text/batching.rs` — the glyph-color override is `apply_text_visual_override` at **`:609`**, reading `slot_override.color` at **`:618`**. (`:288` is the cascade-read block and `:435` is `apply_routed_text_run_update`; neither is the override site.)
- `src/render/panel_shapes/batching.rs` — `apply_shape_visual_override` at **`:1123`**, color read at **`:1132`**. (`:989` is a blank line.)
- **`src/render/image_batch.rs` is NOT an edit site in this phase.** `:628` reads `slot_override.color.map_or(tint, linear_tint)` and keeps doing so until Phase 13 replaces it with `tint`. Images are deliberately out of scope here — see the Spec.
- **`src/render/analytic_paths/batching.rs` is NOT an edit site** — the file contains zero `VisualSlotOverride` references. It consumes a color already resolved and stamped by `panel_text/batching.rs`. Dropped from this phase (and re-examine before trusting Phase 13's list).
- `src/widgets/id.rs:115` — split `CONTENT` into `TEXT` / `IMAGE` / `DRAW`.
- `src/layout/element.rs:1317` — `element_visual_capabilities`; widen the `SDF_MATERIAL` derivation (now at **`:1326`**) to any SDF/text/`PanelDraw` record and emit the three new content bits. (Phase 6 shifted these ~+32 from the `:1285`/`:1293` the Spec above still cites.)
- `src/layout/builder.rs` — `CommonEl::default_state_surfaces` takes the `ElementContent` its two callers already hold (`text_leaf_element`, `El::into_element`) and stops emitting a fill for a state material on an element that emits its own material recipient. See the Phase 5 constraint below.

**Constraints from prior phases:**
- **Phase 1:** `Appearance` is public with `background` / `border_color` / `border_width` / `material`, each a `VisualChange<T>`; adding a fifth field takes it from 80 to 96 bytes, which is why the cascade attributes carry `Arc<Appearance>` and each has its own `size_of` assertion against `CASCADE_ATTRIBUTE_BYTES = 32`. Do not add a `VisualChange` variant.
- **Phase 2:** each recipient index carries a property-capability mask (`VisualElementCapabilities`, `widgets/id.rs:115`) so containers and non-content elements stay excluded. Its one `CONTENT` bit conflates text, image, and draw, and `SDF_MATERIAL` is set only for background-or-border — both must change here, per the Spec.
- **Phase 5:** there is no appearance validation left anywhere. `validated_element_appearance` and its three call sites are deleted; the four `PanelBuildError::State*` variants are deleted. The guarantee is now carried by **record synthesis** — `CommonEl::default_state_surfaces` (`layout/builder.rs`, called from `text_leaf_element` and `El::into_element`) emits the transparent record a state property replaces. Do not go looking for a validator to add a fifth arm to; there is none. `element_visual_capabilities` (`element.rs:1319`) survives and still derives the mask, but nothing consumes it as a rejection gate.
- **Phase 5, the interaction this phase must handle:** `default_state_surfaces` emits a `Color::NONE` fill for a state `material` whenever the element has no border record. It does not look at `ElementContent`, so a text-only or `PanelDraw`-only part authoring a state material gets an SDF fill it does not need. Today that fill is *load-bearing* — `SDF_MATERIAL` is the only route a state material has. Once this phase widens the derivation to text and `PanelDraw`, it becomes waste (one material-table row plus quad geometry per element). Both conversion sites already have `content` in scope, so passing it into `default_state_surfaces` and skipping the fill when the element emits its own recipient is a small change — but it is this phase's change to make, and this phase's gate must assert the circle-only part carries **no** defaulted fill.
- **Phase 6:** the four generated editor parts are recipients; editor text is the canonical `text_color` target. Specifically:
  - The four fluent methods live on `El<L, WidgetElement<EditableField>>` (`layout/builder.rs:1033`–`:1072`).
  - `EditorPart` is `pub(crate)` (`layout/builder.rs:448`), with `into_text` (`:515`), `with_children` (`:524`), and `with_background_if_unset` (`:487`) — the last supplies the built-in `EDITOR_SELECTION` / `EDITOR_CARET` colors, which are **private consts**, so an authored declaration that omits a background still gets the default.
  - **`editor_text` and `editor_validation` become text leaves; `editor_selection` and `editor_caret` become rectangles.** Only the latter two can use `background`.
  - The example currently authors **neither** `editor_text` nor `editor_validation`, because `background` on a text leaf painted an opaque rect over the glyphs. This phase adds both back on `text_color`.
  - **Fan-out:** one `editor_text` declaration reaches up to **eight** generated text elements — `add_text` is called at `ime/editor.rs:1222`, `:1229`, `:1238`, `:1250`, `:1272`, `:1279`, `:1286`, `:1292` (preedit runs, pre/post-selection runs, and the run inside `add_selected_text`). `into_text` clears `common.id` (`layout/builder.rs:516`), so none can be named individually. One authored `text_color` therefore produces N recipients and N part-map entries.

**RESOLVED 2026-07-29 — build error.** A state `text_color` or `path_color` authored on a part that structurally cannot present it is a **`PanelBuildError`**, not a silent no-op.

The problem it settles: Phase 5 made the other four properties unrejectable by synthesizing the record they replace. These two cannot follow — layout can conjure a transparent SDF fill or border out of nothing, but it cannot conjure text or a `PanelDraw`. So `El::new().disabled(Appearance::new().text_color(RED))` on a structural container compiles, is admitted to `part_appearances` by `any_overridden()` (`element.rs:1318-1324`), and can never present. That is the **accepted option must reach the runtime** breach Phase 4 closed with `validated_element_appearance`, whose machinery Phase 5 deleted.

The plan previously recommended declaring it dormant, and argued that a build error was impossible because emission is a runtime predicate. **That argument does not hold**, and the reason is worth recording so it is not reopened: `add_text` early-returns on empty text (`ime/editor.rs:1303-1305`) and creates **no element at all**. The argument conflates two distinct cases —

1. *No element exists* (empty buffer). There is nothing in `part_appearances`, nothing to validate, and no error is possible or needed.
2. *An element exists that structurally cannot present* (a bare grouping container; `editor_selection`, which is a rectangle holding text as a child). Fully knowable at panel build.

Only case 2 is being rejected, and case 2 is a build-time fact.

**Mechanism.** `element_visual_capabilities` (`layout/element.rs:1317`) already derives the needed bits at build time — this phase is splitting `CONTENT` into `TEXT` / `IMAGE` / `DRAW` anyway. Reject when a bundle names `text_color` on an element with no `TEXT` bit, or `path_color` with no `DRAW` bit. Follow the error shape of `PanelBuildError::SliderThumbOutsideSlider` (`element.rs:825`); the recovery text is real here (`add a text child`, `add a draw`), not a record layout could have emitted itself.

**Scope limit, unchanged.** This binds **part-local** authoring only. A global `CascadeDefault`, a panel-level default, or a runtime entity command still cannot promise a present recipient, so a higher-level property with no compatible record at some element stays **dormant** there. That is the existing invariant and this decision does not disturb it.

**This does not cover the typestate rule.** Restricting the four state verbs so they cannot be called on a plain layout element at all is a separate, type-level change with its own phase — see Phase 15. That phase does not subsume this one: the parts rejected here are already inside widgets, so they pass any typestate gate.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase adds a public `Appearance` property and its doc entry, so both doc commands must pass before checkpoint.
- **Live smoke (orchestrator-run — see Delegation Context → Headless only carve-out):** this phase changes what `examples/widgets.rs` renders, so the plan owner runs the example and confirms the focused editable field shows correctly colored editor text before checkpoint. Keyboard only. Do not checkpoint on a passing headless gate alone — that is exactly the state Phase 6 shipped its opaque-bar defect in.
- **Build error, both properties.** A `text_color` on a part with no `TEXT` capability and a `path_color` on a part with no `DRAW` capability each produce a `PanelBuildError` naming the element, with a compile-fail or build-error fixture per property. Two companion assertions prove the rejection is narrow: a `text_color` on a part that *does* emit text builds clean, and a **global or panel-level** default naming `text_color` against a container with no text stays **dormant** and produces no error — the part-local scope limit in the Spec.
- A `const _: () = assert!(size_of::<VisualSlotOverride>() <= …)` records the type's new size, following the per-attribute precedent at `widgets/appearance.rs:219`. Two fields land here; the measured value is **184**. Phase 13 is net-zero on size (removes `color`, adds `tint`) and asserts the same value.
- A disabled slider dims its label.
- A hovered button brightens its caption **without touching its fill**.
- A text element carrying its own background and border changes **only** its text color under a state.
- A circle-only part accepts and presents both material and content color, and carries **no** defaulted `Color::NONE` fill (see the Phase 5 interaction constraint).
- A state material on a text label wins over the `TextMaterial` cascade and restores it when the state clears.
- An element-level `text_color` **and** an element-level `path_color` each survive composition with a slot override on the same element index, proving `apply_element` carries both new fields.
- A part that carries both text and a draw colors them independently: `text_color` alone moves the glyphs and leaves the path, `path_color` alone does the reverse. This is the case that justifies two properties instead of one.
- A disabled editable field dims its editor text — asserted on **every** generated run, not just the first, given the up-to-eight fan-out recorded in the Phase 6 constraints. (Moved here from Phase 6 — editor text color is unreachable until this phase adds `text_color`.)
- A **headless proxy for the Phase 6 defect class**: a text-leaf recipient authoring only `text_color` acquires no `SDF_FILL` capability and no `fill_color` override. This is what would have caught the opaque-bar bug without a GPU.
- `examples/widgets.rs` **adds** an `editor_text` part and an `editor_validation`
  part, both authoring `text_color` (not `background`). Phase 6 deleted both
  calls outright, so there is nothing to "migrate" — grepping for `.editor_text(`
  finds nothing. Both parts are text leaves (`ime/editor.rs:1180` routes validation
  through `add_text` too), which is why `background` was wrong for them. Verified by
  `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`.

**Pending decision:** whether phases that change rendered appearance get an orchestrator-run smoke gate.

Actual problem:
Phase 6 passed every gate — lint, 1130 tests, trybuild, example build — while the
focused field rendered as an opaque black bar. The only thing that caught it was an
orchestrator-run smoke of the live example with a human at the keyboard. Phases 7 and
12 both change the example's rendered appearance.

What exists now:
- The Delegation Context invariant "Headless only" says assertions are "never on
  rendered color" and "No phase needs a GPU, a window, or a screenshot."
- That invariant is what made Phase 6's gate blind to its own defect.

What should change:
- Add an explicit orchestrator-run smoke line to phases 7 and 12, parallel to the
  existing **Docs** line, and spell out the carve-out in the invariant.
- Keep the headless proxy assertion already added to Phase 7's gate — it catches the
  same class without a GPU, but only for capability/override state, not for what a
  pixel actually looks like.

Recommendation:
Add both: the headless proxy as the automated gate, and a one-line orchestrator smoke
as a human check before checkpoint. The smoke costs a launch and a keypress; Phase 6
shows the alternative costs a shipped-broken feature.

**RESOLVED 2026-07-29 — approved by the plan owner.** Phases that change rendered
appearance get a live smoke test run by the plan owner before checkpoint. Add the
orchestrator-run smoke line to phases 7 and 12 parallel to the existing **Docs**
line, keep the headless proxy assertion as the automated gate, and spell out the
carve-out in the "Headless only" invariant. Driving is **keyboard only** — no BRP
mouse control while the plan owner is at the machine.

**Extended 2026-07-30 — approved by the plan owner: Phase 10 gets the smoke gate
too.** Phase 10's skip inversion sends a resolved bundle to *every* element a widget
owns, so it is the phase with the widest reach over rendered output and the closest
match to Phase 6's failure shape. The smoke runs before Phase 10's checkpoint, and an
auto window does not skip it — the window pauses there for the launch.

### Retrospective

**What worked:**
- Splitting `CONTENT` into `TEXT` / `IMAGE` / `DRAW` made the acceptance relation
  expressible; the single bit could not say "material accepts text and draw but
  rejects image-only".
- The two new properties stayed off the generic `color` field, so
  `apply_sdf_visual_override`'s `fill_color.or(color)` / `border_color.or(color)`
  fallback (`render/fill_batch.rs:1359`) never recolors glyphs.
- The live smoke confirmed the channel end to end: focused editor text renders
  near-white over the blue selection fill, and both disappear on blur.

**What deviated from the plan:**
- Predicted `size_of::<VisualSlotOverride>()` was 176; measured **184**.
  `Option<Color>` is 20 bytes here, not 16. Phase 7's assertion and Phase 13's
  three references were corrected to 184.
- Fix pass 1 reported all gates green while a test it had just added was failing
  (`state_path_color_updates_the_draw_row_without_a_text_override`). Its fixture
  authored a draw on a text leaf, where the leaf's bounds clipped every shape
  command away, so there was no draw row to recolor. The draw moved to the widget
  root. Two fix passes were needed, not one.

**Surprises:**
- Subtree seeding in `dispatch_visual_overrides` (`widgets/visual.rs:566-576`) was
  the one writer of the generic `color`, so moving the text and shape routes onto
  the new fields silently broke `Slider::disabled_color` — it stopped dimming text
  and paths. The existing slider test asserts on `disabled.offset` and
  `disabled.border_color`, fields the regression does not touch, so it passed
  vacuously. The seeding now writes all three color fields.
- The blind reviewer's proposed remedy for that regression — restoring a
  `slot_override.color` fallback in the text and shape routes — was rejected: it
  would let a background change recolor an element's glyphs, which is the exact
  coupling this phase exists to remove.
- The widget input context activates only for the OS-focused window
  (`widgets/input.rs:595-615`), so the smoke could not be driven until the plan
  owner clicked the window. `Window.focused` cannot be forced over BRP —
  bevy_winit syncs it back from the real OS state.

**Implications for remaining phases:**
- Phase 13 removes the generic `color` and adds `tint`, a net-zero change, so its
  size assertion is 184, not the "back to 144" the plan carried.
- Phase 13 **deletes** the subtree channel rather than re-pointing it. The
  ordering constraint is that `VisualSlotOverride::color` cannot be removed before
  the subtree branch that writes it, and the dimming that branch provided has to
  be reproduced by per-part authoring in `examples/widgets.rs`.
- Any phase adding a per-role color must add a capability bit alongside it; the
  build error is what makes a part-local override safe, and it needs a bit to test.

### Phase 7 Review

- **Delegation Context refreshed** for the shipped tree: the single `CONTENT`
  capability bit is now `TEXT` / `IMAGE` / `DRAW` (`widgets/id.rs:123`/`:125`/`:127`),
  `VisualSlotOverride` carries its six-field list with the `<= 184` upper-bound caveat,
  `apply` (`visual.rs:202`) is named as the only color-naming method, the subtree-seeding
  branch (`:556-568`) as the only writer of the generic `color`, and the test baseline
  moves to 1146 passed / 2 skipped.
- **Phase 9** gains a gate line re-asserting the part-local scope limit. Phase 7's
  `global_text_color_default_stays_dormant_on_textless_widget_part` passes vacuously
  today because no `CascadePlugin` is registered for the appearance channels
  (`widgets/mod.rs:234`); Phase 9 registers them, which is the first moment that test
  can fail. Its `BuilderData` ref corrected to `panel/builder.rs:189`.
- **Phase 10**: corrected the claim that `WidgetStateCascades::any` has one production
  caller — Phase 7 added `validate_part_state_colors` (`layout/element.rs:1348`),
  calling it at `:1367` and `:1372`; deleting the authored view would break the
  part-local build errors as well as record synthesis. The unsatisfiable
  `&(element_index, _)` grep is scoped to `resolve_part_overrides`. Two pending
  decisions added: the dormancy matrix must be rebuilt as explicit rows because
  `SDF_MATERIAL` is now derived from background/border/text/draw (`element.rs:1341`)
  and leaves only image-only elements incompatible; and the generated editor subtree
  escapes `validate_part_state_colors` entirely, so a state color authored on a caret
  or selection is accepted and permanently dormant.
- **Phase 13**: **Files** gains `layout/element.rs:1348` and `panel/builder.rs:69-74` —
  adding `tint` means adding a third arm to the part-local check and copying the two
  Phase 7 error variants. A constraint records the check's early return
  (`element.rs:1354`) so the `tint` arm inherits its exemptions rather than widening
  them. The example-migration gate now enumerates the fields each `disabled` bundle
  asserts (`background`, `border_color`, `text_color`, `path_color` independently),
  because `disabled_color` dimmed a whole subtree with one value and a single
  wholesale assertion would pass while a route was dropped. The size gate tightens
  `visual.rs:199` from `<= 184` to `== 184`; the unsatisfiable bare `\.with_color\(`
  grep is replaced by the `slot_override.color` grep plus `cargo check`. Its pending
  decision on `SliderFocusedThumbBorderColorRequiresThumbBorder` was re-argued: Phase 7
  reinstated the error class it claimed to be the last of, so the distinguishing test
  is now synthesizability — a thumb border is synthesizable, text and `PanelDraw` are
  not. Recommendation to delete stands, on the new grounds.
- **Phase 14**: **Files** gains `validate_part_state_colors`, which mints
  `PanelElementId::auto(...)` from the element's tree index (`element.rs:1361-1366`)
  rather than through `next_auto_id` — a second, independent auto-id producer that the
  structural-id change must move, or its error messages will name ids the elements do
  not carry.
- **Line-reference drift** corrected across every remaining phase and the Delegation
  Context: `layout/element.rs`, `layout/builder.rs`, `widgets/visual.rs`,
  `widgets/appearance.rs`, and the three presenters. Phases 13 and 15 cited
  `LayoutContentBuilder::with<L, Role>`; the generic `with` is a default method on
  `AcceptsElement` (`builder.rs:1554`, method `:1720`).
- **Reviewed clean:** no remaining phase is redundant or already satisfied, and the
  phase ordering is unchanged.

### Phase 8 — Per-property merge in the cascade · status: done (`298ed48e`)

#### Work Order

**Goal:** Cascade resolution folds every authored level through a per-attribute combine, `Appearance` supplies a per-property merge, and every existing attribute keeps first-override-wins with no edit to it.

**Spec:**

Stock resolution returns the first `Cascade::Override` and stops — `resolve_from_queries` (`bevy_kana/src/cascade.rs:433`) and `resolve_from_world` (`:446`) both do — and the `CascadeDefault<A>` root is a *fallback*, never combined. The design requires **per-property merge at every hop**, including into the global default.

`Appearance`'s six fields (`background`, `border_color`, `border_width`, `text_color`, `path_color`, `material` — Phase 7 added the middle two) are each `VisualChange<T>`, so a bundle is already a sparse per-property record. Merging is field-by-field: **the lower level's `To(value)` wins, otherwise the higher level's field carries through.** Write one

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
- `crates/hana_diegetic/src/widgets/appearance.rs:118` — `Appearance::merge_over`, added to the `impl Appearance` block. **Phase 3 deleted both `layer_onto` methods.** Neither `Appearance::layer_onto` nor `VisualChange::layer_onto` exists any more, so the thin wrapper this bullet used to describe has nothing to wrap: `merge_over` is now the **first and only** per-property fold over `Appearance`'s fields, written out field by field (lower's `To` wins, otherwise the higher value carries through). `VisualChange` (`:26`) carries only `is_authored` (`:36`) today. Phase 3 inlined per-property layering into `WidgetStateCascades::resolve` (`:362`), which accumulates four `Option<&T>` per-property winners across the `LAYER_ORDER` loop (`:428`) rather than folding whole `Appearance` values — a different shape, and not the fold to reuse. Write `merge_over` directly; the former "do not write a third per-property fold / do not add a `VisualChange::or`" prohibition has lost its premise and no longer applies.
- `crates/hana_diegetic/src/widgets/appearance.rs` — the four `Widget*Appearance` types implement `CascadeRoot` with a `combine` delegating to `merge_over`.

**Constraints from prior phases:**
- **Phase 1:** the four wrappers are `Arc<Appearance>` newtypes with hand-written `PartialEq` (`Arc::ptr_eq` then content equality) and per-attribute `size_of` assertions. Every merge allocates a fresh `Arc`, so equality must fall through to content comparison — a merge producing an equal value must still compare equal, or propagation dirties `Resolved<A>` every frame.
- **Phase 7:** `Appearance` now has six `VisualChange` fields — `background`, `border_color`, `border_width`, `material`, `text_color`, `path_color`. `merge_over` covers all six.
- **One merge direction, never varied.** `merge_over` must have a single documented orientation and every call site must go through it. Flutter ships `TextStyle.merge` (the argument wins) and `ButtonStyle.merge` (the receiver wins) in the same framework, and that inconsistency is a documented source of confusion. Pick lower-wins-over-higher, state it in the doc comment, and do not add a second merge with the opposite sense.
- **`CascadeRoot::root_default()` must return a stable cached value, not a freshly computed one.** It stays total — never `Option` — but totality is not enough. SwiftUI's `@Entry` generates a *computed* default, so a reference-type default mints a new instance on every unresolved read, which defeats identity comparison and causes spurious invalidation. `material: VisualChange<Handle<StandardMaterial>>` is exactly where that would bite here: a fresh `Handle` per read would make Bevy change detection fire every frame on an unchanged cascade. Return a cached handle.
- **Keep the sparse per-property record; do not switch to whole-object replacement.** `VisualChange<T>`-per-field is the shape CSS declarations, Flutter's nullable `TextStyle` fields, and SwiftUI's independent environment keys all converge on. Flutter's `ButtonStyle` replaces whole objects instead ("one replaces the other entirely") and that is a documented annoyance.
- **Inherit-or-not is declared per property, never inferred.** Today the answer is "no property inherits by element tree position" (see the Delegation Context invariant), so this costs nothing now — but if a property is ever added that does, it says so at its declaration. CSS `@property` makes `inherits` mandatory and invalidates the rule if it is missing; WPF requires `FrameworkPropertyMetadata.Inherits` and defaults it off. Two independent systems converged on stating it explicitly.
- The existing cascade attributes that must keep replace semantics, all declared through `cascade_attribute!` in `src/cascade/resolved.rs`: `TextAlpha` (`:52`), `FontUnit` (`:58`), `HdrTextCoverageBias` (`:63`), `SdfMaterial` (`:112`), `TextMaterial` (`:125`), `ShapeMaterial` (`:138`), `Lighting` (`:149`), `ShadowCasting` (`:152`), `GlyphShadowMode` (`:155`), `Sidedness` (`:159`), `AntiAlias` (`:163`), `HairlineFade` (`:167`), `WidgetInteractivity` (`:170`). **None of them is edited by this phase** — the macro emits no `combine`, so they inherit the replace default.

**Resolved (approved before Phase 8 dispatch): an empty bundle is a no-op, never a suppression.**

`El::new().hovered(Appearance::new())` inside a panel that authors `hovered(background(BLUE))` resolves to the panel's blue. Authoring an empty bundle is indistinguishable from never calling the state verb. This is the status quo, not a change: `resolve_part_overrides` (`widgets/visual.rs:390`) already drops any resolution equal to `VisualSlotOverride::default()` (`:413`), so an empty bundle writes no override today.

Consequences for this phase:
- `merge_over` treats an all-`Unchanged` bundle as the identity — the gate line `Appearance::new().merge_over(&x) == x` stands as written.
- `VisualChange` keeps its two variants. Do **not** add a `Clear` state; `VisualSlotOverride`'s fields are `Option<T>` where `None` already means "no opinion", leaving no value to spend on "clear".
- **Part-map admission stays override-keyed.** `any_overridden()` (`layout/element.rs:1392`) continues to admit an explicitly empty bundle. The entry is inert under this reading but harmless — the capability mask already prevents the costly part of the waste, and the stored `Override`/`Inherit` distinction is what a future explicit-clear feature would build on. `record_owned_widget_element` and Phase 2's admission test are therefore **not** in this phase's Files.
- `src/widgets/visual.rs` is **not** pulled into this phase's Files.

Suppression, if ever wanted, arrives as an additive explicit clear token distinct from an empty bundle — not as a reinterpretation of empty.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check bevy_kana` and `… check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test bevy_kana` and `… test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint bevy_kana` and `… lint hana_diegetic`
- **`CascadeAttribute` and its blanket impl are byte-identical to `HEAD`**, and no attribute type gains a hand-written `impl CascadeAttribute`.
- A test asserts **every existing cascade attribute still resolves first-override-wins** across a global → panel → widget chain after the fold lands, including the no-override case that returns the `CascadeDefault` root.
- A cycle, a missing `CascadeFrom`, and depth-limit exhaustion each still yield the same value as today for a replace attribute.
- `merge_over` unit tests cover all six properties (`background`, `border_color`, `border_width`, `text_color`, `path_color`, `material`) for the four combinations of (higher names it / does not) × (lower names it / does not).
- `Appearance::new().merge_over(&x)` equals `x` — an empty bundle is a no-op, not a clear.
- A three-level merge test: global naming `background`, panel naming `text_color`, widget naming `background` resolves to the widget's background, the panel's text color, and `Unchanged` elsewhere.

### Retrospective

**What worked:**
- `CascadeRoot` hosted the `combine` rule for free. `CascadeAttribute` and its blanket impl are byte-identical to `HEAD`, and no existing attribute was edited — the `cascade_attribute!` macro emits only `root_default`, so all thirteen inherit the replace default.
- The fold is behavior-preserving on every exit path. The cycle and depth-limit branches are only reachable when no override was found, so the accumulator is `None` there and both still return the root.

**What deviated from the plan:**
- The Spec's `Self(Arc::new(Appearance::new()))` for `root_default` would have minted a fresh `Arc` per call, which the Phase 1 constraint forbids. Shipped as a shared `static EMPTY_APPEARANCE: LazyLock<Arc<Appearance>>` with `Arc::clone`, so all four attributes return one cached value.
- The acceptance gate's attribute-regression test was first written in `bevy_kana` against the synthetic `TestValue`, which never touches `cascade_plugin` or `CascadeRoot`. Review caught it; the real coverage now lives in `cascade/resolved.rs` as one helper generic over `A: CascadeRoot`, called once per attribute.
- `resolve_entity_cascade` initially `?`-returned on a missing `CascadeCombine<A>`, adding a second silent failure condition its caller (`cascade/attributes.rs:352`) converts to `A::root_default()`. It now falls back to `replace_cascade_value` instead, so the pre-phase contract is unchanged.
- Clippy required `const fn replace_cascade_value`, `*value` derefs in `merge_over`, and dropping `Copy` from the `bevy_kana` test value. No behavior change.

**Surprises:**
- A cycle sitting *above* an authored override now warns on every propagation where the old short-circuit returned silently. The resolved value is unchanged; the walk simply no longer stops at the first override, so it reaches the cycle. Deliberate, not a defect.
- `CascadeCombine<A>` cannot live inside `CascadeDefault<A>`: a `fn` pointer is not `Reflect` and that resource is `#[reflect(Resource)]`. It ships as a separate private, non-reflected resource.

**Implications for remaining phases:**
- `Appearance::merge_over(&self, higher: &Self) -> Self` is `pub(crate)` and available now. Orientation is fixed and documented: **the receiver is the lower cascade level and its `To` fields win**; `Unchanged` lets `higher` carry through. Phase 10 uses it at both hops — level-to-level in stage 1, part-against-widget in stage 2 — and must not introduce a second merge with the opposite sense.
- The four `Widget*Appearance` types already implement `CascadeRoot`, so Phase 9 only registers `cascade_plugin::<A>()` for them; the combine wiring is done.
- `bevy_kana::CascadePlugin::with_combine` is public API. Anything registering a merging attribute must go through `cascade_plugin`, not `CascadePlugin::new` directly, or it silently gets replace semantics.

### Phase 8 Review

- **Phase 9 would have panicked.** Its Placement paragraph and its `src/panel/mod.rs:194` Files entry said to register the four `CascadePlugin`s in `HeadlessLayoutPlugin` as well as `WidgetsPlugin`. `HeadlessDiegeticUiPlugin` adds both (`lib.rs:436`) and `CascadePlugin` does not override `Plugin::is_unique`, so that is a `DuplicatePlugin` panic. Registration is now `WidgetsPlugin`-only, matching the `WidgetInteractivity` precedent; the Files entry is gone.
- **Phase 9's four hand-written `CascadeDefault` resources are deleted.** `CascadePlugin::build` inserts them when absent, and Phase 8's `root_default()` returns the shared `EMPTY_APPEARANCE`. Hand-writing them would mint a second `Arc` per attribute — the exact thing the Phase 1 cached-root constraint forbids. The `src/cascade/defaults.rs` Files entry is gone; the bullet now says why not to.
- **Phase 9 gained the `CascadeCombine` silent-replace hazard as a constraint** (registering via `CascadePlugin::new`, or pre-seeding the resource, compiles and propagates but loses per-property merge) and a gate asserting the merge holds in the **reification frame**, where `resolve_entity_cascade`'s replace fallback would otherwise hide a mis-registration.
- **Phase 10's `merge_over` grep gate was false as written** — Phase 8 shipped four production call sites in the `CascadeRoot::combine` impls. The gate now enumerates them.
- **Phase 10's open "removal question" is closed as a constraint:** a live widget never loses its four `Resolved` caches (`propagate_cascade` removes `Resolved<A>` only when `Cascade<A>` is absent; `spawn_widget` inserts all four unconditionally), so presenters take `&Resolved<…>`, not `Option<&…>`.
- **Phase 10 gained `merge_over`'s fixed orientation verbatim** (receiver is the lower level, its `To` fields win) and the fact that the four `CascadeRoot` impls already exist.
- **Phase 10's index-growth risk is bounded by Phase 8's empty root:** with nothing authored, the reduction yields `VisualSlotOverride::default()` and the existing filter at `visual.rs:413` drops it. Recorded so nobody adds a redundant emptiness guard.
- **The "four levels" invariant is a floor, not a ceiling.** The fold merges every authored override along the whole `CascadeFrom` chain, so an application-owned panel source is a real merging level. Restated in the Delegation Context.
- **Ref drift corrected in Phases 10, 13, 14, and 15** — `widgets/appearance.rs` moved +2 to +110, and the `widgets/visual.rs` and `layout/builder.rs` refs drifted again. Each affected Work Order's correction block now says it overrides every line number elsewhere in that Work Order, including its Spec, which the Phase 6 blocks did not.
- **Test-count floor raised** to 1149 passed / 2 skipped.
- Not changed: the cycle-above-an-override warning is a behavior improvement, not a defect, and needs no phase. Phase 10's pending decision (which override channel carries the widget level) was later resolved on 2026-07-30 — the widget level is delivered per element and the slot channel survives as the geometry/named-piece channel — so Phase 10 carries no open decision.

### Phase 9 — Register the four cascades and the panel authoring surface · status: done

#### Work Order

**Goal:** A global default and a panel-level override for each of the four states propagate to widget entities, with the full ownership/teardown lifecycle wired.

**Spec:**

Register four `CascadePlugin` channels over the Phase 1 attribute types and build out the panel authoring surface. Every item below is a **mechanical repetition of the existing `WidgetInteractivity` pattern** — mirror it exactly (`src/widgets/interactivity.rs`, registered via `cascade::cascade_plugin::<WidgetInteractivity>()` at `src/widgets/mod.rs:234`):

- Four `BuilderData` fields, builder methods, component seeds, and `build_panel` assignments (`src/panel/builder.rs:189`).
- Four `seed_panel_value` calls in `seed_panel_overrides` (`src/panel/diegetic_panel.rs:1567`; `seed_panel_value` itself is at `:1608`).
- Four `CascadePlugin` registrations in `WidgetsPlugin`, in the `add_plugins` tuple (`src/widgets/mod.rs:233-237`).
- Four typed `override_*` / `inherit_*` command pairs on `CascadeEntityCommandsExt` (`src/cascade/attributes.rs:30`).
- Four `add_cascade_ownership_observers!` entries. **That macro is invoked in `src/panel/mod.rs:207`, inside `HeadlessLayoutPlugin::build` — not in `lifecycle.rs`, which earlier drafts of this Work Order cited.** The macro is defined at `src/panel/mod.rs:171`.
- Four `teardown_owned_shared_state` entries (`src/panel/lifecycle.rs:775`).
- Four assignments in `replace_from_precompose_helper` (`src/panel/diegetic_panel.rs:452`).
- The four **empty-`Appearance` root resources** arrive for free — `CascadePlugin::build` inserts the root resource when absent, and Phase 8's `root_default()` returns `Arc::clone(&EMPTY_APPEARANCE)` (`widgets/appearance.rs:227`). **Do not hand-write them** in `src/cascade/defaults.rs` or anywhere else: a second `Arc` per attribute is exactly what the Phase 1 cached-root constraint forbids, and pre-inserting the root resource before the plugin makes `build` skip its own insert. Not `PanelDefaults`.

**Resolved (approved 2026-07-30): each of the four attribute types is its own root resource, and its constructor is public.** The merge of `main` at `b873d8f4` brought in `bevy_kana::CascadeRootResource` (`43d3cba8`), which lets an attribute type serve as its own cascade root instead of being wrapped in `CascadeDefault<A>`. Every other attribute in the crate converted; these four were left on `CascadeDefault<Self>` by the merge, which made the global level unreachable from outside the crate — `Widget*Appearance::new` was `pub(crate)`. They now carry `#[derive(Resource)]` + `#[reflect(Resource)]`, a hand-written `impl CascadeRootResource<Self>` (clone, not `Copy`, so the `cascade_root_resource!` macro in `resolved.rs` does not apply), `type Root = Self`, and a `pub fn new`. An application authors a global with one insert:

```rust
app.insert_resource(WidgetHoveredAppearance::new(
    Appearance::new().background(Color::BLACK),
));
```

`CascadeDefault` no longer appears anywhere in `hana_diegetic` — its `pub(crate) use` re-export in `src/cascade/mod.rs` is removed. **This is the crate's convention: a new cascade attribute is its own root resource.**

**Placement:** registration lives in `WidgetsPlugin` **only**, exactly like `WidgetInteractivity` (`src/widgets/mod.rs:234`); panel ownership observers and construction seeding stay in `HeadlessLayoutPlugin` (`src/panel/mod.rs:196`), matching the current division. **Do not also register the four in `HeadlessLayoutPlugin`.** `HeadlessDiegeticUiPlugin` adds `HeadlessLayoutPlugin` *and* `WidgetsPlugin` together (`src/lib.rs:436`) and `CascadePlugin` does not override `Plugin::is_unique`, so a second registration is a `DuplicatePlugin` panic. Headless tests reach these cascades through `WidgetsPlugin`, which is why `WidgetInteractivity` needs no `HeadlessLayoutPlugin` entry today; the five attributes `HeadlessLayoutPlugin` does register are panel attributes that participate without any widget.

**Beyond the checklist:**
- Command documentation matching the existing `WidgetInteractivity` **durability boundary**: a command applied directly to a derived widget entity may be replaced by reification, so durable edits belong in the panel's authored tree.

The four attribute types are **already exported** from the crate root — Phase 1 shipped them (`src/lib.rs:392` disabled, `:397` focused, `:398` hovered, `:408` pressed). This phase's new public surface is the panel-builder methods and the eight commands, and only those need export work.

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
- `src/panel/builder.rs:189` — four `BuilderData` fields + builder methods + seeds + `build_panel` assignments.
- `src/panel/diegetic_panel.rs` — four `seed_panel_value` calls inside `seed_panel_overrides` (`:1567`), four `replace_from_precompose_helper` assignments (`:452`).
- `src/panel/mod.rs:207` — four `add_cascade_ownership_observers!` entries (macro defined at `:171`). This is the **only** `panel/mod.rs` edit; do not add `CascadePlugin` registrations here.
- `src/panel/lifecycle.rs:775` — four `teardown_owned_shared_state` entries.
- `src/cascade/attributes.rs:30` — four typed command pairs, with durability documentation.
- `src/lib.rs:346-410` — the `pub use widgets::*` block (`:346` first entry, `:410` last), shifted by Phase 4's eight new `layout::` exports. Crate-root exports for the panel-builder methods and commands only; the four attribute types are already exported inside it.

**Constraints from prior phases:**
- **Phase 1:** the four attribute types already exist as `Arc<Appearance>` newtypes with `Reflect`, hand-written `PartialEq`, and per-attribute size assertions — they satisfy `CascadeAttribute`'s bounds as-is, and they are already re-exported from the crate root.
- **Phase 2:** every widget entity already carries all four `Cascade<Widget*Appearance>` components, `Cascade::Inherit` included (`reify.rs` `spawn_widget` `:296`, synchronized per channel by `update_widget_appearance` `:482`). That is precisely what `propagate_cascade` (`bevy_kana/src/cascade.rs:386-436`, its `Resolved<A>` removal branch at `:418-423` and its `resolve_from_queries` call at `:425-431`) needs in order not to strip `Resolved<A>`, so these registrations work on existing entities with no reify change.
- **Phase 8:** `CascadeRoot` (`src/cascade/resolved.rs:175`) carries a defaulted `combine` that replaces; the four appearance attributes override it with `Appearance::merge_over`. `cascade_plugin::<A>()` (`src/cascade/mod.rs:44`) already forwards `A::combine` to `CascadePlugin::with_combine`, so these four registrations merge per property with no extra wiring at the call site — register them exactly like `WidgetInteractivity`. `CascadeAttribute` is unchanged. **Register through `cascade::cascade_plugin::<A>()`, never `bevy_kana::CascadePlugin::new` directly** — `new` defaults `combine` to replace, so a direct registration compiles, propagates, and silently loses per-property merge. `Plugin::build` inserts `CascadeCombine<A>` (`bevy_kana/src/cascade.rs:240`, `with_combine` at `:279`) only when the resource is absent (`:297`), so pre-seeding it has the same silent effect. `render/mod.rs:395-398` already establishes a pre-seed-then-register pattern for other attributes — do not copy it for these four.
- **Invariant:** `missing_docs = "deny"` — the four attribute types, four builder methods, and eight commands all need doc comments.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase adds the public panel authoring surface and the runtime override/inherit commands, so both doc commands must pass before checkpoint.
- A global root-resource insert reaches every widget entity's `Resolved<…>` with no per-widget authoring.
- **Panel beats global** for each of the four states, asserted on `Resolved<…>`.
- **The merge holds in the reification frame, not only after a later `update`.** `resolve_inserted_cascade` (`bevy_kana/src/cascade.rs:364`) seeds `Resolved<A>` through `resolve_entity_cascade` (`:352`), which falls back to replace semantics when `CascadeCombine<A>` is missing, whereas `propagate_cascade` takes it as a `Res<…>` and would fail loudly. A mis-registration is therefore silent on the observer path only — assert the merged value in the insertion frame, mirroring `disabled_widget_is_marked_in_its_reification_frame`.
- Level-to-level merge holds: a panel bundle naming only `border_color` against a global default of `background` + `text_color` resolves to all three.
- Lifecycle tests cover a pre-existing application-owned `Cascade`/`Resolved` pair, precompose replacement, role removal, and role re-addition.
- Runtime `override_widget_*_appearance` / `inherit_widget_*_appearance` commands change the resolved value and restore inheritance.
- **Re-assert the part-local scope limit for `text_color`, now that a global default can actually reach a part.** Phase 7 added `global_text_color_default_stays_dormant_on_textless_widget_part` (`widgets/visual.rs:995`, not `layout/element.rs` as earlier drafts said), but it passes vacuously today: no `CascadePlugin` is registered for `WidgetHoveredAppearance` (`widgets/mod.rs:234` registers only `WidgetInteractivity`), so nothing propagates and the "dormant" assertion is trivially true. This phase registers all four channels, which is the first moment the test can fail. Re-assert it here: a global `hovered` default naming `text_color` resolves onto a widget part that has no text, and produces **no** `text_color` in that part's resolved override and **no** `PanelBuildError` — the `StateTextColorRequiresText` error is authored-scope only, never triggered by an inherited default.
- **Propagating an unchanged bundle does not dirty `Resolved<…>`.** Re-running propagation with no authoring change must leave `Resolved<Widget*Appearance>` unmarked, and Phase 3's presenter-isolation tests must pass unchanged. All three presenters already carry the four `Changed<Cascade<Widget*Appearance>>` dirty terms (`button.rs:145-148`), so a propagation that rewrites the `Arc` with a content-equal value wakes every presenter every frame — the Phase 1 constraint above is the cause, this gate is the effect that proves it.

### Retrospective

**What worked:** Mirroring `WidgetInteractivity` made every checklist item mechanical — four registrations, four builder methods, four seeds, four observers, four teardown entries, eight commands — and the phase landed with no design surprises. The two hazards the Work Order named ahead of time (`DuplicatePlugin` from a second registration in `HeadlessLayoutPlugin`, and silent replace-semantics from hand-written root resources or `CascadePlugin::new`) were both avoided because they were written down before dispatch.

**What deviated from the plan:** The merge of `main` at `b873d8f4` brought in `bevy_kana::CascadeRootResource`, which arrived after the Work Order was written. The four attribute types were left on `CascadeDefault<Self>` while every other attribute in the crate converted, and their `new` constructors were `pub(crate)` — so the global level the phase's own acceptance gate requires was unreachable from outside the crate. Resolved 2026-07-30: each type is now its own root resource with a public constructor, and `CascadeDefault` no longer appears anywhere in `hana_diegetic`. This is now the crate's convention for new cascade attributes.

**Surprises:**
- The `Cargo.toml` workspace glob `members = ["crates/*"]` matched an empty `crates/.claude/` editor-tooling directory, so cargo refused to load the workspace and the delegate's verification was blocked for a full pass. Removing the directory fixed it; the failure looked like a code error and was not one.
- `HeadlessLayoutPlugin::build` crossed clippy's 100-line limit when the four ownership-observer entries were added. Extracting the registrations into a private helper was the fix; `#[allow]` was not permitted.
- The acceptance gate's level-to-level merge bullet was initially satisfied only by a test whose expected value was computed by calling `Appearance::merge_over` — the same function under test — and whose panel bundles all authored `background`, so the "panel names only `border_color`" case never ran. Caught by blind review, fixed with a literal-expectation test (`widgets/appearance.rs:832`). The lesson generalizes: a cascade-merge assertion must spell out its expected value, never derive it from the merge.

**Implications for remaining phases:**
- Phase 10 consumes `Resolved<Widget*Appearance>` on the widget entity; all four channels now exist there on every widget, `Cascade::Inherit` included, and merge per property across global → panel → widget.
- Phase 10 and later phases that add a cascade attribute follow the root-resource convention (`impl CascadeRootResource<Self>`, `type Root = Self`, public `new`), not `CascadeDefault`.
- Registration for widget-scoped attributes belongs in `WidgetsPlugin` only. `HeadlessDiegeticUiPlugin` adds `HeadlessLayoutPlugin` and `WidgetsPlugin` together and `CascadePlugin` does not override `Plugin::is_unique`, so a second registration panics.

### Phase 9 Review

Nineteen findings, all mechanical — no decision was deferred to the user.

- **Phase 10 — `appearance()` reach.** `Widget*Appearance::appearance()` is module-private, so `visual.rs` could not get an `&Appearance` out of a `&Resolved<…>`. Spec now says: widen the four accessors to `pub(crate)`; do not invent an opaque resolved-side wrapper.
- **Phase 10 — presenter query arity.** The Work Order said to *add* four `Changed<Resolved<…>>` terms; `present_button_state` already carries a 9-term filter and an 11-element tuple, so that lands at 13/15 — Bevy's limit. Changed to *replace* the `Cascade` terms, which is behaviorally stronger (it also fires on a global or panel change).
- **Phase 10 — the widget-root block.** Files said the three presenters were "query-signature changes only" while the Spec said their `set(BUTTON_ROOT/…)` writers are what the phase replaces. Files now names the root block (`button.rs:219-231` and its two counterparts) explicitly; leaving it alone would mean the widget root never sees the global or panel level.
- **Phase 10 — merge matrix narrowed.** Phase 9 already proves global→panel and panel→widget. The matrix is now the widget→part hop plus one end-to-end row, citing the Phase 9 tests for the upper hops instead of rebuilding a 6 × 4 × 4 × 3 table.
- **Phase 10 — `Resolved<A>` does not `Deref`.** It is a tuple struct; access is `.0`.
- **Phase 10 — root-resource insert ordering** added as a constraint: inserting before `WidgetsPlugin` keeps the author's value, after overwrites the plugin's empty root. Phase 9's harnesses insert after; follow that.
- **Phase 11 — sweep widened by sixteen surfaces.** The `impl Into<Appearance>` sweep missed the eight runtime commands (`src/cascade/attributes.rs`, absent from Files entirely), the four panel builder methods, and the four public `new` constructors. `src/panel/field.rs` was in Files and should not be — Phase 9 never touched it.
- **Phase 13 — `tint` is the seventh property**, not the "sixth-and-final"; `Appearance` already carries six.
- **Line-reference drift**, corrected throughout: `bevy_kana/src/cascade.rs` shifted ~+50 by the `main` merge (which also gave `CascadePlugin` a second type parameter), `widgets/appearance.rs` by ~+130, `widgets/mod.rs` +4, `panel/builder.rs` +5, `panel/mod.rs` by the clippy helper extraction, plus `layout/element.rs` and `layout/builder.rs` refs in Phases 14 and 15. Phase 14's own "these refs are unaffected" note was wrong about two of them and now says so.
- **`CascadeDefault` scrubbed** from Phase 10's constraints and the index-growth risk — the type is gone from this crate.
- **Two Delegation Context invariants updated or added:** the test-count baseline is now 1156 passed / 2 skipped at Phase 9 via `verify.sh test` (a bare `cargo nextest run` reports a different number because it selects more targets); and a merge or cascade assertion must spell out its expected value literally rather than deriving it from the function under test — the defect blind review caught in this phase.
- **Harness note:** `HeadlessLayoutPlugin` alone registers only the five panel attributes, so any widget-appearance assertion under it passes vacuously. `WidgetsPlugin` must be added too.

### Phase 10 — Two-stage resolution and reification · status: done (`916ec2ee`)

#### Work Order

**Goal:** A resolved bundle reaches every element a widget owns, merged per property across global → panel → widget → part, with state layering applied only afterward.

**Spec:**

Resolution is **two-stage**, because `Cascade<T>` and `Resolved<T>` are per-entity components while parts are layout indices on one widget entity — a single `Resolved<T>` cannot carry a distinct value per part, and spawning an entity per part would add roughly eight entities, their relationships, and eight cascade components each per slider.

1. `CascadePlugin` resolves **global → panel → widget** on the widget entity, over the four attribute types (already wired in Phase 9).
2. Presentation resolves **part against widget** by reference: each sparse map entry is a part-local `Cascade<…>` resolved against the widget's `Resolved<…>`, through **one typed helper** rather than precedence spelled out in each presenter. **That helper already exists.** Phase 3 shipped `widgets::visual::resolve_part_overrides` (`widgets/visual.rs:390`), called identically by all three presenters (`button.rs:232`, `editable.rs:121`, `slider.rs:1202`) — it is already the single part-resolution seam. Extend it to take the four `&Resolved<Widget*Appearance>` as parameters. **Do not write a second helper in `src/cascade/`**: that duplicates the seam Phase 3 established and leaves the three presenters resolving through two different paths.

**Stage 2 is a reduction over an ordered bundle slice, not a two-argument merge.** For one state channel at one recipient, resolution takes the authored levels **lowest precedence first** as `&[&Appearance]` and reduces them with `Appearance::merge_over` (Phase 8). It does **not** take a widget bundle and a part bundle as two named parameters, and no presenter inlines the precedence.

```rust
/// Reduces one state channel's authored levels into a single bundle.
///
/// `levels` is ordered lowest precedence first, so the last entry wins per
/// property. The arity is the caller's business: today a recipient supplies
/// the widget's resolved bundle and, when it has one, its own part bundle.
fn merge_levels(levels: &[&Appearance]) -> Appearance {
    levels
        .iter()
        .rev()
        .copied()
        .fold(Appearance::new(), |lower, higher| lower.merge_over(higher))
}
```

`merge_over` is *lower wins over higher*, so the reduction runs from the most specific end and accumulates the winner. Either traversal direction is acceptable as long as **one** function owns the orientation and every call site goes through it.

The slice has exactly two entries today. **Do not encode that arity anywhere**: no `(widget, part)` parameter pair on the resolved-side state view, no two-branch `if let` precedence in `button.rs` / `slider.rs` / `editable.rs`. The three presenters pass their four `&Resolved<Widget*Appearance>` into `resolve_part_overrides` (`widgets/visual.rs:390`) and the slice is built inside `visual.rs`. Reason: a further level is then a longer slice and nothing more. A two-parameter signature turns that into a rewrite of the seam plus all three call sites plus the reduction. This is insurance, and it is required whether or not a further level is ever added — see the Delegation Context invariant on element-tree inheritance.

**Read the capability mask; do not destructure it away.** `resolve_part_overrides` currently binds `&(element_index, _)` at `widgets/visual.rs:377`, discarding the `VisualElementCapabilities` that `WidgetVisualSlots::elements` (`:120`) carries. Bind it and use it: a recipient whose mask can present **no** property the reduced bundle names produces no `VisualOverrideIndex` entry. This is the only bound on the index growth this phase's skip inversion creates, and it is the gate the dormancy acceptance lines depend on.

**Then, and only then,** layer the active states in `LAYER_ORDER` (`widgets/appearance.rs:555`, `[Focused, Hovered, Pressed, Disabled]`) and build the record override. The two axes must not be interleaved.

**This phase needs two state views, not one.** `WidgetStateCascades<'a>` (`widgets/appearance.rs:421`) holds `&'a Cascade<Widget*Appearance>` and its `layer` (`:452`) reads through `Cascade::as_override()`. Presentation here reads `Resolved<Widget*Appearance>`, which is a **tuple struct with no `Deref`** (`bevy_kana/src/cascade.rs:265`) — access is `.0` — and is never a `Cascade`, so the resolved path needs its own view over four `&Appearance` (or four `&Widget*Appearance`).

**Reaching `&Appearance` from `visual.rs`: widen the four accessors.** `Widget*Appearance::appearance()` (`widgets/appearance.rs:245`/`:287`/`:329`/`:371`) is module-private, so `visual.rs` cannot get an `&Appearance` out of a `&Resolved<Widget*Appearance>` as the code stands. Make those four `pub(crate)`. Do **not** invent an opaque resolved-side wrapper in `appearance.rs` to carry them across — the crate already passes `&Appearance` internally, and a second wrapper type buys nothing the visibility change does not. The authored view must stay, for two reasons: `resolve_part_overrides` calls `cascades().resolve(...)` on the part's **authored** `StateAppearance` (`visual.rs:391`), and `CommonEl::default_state_surfaces` (`layout/builder.rs`) calls `any` (`:317`) to decide which records to synthesize. (Phase 5 deleted the build-time validation this paragraph used to cite, but Phase 7 added a second production caller: `validate_part_state_colors` (`layout/element.rs:1348`) calls `cascades.any(…)` at `:1367` and `:1372` to decide whether a part authored `text_color` / `path_color` in any state. So the authored view has two live callers, not one, and deleting it breaks the part-local build errors as well as the synthesis path.) Factor the shared `LAYER_ORDER` fold so both views call one implementation rather than duplicating `layer`/`resolve`.

Both hops use `Appearance::merge_over` from Phase 8. For one element in one state:

1. Cascade resolves the widget's bundle down the levels (global → panel → widget).
2. For each property: the part's value if the part names it, else the widget's resolved value, else the ordinary look.
3. Record-specific render routes consume only the properties they can present; the rest are **dormant** at that element.

**Invert `resolve_part_overrides`'s skip.** Today the merge-walk `continue`s for any recipient with no `part_appearances` entry (`widgets/visual.rs:379-390`), so a widget-level bundle would reach nothing. Every recipient must now receive the widget's resolved bundle whether or not it has a part entry; a part entry, when present, merges over it.

**That inversion makes `VisualElementCapabilities` load-bearing for the first time.** The mask is stored in `WidgetVisualSlots::elements` (`widgets/visual.rs:84`, read at `:120`) but the merge-walk destructures it away — `&(element_index, _)` at `:355` — and **no production code reads it today**; Phase 2 built it and nothing has consumed it since. Wire it here: a recipient whose capabilities cannot present any property the resolved bundle names must produce no `VisualOverrideIndex` entry. Without this the dormancy gate below cannot pass, because every recipient would now get an entry.

**Named risk — index growth.** Once every recipient receives the widget bundle, one global root-resource insert naming a single property produces index entries proportional to widgets × recipients, where today it produces none. **The multiplier applies only when some level actually authors:** with nothing authored anywhere, Phase 8's `root_default()` hands every widget the shared `EMPTY_APPEARANCE`, the reduction yields `VisualSlotOverride::default()`, and the existing filter at `visual.rs:413` drops it. Do not add a redundant "bundle is empty" guard. The capability mask is the only bound on that, which is the second reason it must be wired in this phase rather than deferred. **Since Phase 5 the recipient set is no longer fixed by ordinary declarations:** `element_visual_capabilities` derives `SDF_FILL` / `SDF_BORDER` from `background.is_some()` / `border.is_some()` (`element.rs:1319`), and a structural container authoring `.hovered(Appearance::new().background(X))` now gets a synthesized fill — so it becomes a full SDF recipient where it was previously a build error. State authorship can create recipients, so the multiplier is larger than the pre-Phase-5 estimate.

**Reification.** Widgets already receive `CascadeFrom::new(panel)` on spawn (`bevy_kana/src/cascade.rs:197`) and `update_widget` (`reify.rs:352`) repairs a wrong relationship. The existing order is cycle-free: `CascadeSet::Propagate → PanelSystems::ComputeLayout → WidgetSystems::Reify → ReifyCommandsApplied → presentation`, with `ReifyCommandsApplied` flushing both the widget insertions and the `resolve_inserted_cascade` observer (`bevy_kana/src/cascade.rs:339`) that seeds `Resolved<A>` — the existing `disabled_widget_is_marked_in_its_reification_frame` test already proves same-frame behavior for `WidgetInteractivity`.

This phase must additionally:
- Keep `CascadeFrom::new(panel)` in the same deferred insertion.
- Order presentation after **both** `CascadeSet::Propagate` and `WidgetSystems::ReifyCommandsApplied` — the set declarations are `widgets/mod.rs:143`, the `configure_sets` call is `:238-268`, and the presenters are added at `:299-307` (Phase 3 removed the `.run_if(...)` that used to sit on each of the three).
- Add the four `Changed<Resolved<…>>` filters and the part map to presentation's dirty inputs.
- **The removal question is settled: a live widget never loses its four `Resolved` caches, so omit their removal streams and take `&Resolved<…>`, not `Option<&…>`.** `propagate_cascade` removes `Resolved<A>` only when the entity has no `Cascade<A>` (`bevy_kana/src/cascade.rs:418-423`); `spawn_widget` (`widgets/reify.rs:296`) inserts all four channels unconditionally and `update_widget_appearance` (`:482`) only replaces them; and Phase 8's `root_default()` guarantees a value with nothing authored. A query requiring all four caches is therefore correct.

**Documentation.** Update `docs/hana_diegetic/widgets-deferred.md` in this phase: replace "direct widget-builder inputs" with global/panel/widget/part appearance authoring; remove global-versus-instance placement and state-dependent child addressing from the open questions; keep presets, named variants, later widget states, extended materials, animations, slider geometry, and tooltip reuse deferred. Its stale current-plan link is already fixed — it now points at `as-built/widgets.md`.

**Files:**
- `src/widgets/visual.rs:390` — extend Phase 3's existing `resolve_part_overrides` (the part-against-widget seam) to take the four `&Resolved<Widget*Appearance>`; invert the no-part-entry skip (`:398-410`, the two `continue`s at `:406`/`:409`); read `VisualElementCapabilities` at `:398` instead of discarding it; the default-drop filter is at `:413`. **No new helper in `src/cascade/`.**
- `src/widgets/mod.rs:242` (`configure_sets`) and `:303`/`:306`/`:309` (the three presenter registrations) — presentation ordering after `Propagate` and `ReifyCommandsApplied`.
- `src/widgets/button.rs:232`, `src/widgets/slider.rs:1202`, `src/widgets/editable.rs:121` — the three `resolve_part_overrides` call sites; each passes the four `&Resolved<…>` in. **Swap `Cascade` → `Resolved` in the query; do not add a second set of terms.** `present_button_state` (`button.rs:139`) already carries a **9-term** `Or<>` filter including all four `Changed<Cascade<Widget*Appearance>>` and an **11-element** data tuple holding the four `&Cascade<…>`. Adding four more of each yields 13 filter terms and a 15-element tuple, at Bevy's arity limit. Replacing them keeps the arity flat and is behaviorally equivalent: propagation rewrites `Resolved<A>` whenever `Cascade<A>` changes, so `Changed<Resolved<A>>` is the strictly better dirty term — it also fires on a global or panel change, which `Changed<Cascade<A>>` does not.
- **Each presenter's widget-root block, not only the part seam.** `button.rs:219-231` builds `WidgetStateCascades::new(hovered, pressed, focused, disabled)` from the widget's own four `&Cascade<…>`, calls `appearance.resolve(&active, panel)`, and writes `desired.set(VisualSlotId::BUTTON_ROOT, …)`; `editable.rs` and `slider.rs` carry the same shape. That block is what this phase reroutes to read `Resolved` and write through `set_element` — if it is left alone, the widget root never sees the global or panel level and the phase's own acceptance gate cannot pass. The resolution logic still stays in `visual.rs`.
- `src/widgets/appearance.rs:406-560` — add the resolved-side state view alongside the authored `WidgetStateCascades<'a>` (`:421`) and share one `LAYER_ORDER` (`:555`) fold between them; widen the four `appearance()` accessors (`:245`/`:287`/`:329`/`:371`) to `pub(crate)`. `resolve` (`:489`) composes the merged bundles in `LAYER_ORDER` after level resolution, not during it. **This is a smaller change than it sounds**, though not for the reason previously recorded here: Phase 3 rewrote `resolve`, and it does **not** layer against an `Appearance::default()` accumulator. It accumulates four `Option<&T>` per-property winners across the `LAYER_ORDER` loop (`:443`) and builds a `VisualSlotOverride` directly (`:432-469`), taking `panel: Option<&DiegeticPanel>` for border-width conversion. It does already keep the two axes separate. What changes is only where each layer comes from — the resolved bundles passed in, instead of `layer(state)` (`:395`) reading this record's own `Cascade`s. Do not rewrite the layering algorithm.
- `docs/hana_diegetic/widgets-deferred.md` — the four documentation edits above.

**Constraints from prior phases:**
- **Phase 9:** the four `CascadePlugin` channels, panel builder methods, and typed commands all exist; `Resolved<Widget*Appearance>` is present on every widget entity, merged per property across global → panel → widget. Registration is in `WidgetsPlugin` **only**; ownership observers and construction seeding are in `HeadlessLayoutPlugin`. **There are no `CascadeDefault` resources** — each attribute type is its own root resource (`impl CascadeRootResource<Self>`, `type Root = Self`), authored by `app.insert_resource(WidgetHoveredAppearance::new(Appearance::new().background(…)))`. Grepping for `CascadeDefault` in `hana_diegetic` returns nothing.
- **Phase 9 — root-resource insert ordering.** `CascadePlugin::build` (`bevy_kana/src/cascade.rs:337-343`) inserts the root only when the resource is absent, and the root type is now the attribute itself. Inserting `WidgetHoveredAppearance::new(…)` **before** `WidgetsPlugin` keeps the author's value; **after** overwrites the plugin's `EMPTY_APPEARANCE`. Both reach the same end state, but a test must pick one deliberately — Phase 9's tests all insert *after* the plugins, via `cascade_test_app` (`widgets/appearance.rs:635`) and `widgets_test_app` (`widgets/visual.rs:661`). Follow that. `propagate_cascade` takes the root as `Res<R>` and re-propagates on `default.is_changed()`, which is what makes this phase's live global-default mutation gate reachable at all.
- **Phase 8:** `Appearance::merge_over(&self, higher: &Self) -> Self` (`widgets/appearance.rs:138`) is the single merge used at both hops. **Orientation is fixed and documented: the receiver is the lower cascade level and its `To` fields win; an `Unchanged` field lets `higher` carry through.** Do not introduce a second merge with the opposite sense. The four `Widget*Appearance` types already implement `CascadeRoot` with a `combine` delegating to it, so no attribute type needs a `combine` in this phase. `CascadeRoot::combine` already makes stage 1 merge per property, and the root resource participates in that merge rather than acting as a fallback. Stage 2 calls `merge_over` directly — it is not a cascade hop and needs no `combine`.
- **Phase 2:** the sparse part map is sorted by element index, capability-masked, revision-scoped, and stored **separately** from the four root `Cascade` values — the root's bundle is the widget's own override and must not be applied a second time as a part override. Phase 2 also landed the entity shape this phase resolves through: `StateAppearance` is not a `Component`, `spawn_widget` inserts all four `Cascade<Widget*Appearance>` channels including `Cascade::Inherit`, `update_widget` synchronizes them per channel, and `WidgetStateCascades<'_>` is the borrowed view the presenters already use.
- **Phase 3:** presenters already merge-walk recipients and already own their `Changed`/`RemovedComponents` drains; this phase adds four more `Changed` filters to the drains they own, not to a run condition.
- **Phase 5:** part-local authoring is never rejected — a state property with no ordinary declaration gets a transparent record to replace, emitted by `CommonEl::default_state_surfaces` (`layout/builder.rs`) at element construction. Higher-level properties with no compatible recipient are likewise **dormant**, not errors. There is no appearance validation left to route them through.
- **Phase 7:** `text_color` and `path_color` are the fifth and sixth properties, and the merge covers both.

**Resolved (approved 2026-07-30): the widget level is delivered per element, and the slot channel survives as the geometry/named-piece channel.**

`WidgetVisualOverrides` has two channels carrying the same `VisualSlotOverride` payload: per-element (`set_element` `widgets/visual.rs:341`, read back via `element_overrides` `:361`) and whole-slot (`set` `:325`, read back via `slot_overrides` `:357`). `dispatch_visual_overrides` (`:527`) composes them in that order — slot is the baseline, the element override lays over it per property through `apply_element` (`:218`, applied at `:581`). `apply` (`:202`) is per-property `overlay.or(self)`; `apply_element` saves `offset`, delegates to `apply`, and restores `offset`, so an overlay naming `border_color` replaces it and one leaving it `Unchanged` preserves what was underneath, all without the overlay disturbing a thumb translation.

Binding consequences for this phase:

- **Write the resolved widget-level bundle through `set_element`, once per recipient.** Composition against the authored slot baseline is then the existing `apply_element` path — no new composition code, and both cascade hops share one code path with the part level.
- **Do not delete or bypass the slot channel.** A sweep of `crates/hana_diegetic/src` (2026-07-30) found exactly four production `set(` writers, all in presenters; every other hit is inside a `#[cfg(test)]` module. Three carry appearance and are what this phase replaces: `BUTTON_ROOT` (`button.rs:229`), `EDITABLE_ROOT` (`editable.rs:118`), `SLIDER_ROOT` (`slider.rs:1199`). The fourth does not: `SLIDER_THUMB` (`slider.rs:1225`) carries the thumb's `offset` — its position along the track, recomputed each frame from the slider value and converted layout-frame → render-frame at `slider.rs:1211-1215`. That is geometry, never produced by the appearance cascade, and the clear-on-reauthor semantics documented at `slider.rs:1203-1210` ride on it through `write_widget_overrides` (`:421`). It keeps writing through the slot channel.
- Slot is a name-to-index convenience over the element channel, not a distinct capability — the slider already resolves the name itself at `slider.rs:1196`. Collapsing the two channels into one is a separate mechanical change and is **not** in this plan.

Consequence recorded in Phase 13's **Constraints from prior phases**: its focus-border rework collapses to deleting the `!(disabled && slider.disabled_color.is_some())` guard at `slider.rs:1221-1222`.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic headless_widgets`
- **Stage 2 is arity-agnostic.** The part-against-widget merge is a single reduction over an ordered `&[&Appearance]` (lowest precedence first) living in `widgets/visual.rs`; no signature and no presenter names a widget bundle and a part bundle as separate parameters. A unit test drives it with slices of length **0, 1, 2, and 3** and asserts the most specific entry wins per property — the length-3 case exists specifically to prove the reduction is not arity-limited to today's two levels, so it must not be deleted as redundant. `rg -n 'merge_over' crates/hana_diegetic/src/widgets` shows exactly: the definition (`appearance.rs:138`), the four `CascadeRoot::combine` impls Phase 8 shipped (`appearance.rs:251`, `:279`, `:307`, `:335`), one reduction in `visual.rs`, and tests. No presenter and no second reduction.
- **The capability mask is read, not discarded.** `resolve_part_overrides` no longer binds `&(element_index, _)` (check that function specifically — the identical binding in the `dispatch_visual_overrides` subtree-seeding loop at `visual.rs:557` is **not** this phase's to remove; Phase 13 deletes it), and a recipient whose `VisualElementCapabilities` can present no property the reduced bundle names produces **no** `VisualOverrideIndex` entry — asserted by the three enumerated dormancy rows below, sharing their fixtures.
- **Merge matrix, table-driven.** For each of the six properties, all four widget × part combinations, asserting the resolved value at the part:

  | widget names it | part names it | resolved at part |
  |---|---|---|
  | no | no | ordinary look |
  | yes | no | widget's value |
  | no | yes | part's value |
  | yes | yes | part's value |

  Run the matrix for all four states at the **widget→part** hop — that is the hop this phase creates and the only one not already proven. The upper hops (global→panel, panel→widget) are covered by Phase 9's tests: `global_state_appearance_defaults_reach_every_widget_without_state_authoring` (`widgets/appearance.rs:734`), `panel_state_appearances_merge_with_globals_in_the_reification_frame` (`:767`), and `panel_hovered_appearance_preserves_global_properties_through_the_cascade` (`:833`). Add exactly one end-to-end row that carries a global-level property through panel and widget to a **part** — proving the two stages compose — and cite the Phase 9 tests for the rest rather than rebuilding a 6 × 4 × 4 × 3 table of which one hop is new work. Every expected value is written out literally, never computed by calling `merge_over`.
- **Generated-part exclusion.** A widget-level `focused` bundle naming `background` on an editable field, with **no** `editor_caret` / `editor_selection` declaration, reaches the field's own recipients and produces **no** `VisualOverrideIndex` entry for the generated caret or selection elements — those keep `EDITOR_CARET` / `EDITOR_SELECTION`. Its companion asserts the exclusion is scoped to the widget hop: the same fixture plus an `editor_caret` declaration **does** resolve at the caret, proving Phase 6's part surface still reaches generated elements.
- A part hovered bundle carrying **only** a border color keeps the widget's inherited hovered background and replaces only the border.
- A part naming the **ordinary value** for a property holds that property against a widget bundle.
- An **explicit empty part bundle resolves to the widget's inherited bundle at that element, identically to a recipient with no part entry** — its own named test. State it against the post-inversion path: Phase 3's default-drop filter (`visual.rs:392`) already makes an empty bundle produce nothing today, so the previous wording ("resolves identically to no part bundle") passes on the current tree without proving anything this phase builds.
- **Dormancy:** a widget bundle naming a property its recipient cannot present leaves that recipient unchanged, produces no error, and creates no `VisualOverrideIndex` entry for it. **Exactly three tests — the enumerated rows in the resolved block below**: `border_color` vs a text-only label, `material` vs an image-only element, `background` vs a text-only label with no background. Do not expand a property × recipient formula; most of its cells describe compatible recipients after Phase 7. Each fixture's element must author no state bundle of its own, or `CommonEl::default_state_surfaces` (`layout/builder.rs:587`) synthesizes the very record the test claims is absent and the assertion inverts.
- Every test in this phase authors its bundles through the public part-authoring surface Phase 4 shipped, never through `set_element_state_appearance` (`element.rs:475`, `#[cfg(test)]`). That helper assigns `element.appearance` after construction and is the one path that skips Phase 5's defaulting — a bundle placed through it gets no synthesized record, no capability bit, and no recipient, making the test structurally incapable of proving presentation.
- A test sources focused, hovered, pressed, and disabled from **four different levels**, including an explicit empty part bundle, and asserts `LAYER_ORDER` still governs.
- Runtime global-default and panel-override mutations repaint live buttons, sliders, and editable fields while widget state is unchanged; editable tests confirm pressed appearance never applies.
- A **first-update test** covers global, panel, widget-root, and part inheritance in the reification frame.
- **Phase 3's presenter-isolation tests pass unchanged** once the four `Changed<Resolved<…>>` terms are added: `button_press_edges_do_not_rebuild_slider_overrides`, plus the detector that removes `WidgetVisualOverrides` from the peer widget, drives the other, and asserts the peer's component is not re-inserted. Propagating an unchanged bundle must not dirty `Resolved<…>` and must not wake a presenter.
- `docs/hana_diegetic/widgets-deferred.md` carries none of the four stale statements.
- **Orchestrator-run live smoke, before checkpoint** (approved 2026-07-30; the "Headless only" carve-out). The plan owner launches `examples/widgets.rs` and confirms by eye that the button, slider, and editable field still render correctly in every state they can reach from the keyboard — focus, hover, disabled, and a focused field mid-edit. This phase sends a resolved bundle to *every* element a widget owns, which is the widest rendered-output reach in the plan and the same failure shape as Phase 6: lint, tests, trybuild, and the example build all passed there while the focused field rendered as an opaque black bar. Driving is **keyboard only** (`brp_extras_send_keys` / `brp_extras_type_text`) — no BRP mouse control. An auto window does not skip this; it pauses here for the launch.

**Resolved (recorded 2026-07-30): the dormancy matrix is three enumerated rows, not a formula.**

"One test per property × incompatible-recipient pair" was written against the
pre-Phase-7 capability derivation and most of its rows no longer describe anything.
`element_visual_capabilities` (`layout/element.rs:1319`) sets `SDF_MATERIAL` whenever
`background.is_some() || border.is_some() || has_text || has_draw` (`:1342`), so
expanding the formula would produce tests asserting dormancy on recipients that are
in fact compatible — passing for the wrong reason. Write these three rows and no
others:

1. **`border_color` vs a text-only label.** The label must author no state border of
   its own; otherwise `CommonEl::default_state_surfaces` (`layout/builder.rs:587`)
   synthesizes a transparent border at `:608-610` and the label becomes a legitimate
   `SDF_BORDER` recipient.
2. **`material` vs an image-only element.** After Phase 7 this is the only
   incompatible `material` recipient left — `SDF_MATERIAL` is absent only when the
   element has no background, no border, no text, and no draw shapes.
3. **`background` vs a text-only label with no background.** `SDF_FILL` is set only
   from `element.background.is_some()` (`:1321`), and Phase 5's defaulting does not
   close this: `default_state_surfaces` reads the element's **own** authored state
   bundles (`self.appearance`, `:588-595`) and synthesizes nothing for an element
   that authors none. An inherited widget-level `background` is therefore dormant
   there. The fixture's label must author no state background or state material of
   its own — either one triggers the `Color::NONE` synthesis at `:602-607`.

`text_color` and `path_color` are **not** Phase 10 rows. Phase 7 made them a build
error (`StateTextColorRequiresText` / `StatePathColorRequiresDraw`,
`panel/builder.rs:76`/`:79`) when authored on a part that cannot present them, so at
the part level they are not dormancy cases. They remain dormancy cases only on the
inherited path, where no error is raised — that is the Phase 9 gate line, not this
phase's.

**Resolved (approved 2026-07-29): the widget-level bundle does not reach editor-generated elements.**

The problem it settles: Phase 10 inverts the skip so every recipient receives the
widget-level bundle, and the generated caret and selection elements are legitimate
recipients — both branches of `add_caret` / `add_selected_text` give them a background
(`with_background_if_unset(EDITOR_CARET)` at `ime/editor.rs:1362`, `EDITOR_SELECTION`
at `:1329`, and the `None` branches at `:1340` / `:1371`), so they carry `SDF_FILL`.
Without a rule, one `widget_focused_appearance(Appearance::new().background(X))` would
recolor every caret and selection highlight in the panel.

The plan's original recommendation was to accept that and gate it — everything the
widget owns is the plan's premise, and generated parts are owned. **It was withdrawn
during the discussion.** The caret and
selection highlight signal cursor position and selection extent by contrasting with
the field's own background, so inheriting that background makes the caret vanish into
the box and renders selected text indistinguishable from unselected text. Both failure
modes fire on the most ordinary authoring line — a widget-level `focused` background.
That is a correctness defect, not a style preference, so it outweighs the
everything-the-widget-owns premise for these two elements specifically.

**The rule Phase 10 implements.** The widget-level bundle reaches every authored
recipient in the widget's subtree. It does **not** reach an element the editor
generates. Those elements keep their built-in look
(`EDITOR_CARET` / `EDITOR_SELECTION`, `ime/editor.rs:74`/`:76`) unless the author
writes a part declaration for them, which is exactly the Phase 6 surface
(`editor_text` / `editor_selection` / `editor_caret` / `editor_validation`).
Part-level authoring is unaffected by this exclusion — it still reaches them, and it
remains the only way to restyle them.

**Mechanism.** The four editor generation sites are already known and cited above:
`add_text` (`ime/editor.rs:1264`, called for committed/preedit runs and for the
validation message), `add_selected_text` (`:1271`, body at `:1326-1345`), and
`add_caret` (`:1285`, body at `:1348-1377`). Mark the elements those sites construct
and skip marked elements in the widget-level pass of `resolve_part_overrides`
(`widgets/visual.rs:390`) only — the part-level merge that follows must still apply to
them. Do not key the exclusion off "has no part entry": that is the condition the skip
inversion removes, and reusing it would defeat the inversion for every authored child.

Phase 6's four `editor_*` methods and their tests are unaffected — this decision
constrains only the new widget-level hop.

**Resolved (approved 2026-07-30): the mistake is removed from the authoring surface, not validated. No check is added to this phase — the work moves to a new Phase 11.**

The problem: Phase 7's part-local check `validate_part_state_colors`
(`layout/element.rs:1348`) runs from `LayoutTree::validate_widgets` (`:782`) at panel
build, and the generated editor elements do not exist then — they are minted at runtime
by `set_field_editing_content` (`:1033`). So
`.editor_selection(El::new().disabled(Appearance::new().text_color(RED)))` builds clean
and never applies: the accepted-but-ignored shape Phase 7 was written to eliminate. The
editable field itself is exempted anyway by the early return at `:1354`.

Validation — at build or at generation — was rejected in favour of making the mistake
unwriteable. **This phase adds nothing.** Phase 11 (below) narrows the authoring
surface so no property name is exposed on an editor part at all.

Binding on this phase: do **not** add a fifth arm to `validate_part_state_colors`, do
**not** validate in `set_field_editing_content`, and do **not** treat the accepted-but-
ignored `text_color` as a Phase 10 gate line. It is Phase 11's to remove.

**Ref corrections and added constraints (Phase 6 review):**
- **Re-verified at Phase 8 — these override every line number elsewhere in this Work Order, including its Spec.** The merge-walk destructure is at
  `widgets/visual.rs:398`, not the `:355` this phase's Spec cites.
  `resolve_part_overrides` **`:390`**, the two `continue` skips **`:406`** / **`:409`**,
  the skip range **`:398-410`**, and the default-drop filter **`:413`**. The
  subtree-seeding destructure at `visual.rs:557` cited in Phase 10's gate is correct.
- **Second index-growth multiplier.** Phase 6 added
  `self.widget_records = tree.computed_widget_records(result)` and the tooltip
  equivalent to `regenerate_commands` (`panel/diegetic_panel.rs`), so an
  appearance-only edit — classified `VisualOnly` by `visual_only_properties_changed`
  (`layout/element.rs`) — now rebuilds every `ComputedWidgetRecord` and re-inserts
  `WidgetVisualSlots`, waking `dispatch_visual_overrides` into a full index rebuild.
  Record this alongside recipients-per-widget under "Named risk — index growth".

### Retrospective

**What worked:**
- The two-stage split held: `merge_levels` (`widgets/visual.rs:504`) is the single
  reduction over an ordered `&[&Appearance]`, and state layering runs only on its
  result. No presenter calls `merge_over`.
- Keying the generated-element exclusion off `slots.is_generated_editor_element()`
  rather than "has no part entry" survived review unchanged, and the part layer
  still reaches generated elements.

**What deviated from the plan:**
- Clippy's 100-line limit forced `configure_widget_system_sets` out of
  `WidgetsPlugin::build` (`widgets/mod.rs`). Non-functional; the same diff adds the
  intended ordering edges (`WidgetSystems::ReifyCommandsApplied`,
  `cascade::CascadeSet::Propagate`).
- Export renamed `WidgetStateCascades` → `ResolvedWidgetStateAppearances`.

**Surprises:**
- Five of the first-pass tests passed while proving nothing. Three shapes recurred:
  an expectation derived by calling `merge_over`; a fixture whose axes made both the
  required and the forbidden algorithm return the same value; and a mutation that was
  not a dirty term for the presenter under test, so the presenter never ran. Every
  corrected test now carries a negative check.
- `Appearance::merge_over(&self, higher)` reads backwards: `self` wins per property
  and the parameter only fills gaps.
- `CommonEl::default_state_surfaces` (`layout/builder.rs:587`) synthesizes a
  transparent border / `Color::NONE` background for any element authoring its own
  state bundles, which inverts a dormancy fixture that authors one.

**Implications for remaining phases:**
- Phase 11 removes the accepted-but-ignored `text_color` on editor parts; Phase 10
  deliberately added nothing there, as its Work Order binds.
- Phase 13 still owns the `dispatch_visual_overrides` subtree-seeding destructure —
  `for &(element_index, _) in slots.elements()` inside the `subtree_color` branch, now
  at `widgets/visual.rs:650` under the guard at `:649`. Phase 10 left it alone by
  instruction; the `:557` cited in Phase 10's own Work Order was its pre-Phase-10
  location.
- Phase 10's test insertions moved line numbers in four files by **non-uniform**
  amounts, including within a single file. Every line ref in Phases 11–15 was
  re-verified against the working tree by the Phase 10 review and corrected in place;
  a later phase must not assume a flat offset when re-checking.
- Any later phase adding a cascade level extends the `levels` slice at the stage-2
  merge site — the reduction is arity-agnostic and
  `merge_levels_accepts_zero_through_three_authored_levels` guards that.

### Phase 10 Review

- **No remaining phase is redundant, mis-scoped, or mis-sequenced.** Phases 11–15 were re-read against the implemented tree; their goals, boundaries, and ordering all hold. Phase 13's focus-border section was already pre-trimmed to a deletion by Phase 10's own embedded resolution, and Phase 14 remains correctly independent of the appearance work.
- **Delegation Context test-count floor raised to 1172 passed / 2 skipped** (Phase 10 completion), re-measured with `verify.sh test hana_diegetic`. Phase 11 inherits that as its floor.
- **Delegation Context invariant added — *A test must be able to fail*.** Phase 10 shipped five passing-but-vacuous tests in its first pass, in three distinct shapes: expectation derived from the function under test (already an invariant), a fixture where the required and the forbidden algorithm return the same value, and a mutation that is not a change term for the system under test so it never ran. All three are now written out, with the fix pattern for each.
- **Phase 13 and Phase 14 each gained a constraint naming the vacuity trap specific to its own gate** — Phase 13's focused × disabled × dragging matrix (both branches must produce different colors, and every absence row needs a positive control), Phase 14's zero-drop assertions (each zero-drop frame also asserts the live row count; insertion stability asserts on the key, not a row count).
- **Every line reference in Phases 11–15 was re-verified against the working tree and corrected in place.** Phase 10's ~1000 new test lines shifted four files by **non-uniform** amounts, including within a single file: `layout/builder.rs` has three bands (+1, +20, +21), `widgets/visual.rs` three (+27, +47, +93), `layout/element.rs` a flat +14 from `EditorElementOrigin` on, and `widgets/slider.rs` drifted −1 to −4. Phase 13 carried the most stale refs, since it edits the two files Phase 10 changed most.
- **Four pre-existing wrong references were found and fixed in the same pass** — they predate Phase 10 and would have sent a delegate to the wrong code: `SliderFocusedThumbBorderColorRequiresThumbBorder`'s two raise sites and its producer (Phase 13), the `El<L, LayoutOnly>` `disabled` line (Phase 15), the second `with<L, Role>` method (Phase 15), and `RoleSealed` (Phase 11).
- **`AcceptsElement::with` is not a trait default method.** Phases 13 and 15 both described it that way; there are two **inherent** `with<L, Role>` methods, `LayoutBuilder::with` (`layout/builder.rs:1741`) and `WidgetBuilder::with` (`:1969`). Corrected wherever cited, since Phase 15's central difficulty is exactly this signature.
- **Verified non-impact, recorded so a later pass does not re-check:** `widgets/mod.rs`'s `configure_widget_system_sets` extraction and the new `ResolvedWidgetStateAppearances` type are referenced by no remaining phase; and Phase 11's `StateColors` change is orthogonal to Phase 10's generated-origin marking, because the marking happens downstream in `EditorPart::into_text` regardless of the caller-facing parameter type.
- **Phase 10's own Work Order was left byte-for-byte as dispatched**, including the `visual.rs:557` subtree-seeding ref that was correct at dispatch time. The Retrospective carries the current location (`:650`, under the guard at `:649`) for Phase 13 to use.

### Phase 11 — Narrow the editor-part authoring surface and accept bare colors · status: done

**Goal:** an editor part is authored by naming a state and giving it a color, with no property name exposed; and every state verb accepts a bare `Color` as shorthand for a background.

#### Work Order

**Spec:**

Two changes, one phase. Both remove ceremony; the first also removes a class of silent failure.

**1. `StateColors` for the four editor parts** (name TBD — the plan owner approved the shape, not the identifier).

`editor_text` / `editor_selection` / `editor_caret` / `editor_validation` (`layout/builder.rs:1096`/`:1111`/`:1126`/`:1139`) stop taking `El<L2, WidgetPart>` and take a colors-only value instead:

```rust
.editor_caret(StateColors::focused(CARET_FOCUSED).disabled(CARET_DISABLED))
.editor_selection(StateColors::focused(SELECTION_FOCUSED).disabled(SELECTION_DISABLED))
.editor_text(StateColors::focused(TEXT_FOCUSED))
```

The type exposes **state names only** — `focused`, `hovered`, `disabled` — and no property names. Each arm interprets the color for its own role: `editor_caret` and `editor_selection` paint the fill (`background`); `editor_text` and `editor_validation` paint the glyphs (`text_color`). This is what makes the accepted-but-ignored `text_color`-on-a-caret unwriteable rather than diagnosed — see Phase 10's resolved block.

**`pressed` is rejected by type, copying the pattern already in the tree.** An editable field never enters `WidgetState::Pressed`: its presenter builds only focused/hovered/disabled and maps a pointer press into `Hovered` (`widgets/editable.rs:102-110`). `StateColors::pressed(color)` therefore returns a **different type** — mirroring `El<L, WidgetPart>::pressed` returning `El<L, PressedPart>` (`layout/builder.rs:1189`) and `AcceptsElement<PressedPart>` existing only for `WidgetBuilder<'_, W> where W: Pressable` (`:1927`). The four `editor_*` arms do not accept that type; a button's or slider's part arm does. Existing precedent to match, including its compile-fail tests: `tests/trybuild/fail/editable_widget_has_no_pressed_state.rs` and `editable_widget_root_has_no_pressed_state.rs`.

**Cost accepted by the plan owner:** a caret or selection box can no longer be given a border through this surface, only a fill. Nothing in the repo authors one. A full-`Appearance` arm may be added later if a caller needs it — do not add one speculatively.

**2. `From<Color> for Appearance`, and state verbs take `impl Into<Appearance>`.**

A bare color means the background:

```rust
impl From<Color> for Appearance {
    fn from(color: Color) -> Self { Self::new().background(color) }
}
```

Every state verb that takes `Appearance` today changes to `impl Into<Appearance>`. There are **15 of them across five impl blocks** in `layout/builder.rs` — verified 2026-07-30; these line numbers override any conflicting ref elsewhere in this Work Order and in the Delegation Context, whose `builder.rs` state-verb ranges are stale:

| Block | `hovered` | `focused` | `disabled` | `pressed` |
| --- | --- | --- | --- | --- |
| `El<L, LayoutOnly>` (`:944`) | `:948` | `:956` | `:964` | `:973` → `El<L, PressedPart>` |
| `El<L, WidgetElement<W>>` (`:1047`) | `:1061` | `:1071` | `:1081` | — |
| `El<L, W: Pressable> WidgetElement<W>` (`:1148`) | — | — | — | `:1154` |
| `El<L, WidgetPart>` (`:1160`) | `:1164` | `:1172` | `:1180` | `:1189` → `El<L, PressedPart>` |
| `El<L, PressedPart>` (`:1195`) | `:1199` | `:1207` | `:1215` | `:1224` |

All 15 change, plus the panel-level and global surfaces Phase 9 adds. Missing the `LayoutOnly` and `WidgetElement<W>` blocks leaves the example's own button lines unable to take a bare color. Multi-property states keep the long form, which is correct — they are saying more than one thing:

```rust
.hovered(BUTTON_FILL_HOVERED)                     // was Appearance::new().background(…)
.pressed(BUTTON_FILL_PRESSED)
.focused(Appearance::new().border_color(A).border_width(B))   // unchanged
```

**Do not give buttons or sliders a `StateColors` equivalent.** A button is a fill *and* a border, so a single color per state is ambiguous — `examples/widgets.rs` proves it: `hovered` (`:1461`) and `pressed` (`:1462`) set only the fill, but `disabled` (`:1193`) sets fill and border color, and `focused` (`:1464`) sets border color and border width. The bare-color shorthand is the whole of their shortening.

**Files:**
- `src/layout/builder.rs` — the `StateColors` type and its pressed-role sibling; the four `editor_*` arms in `impl<L> El<L, WidgetElement<EditableField>>` (`:1090`; arms at `:1096`/`:1111`/`:1126`/`:1139`); all 15 state-verb parameters per the table in the Spec. Role machinery to mirror for the pressed sibling: `WidgetPart` (`:107`), `PressedPart` (`:111`), `impl<W: Pressable> AcceptsElement<PressedPart> for WidgetBuilder<'_, W>` (`:1927`).
- `src/widgets/appearance.rs` — `From<Color> for Appearance` beside `Appearance::new` (`:122`); the doctest at `:100` gains a bare-color line.
- `src/panel/builder.rs:422`/`:430`/`:438`/`:446` — the four panel-level verbs Phase 9 added (`widget_hovered_appearance`, `widget_pressed_appearance`, `widget_focused_appearance`, `widget_disabled_appearance`) take `impl Into<Appearance>` too. **Not `src/panel/field.rs`** — Phase 9 did not touch it and it holds no state verb.
- `src/cascade/attributes.rs` — the eight `CascadeEntityCommandsExt` commands Phase 9 added (`:58`, `:66`, … `:106`; impls from `:202`) each take `Appearance` by value and are part of this sweep.
- `src/widgets/appearance.rs:243`/`:286`/`:329`/`:372` — the four public `Widget*Appearance::new` constructors, the global authoring entry point, likewise.
- `crates/hana_diegetic/examples/widgets.rs` — the four editor lines (`:1286`-`:1291`) become `StateColors`; the two single-property button lines (`:1461`, `:1462`) become bare colors. Leave `:1193` and `:1464` in the long form.
- `tests/trybuild/fail/` — two new compile-fail fixtures (`.rs` + `.stderr` each): one proving a property name cannot be reached on an editor part, one proving `StateColors::pressed` is rejected by `editor_caret`. **Name both with the `editable_widget_` prefix** so they match the existing `tests/trybuild/fail/editable_widget_*.rs` glob (`tests/trybuild.rs:11`). A fixture matching no glob is never compiled and its gate line is vacuous. If a different prefix is chosen, `tests/trybuild.rs` must be edited to add the glob and added to this Files list.

**Constraints from prior phases:**
- **Phase 10:** the widget-level bundle does **not** reach editor-generated elements; part-level authoring is the only way to restyle them. That part-level surface is exactly what this phase narrows, so the two are consistent by construction — do not reopen the exclusion.
- **Phase 10:** no validation was added for the editor-part color mistake. This phase removes it from the type surface instead. If any `validate_part_state_colors` arm for editor parts exists when this phase starts, it was added in error; delete it.
- **Phase 7:** `StateTextColorRequiresText` / `StatePathColorRequiresDraw` (`panel/builder.rs:76`/`:79`) stay — they cover ordinary widget parts, which keep the full `Appearance` surface. This phase neither deletes nor extends them.
- **Phase 1:** a state verb **replaces** the whole bundle for its state. `StateColors` must keep that rule — a second `focused(…)` discards the first.
- **Phase 8:** `Appearance::merge_over` and the four `CascadeRoot::combine` impls operate on `Appearance`. `From<Color>` produces an ordinary `Appearance`, so nothing in the cascade changes. Do **not** make `Appearance` generic over a role — it is the cascade attribute type and that genericity would reach every `Cascade`/`Resolved` site.
- **Phase 9:** the panel-level and global authoring surfaces already exist when this phase starts, and there are **sixteen** of them beyond the fifteen element verbs: four panel builder methods (`src/panel/builder.rs:422-446`), eight runtime commands (`src/cascade/attributes.rs:58-106`), and four public `Widget*Appearance::new` constructors (`src/widgets/appearance.rs:243`/`:286`/`:329`/`:372`). All sixteen take `impl Into<Appearance>` in this sweep, or a bare color works on an element and not on a panel, a runtime command, or a global insert — the exact asymmetry this phase exists to remove. `src/panel/field.rs` is **not** involved.
- **Phase 4:** the sealed-role machinery this phase mirrors is already shipped. `ElementRole` is sealed via `RoleSealed` (`layout/builder.rs:1435`); a role is admitted to a builder only through an `AcceptsElement<Role>` impl, and `PressedPart`'s exists only for `WidgetBuilder<'_, W> where W: Pressable` (`:1927`). `StateColors`' pressed sibling copies that shape — a distinct type that the four `editor_*` arms do not accept. Do not add a runtime check as a substitute.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic trybuild`
- **Orchestrator-run docs gate** (this phase adds public API — `StateColors`, its pressed sibling, and `From<Color> for Appearance` — and `verify.sh` has no rustdoc verb): `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p hana_diegetic` passes clean before checkpoint.
- A compile-fail fixture proves no property name is reachable on an editor part.
- A compile-fail fixture proves `editor_caret` rejects the pressed-role value.
- A test proves `.hovered(SOME_COLOR)` and `.hovered(Appearance::new().background(SOME_COLOR))` resolve identically.
- A test proves `editor_text` paints glyphs and `editor_caret` paints the fill from the same `StateColors::focused(c)` input.

### Retrospective

**What worked:**
- The pressed-role-as-a-distinct-type pattern copied cleanly from the existing `PressedPart` machinery — `EditorStateColors::pressed` returns `PressedEditorStateColors`, which no `editor_*` arm accepts, so the mistake is unwriteable rather than diagnosed.
- The `impl Into<Appearance>` sweep across all 31 surfaces (15 element verbs, 4 panel verbs, 8 runtime commands, 4 global constructors) required no call-site annotation anywhere in the tree.

**What deviated from the plan:**
- The delegate first shipped `focused` and `pressed` as associated constructors while `hovered` and `disabled` were chainable setters, so state order was constrained and the natural pressed spelling (`.focused(A).pressed(B)`) produced a generic "no method named `pressed`" rather than the designed type mismatch. The plan owner approved converting all four to chainable setters. The authoring spelling is therefore `EditorStateColors::new().focused(…)` — one call longer than the Spec's example at line 1606.

**Surprises:**
- **The `verify.sh test hana_diegetic trybuild` gate line proves nothing.** The runner's sole test in `crates/hana_diegetic/tests/trybuild.rs` carries `#[ignore]` (deliberate — it takes 89 seconds and CI runs ignored tests separately), so the command reports `0 run / 0 passed / 1 skipped` and exits "no tests to run". Any phase whose acceptance depends on a trybuild fixture must instead run `cargo nextest run -p hana_diegetic --test trybuild --run-ignored all` and require **1 passed**.
- The first version of the bare-color equivalence test built its expected value with the same `Appearance::new().background(…)` expression the code under test uses — vacuity shape (1) from the Delegation Context invariant. Fixed by writing all six `Appearance` fields out literally.
- `impl Into<Appearance>` breaks a previously valid `.hovered(Default::default())` with an ambiguous-type error (E0283); callers must write `Appearance::default()`. Intrinsic to the shorthand, accepted, documented on the `From<Color> for Appearance` impl.

**Implications for remaining phases:**
- `EditorStateColors` and `PressedEditorStateColors` were inserted near the top of `layout/builder.rs` (roughly lines 126–219), so every reference below that point in Phases 12–15 has shifted downward. Phase 10 shifted one file by three different amounts in three bands — do not assume a uniform offset here either.
- Editor parts no longer accept an element. Any later phase that assumes `editor_*` takes `El<L2, WidgetPart>`, or that a state verb takes a bare `Appearance` rather than `impl Into<Appearance>`, is now wrong.

### Phase 11 Review

- **A new phase was inserted and the spine resequenced.** The old Phases 12/13/14 are now **13/14/15**; no `done` phase was renumbered, so every checkpoint commit message still matches its phase number. Every cross-reference in the document — Work Orders, Retrospectives, prior Review blocks, ranges — was updated in the same pass.
- **New Phase 12 — one named wrapper per color property.** Phase 13's `tint` would have collided with Phase 11's `From<Color> for Appearance`: a bare color always means *background*, so `.hovered(RED)` on an image part would compile, cascade, and paint a fill behind an opaque texture. The plan owner rejected documenting, refusing, or redirecting the shorthand and chose to remove the guess — `Background`/`BorderColor`/`TextColor`/`PathColor` newtypes reaching the verbs through a crate-owned `IntoAppearance` trait with `#[diagnostic::on_unimplemented]`, and no impl for `Color`. A blanket `impl<T: ColorProperty> From<T> for Appearance` was ruled out: it is E0119 against core's reflexive `impl<T> From<T> for T`.
- **Phase 15 must name a crate-internal promotion that survives it.** `EditorStateColors::into_editor_part` (`layout/builder.rs:174`) performs the `LayoutOnly → WidgetPart` promotion Phase 15 abolishes, privately, with no enclosing widget in the type. It is correct — the `editor_*` arms are the widget — but a delegate reading only "abolish the promotion" either breaks `editor_*` or widens the public relation to accommodate it.
- **Phase 15's acceptance was entirely vacuous and now is not.** Its whole proof is trybuild fixtures, and `verify.sh test hana_diegetic trybuild` cannot run them: the runner's sole test is `#[ignore]` by deliberate repo policy, so the command reports `0 run / 0 passed / 1 skipped`. Phases 12 and 15 now require `cargo nextest run -p hana_diegetic --test trybuild --run-ignored all` with **1 passed**. Delegation Context line 46 claimed the test carried **no** `#[ignore]` — the opposite of the truth — and was corrected.
- **Phase 13's `color`-consumer list was wrong by five files.** `VisualSlotOverride::color` has exactly four readers (`fill_batch.rs:1364`/`:1369`, `image_batch.rs:628`, `visual.rs:230`). The five other files the Work Order named have no `VisualSlotOverride` color read at all — their `with_color` hits are the unrelated `TextStyle::with_color`. The list was replaced with the verified set.
- **Phase 13's "delete the guard at `slider.rs:1217-1222`" would have deleted the thumb-offset slot write.** `:1221-1223` is the `SLIDER_THUMB` write the same Work Order says must survive. Corrected to line `:1217` only, in all three places the range appeared.
- **Phase 14 named one auto-id producer; there are five.** `PanelElementId::auto` is also minted at `layout/element.rs:838`, `:1161` (inside `clone_reminting_auto_ids_into`, the per-keystroke editor path the Work Order called its highest-churn generator without ever locating), and `:1314`. Change 2 is unimplementable unless all five move together.
- **Phase 14's "Ref corrections" block declared a stale set authoritative and contradicted itself four sentences later.** The Phase-6-vintage overrides were deleted rather than corrected, and the block now carries the re-verified values.
- **`AcceptsElement::with` does not exist** — corrected in Phases 13 and 15 for the third time. The trait (`builder.rs:1696`) declares only `type ChildBuilder` and `with_child_builder`; the method taking `El<L, LayoutOnly>` is `LayoutContentBuilder::with` at `:1718`. Phase 15's central difficulty is that signature, so the wrong name pointed at the wrong sealing boundary.
- **Every remaining line reference was re-verified and corrected.** Phase 11 inserted `EditorStateColors` and `PressedEditorStateColors` near the top of `layout/builder.rs`, shifting everything below by **+121**; `widgets/visual.rs` shifted in two bands (`+1`, then `+113`); `layout/element.rs` and `material_table.rs` did not move. The Delegation Context's `builder.rs` inventory was Phase-2 vintage and is now marked superseded with current values inline, and its `ime/editor.rs`, `widgets/appearance.rs`, `examples/widgets.rs`, and trybuild-fixture-count entries were corrected.
- **Two constraints added to Phases 13 and 15:** `.hovered(Default::default())` no longer infers (write `Appearance::default()`), and editor parts get no `tint` role — `EditorPartColorRole` has exactly `Fill` and `Text`, and Phase 13 must not extend it.
- **Phase 13's size gate must be measured, not asserted from the plan.** Tightening `size_of::<VisualSlotOverride>()` from `<= 184` to `== 184` is right in intent, but the number has to come from a printed measurement, or the assertion is the same vacuous shape it exists to close.
- **Rejected — the reviewer's request for a button or slider consumer of `PressedEditorStateColors`.** Plan line 1645 forbids giving buttons or sliders a `StateColors` equivalent; the "part arm" it refers to is the pre-existing `PressedPart` machinery. Not relitigated.
- **Rejected — removing the `#[ignore]` from `tests/trybuild.rs`.** That reverses deliberate repo policy (89-second compile; CI runs ignored tests as a separate job). The vacuity was a plan defect and was fixed in the gate lines instead.

### Phase 12 — One named wrapper per color property; a bare `Color` no longer authors a state · status: todo

#### Work Order

**Goal:** A state verb never guesses which property a color was meant for. `.hovered(RED)` stops compiling; `.hovered(Background(RED))` and `.hovered(TextColor(RED))` say what they set, and the compiler names the fix when an author writes the bare form.

**Spec:**

**Why this exists.** Phase 11 shipped `impl From<Color> for Appearance` = `Self::new().background(color)`, so a bare color at a state verb silently means *background*. Phase 13 adds `tint`, the property that multiplies an image's texture. From that point `.hovered(HOVER_RED)` on an image part compiles, validates, cascades, resolves — and paints a fill behind an opaque texture where nothing is visible. That is the same "accepted but does nothing" class Phase 11 removed from the editor-part surface. The fix is to stop encoding a property choice in an unwrapped `Color` at all, rather than to document the guess or reject one recipient kind.

**Delete `impl From<Color> for Appearance`** (`widgets/appearance.rs`, with its doc paragraph at `:229-231`). This partially reverses Phase 11's authoring shorthand and is intended to.

**Add one newtype per color property**, each a tuple struct over `bevy::prelude::Color`, each `Copy + Clone + Debug + PartialEq`, living beside `Appearance` in `widgets/appearance.rs`:

| wrapper | sets |
| --- | --- |
| `Background` | `Appearance::background` |
| `BorderColor` | `Appearance::border_color` |
| `TextColor` | `Appearance::text_color` |
| `PathColor` | `Appearance::path_color` |

`Tint` is **not** added here — the `tint` property does not exist until Phase 13, which adds the wrapper alongside it. `border_width` and `material` get no wrapper; they are not colors, and a state that sets them still writes `Appearance::new()`.

Each wrapper may carry `impl From<Color> for Background` and so on for its own ergonomics. **That does not make `.hovered(RED)` compile** — Rust never chains two user conversions, and a blanket impl that would is not writable (see below). No call site may depend on it.

**Replace `impl Into<Appearance>` with a crate-owned conversion trait.** The obvious spelling does not compile:

```rust
trait ColorProperty { fn apply(self) -> Appearance; }
impl<T: ColorProperty> From<T> for Appearance { … }   // E0119
```

It overlaps core's reflexive `impl<T> From<T> for T`, which already supplies `From<Appearance> for Appearance`; the compiler cannot rule out `Appearance: ColorProperty` and there are no negative bounds. So own the conversion instead:

```rust
#[diagnostic::on_unimplemented(
    message = "a bare `Color` does not say which property it sets",
    label = "wrap it: `Background({Self})`, `TextColor({Self})`, \
             `BorderColor({Self})`, or `PathColor({Self})`",
)]
pub trait IntoAppearance {
    fn into_appearance(self) -> Appearance;
}

impl IntoAppearance for Appearance  { fn into_appearance(self) -> Appearance { self } }
impl IntoAppearance for Background  { … }
impl IntoAppearance for BorderColor { … }
impl IntoAppearance for TextColor   { … }
impl IntoAppearance for PathColor   { … }
```

Every impl is written out — no blanket, no coherence conflict, and the convertible set is closed rather than inherited from whatever `From` impls exist. **There is deliberately no `impl IntoAppearance for Color`**; that absence is the phase. The `on_unimplemented` attribute (stable since Rust 1.78) is what turns an old bare-color call site into a directive error instead of a trait-bound dump, and it is a required part of this phase, not a nicety.

**Sweep every parameter that took `impl Into<Appearance>` to `impl IntoAppearance`.** Phase 11 introduced that bound in four groups:
- the **16** element state verbs in `layout/builder.rs` — `:1069`, `:1078`, `:1087`, `:1096`, `:1185`, `:1196`, `:1207`, `:1268`, `:1279`, `:1288`, `:1297`, `:1306`, `:1317`, `:1326`, `:1335`, `:1344`;
- the four panel-level state verbs in `panel/builder.rs` — `:422` / `:430` / `:438` / `:446`;
- the four `Widget*Appearance::new` constructors in `widgets/appearance.rs` — `:251` / `:294` / `:337` / `:380`;
- the runtime state-appearance commands.

Find the full set with `rg -n 'impl Into<Appearance>' crates/hana_diegetic` before editing; the counts above are a floor, not a census.

**`EditorStateColors` is unaffected.** Its setters take a bare `Color` by design (`layout/builder.rs:149`/`:155`/`:163`/`:169`) — the *method name* already says which property, so there is nothing to disambiguate. Do not wrap them.

**This does not replace Phase 7's part-local build errors.** A wrapper says which property the author meant; it cannot say whether the recipient can render it. `TextColor` on a part with no text is still `PanelBuildError::StateTextColorRequiresText`, and Phase 13 still adds the `tint` arm.

**Files:**
- `src/widgets/appearance.rs` — delete `impl From<Color> for Appearance` and its `:229-231` doc paragraph; add the four wrappers and the `IntoAppearance` trait with its five impls and `#[diagnostic::on_unimplemented]`; change the four `Widget*Appearance::new` bounds (`:251`/`:294`/`:337`/`:380`). The Phase 11 equivalence test at `:750` asserts the deleted `From<Color>` behavior and is replaced by a per-wrapper equivalence test.
- `src/layout/builder.rs` — the 16 state-verb bounds listed above.
- `src/panel/builder.rs:422`, `:430`, `:438`, `:446` — the four panel-level state verbs.
- `src/lib.rs` — the crate-root `pub use widgets::*` block (`:346-410`) gains the four wrappers and `IntoAppearance`.
- `examples/widgets.rs` (1702 lines) — every bare-color state call Phase 11 introduced becomes a wrapper call. `apply_state_appearance` is at `:1458`.
- `crates/hana_diegetic/tests/trybuild/fail/` — a new fixture proving `.hovered(RED)` fails, and that the `on_unimplemented` message appears in the `.stderr` snapshot. `tests/trybuild.rs` must already glob it, or the fixture is never compiled — name the file so an existing `compile_fail` glob matches.

**Constraints from prior phases:**
- **Phase 11 — this phase reverses part of what Phase 11 shipped, on purpose.** Phase 11's title says "accept bare colors"; that decision is superseded here. Its `From<Color> for Appearance` impl and the doc paragraph justifying it both go. Nothing else Phase 11 built changes.
- **Phase 11 — `Default::default()` at a state verb.** `.hovered(Default::default())` already fails with E0283 under `impl Into<Appearance>`; it still fails under `impl IntoAppearance`, and the fix is still `Appearance::default()`. `Appearance` implements the new trait, so no bundle call site changes.
- **Phase 11 — the trybuild gate line is a trap.** `verify.sh test hana_diegetic trybuild` reports `0 run / 0 passed / 1 skipped`, because the runner's sole test is `#[ignore]` (deliberate repo policy — 89-second compile, CI runs ignored tests separately). This phase's fixture is proved only by the `--run-ignored all` line in the gate below. **Do not remove the `#[ignore]`.**
- **Phase 11 — a fixture whose filename matches no glob is never compiled.** Phase 11's two new fixtures were named with the `editable_widget_` prefix precisely so the existing glob caught them. Apply the same check here.
- **Phase 7:** `text_color` and `path_color` already exist on `Appearance`, so `TextColor` and `PathColor` have real properties to set. `border_width` and `material` are not colors and get no wrapper.
- **Delegation Context → *A test must be able to fail*.** The per-wrapper equivalence test replacing `appearance.rs:750` must **not** build its expected value from the same expression the wrapper uses — that is exactly the vacuity the Phase 11 review caught in the first version of that test. Write all six `Appearance` fields out literally, once per wrapper, and give each wrapper a distinct color so a wrapper writing the wrong field cannot pass.

**Pending decision:** the four wrapper names.

`TextColor` collides with `bevy::prelude::TextColor`, and `hana_diegetic` re-exports its widget surface through the crate root (`lib.rs:346-410`), so an author who imports both preludes gets an ambiguity error at the `use` — in the one place this phase exists to make error messages better. `Background` and `BorderColor` need the same check against `bevy::prelude` and against the crate's own root exports. (`Text` and `Border` were ruled out at design time; both are already types in `layout/builder.rs`.)

Three options:
- **Keep the plain names and accept the collision** — authors who hit it write `use hana_diegetic::TextColor;` explicitly. Shortest call sites.
- **Prefix them `State`** — `StateBackground`, `StateTextColor`, `StateBorderColor`, `StatePathColor`. Collision-free and self-describing at the call site; four characters longer, and `.hovered(StateBackground(RED))` is wordy for the most common case.
- **Put them in a non-glob module** — `appearance::color::{Background, TextColor, …}`, kept out of the crate-root re-export. Short names, no collision, but every authoring file needs an extra `use`.

Recommendation: **prefix them `State`.** The wrappers appear only at state verbs, so the prefix reads as accurate rather than redundant, and it keeps them in the crate-root export where every other widget symbol lives. Resolve before dispatching this phase, and verify the chosen names against `bevy::prelude` before writing code either way.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **`cargo nextest run -p hana_diegetic --test trybuild --run-ignored all` reports `1 passed`**, and its `.stderr` snapshot for the new fixture contains the `on_unimplemented` message — proving both that the bare form is rejected and that the author is told what to write. The ordinary `verify.sh … trybuild` line cannot run this; see the constraint above.
- `rg -n 'impl Into<Appearance>' crates/hana_diegetic` returns nothing.
- `rg -n 'From<Color> for Appearance' crates/hana_diegetic` returns nothing.
- **One equivalence test per wrapper**, each asserting a literal six-field `Appearance` with a distinct color, so a wrapper that writes the wrong field fails.
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase removes a public trait impl and changes the signature of every state verb, so both doc commands must pass before checkpoint.

### Phase 13 — Remove `Slider::disabled_color` and the subtree channel · status: todo

#### Work Order

**Goal:** The blunt subtree-recolor path is gone, images get their own `tint` property, and the slider's focus border composes correctly against a cascaded disabled bundle.

**Spec:**

Delete `Slider::disabled_color` — the field (`widgets/slider.rs:172`), its constructor default (`:191`), its builder method (`:233`), its `El` forward — `El<L, WidgetElement<Slider>>::disabled_color` at `layout/builder.rs:1399` (its `impl` block opens at `:1364`), **not** the `El<L, WidgetElement<W>>::disabled` verb at `:1207` — and its test `disabled_color_recolors_every_slider_element_and_suppresses_focus_border` (`:5279`). **There is no crate-internal setter** — an earlier revision of this Work Order claimed one at `slider.rs:254`; that line is inside `fn validated()` and no `set_disabled_color` exists anywhere in the crate. Delete `WidgetVisualOverrides::subtree_color` (`widgets/visual.rs:335`), `set_subtree_color` (`:342`), the getter (`:347`), the `set_subtree_color` seeding call in `slider.rs:1177`, and its consumption in `dispatch_visual_overrides` (`visual.rs:649-661`).

With its only production producer gone, delete `VisualSlotOverride::color` (`visual.rs:201`), its overlay logic, and the `with_color` test helper (`:278`); move the text consumer to `text_color` and the draw-primitive consumer to `path_color`, both added in Phase 7; the image consumer moves to this phase's new `tint`. **The overlay logic lives in `apply` alone** (`:229`) — `apply_element` (`:245`) saves `offset`, delegates to `apply`, and restores `offset`, so it names no color field and needs no edit.

**Keep the `HashMap<usize, VisualSlotOverride>`.** The former instruction to delete it "only if Phase 3's element channel did not take it over" is now answered: it did. The map is built once at `visual.rs:648` and serves three producers — subtree seeding (`:649-661`), slot overlays (`:662-670`), and Phase 3's element channel (`:671-676`). Delete **only** the subtree branch at `:649-661`; the map and the other two producers stay.

**Add `tint` for images, in this phase.** Phase 7 deliberately left images alone because the generic `color` field they read is what this phase deletes, so the replacement lands with the removal. `tint` is a **seventh** `Appearance` property with capability `IMAGE` (the bit Phase 7 created) — `Appearance` (`widgets/appearance.rs:107`) already carries six `VisualChange` fields and `merge_over` (`:140`) six arms, so this adds a seventh to both, and to any merge matrix written as "for each of the six properties". It is a **separate property from `text_color` and `path_color` because it does something different**: `image_batch.rs:136` documents "Linear RGBA tint multiplied after texture sRGB decode" — the image route *multiplies*, where the text and shape routes *replace*. Naming one property for both operations was the reason `content_color` was cancelled; do not undo that here by routing images through `text_color`. **`tint` also gets a `Tint` wrapper** — Phase 12 added one newtype per color property and the trait impl that lets it reach a state verb, so a seventh property without its wrapper is unauthorable as a single-property state. Add `Tint(Color)`, its `impl IntoAppearance`, its crate-root re-export, and its literal-field equivalence test with the property itself.

**Size is net-zero.** Phase 7 asserted `VisualSlotOverride` at **184** bytes. This phase removes the generic `color` and adds `tint`, one field out and one in, so the assertion stays at 184. Any earlier text in this plan claiming a return to 144 is stale.

**Focus-border composition.** The thumb focus border cannot be suppressed by "a resolved disabled bundle exists" — under a cascade every state always resolves to something, so presence is always true, and a disabled bundle changing only a background would delete the focus border. Compose `Slider::focused_thumb_border_color` as a **focused-thumb layer before normal state composition**:
- a disabled `border_color: To(…)` **replaces** it,
- a disabled `border_color: Unchanged` **preserves** it,
- an element overlay without `offset` leaves the thumb translation alone.

**Phase 3 already satisfies all three, and Phase 10's resolved decision routes the widget level through the element channel — so this section's remaining work is a deletion, not a rewrite.** `apply_element` (`visual.rs:245`, applied at `:674`) is per-property `overlay.or(self)`: a named `border_color` replaces, an `Unchanged` one preserves, and `offset` is untouched because the overlay never names it. Remove the `!(disabled && slider.disabled_color.is_some())` clause from the condition at `slider.rs:1217` and let the resolved disabled bundle compose on its own. **Edit line `:1217` only.** The guard's `if` spans `:1217-1219`; `:1221-1223` is the `desired.set(VisualSlotId::SLIDER_THUMB, thumb_override)` write that carries the thumb `offset` and must survive — deleting the range `:1217-1222` takes it out. Do not hand-compose the layer.

Convert `examples/widgets.rs`'s slider (`add_slider` `:1204`, `.disabled_color` use `:1164`) to author its parts explicitly. **Write it in Phase 12's wrapper form** — a single-property state is a named wrapper (`.hovered(Background(SLIDER_TRACK_HOVERED))`), and only a state that sets two or more properties uses `Appearance::new()`. A bare `Color` does not compile. The slider's two deleted knobs map to `BorderColor` on the thumb's focused state and `Background` on the widget's disabled state, so both are expressible.

**Material churn contract.** Per-element authoring lets one hover transition swap materials on label, track, and thumb together. A compatibility-preserving swap updates material-table rows in place; an incompatible one removes and re-inserts records across batches (`render/fill_batch.rs:1358`, `render/batch_store.rs:201`), rebuilds text runs (`render/panel_text/batching.rs:430`, `render/analytic_paths/batching.rs:314`), despawns empty batches, and allocates entity, mesh, material, and storage buffers for new ones. Incompatible materials stay **permitted**, but this phase must:
- document compatibility-preserving swaps as the steady-state path,
- keep built-in defaults and examples compatibility-preserving,
- add a label/track/thumb transition test asserting **no batch-key move and no batch entity creation** for compatible materials,
- add one incompatible case asserting **only the affected members migrate**.

**Files:**
- `src/widgets/slider.rs` — delete `disabled_color` (field `:172`, default `:191`, builder `:233`; there is no crate-internal setter) and the `set_subtree_color` seeding call (`:1177`); rework focus-border composition in `present_slider_state` (`:1120`), which under the element-channel outcome means editing line `:1217` only to drop the `&& !(…)` clause — **not** deleting `:1217-1222`, because `:1221-1223` is the `SLIDER_THUMB` slot write that must survive; delete the `:5279` test.
- `src/widgets/visual.rs` — delete `subtree_color` (`:335`, `set_subtree_color` `:342`, getter `:347`), `VisualSlotOverride::color` (`:201`) and its overlay logic in `apply` (`:229`) only — `apply_element` (`:245`) delegates to `apply` and names no color field, the subtree branch of `dispatch_visual_overrides` (keeping its map — see the Spec above for the verified line ranges in that function), and `with_color` (`:278`). Phase 3 added seven `with_color` sites in this file, at `:1032`, `:2231`, `:2599`, `:2622`, `:2643`, `:2663`, `:2677`, whose assertions read `VisualSlotOverride::color` — they migrate to `text_color` / `path_color` / `tint` with the rest, each to the property matching its route. (Phase 11 shifted this file in two bands: `+1` from one added `use`, then `+113` from three added tests. The non-test items above did not move.)
- **The `color`-consumer list is exactly four read sites in three files** — re-verified 2026-07-30: `src/render/fill_batch.rs:1364` and `:1369`, `src/render/image_batch.rs:628`, `src/widgets/visual.rs:230`. Earlier revisions of this list also named `panel_text/batching.rs`, `panel_text/reify.rs`, `panel_shapes/batching.rs`, `analytic_paths/batching.rs`, and `widgets/tooltip.rs`; **none of those reads `VisualSlotOverride::color`** — their `with_color` hits are the unrelated `TextStyle::with_color`. Do not migrate them. `VisualSlotOverride::with_color` has 15 call sites in three files: `fill_batch.rs` ×7, `visual.rs` ×7, `widgets/reify.rs` ×1.
- `src/layout/builder.rs:1399` — remove the `El<L, WidgetElement<Slider>>::disabled_color` forward.
- `src/layout/element.rs:1362` — `validate_part_state_colors`, the Phase 7 part-local check. Adding `tint` means adding a third arm here (`IMAGE` capability, `PanelBuildError::StateTintRequiresImage`) alongside the `text_color` arm (`:1384`) and the `path_color` arm (`:1389`), and adding `tint` to `Appearance`'s per-property walk.
- `src/panel/builder.rs:73-79` — `PanelBuildError::StateTextColorRequiresText` (`:76`) and `StatePathColorRequiresDraw` (`:79`) are the two variants to copy for `tint`, together with their `Display` rows.
- `examples/widgets.rs:1164`, `:1204` — author slider parts explicitly.

**Constraints from prior phases:**
- **Phase 7:** `text_color` and `path_color` exist on `Appearance` and `VisualSlotOverride`, consumed by the text route and the `PanelDraw` route respectively. Images were deliberately left reading the generic `color` in Phase 7, because this is the phase that deletes it — so this phase both removes `color` and adds `tint` for them. Phase 7 asserted `VisualSlotOverride` at **184** bytes; this phase is net-zero on size (one field out, one in) and asserts the same number.
- **Phase 10:** every state always resolves to something under the cascade, which is exactly why "a disabled bundle exists" cannot gate the focus border. The resolved override reaching the thumb is an element override composed on top of the authored slot baseline (Phase 3), so the presentation-owned `offset` is already preserved unconditionally. **Resolved 2026-07-30 — Phase 10 delivers the widget level through the element channel**, so the composition this phase needs is inherited from `apply_element` (`widgets/visual.rs:245`) and the focus-border work is a deletion: remove the `!(disabled && slider.disabled_color.is_some())` guard at `slider.rs:1217-1222` and let the resolved disabled bundle's `border_color` replace or preserve on its own. Nothing here is written by hand.
- **Phase 11 — the authoring surface is already shortened when this phase starts.** The four `editor_*` arms take **`EditorStateColors`** (`layout/builder.rs:120`, re-exported at `lib.rs:169` and `layout/mod.rs:60`), not `El<L, WidgetPart>` — if this phase touches an editable field's parts, use that form. It is built `EditorStateColors::new()` and chained: `focused` / `hovered` / `disabled` are `const fn -> Self`, and `pressed` returns the sibling type `PressedEditorStateColors` (`:133`), which the `editor_*` arms reject by type.
- **Phase 12 — a bare `Color` no longer authors a state, and this phase's example migration must be written in wrapper form.** Every state verb takes `impl IntoAppearance`; there is no `From<Color> for Appearance`. A single-property state is written `.hovered(Background(SLIDER_TRACK_HOVERED))` — the wrapper names the property — and a state setting two or more properties still builds one `Appearance`. The wrapper set is `Background`, `BorderColor`, `TextColor`, `PathColor` (subject to Phase 12's pending naming decision), and **this phase adds the fifth, `Tint`, alongside the `tint` property it introduces**, with its own `impl IntoAppearance` and its own literal-field equivalence test. The slider's two deleted knobs map to `BorderColor` on the thumb's focused state and `Background` on the widget's disabled state.
- **Phase 12 — `EditorStateColors` was deliberately left alone.** Its setters still take a bare `Color`, because the method name already says which property. Do not wrap them.
- **Phase 11/12 — `Default::default()` does not infer at a state verb.** `.disabled(Default::default())` fails with E0283 under both the old `impl Into<Appearance>` and the current `impl IntoAppearance`; write `Appearance::default()`. This phase's slider migration authors bundles, so it will hit this.
- **Phase 11 — editor parts get no `tint` role, and `EditorPartColorRole` must not grow one.** `EditorPartColorRole` (`layout/builder.rs:220`) has exactly two variants, `Fill` and `Text`: `editor_caret` / `editor_selection` paint fill, `editor_text` / `editor_validation` paint glyphs. An editor part can express no border, no material, and — after this phase — no `tint`. The seventh `Appearance` property this phase adds stops at the `Appearance` / `VisualSlotOverride` layer; do not extend `EditorPartColorRole`.
- **Phase 10 — the slot channel is not removed.** This phase's title names the **subtree** channel (`subtree_color` field `widgets/visual.rs:335`, `set_subtree_color` `:342`, getter `:347`), which is a third thing. The whole-slot channel (`set` `:372`, `slot_overrides` `:404`) and Phase 3's element channel (`element_overrides` `:408`) both survive Phase 10 and this phase: `SLIDER_THUMB` (`slider.rs:1221`) still carries the thumb's `offset` through it, which is geometry the appearance cascade never produces. Do not let the `subtree_color|disabled_color` grep gate below sweep up slot-channel call sites.
- **Phase 10 — the focused × disabled × dragging matrix is the shape that goes vacuous.** Delegation Context → *A test must be able to fail* names three ways a passing test proves nothing; two of them are live here. Pick disabled and focused values that make "disabled replaces the focus border" and "disabled preserves it" produce **different** colors, or both branches of the matrix agree and neither can fail. And a row asserting a property is *absent* needs a positive control in the same frame proving the presenter ran — `ButtonPress`-style mutations that are not change terms for `present_slider_state` leave it asleep and the absence assertion passes on nothing.
- **Phase 4:** the slider's track, thumb, and label can carry their own bundles as `El<L, WidgetPart>`, which is what the example migration uses. The role is monomorphic; the `Slider` owner comes from the enclosing `WidgetBuilder<'_, Slider>`.
- **Phase 1:** a state verb **replaces** the whole bundle for its state — a second `hovered(…)` on the same element discards what the first authored. The example migration below authors several properties per state per part; each state must be built as one `Appearance` and passed in a single call, never as chained calls that each name one property. That chained form worked before Phase 1 and silently drops all but the last bundle now.
- **Phase 2:** structural containers are excluded from the recipient list, so the example's resolved overrides cover exactly root, track, thumb, and label.
- **Phase 4 — declaration order is forced.** `button`, `slider`, `widget`, and `editable_field` live in `impl<L> El<L, LayoutOnly>` (`layout/builder.rs:1065`; `editable_field` `:1109`, `button` `:1125`), so `El::new().disabled(...).slider(...)` does **not** compile. A widget root must declare its widget before any state verb; the example migration has to be written that way round.
- **Phase 7 — the part-local color check and its exemptions.** `validate_part_state_colors` (`layout/element.rs:1362`) runs from `LayoutTree::validate_widgets` (`:796`) at panel build. It returns early for anything that is not an owned widget part: `owning_widget.is_none() || element.widget.is_some() || element.editable.is_some()` (`:1367`) — so widget roots and editable-field elements are exempt, and the editor-generated subtree never reaches it at all (those elements are minted at runtime by `set_field_editing_content`, `:1047`, long after build validation). The `tint` arm this phase adds inherits every one of those exemptions; do not widen the early return to compensate.
- **Phase 4 — a part-authoring helper cannot be generic over the builder.** `LayoutContentBuilder::with` takes `El<L, LayoutOnly>` (`layout/builder.rs:1718`) — **`AcceptsElement` has no `with` method**; the trait (`:1696`) declares only `type ChildBuilder` and `with_child_builder`. So a helper that authors parts must take `&mut WidgetBuilder<'_, W>` for a concrete owner. `tests/trybuild/pass/typestate_helpers.rs::add_widget_content` is the worked example.

**Pending decision:** whether `SliderFocusedThumbBorderColorRequiresThumbBorder` survives.

**The premise changed in Phase 7 — re-argue before deciding.** Phase 5 abolished the "a state property needs its ordinary declaration" error class, and this decision was originally written as "except here". Phase 7 reinstated the class: `StateTextColorRequiresText` (`panel/builder.rs:76`) and `StatePathColorRequiresDraw` (`:79`) are exactly that shape, raised from `validate_part_state_colors` (`layout/element.rs:1384` / `:1389`). "It is the only one left" is therefore no longer the argument.

The distinguishing test is **synthesizability**. Phase 5 could delete its four errors because `CommonEl::default_state_surfaces` can mint the missing record: a transparent `Border::all(Px(0.0), Color::NONE)` or a transparent background is a real, inert record the state layer can then override. Text and `PanelDraw` cannot be synthesized — there is no such thing as an empty text run or a zero-path draw to hang a color on — so Phase 7 had to reject instead. The slider case falls on the **synthesizable** side: a thumb border is exactly the record Phase 5 already synthesizes. That is what makes deleting it consistent rather than an exception, and it is a stronger argument than the one this block originally carried.

`PanelBuildError::SliderFocusedThumbBorderColorRequiresThumbBorder` is still live: declared at `panel/builder.rs:73`, its `Display` row at `:1055`, raised at `layout/element.rs:827` and `:858`, produced at `widgets/slider.rs:5482`. It rejects `Slider::focused_thumb_border_color` when the thumb declares no `El::border` — the same condition, on the same record, that `CommonEl::default_state_surfaces` now handles by synthesizing `Border::all(Px(0.0), Color::NONE)`.

Two options:
- **Delete it** — remove the variant, its `Display` row, both raise sites, and the producer, and let the defaulting cover the thumb like every other element. One authoring rule instead of two.
- **Keep it as a deliberate exception** — a focused thumb border color with no thumb border is arguably a typo rather than a state-only role, and a transparent widened border on a slider thumb is invisible in a way an author would not expect.

Recommendation: **delete it.** Under the synthesizability test above it is the one rejection in the codebase whose missing record the builder can already mint, so it is a special case rather than a category. The recovery — declare the thumb border with its resting color — is exactly what `Appearance::border_width`'s doc already tells authors. Resolve before dispatching Phase 13.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase removes public API and rewrites doc examples that referenced it, so both doc commands must pass before checkpoint.
- **Live smoke (orchestrator-run — see Delegation Context → Headless only carve-out):** this phase converts the example's slider off `disabled_color` to explicit per-part authoring, so the plan owner runs the example and confirms the slider's disabled and hovered looks are unchanged before checkpoint. Keyboard only. The exact-override assertion below proves the resolved values; it cannot prove they render as intended.
- `rg -n 'SliderFocusedThumbBorderColorRequiresThumbBorder' crates/hana_diegetic` matches whatever the pending decision resolved to — nothing if deleted, or the variant plus a test asserting the exception is deliberate. The `subtree_color|disabled_color` grep below does not reach it.
- `rg -n 'subtree_color|disabled_color' crates/hana_diegetic` returns nothing, and `rg -n 'VisualSlotOverride::color|slot_override\.color' crates/hana_diegetic` returns nothing. **Do not grep bare `\.with_color\(`** — `TextStyle::with_color` (`layout/text_props.rs:480`) is an unrelated public builder with matches in 39 files including every example and `benches/fixtures/panels.rs`, so that pattern can never return nothing. `VisualSlotOverride::with_color` (`visual.rs:278`) is `#[cfg(test)]` with roughly 20 sites; `cargo check` plus the `slot_override.color` grep is what actually proves the migration.
- `VisualSlotOverride` is **still 184 bytes**, and this phase **tightens the assertion at `visual.rs:226` from `<= 184` to `== 184`**. Phase 7 shipped an upper bound, which cannot detect a shrink — exactly the failure this gate exists to catch, since this phase removes one field and adds one. **Print the real `size_of::<VisualSlotOverride>()` before tightening** and use the measured value; `== 184` written from this Work Order's expectation rather than from a measurement is the same vacuous shape the assertion is meant to close. If the measured number moves, a field was dropped or added that this Work Order does not describe. (Phase 11 added no field, so the assertion at `visual.rs:226` is unchanged going in.)
- **`tint` works on images and only on images.** A state `tint` on an image recipient multiplies its texture color and restores on state exit; a `tint` on a part with no `IMAGE` capability is a `PanelBuildError`, matching the treatment `text_color` and `path_color` got in Phase 7. `rg -n 'slot_override\.color' crates/hana_diegetic/src/render` returns nothing.
- A **focused × disabled × dragging matrix** is tested for both a background-only disabled bundle (focus border survives) and a border-authoring one (focus border replaced), asserting the thumb `offset` is unchanged in every case and that disabled remains the last normal layer. The matrix includes the pressed/dragging state and the frame that queues `SliderDrag` removal.
- The example's final resolved overrides for root, track, thumb, and label are asserted **exactly** — the headless harness produces no pixels, so visual equality is not a gate. **Enumerate the fields, do not assert the bundle wholesale.** Each migrated `disabled` bundle is checked field by field — `background`, `border_color`, `text_color` on the label, `path_color` on any part carrying a `PanelDraw` — with a separate `assert_eq!` per field reading `Some(expected)`. `disabled_color` used to dim a whole subtree with one value, so a single per-part assertion will pass while a property route is silently dropped: that is the vacuous-pass class Phase 7's review caught in the slider tests (`widgets/slider.rs:5310`, `:5341`), and the migration is where it recurs.
- Material churn: a compatible label/track/thumb transition causes no batch-key move and no batch entity creation; an incompatible one migrates only the affected retained members.

**Ref corrections (re-verified at the Phase 11 review, 2026-07-30) — these override every line number elsewhere in this Work Order, including its Spec:**
- `El<L, WidgetElement<Slider>>::disabled_color` → **`:1399`**, its `impl` block at **`:1364`**
- `impl<L> El<L, LayoutOnly>` → **`:1065`** (`editable_field` **`:1109`**, `button` **`:1125`**)
- `El<L, WidgetElement<W>>::disabled` — the verb **not** to delete → **`:1207`**
- `LayoutContentBuilder::with`, which takes `El<L, LayoutOnly>` → **`:1718`**. The
  `AcceptsElement<Role>` trait is at **`:1696`** and **has no `with` method** —
  it declares `type ChildBuilder` and `with_child_builder` only.
- `widgets/visual.rs`: `VisualSlotOverride::color` **`:201`**, `with_color` **`:278`**,
  `subtree_color` field **`:335`**, `set_subtree_color` **`:342`**, getter **`:347`**,
  the `by_element` map **`:648`**, the subtree branch **`:649-661`**
- the `disabled_color` test → **`:5279`** in `widgets/slider.rs`; the
  `SliderFocusedThumbBorderColorRequiresThumbBorder` producer → **`:5482`**
- `widgets/appearance.rs`: `Appearance` **`:107`**, `merge_over` **`:140`**
- **`src/render/image_batch.rs:628`** is both a `color`-removal site and this phase's `tint` site. It reads `slot_override.color.map_or(tint, linear_tint)`; the `rg -n 'VisualSlotOverride::color|\.with_color\('` gate cannot see it, so only `cargo check` or the explicit `slot_override\.color` grep in the gate catches a miss.

### Phase 14 — Stable material keys: no dropped material rows · status: todo

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

1. **Key on identity, not position.** Replace the `command_index` field with the element's `PanelElementId` (`ime/ids.rs:85`), already stored on every `Element` as `id: Option<PanelElementId>` (`layout/element.rs:109`) and readable via `element_id` (`:710`). Keep `panel` and `role`.

2. **Make `Auto` ids structural.** `PanelElementId::Auto` is minted from a flat per-build counter — `next_auto_id` declared at `layout/builder.rs:1635`, initialized in **three** constructors (`new` `:1763`, `with_capacity` `:1788`, and Phase 4's shared `from_root` `:1842`, which both `with_root` and `with_widget_root` route through), and minted by `take_auto_id` at `:1906` — so an unnamed element's auto id shifts on insertion exactly as the index did — change 1 alone fixes only *named* elements. Derive the auto id from the element's path through the layout tree instead of build order, so inserting a sibling above an unnamed element leaves that element's id unchanged.

   **There are five auto-id minting paths, not one.** `take_auto_id` is only the builder's. `PanelElementId::auto` is also called directly at `layout/element.rs:838` (inside `validate_widgets`), `:1161` (inside `Element::clone_reminting_auto_ids_into`, declared `:1146` — this is the per-keystroke editor path named below as the proving case), `:1314` (`validated_element_widget_owner`), and `:1378` (`validate_part_state_colors`), plus `LayoutTree::tooltip_add_text` (`:2181-2198`, counter parameter `:2185`). This change is unimplementable unless every one of them moves to the structural id together — a single leftover positional producer re-keys its elements on insertion exactly as before.

3. **Remove the growth lag.** A cold start with more surfaces than the initial 128-row capacity drops on frame 1 regardless of key stability, and a panel respawn changes `panel: Entity` and re-keys wholesale no matter what changes 1 and 2 do. Either promote a grown buffer in the same frame it is staged, or stop clamping the CPU append window to the active capacity and truncate the *upload* instead at `encode_material_table_upload` (`:1390-1399`) / `padded_rows` (`:509-517`). Both remove the drop; the second costs one frame of stale rows for the overflow.

4. **Widen the growth headroom.** `CAPACITY_HEADROOM_DIVISOR = 8` (`:114`, applied at `:826`) reserves 12.5%; a wholesale re-key needs ~100%. Raise it so a re-key of the current live set fits without growing.

**Named risk — `Named(String)` in a per-frame hash key.** `PanelElementId::Named` holds a `String` (`ime/ids.rs:87`). After change 1 that string is hashed once per SDF surface per frame in the render path, where the current key hashes a `usize`. Intern element ids to a `u32` handle for the render-side key, or measure and accept the cost — do not ship an unmeasured `String` hash into the per-frame loop.

**Drop-count amplification (verify, do not assume).** `append_sdf_record_materials` (`render/fill_batch.rs:988`, body running to ≈`:1030`) is atomic per surface: if `Border` hits the limit after `Fill` succeeded it calls `rollback_assignments_after` (`:1009`, `:1025`), returning the slot to `retired` with `reusable_at_frame: self.frame` — immediately reusable (`:665-668`). At the limit the next surface claims that freed slot for `Fill` and fails on `Border`, so `dropped_records` may increment once per surface rather than once per missing row, inflating the warned number. This is inferred from the code, not measured. Confirm or refute it while writing the zero-drop tests; if real, the gate below still holds, since the target is zero.

**Files:**
- `src/render/fill_batch.rs:167-175` — `SdfMaterialSourceKey`: `command_index` (`:171`) → element identity. `:988`-≈`:1030` — the paired Fill/Border append and its rollback path; all key construction sites move with the field.
- `src/render/material_table.rs` — `:114` `CAPACITY_HEADROOM_DIVISOR`; `:509-517` `padded_rows`; `:545` `source_slots`; `:617-632` the drop guard; `:785` `clear_with_active_capacity` and `:1213-1222` its caller; `:1205-1211` / `:1265-1268` the stage-then-promote lag; `:1390-1399` upload encoding; `:1417-1425` `warn_material_table_drops`; `:2502` `probe_rekey_drop`, the test the zero-drop gate promotes.
- `src/layout/builder.rs:1635` (declaration), `:1763`/`:1788`/`:1842` (the three constructors), `:1906` (`take_auto_id`) — auto-id minting becomes structural.
- `src/layout/builder.rs:2181-2198` — **a second minting path the original Work Order missed.** `LayoutTree::tooltip_add_text` mints `PanelElementId::auto` from a caller-held counter (parameter at `:2185`); structural auto ids must cover it or tooltip content keeps positional ids.
- `src/layout/element.rs:838`, `:1161` (inside `Element::clone_reminting_auto_ids_into`, `:1146`), `:1314` — **three further `PanelElementId::auto` call sites** outside the builder. `:1161` is the per-keystroke editor path (see the proving case below). All five producers must adopt the structural id together.
- `src/ime/ids.rs:85-103` — `PanelElementId`; add the interned render-side handle if that is the chosen answer to the named risk.
- `src/layout/element.rs:109`, `:710` — element id storage and accessor.
- `src/render/draw_order.rs:30-33` — `CommandIndex` loses this consumer; delete it only if no other consumer remains.
- `src/layout/element.rs:1362` — `validate_part_state_colors` (Phase 7) mints the id it reports with `PanelElementId::auto(...)` from the element's **tree index** — the `element_id` closure at `:1374-1379`, minting at `:1378` — not through `next_auto_id`. It is therefore a second, independent producer of auto ids and must move to whatever structural id this phase adopts; leaving it alone makes the `StateTextColorRequiresText` / `StatePathColorRequiresDraw` messages name an id the element does not carry.

**Constraints from prior phases:**
- **Independent of phases 1-13.** This is a render-layer defect in material-row identity; no widget appearance behavior depends on it and it gates none of the earlier phases. It is sequenced last because Phase 13 edits `render/fill_batch.rs:1358` and the color-property migration, and this phase should start from that settled tree.
- **Phase 10 — every zero-drop assertion needs a positive control.** `assert_eq!(dropped_record_count(), 0)` passes identically when the frame did no work at all. Delegation Context → *A test must be able to fail* is the rule; here it means each zero-drop frame also asserts the expected live row count, so a test that silently stopped exercising the re-key cannot read as a pass. The insertion-stability row has the mirror-image trap: assert on the key itself, never on a row count that is stable for the wrong reason.
- **Phase 13:** `VisualSlotOverride::color` is gone and all consumers read `text_color` / `path_color` / `tint`. Do not reintroduce a `color` read while touching the batching files.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **Zero drops under wholesale re-key.** A headless test drives a full re-key of a live source set across several frames and asserts `dropped_record_count() == 0` on **every** frame, including the re-key frame. The existing `probe_rekey_drop` test (`render/material_table.rs:2502`) is this test's starting point — it currently *demonstrates* the 72-row drop; it is promoted to a regression test asserting zero.
- **Zero drops on cold-start growth.** A test whose first frame requires more rows than the initial capacity asserts zero drops on that first frame — this is the case key stability alone does not cover.
- **Zero drops on panel respawn.** A test that despawns and respawns a panel with the same content asserts zero drops across the transition, since the `panel: Entity` field re-keys every row regardless of element identity.
- **Element identity survives insertion.** Inserting an element above an existing **unnamed** element leaves that element's resolved material key unchanged. Assert on the key, not on the row count — a stable row count with shifted keys passes by accident.
- **Warn reachability.** `warn_material_table_drops` (`:1417-1425`) stays a `warn!`, not `warn_once!`. With drops unreachable, one firing is a defect, and demoting it would hide the regression this phase exists to prevent.

**Ref corrections and added constraints (re-verified at the Phase 11 review, 2026-07-30):**
- `layout/builder.rs` drifted again in Phase 11, which inserted `EditorStateColors`
  and `PressedEditorStateColors` near the top of the file and pushed everything
  below down by **121 lines**. **These override every line number elsewhere in this
  Work Order:** `next_auto_id` declaration **`:1635`**; the three `next_auto_id: 0`
  inits **`:1763`** / **`:1788`** / **`:1842`**; `take_auto_id` **`:1906`**;
  `tooltip_add_text` **`:2181-2198`** with its counter parameter at **`:2185`**;
  `AcceptsElement<Role>` trait **`:1696`**; `impl<L> El<L, LayoutOnly>` **`:1065`**;
  `EditorPart::into_text` **`:644`** with its `common.id = None` at **`:645`**.
  A previous revision of this block declared `:1493` / `:1621` / `:1646` / `:1700` /
  `:1764` / `:2039` authoritative — that set was Phase-6 vintage and was already
  wrong when written; it is deleted, not corrected.
  `layout/element.rs:109` / `:710` / `:1362` and every `render/material_table.rs`
  ref are unaffected by Phase 11 — only that file's test module moved.
- **Add the editor content tree as the proving case.** `inline_editor_content_tree`
  (`ime/editor.rs:1146`) is now the highest-churn auto-id generator in the crate: it is
  rebuilt per keystroke with a varying element count (empty runs skipped, selection
  box present or absent, caret always, validation conditional), so every unnamed
  element after it re-keys on each edit. Its minting happens in
  `Element::clone_reminting_auto_ids_into` (`layout/element.rs:1146`, minting at
  `:1161`) — one of the five producers named in the Files list above.
  `EditorPart::into_text` sets `common.id = None` (`layout/builder.rs:645`), so an
  author cannot stabilize them with a `Named` id. Use this path as the case that
  proves structural ids actually work.

### Phase 15 — The state verbs require a widget in the type · status: todo

#### Work Order

**Goal:** `.hovered()` / `.focused()` / `.pressed()` / `.disabled()` cannot be called on a plain layout element at all — the element's type must already carry the enclosing widget — and every widget-child authoring site produces such an element.

**Spec:**

Today all four state verbs live on `impl<L> El<L, LayoutOnly>` (`layout/builder.rs:1065`, `disabled` at `:1087`), and calling one **promotes** the element to `WidgetPart`. That promotion is Phase 4's shipped builder-acceptance relation, and it means this compiles:

```rust
// A container that belongs to no widget. Nothing rejects it today.
El::new().disabled(Appearance::new().background(GRAY))
```

The plan owner's rule: **the element's type must carry the widget it belongs to.** A state verb on an element with no enclosing widget is not a runtime mistake to diagnose, it is a call that must not exist.

**This is type-level, not a build error.** Do not implement it by adding a `PanelBuildError` — that was considered and rejected. Move the four verbs off `El<L, LayoutOnly>` onto a role that only a widget-owned element can hold, so the call above fails to resolve rather than failing to build a panel.

**The hard part is the child-building closure, not the verbs.** `with<L, Role>` — three **inherent** methods, not a trait default: `LayoutContentBuilder::with` (`layout/builder.rs:1718`), `LayoutBuilder::with` (`:1862`), and `WidgetBuilder::with` (`:2090`); every other `with` ref in this plan is stale. `AcceptsElement` (`:1696`) has **no** `with` method at all — it declares `type ChildBuilder` and `with_child_builder` only, so a plan line reading `AcceptsElement::with` names a method that does not exist and points at the wrong sealing boundary. The closure accepts any role, and every child inside it starts life as `El::new()`, which is `LayoutOnly`. Moving the verbs without changing that closure makes widget parts unauthorable. The closure must hand out elements that already carry their owner, which is what makes this phase touch every widget-child call site and revisit Phase 4's acceptance relation.

**Scope boundary — this does not subsume Phase 7's build error.** The parts Phase 7 rejects (a grouping row inside a button; `editor_selection`, a rectangle holding text as a child) are already inside widgets and already carry the owner in their type. They pass this gate and still cannot present a `text_color`. Both mechanisms are needed; neither replaces the other.

**A crate-internal promotion survives this phase — name it or a delegate breaks it.** `EditorStateColors::into_editor_part` (`layout/builder.rs:174`, private, added in Phase 11) builds `El::new()`, writes `common.appearance`, and calls `into_role()` — exactly the `LayoutOnly → WidgetPart` promotion this phase abolishes, performed inside the crate with no enclosing widget in the type. It is correct: the four `editor_*` arms are the enclosing widget, and the caller cannot reach it. Whatever mechanism replaces the public promotion must leave a crate-internal construction path for it. Do not "fix" it by widening the public acceptance relation, and do not delete it — the `editor_*` surface Phase 11 shipped depends on it.

**Files:**
- `src/layout/builder.rs:1065` — the `impl<L> El<L, LayoutOnly>` block holding all four verbs (`hovered` `:1069`, `focused` `:1078`, `disabled` `:1087`, `pressed` `:1096`); the three inherent `with<L, Role>` methods, `LayoutContentBuilder::with` (`:1718`), `LayoutBuilder::with` (`:1862`), `WidgetBuilder::with` (`:2090`); the role markers `WidgetPart` (`:107`) / `PressedPart` (`:111`) behind sealed `ElementRole` (`:235`); `AcceptsElement<Role>` (`:1696`) and its five impls (`:1987`, `:2000`, `:2014`, `:2031`, `:2048`).
- `src/layout/builder.rs:174` — `EditorStateColors::into_editor_part`, the crate-internal promotion described above.
- `crates/hana_diegetic/tests/trybuild/` — the compile-fail fixtures for this phase, plus any existing `pass/` fixture that authors a state verb on a bare `El::new()` and would now correctly stop compiling. Phase 11 added two `fail/` fixtures that author `El::new().editable_field(…)` — `editable_widget_editor_part_has_no_property.rs` and `editable_widget_editor_part_rejects_pressed_colors.rs` — so if the acceptance relation moves, both `.stderr` snapshots need re-blessing.
- Every widget-child authoring site — enumerate with `rg -n '\.with\(' crates/hana_diegetic/src` and the example.

**Constraints from prior phases:**
- **Phase 4:** the acceptance relation and the five `AcceptsElement` impls are what this phase revises. `tests/trybuild/pass/typestate_helpers.rs::add_widget_content` is the worked example of a part-authoring helper; it takes `&mut WidgetBuilder<'_, W>` for a concrete owner because `LayoutContentBuilder::with` (`layout/builder.rs:1718`) could not be made generic over the builder. That constraint is what this phase has to solve properly.
- **Phase 4:** `button`, `slider`, `widget`, and `editable_field` live in `impl<L> El<L, LayoutOnly>`, so a widget root declares its widget **before** any state verb. That ordering is already forced and must survive.
- **Phase 7:** the build error for a state color on a contentless part is a separate mechanism and stays.
- **Phase 12:** every state verb takes `impl IntoAppearance` — a crate-owned conversion trait, **not** `impl Into<Appearance>` and not a bare `Appearance`. The convertible set is `Appearance` itself plus one newtype per color property (`Background`, `BorderColor`, `TextColor`, `PathColor`, and Phase 13's `Tint`); there is deliberately **no impl for `Color`**. The `El<L, LayoutOnly>` verbs this phase removes (`layout/builder.rs:1069`/`:1078`/`:1087`/`:1096`) carry that bound when this phase starts, and whatever replaces them must keep it, or every wrapper call site in `examples/widgets.rs` breaks. **Sixteen** verbs carry the bound in `layout/builder.rs`: `:1069`, `:1078`, `:1087`, `:1096`, `:1185`, `:1196`, `:1207`, `:1268`, `:1279`, `:1288`, `:1297`, `:1306`, `:1317`, `:1326`, `:1335`, `:1344`.
- **Phase 12 — the error message is part of the contract.** `IntoAppearance` carries `#[diagnostic::on_unimplemented]` telling an author to wrap a bare `Color`. This phase adds its own compile-fail fixtures on the *same* verbs; make sure the new rejection reports a missing widget rather than shadowing that message, and re-bless Phase 12's bare-color fixture if the wording moves.
- **Phase 11/12 — `Default::default()` does not infer at a state verb.** `.hovered(Default::default())` fails with E0283 because the verbs are generic; write `Appearance::default()`. This phase's `pass/` fixture authors bundles, so it will hit this.
- **Phase 11:** the four `editor_*` arms no longer take `El<L2, WidgetPart>` — they take `EditorStateColors` (`layout/builder.rs:120`), a colors-only value with no property names, and reject the distinct pressed-role sibling `PressedEditorStateColors` (`:133`) by type. That surface is settled; this phase revises the acceptance relation around it and does not widen it back to a full `Appearance`.
- **Sequencing:** last. It revisits the builder surface every earlier phase authors against, so running it before the cascade phases would mean rewriting their call sites twice.

**Acceptance gate:**
- `bash ~/.claude/scripts/delegate/verify.sh check hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh test hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh lint hana_diegetic`
- `bash ~/.claude/scripts/delegate/verify.sh example hana_diegetic widgets`
- **`cargo nextest run -p hana_diegetic --test trybuild --run-ignored all` reports `1 passed`.** This phase's entire proof is trybuild fixtures, and the ordinary `verify.sh test hana_diegetic trybuild` line **cannot run them**: the runner's sole test carries `#[ignore]` (`tests/trybuild.rs`), so that command reports `0 run / 0 passed / 1 skipped` and every acceptance row below would pass on nothing. Use the `--run-ignored all` form and require `1 passed`. **Do not remove the `#[ignore]`** — it is deliberate repo policy (89-second compile; CI runs ignored tests as a separate job).
- **A trybuild compile-fail fixture per verb** — `El::new().hovered(…)`, `.focused(…)`, `.pressed(…)`, `.disabled(…)` on an element with no enclosing widget each fail to compile, with the error naming the missing widget rather than a generic trait mismatch.
- **A trybuild pass fixture** proving every legitimate authoring shape still compiles: a widget root, a direct part, a part nested inside a child-building closure, and a part authored through a helper function.
- `rg -n 'impl<L> El<L, LayoutOnly>' crates/hana_diegetic/src/layout/builder.rs` shows that block no longer defines any of the four state verbs.
- `EditorStateColors::into_editor_part` (`layout/builder.rs:174`) still compiles and the `editor_*` tests still pass — the crate-internal promotion named in the Spec survives.
- No `PanelBuildError` variant was added — this phase's rejection is entirely in the type system.
- **Docs (orchestrator-run — see Delegation Context → Docs):** this phase changes the public builder surface, so both doc commands must pass before checkpoint.

## Outstanding items

<!-- Project state outside the phase spine. Not dispatched by /plan:delegate. -->

- **Uncommitted work.** Three rounds sat uncommitted on `feature/widgets` at `2f12a56d` — the `apply_state_appearance` / `_with` renames, the editable-field state fix (hover and disabled present on fields; `pressed_*` gated behind `HasPressedState`) with four new tests and a trybuild case, and the `HasPressedState` doc comment. These landed as `64f8bdc0`, which is current `HEAD`.
- **`docs/hana_diegetic/widgets.md`** — done. Rewritten as `docs/hana_diegetic/as-built/widgets.md`, current-state only (state appearance described as the four `Appearance` verbs, not the removed flat builders), and the old phased plan deleted. Inbound links in `surface-panels.md` and `widgets-deferred.md` repointed.
- **Widget demonstration checkpoint.** The retired widget plan ended with an undelivered discussion phase: decide with the owner how to demonstrate the whole widget system working together — buttons, sliders, tooltips, focus traversal, disabled state, panel ordering, and IME/text input coexisting on one panel — and name both the live demonstration and the deterministic integration gate, including the tooltip's final retained transform after first reveal and after a replacement creates a fresh controller. `examples/widgets.rs` is the cumulative baseline; do not reopen which example owns that path, remove either input-integration proof, replace the diagnostic rows, or change the established picking policies.
- **`WidgetElement<ImeEditableFieldSpec>`** — settled by Phase 4's `EditableField` marker.
- **`HasPressedState`** — renamed to `Pressable` in Phase 4. Resolved; no longer outstanding.
