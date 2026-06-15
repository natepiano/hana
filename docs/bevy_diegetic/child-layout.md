# Child Layout Typestate

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Builds a type-safe child-layout API where row/column can have gaps and dividers, overlay cannot, and the layout engine still stores a plain internal enum.

## Delegation Context

- **Project:** `bevy_diegetic` — workspace crate for retained-mode, Clay-inspired diegetic UI panels and text in Bevy 3D scenes.
- **Stack:** Rust 2024 workspace; Bevy `0.19.0-rc.2`; `bevy_diegetic` default feature `typography_overlay`; layout uses `bevy_kana`, `parley`, `smallvec`, and dev parity checks against vendored `clay-layout`.
- **Layout:** `crates/bevy_diegetic/src/layout/` — public `El` API, internal `Element` storage, sizing/positioning/wrapping/render commands; `crates/bevy_diegetic/src/layout/engine/*` — layout behavior/tests; `crates/bevy_diegetic/examples/` and `crates/fairy_dust/src/` — call-site migration; `docs/bevy_diegetic/child-layout.md` — implementation plan; `crates/bevy_diegetic/tests/trybuild/**` — compile-fail/pass fixtures if added.
- **Key files:** `crates/bevy_diegetic/src/layout/builder.rs:65` — current untyped `El` compatibility fields and setters; `crates/bevy_diegetic/src/layout/builder.rs:287` — `El::into_element` lowers through `ChildLayout::for_direction(...)`; `crates/bevy_diegetic/src/layout/builder.rs:407` — `LayoutBuilder::with_root`; `crates/bevy_diegetic/src/layout/builder.rs:473` — `LayoutBuilder::text_element`; `crates/bevy_diegetic/src/layout/builder.rs:509` — `LayoutBuilder::text_id_element`; `crates/bevy_diegetic/src/layout/builder.rs:519` — private `LayoutBuilder::add_text`; `crates/bevy_diegetic/src/layout/builder.rs:554` — `LayoutBuilder::image`; `crates/bevy_diegetic/src/layout/child_layout.rs:8` — internal `ChildLayout` enum; `crates/bevy_diegetic/src/layout/child_layout.rs:33` — compatibility lowering helper; `crates/bevy_diegetic/src/layout/child_layout.rs:96` — gap unit scaling; `crates/bevy_diegetic/src/layout/element.rs:75` — current `Element` storage; `crates/bevy_diegetic/src/layout/element.rs:600` — `LayoutTree::scaled`; `crates/bevy_diegetic/src/layout/element.rs:709` — child-layout change classifier; `crates/bevy_diegetic/src/layout/mod.rs:54` — layout public exports; `crates/bevy_diegetic/src/layout/sizing.rs:225` — current `Direction`; `crates/bevy_diegetic/src/layout/geometry.rs:189` — `Border::between_children`; `crates/bevy_diegetic/src/layout/engine/sizing.rs:45` — fit-size propagation; `crates/bevy_diegetic/src/layout/engine/sizing.rs:156` — top-down sizing; `crates/bevy_diegetic/src/layout/engine/sizing.rs:545` — row/column `is_layout_axis` helper that overlay must replace; `crates/bevy_diegetic/src/layout/engine/positioning.rs:473` — child positioning context; `crates/bevy_diegetic/src/layout/engine/positioning.rs:729` — between-child border emission; `crates/bevy_diegetic/src/layout/engine/wrapping.rs:240` — parent content width for wrapping; `crates/bevy_diegetic/src/layout/engine/integration_tests.rs:808` — existing gap/alignment tests; `crates/bevy_diegetic/Cargo.toml` — add `trybuild` dev-dependency if compile-fail tests are added; `crates/bevy_diegetic/examples/panel_draw_order.rs` — currently may be a text-fit probe with the old draw-order demo commented; rebuild the active overlay-based example in Phase 6; `crates/fairy_dust/src/camera_control_panel/layout.rs:205` — downstream helper returning bare `El`.
- **Build:** `cargo check -p bevy_diegetic --examples`; after call-site migration also run `cargo check --workspace --all-targets`.
- **Test:** `cargo nextest run -p bevy_diegetic`.
- **Lint:** `cargo clippy --workspace --all-targets`; `cargo +nightly fmt --all`; audit stale APIs with `rg -n "\\.direction\\(|\\.child_gap\\(|child_align_x\\(|child_align_y\\(|child_alignment\\(|child_gap\\(-" crates docs`.
- **Style:** `zsh ~/.claude/scripts/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_diegetic_gpu_meter`; repo has `origin` owned by `natepiano`, so use `cargo +nightly fmt --all`, never plain `cargo fmt`; run Cargo-family commands directly and keep `sccache`/`RUSTC_WRAPPER` intact.
- **Invariants:** Public API must make invalid overlay states unrepresentable: `El::overlay().gap(...)`, `.child_gap(...)`, `.direction(...)`, and child dividers must not compile; internal layout engine stays non-generic and lowers `El<L>` to a plain internal `ChildLayout`; `Row`, `Column`, `Overlay`, and `ChildLayoutState` are public where `El` is public, while internal `ChildLayout`/lowering trait stay hidden; row/column behavior remains unchanged; overlay has no gap, sizes by max child extent on both axes, positions every child in the content box, uses independent X/Y scroll extents, and never emits between-child dividers; text/image leaves normalize inert child layout state; `DrawZIndex` semantics are unchanged; negative-gap overlap patterns migrate to `El::overlay()` rather than `.gap(...)`.

## Phases

### Phase 1 — Internal `ChildLayout` for row and column  · status: done (uncommitted)

#### Work Order

**Goal:** Replace independent internal child-layout fields with an internal `ChildLayout::{Row, Column}` enum while preserving the public API and current behavior.

**Spec:**

Add internal child-layout storage before adding any new public typestate API. The public `El` call surface remains source-compatible in this phase: `El::new()`, `.direction(...)`, `.child_gap(...)`, `.child_align_x(...)`, `.child_align_y(...)`, and `.child_alignment(...)` continue to work.

Introduce an internal enum, preferably in a small layout module such as `crates/bevy_diegetic/src/layout/child_layout.rs`:

```rust
pub(crate) enum ChildLayout {
    Row {
        gap: Dimension,
        align_x: AlignX,
        align_y: AlignY,
    },
    Column {
        gap: Dimension,
        align_x: AlignX,
        align_y: AlignY,
    },
}
```

`Element` stores `child_layout: ChildLayout` instead of these independent fields:

```rust
child_gap:     Dimension,
direction:     Direction,
child_align_x: AlignX,
child_align_y: AlignY,
```

`El` may keep the old fields in this phase to preserve the public builder API, but `El::into_element(...)` must lower those fields into `ChildLayout`.

Add helper methods so existing engine logic does not open-code variant details:

```rust
impl ChildLayout {
    fn direction(&self) -> Direction;
    fn gap(&self) -> Dimension;
    fn align_x(&self) -> AlignX;
    fn align_y(&self) -> AlignY;
    fn is_row(&self) -> bool;
    fn is_column(&self) -> bool;
}
```

Move unit conversion into `ChildLayout`:

```rust
impl ChildLayout {
    fn to_points(self, layout_scale: f32) -> Self {
        match self {
            Self::Row { gap, align_x, align_y } => Self::Row {
                gap: Dimension {
                    value: gap.to_points(layout_scale),
                    unit: None,
                },
                align_x,
                align_y,
            },
            Self::Column { gap, align_x, align_y } => Self::Column {
                gap: Dimension {
                    value: gap.to_points(layout_scale),
                    unit: None,
                },
                align_x,
                align_y,
            },
        }
    }
}
```

Update `LayoutTree::scaled` to call the child-layout conversion instead of resolving `element.child_gap` directly.

Update `LayoutTreeChange` classification to classify child-layout changes through a dedicated helper. Do not compare the enum directly:

```rust
fn classify_child_layout_change(
    old: &ChildLayout,
    next: &ChildLayout,
) -> LayoutTreeChange {
    match (old, next) {
        (ChildLayout::Row { .. }, ChildLayout::Row { .. }) => { ... }
        (ChildLayout::Column { .. }, ChildLayout::Column { .. }) => { ... }
        _ => LayoutTreeChange::LayoutAffecting,
    }
}
```

Update row/column sizing and positioning code to read `child_layout` helpers. Behavior must remain unchanged for row/column layout, child gaps, alignment, scroll, wrapping, and between-child borders.

**Files:**

- `crates/bevy_diegetic/src/layout/child_layout.rs` — new internal enum and helpers, if a new module is used.
- `crates/bevy_diegetic/src/layout/mod.rs` — wire the new module and crate-internal exports.
- `crates/bevy_diegetic/src/layout/builder.rs` — lower existing public `El` fields to `ChildLayout`.
- `crates/bevy_diegetic/src/layout/element.rs` — replace stored fields, update defaults, scaling, and change classification.
- `crates/bevy_diegetic/src/layout/engine/sizing.rs` — replace direct `direction`/`child_gap` access.
- `crates/bevy_diegetic/src/layout/engine/positioning.rs` — replace direct `direction`/`child_gap`/alignment access.
- `crates/bevy_diegetic/src/layout/engine/integration_tests.rs` — add focused regression tests for unit-backed gap scaling and layout-change classification.

**Constraints from prior phases:** None.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` passes; `cargo check -p bevy_diegetic --examples` passes; existing row/column layout tests still pass; add tests proving row/column unit-backed gaps still scale and row/column child-layout changes classify as layout-affecting.

#### Retrospective

**What worked:**

- `crates/bevy_diegetic/src/layout/child_layout.rs` now owns internal row/column direction, gap, alignment helpers, and gap unit conversion.
- `El::into_element(...)` kept the existing public builder fields and lowers them through `ChildLayout::for_direction(...)`, so Phase 1 did not create a public API migration.

**What deviated from the plan:**

- The implementation added `ChildLayout::for_direction(...)` as the compatibility lowering point; otherwise the Phase 1 file scope and behavior matched the work order.

**Surprises:**

- The blind reviewer found no issues; it was a static review and did not rerun Cargo commands.

**Implications for remaining phases:**

- Phase 2 can add row/column aliases on top of the existing public `El` fields without changing internal `Element` storage again.
- Phase 4 should preserve `ChildLayout::for_direction(...)` or replace it with an equivalent typestate lowering path when `El<L>` lands.

#### Phase 1 Review

- Remaining phases stay in order; Phase 1 only moved internal row/column storage and did not satisfy the public row/column, divider, typestate, or overlay phases.
- Phase 2 now has to migrate deferred negative-gap sites away from `.direction(...)` while still recording them for Phase 6 overlay work, so Phase 4 can remove `.direction(...)` without a compile cliff.
- Phase 3 now stores `ChildDivider` inside the row/column `ChildLayout` variants instead of adding unrelated `Element` fields.
- Phase 4 now includes every `LayoutBuilder` entry point that accepts `El`, including `text_id_element(...)` and private `add_text(...)`.
- Phase 5 now explicitly replaces row/column-only `ChildLayout::direction()` and `is_layout_axis(...)` assumptions with an overlay-aware axis-role branch.
- Phase 6 now rebuilds the active `panel_draw_order` example as an overlay-based `DrawZIndex` teaching example, even if the old negative-gap demo is only present in comments.

### Phase 2 — Row/column convenience API and call-site classification  · status: todo

#### Work Order

**Goal:** Add row/column convenience constructors and migrate ordinary call sites away from `.direction(...)` and `.child_gap(...)` before typestate removal.

**Spec:**

Add compatibility convenience APIs to the existing public `El` shape:

```rust
impl El {
    pub fn row() -> Self { ... }      // LeftToRight, zero gap
    pub fn column() -> Self { ... }   // TopToBottom, zero gap
    pub fn gap(self, gap: impl Into<Dimension>) -> Self { ... }
    pub fn align_x(self, align: AlignX) -> Self { ... }
    pub fn align_y(self, align: AlignY) -> Self { ... }
    pub fn alignment(self, x: AlignX, y: AlignY) -> Self { ... }
}
```

`El::new()` remains the current default row behavior.

Migrate non-overlap call sites in `crates/**`, examples, and docs:

- `El::new().direction(Direction::LeftToRight)` becomes `El::row()`.
- `El::new().direction(Direction::TopToBottom)` becomes `El::column()`.
- `.child_gap(...)` becomes `.gap(...)` when the gap is normal positive spacing.
- `.child_align_x(...)` becomes `.align_x(...)` in touched call sites.
- `.child_align_y(...)` becomes `.align_y(...)` in touched call sites.
- `.child_alignment(x, y)` becomes `.alignment(x, y)` in touched call sites.

Classify negative-gap call sites separately instead of blindly treating them as ordinary spacing. Negative gaps may be intentional overlap workarounds. They must still migrate away from `.direction(...)` in this phase so Phase 4 can remove `.direction(...)` without a compile cliff:

- `El::new().direction(Direction::LeftToRight).child_gap(-...)` becomes `El::row().gap(-...)`.
- `El::new().direction(Direction::TopToBottom).child_gap(-...)` becomes `El::column().gap(-...)`.

Record a short list in this plan under the Phase 6 constraints or in a comment in the migration commit message. At minimum classify the known overlap examples:

- `crates/bevy_diegetic/examples/panel_draw_order.rs` — negative gap overlap demo, migrate after overlay exists.
- `crates/bevy_diegetic/examples/diegetic_text_stress.rs` — negative gap overlap pattern, migrate or explicitly justify after overlay exists.

Do not remove `.direction(...)` or `.child_gap(...)` yet. This phase prepares the tree for typestate without creating a compile cliff.

**Files:**

- `crates/bevy_diegetic/src/layout/builder.rs` — add row/column/gap/align aliases.
- `crates/bevy_diegetic/examples/**/*.rs` — migrate ordinary call sites.
- `crates/fairy_dust/src/**/*.rs` — migrate downstream helper call sites where touched by compile errors or stale API audits.
- `docs/**/*.md` — stop teaching `.direction(...)` and `.child_gap(...)` in updated examples, except where documenting migration.
- `docs/bevy_diegetic/child-layout.md` — if negative-gap overlap sites are recorded here, update Phase 6 constraints.

**Constraints from prior phases:** Phase 1 stored row/column internally as `ChildLayout`, added `ChildLayout::for_direction(...)` as the compatibility lowering point from the existing public `El` fields, and left the public `El` API untyped.

**Acceptance gate:** `cargo check -p bevy_diegetic --examples` passes; `cargo nextest run -p bevy_diegetic` passes; `rg -n "\\.direction\\(|\\.child_gap\\(|child_align_x\\(|child_align_y\\(|child_alignment\\(" crates docs` shows only compatibility internals, Clay parity calls, migration notes, or explicitly documented deferred overlap sites that have already been converted to `El::row().gap(-...)` / `El::column().gap(-...)`.

### Phase 3 — Split outer borders from child dividers  · status: todo

#### Work Order

**Goal:** Separate common outer border styling from row/column child dividers so overlay cannot inherit a divider-capable common border API later.

**Spec:**

The current `Border` type contains both outer border fields and `between_children`. That makes this invalid overlay state representable once `El::overlay()` exists:

```rust
El::overlay().border(Border::new().between_children(In(0.01)));
```

Split the model into an outer border concept and a child-divider concept.

The final API should have separate concepts:

- `OuterBorder`: common element outline, valid for every `El<L>`.
- `ChildDivider`: row/column separator between adjacent child slots, invalid for `El<Overlay>`.

To reduce call-site churn, it is acceptable to keep the public name `Border` as the outer-border type during migration, but it must no longer be the owner of child dividers in the final API. If an `OuterBorder` type is introduced, provide a temporary `pub type Border = OuterBorder` only if needed to keep ordinary border call sites readable during migration.

Introduce a child-divider type:

```rust
pub struct ChildDivider {
    width: Dimension,
    color: Color,
}

impl ChildDivider {
    pub fn new(width: impl Into<Dimension>, color: Color) -> Self { ... }
    pub(crate) fn to_points(self, layout_scale: f32) -> Self { ... }
}
```

Store child-divider data inside the row/column `ChildLayout` variants in this phase, not as a separate unrelated `Element` field:

```rust
pub(crate) enum ChildLayout {
    Row {
        gap: Dimension,
        align_x: AlignX,
        align_y: AlignY,
        divider: Option<ChildDivider>,
    },
    Column {
        gap: Dimension,
        align_x: AlignX,
        align_y: AlignY,
        divider: Option<ChildDivider>,
    },
}
```

Add a temporary untyped `El::child_divider(...)` in this phase if typestate does not exist yet. It must lower into the current row/column `ChildLayout`; Phase 4 must move this method onto row/column states only.

Update render-command emission so between-child separator rectangles read row/column `ChildLayout` divider data rather than `Border::between_children`. Keep row/column separator rendering behavior equivalent to the old `Border::between_children` behavior.

Update scaling and change classification so divider width/color are classified correctly:

- divider width changes are layout-affecting;
- divider color-only changes are visual-only if the existing classifier can represent that distinction for dividers;
- outer border width remains layout-affecting;
- outer border color remains visual-only.

Remove, deprecate, or stop using `Border::between_children`. If a compatibility shim remains temporarily, it must lower into `ChildDivider` and must not survive the final API.

**Files:**

- `crates/bevy_diegetic/src/layout/geometry.rs` — split child divider from outer border; remove/deprecate `Border::between_children`.
- `crates/bevy_diegetic/src/layout/builder.rs` — add temporary `El::child_divider(...)` if needed.
- `crates/bevy_diegetic/src/layout/child_layout.rs` — add row/column divider storage, scaling, and helper accessors.
- `crates/bevy_diegetic/src/layout/element.rs` — update scaling and change classification for divider data owned by `ChildLayout`.
- `crates/bevy_diegetic/src/layout/engine/positioning.rs` — read child dividers from the new storage.
- `crates/bevy_diegetic/src/layout/engine/integration_tests.rs` — update between-child border tests to child-divider tests.
- `crates/bevy_diegetic/examples/**/*.rs` and `docs/**/*.md` — migrate `Border::between_children` call sites.

**Constraints from prior phases:** Phase 1 moved row/column child-flow fields into `ChildLayout`; Phase 2 migrated ordinary call sites to `El::row()`, `El::column()`, `.gap(...)`, and `.alignment(...)` but did not remove old compatibility methods yet. Phase 3 should keep divider ownership inside `ChildLayout` so overlay can later omit divider storage entirely.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` passes; `cargo check -p bevy_diegetic --examples` passes; row/column child divider rendering matches old between-child border behavior; `rg -n "between_children" crates docs` shows only compatibility/deprecation code or migration notes.

### Phase 4 — Typestate row/column public API  · status: todo

#### Work Order

**Goal:** Introduce `El<L = Row>` with public row/column typestate, generic builder methods, and leaf normalization, without adding overlay yet.

**Spec:**

Make the public `El` type generic over a child-layout state:

```rust
pub struct El<L = Row> {
    common: CommonEl,
    child_layout: L,
}

pub struct Row {
    gap: Dimension,
    divider: Option<ChildDivider>,
}

pub struct Column {
    gap: Dimension,
    divider: Option<ChildDivider>,
}
```

Move alignment into `CommonEl`, not into the typestate marker:

```rust
struct CommonEl {
    width: Sizing,
    height: Sizing,
    padding: Padding,
    align_x: AlignX,
    align_y: AlignY,
    background: Option<Color>,
    border: Option<OuterBorder>,
    // remaining visual and behavior fields from the current El
}
```

Expose public marker names anywhere `El` is exported:

```rust
pub struct Row { ... }
pub struct Column { ... }
pub trait ChildLayoutState: private::Sealed {}
```

Keep the internal runtime enum hidden. The public trait must not expose `ChildLayout`:

```rust
pub trait ChildLayoutState: private::Sealed {}

mod private {
    pub trait Sealed {
        fn into_child_layout(self, common: &CommonEl) -> ChildLayout;
    }
}
```

Common setters are implemented for every layout state:

```rust
impl<L> El<L> {
    pub fn width(self, sizing: Sizing) -> Self { ... }
    pub fn height(self, sizing: Sizing) -> Self { ... }
    pub fn size<DM: DimensionMatch>(self, w: DM, h: DM) -> Self { ... }
    pub fn padding(self, padding: Padding) -> Self { ... }
    pub fn background(self, color: Color) -> Self { ... }
    pub fn border(self, border: OuterBorder) -> Self { ... }
    pub fn z_index(self, z_index: DrawZIndex) -> Self { ... }
    pub fn align_x(self, align: AlignX) -> Self { ... }
    pub fn align_y(self, align: AlignY) -> Self { ... }
    pub fn alignment(self, x: AlignX, y: AlignY) -> Self { ... }
}
```

Only row and column expose `gap` and child dividers:

```rust
impl El<Row> {
    pub fn new() -> Self { Self::row() }
    pub fn row() -> Self { ... }
    pub fn gap(self, gap: impl Into<Dimension>) -> Self { ... }
    pub fn child_gap(self, gap: impl Into<Dimension>) -> Self { ... } // temporary alias only
    pub fn child_divider(self, divider: ChildDivider) -> Self { ... }
}

impl El<Column> {
    pub fn column() -> Self { ... }
    pub fn gap(self, gap: impl Into<Dimension>) -> Self { ... }
    pub fn child_gap(self, gap: impl Into<Dimension>) -> Self { ... } // temporary alias only
    pub fn child_divider(self, divider: ChildDivider) -> Self { ... }
}
```

Do not keep `.direction(...)` on `El<L>`. A runtime `Direction` argument cannot return `El<Row>` for one value and `El<Column>` for another without erasing the typestate. Phase 2 already migrated ordinary call sites away from `.direction(...)`; any stale use must be fixed in this phase.

Make `LayoutBuilder` methods generic:

```rust
impl LayoutBuilder {
    pub fn with_root<L>(el: El<L>) -> Self
    where
        L: ChildLayoutState,
    { ... }

    pub fn with<L>(&mut self, el: El<L>, children: impl FnOnce(&mut Self)) -> &mut Self
    where
        L: ChildLayoutState,
    { ... }

    pub fn text_element<L>(
        &mut self,
        el: El<L>,
        text: impl Into<String>,
        config: TextStyle,
    ) -> &mut Self
    where
        L: ChildLayoutState,
    { ... }

    pub fn text_id_element<L>(
        &mut self,
        id: impl Into<PanelFieldId>,
        el: El<L>,
        text: impl Into<String>,
        config: TextStyle,
    ) -> &mut Self
    where
        L: ChildLayoutState,
    { ... }

    fn add_text<L>(
        &mut self,
        id: PanelFieldId,
        el: El<L>,
        text: impl Into<String>,
        config: TextStyle,
    ) -> &mut Self
    where
        L: ChildLayoutState,
    { ... }

    pub fn image<L>(&mut self, el: El<L>, handle: Handle<Image>, tint: Color) -> &mut Self
    where
        L: ChildLayoutState,
    { ... }
}
```

Text and image leaves can accept any `El<L>`, but their internal child layout is inert. Text/image leaf lowering must normalize the internal child layout to an exact default row with zero gap, no divider, and default alignment. Container lowering must preserve authored layout: `with_root(...)` and `with(...)` initially lower containers as `ElementContent::Empty`, but those nodes may receive children afterward.

Update helper signatures deliberately:

- helpers that return a known row use `El<Row>`;
- helpers that return a known column use `El<Column>`;
- helpers that forward arbitrary element states are generic over `L: ChildLayoutState`;
- bare `El` means intentionally `El<Row>`.

Known downstream cases from the Phase 2 audit:

- `crates/fairy_dust/src/camera_control_panel/layout.rs:205` returns a column element from `action_rows_element(...)`; change it to `El<Column>`.
- `crates/fairy_dust/src/screen_panels/title_bar.rs` builds either a row or column into one local binding; split the orientation branches before assigning to a single typed `El<Row>` / `El<Column>` local.

Add compile-pass tests, preferably through `trybuild`, for downstream-style helper signatures:

```rust
fn row_panel() -> El<Row> { El::row() }
fn column_panel() -> El<Column> { El::column() }
fn decorate<L: ChildLayoutState>(el: El<L>) -> El<L> { el.padding(Padding::all(1.0)) }
```

**Files:**

- `crates/bevy_diegetic/src/layout/builder.rs` — generic `El<L>`, marker types, common setters, row/column-only methods, generic builder methods including `text_id_element(...)` and private `add_text(...)`.
- `crates/bevy_diegetic/src/layout/child_layout.rs` — public markers and sealed trait if this module owns them.
- `crates/bevy_diegetic/src/layout/mod.rs` — re-export `Row`, `Column`, and `ChildLayoutState` beside `El`.
- `crates/bevy_diegetic/src/layout/element.rs` — leaf normalization and classification updates.
- `crates/fairy_dust/src/**/*.rs` and `crates/bevy_diegetic/examples/**/*.rs` — update helper signatures and stale call sites.
- `crates/bevy_diegetic/Cargo.toml` — add `trybuild` dev-dependency if compile-pass fixtures are introduced.
- `crates/bevy_diegetic/tests/trybuild.rs` and `crates/bevy_diegetic/tests/trybuild/pass/*.rs` — compile-pass fixtures.

**Constraints from prior phases:** Phase 1 added `ChildLayout::for_direction(...)`; Phase 2 migrated ordinary `.direction(...)`, `.child_gap(...)`, and `.child_alignment(...)` call sites; Phase 3 split child dividers from outer border and stores dividers inside row/column `ChildLayout`, so dividers can now become row/column-only methods.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` passes including compile-pass fixtures; `cargo check -p bevy_diegetic --examples` passes; `cargo check --workspace --all-targets` passes or every workspace crate touched by stale API errors is checked explicitly; `rg -n "\\.direction\\(|\\.child_gap\\(|child_alignment\\(" crates docs` shows no production call sites outside migration notes or removed compatibility code.

### Phase 5 — Overlay layout mode and compile-fail guarantees  · status: todo

#### Work Order

**Goal:** Add `El::overlay()` and `ChildLayout::Overlay`, implement overlay sizing/positioning/scroll behavior, and prove invalid overlay calls do not compile.

**Spec:**

Add the public marker:

```rust
pub struct Overlay;

impl El<Overlay> {
    pub fn overlay() -> Self { ... }
}
```

Re-export `Overlay` anywhere `El`, `Row`, and `Column` are public.

Extend internal storage:

```rust
pub(crate) enum ChildLayout {
    Row {
        gap: Dimension,
        align_x: AlignX,
        align_y: AlignY,
        divider: Option<ChildDivider>,
    },
    Column {
        gap: Dimension,
        align_x: AlignX,
        align_y: AlignY,
        divider: Option<ChildDivider>,
    },
    Overlay {
        align_x: AlignX,
        align_y: AlignY,
    },
}
```

Overlay semantics:

- children do not advance a main-axis cursor;
- every child is positioned against the parent content box;
- `align_x` positions each child horizontally within that content box;
- `align_y` positions each child vertically within that content box;
- parent fit width is the maximum child width plus horizontal padding and border;
- parent fit height is the maximum child height plus vertical padding and border;
- percent child sizes resolve against the parent content size;
- clipping behavior is unchanged;
- scroll extents are axis-independent: horizontal scroll range is `max_child_extent_x - content_width`, vertical scroll range is `max_child_extent_y - content_height`, each clamped at zero;
- child dividers are not part of overlay's public API;
- legacy divider data, if any remains during migration, is ignored and classified as inert for overlay;
- `DrawZIndex` controls render order exactly as it does for row and column children.

Do not implement overlay as "cross axis on both axes." Replace `is_along` boolean logic with a child-layout classification:

```rust
enum AxisRole {
    RowMain,
    ColumnMain,
    Cross,
    Overlay,
}
```

Do not make `ChildLayout::Overlay` fit through the Phase 1 compatibility helpers:

- `ChildLayout::direction()` must not return a fake row or column for overlay.
- `ChildLayout::is_row()` / `is_column()` and `engine/sizing.rs::is_layout_axis(...)` cannot be the only axis classification once overlay exists.
- Replace row/column-only callers with an overlay-aware axis-role helper before adding the overlay variant.

Overlay sizing branch:

- `Sizing::Percent(frac)` resolves against the parent content box on that axis;
- `Sizing::Grow` fills the parent content box on that axis;
- `Sizing::Fit` keeps the propagated natural size, clamped by its bounds;
- no sibling compression or grow distribution runs for overlay children;
- no gap is subtracted or accumulated.

Use one content-box helper for percent/grow sizing, wrapping, scroll extent, and positioning. Wrapping must subtract both padding and border so bordered overlay text does not wrap underneath the border.

Split scroll anchoring by axis:

```rust
struct Element {
    scroll_offset: Vec2,
    scroll_anchor_x: ScrollAnchor,
    scroll_anchor_y: ScrollAnchor,
}
```

Builder methods mutate only their axis:

- `scroll_x(...)` changes `scroll_offset.x` and `scroll_anchor_x`;
- `scroll_y(...)` changes `scroll_offset.y` and `scroll_anchor_y`;
- `scroll_y_from_end(...)` changes only `scroll_offset.y` and `scroll_anchor_y`.

Add concrete `trybuild` compile-fail fixtures for invalid overlay calls:

```rust
El::overlay().gap(In(0.08));
El::overlay().child_gap(In(0.08));
El::overlay().direction(Direction::TopToBottom);
El::overlay().child_divider(ChildDivider::new(In(0.01), Color::WHITE));
```

Also add compile-pass fixtures proving row/column `.gap(...)`, row/column `.child_divider(...)`, and generic `El<L>` helper signatures still compile.

**Files:**

- `crates/bevy_diegetic/src/layout/child_layout.rs` — add overlay variant, axis role helpers, scaling/classification variants.
- `crates/bevy_diegetic/src/layout/builder.rs` — add `Overlay` marker and `El::overlay()`, preserve row/column-only gap/divider methods.
- `crates/bevy_diegetic/src/layout/mod.rs` — re-export `Overlay`.
- `crates/bevy_diegetic/src/layout/element.rs` — scroll-anchor split, overlay classification, leaf normalization tests.
- `crates/bevy_diegetic/src/layout/engine/sizing.rs` — explicit overlay fit/percent/grow sizing.
- `crates/bevy_diegetic/src/layout/engine/positioning.rs` — overlay positioning, independent scroll extents, no overlay child-divider emission.
- `crates/bevy_diegetic/src/layout/engine/wrapping.rs` — shared content-box width including border subtraction.
- `crates/bevy_diegetic/src/layout/engine/integration_tests.rs` — overlay runtime tests.
- `crates/bevy_diegetic/Cargo.toml` — add `trybuild` dev-dependency if not already added.
- `crates/bevy_diegetic/tests/trybuild.rs`, `crates/bevy_diegetic/tests/trybuild/fail/*.rs`, `crates/bevy_diegetic/tests/trybuild/pass/*.rs` — compile-fail/pass fixtures.

**Constraints from prior phases:** Phase 4 introduced generic `El<L>`, public row/column marker types, row/column-only gap/divider methods, and leaf normalization. Overlay must follow that public API shape and must not expose row/column-only methods.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` passes and includes `trybuild` cases; `cargo check -p bevy_diegetic --examples` passes; runtime tests cover overlay fit max sizing, top-left/center/bottom-right alignment, padding/border offsets, percent/grow sizing, independent scroll extents, `scroll_y_from_end(0.0)` not changing horizontal anchoring, bordered overlay text wrapping, no between-child divider commands for overlay, `DrawZIndex` ordering over overlapped children, and leaf marker normalization; code review confirms overlay is not routed through a fake `Direction` or a row/column-only `is_layout_axis(...)` result.

### Phase 6 — Migrate overlap examples, docs, and final audits  · status: todo

#### Work Order

**Goal:** Replace negative-gap overlap workarounds with `El::overlay()`, update the draw-order example, and run final workspace/API audits.

**Spec:**

Migrate all intentional overlap patterns identified in Phase 2 to `El::overlay()` instead of row/column negative gaps.

Known Phase 2 audit findings to carry forward:

- `crates/bevy_diegetic/examples/panel_draw_order.rs` had the old DrawZIndex overlap demo using `-SWEEP_LANE_WIDTH`; in this worktree the active example may instead be a text-only fit probe with the old demo commented out. Rebuild the active example as an overlay-based DrawZIndex teaching example rather than only migrating active negative-gap lanes.
- `crates/bevy_diegetic/examples/diegetic_text_stress.rs` uses `-GPU_PIPELINE_LANE_HEIGHT` to overlap GPU lane labels and bars; migrate that pattern to overlay or document why it remains outside this phase.

The draw-order example should express layered panel content directly:

```rust
builder.with(
    El::overlay()
        .size(page_width, page_height)
        .background(DEFAULT_PANEL_BACKGROUND)
        .border(page_border()),
    |builder| {
        builder.with(El::column().padding(Padding::all(In(0.24))), |builder| {
            builder.text(STORY_TEXT, story_style());
        });

        builder.with(El::column().z_index(DrawZIndex(1)), |builder| {
            builder.with(sweep_panel(), |_| {});
        });
    },
);
```

`panel_draw_order` should be an active, visually useful DrawZIndex example. It should no longer use negative `child_gap` or negative `.gap(...)` to create overlap. Keep the example focused on `DrawZIndex`: behind/front changes only the sweep element's `DrawZIndex`. The example should teach that `DrawZIndex` is panel-scoped and named differently from Bevy UI's `ZIndex`.

Update docs and examples so new educational material teaches:

- `El::row().gap(...)` for horizontal layout;
- `El::column().gap(...)` for vertical layout;
- `El::overlay()` for siblings sharing the same content rectangle;
- row/column-only child dividers;
- no use of `.direction(...)` or `.child_gap(...)` in new examples.

Final stale API audit:

```sh
rg -n "\\.direction\\(|\\.child_gap\\(|child_align_x\\(|child_align_y\\(|child_alignment\\(" crates docs
```

Negative gap audit must catch both literal and named-constant overlap patterns. Do not rely only on `-0.1` regexes. Inspect any remaining negative gap call manually and either remove it or document why it is ordinary spacing rather than overlap.

**Files:**

- `crates/bevy_diegetic/examples/panel_draw_order.rs` — rewrite overlap lanes to `El::overlay()`.
- `crates/bevy_diegetic/examples/diegetic_text_stress.rs` — migrate or justify negative-gap overlap pattern.
- `crates/bevy_diegetic/examples/**/*.rs` — final stale API migration.
- `crates/fairy_dust/src/**/*.rs` — final stale API migration.
- `docs/**/*.md` — update examples and remove old layout teaching.
- `docs/bevy_diegetic/child-layout.md` — update this plan's remaining constraints if a phase review discovers extra overlap sites.

**Constraints from prior phases:** Phase 2 converted deferred overlap sites away from `.direction(...)` while preserving their negative `.gap(...)` values for this phase's audit; Phase 5 added `El::overlay()` and proved overlay cannot accept row/column gap/divider methods. Use overlay for overlap, not negative row/column gaps.

**Acceptance gate:** `cargo nextest run -p bevy_diegetic` passes; `cargo check -p bevy_diegetic --examples` passes; `cargo check --workspace --all-targets` passes or any impossible workspace-wide failure is documented with a narrower checked set; `cargo +nightly fmt --all` passes; stale API audit has no production matches except compatibility/deprecation code; no intentional overlap remains implemented through negative row/column gaps; `panel_draw_order` runs and demonstrates overlay layering through `DrawZIndex`.
