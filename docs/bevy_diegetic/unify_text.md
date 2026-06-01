# Unifying diegetic text: WorldText, ScreenText, and one TextStyle

## Goal

Add a screen-space text primitive to `bevy_diegetic` without multiplying types.
The driving constraint is **simplify and unify** — collapse the existing
layout-vs-standalone text style split into one `TextStyle`, and expose two thin
component primitives (`WorldText`, `ScreenText`) plus the existing `Panel`
(`DiegeticPanel`) builder. No new split style types.

## Where text styling stands today

Panel text and standalone world text are styled by the **same struct**,
`TextProps<C>`, parameterized by a zero-size marker `C`:

- `LayoutTextStyle = TextProps<ForLayout>` — text laid out inside a panel
  element. Passed as the second argument to `LayoutBuilder::text(string, style)`.
- `WorldTextStyle = TextProps<ForStandalone>` — the companion style for a
  standalone `WorldText` component, placed by `Transform` with its own render
  path (`Without<PanelChild>`).

Every field lives on both: `font_id, size, weight, slant, line_height,
letter_spacing, word_spacing, wrap, color, align, anchor, render_mode,
shadow_mode, sidedness, lighting, font_features, unit, world_scale, alpha_mode`.
The marker only changes (a) which authoring methods are exposed, (b) the
`new()` signature, and (c) two defaults. The data is already unified, and the
engine already inter-converts via `as_standalone()` / `as_layout_config()`.

Marker-specific authoring today:

| concern | shared (`impl<C>`) | `ForLayout` only | `ForStandalone` only |
|---|---|---|---|
| construct | — | `new(Px/Pt/Mm/In/f32)` — unit-bearing | `new(f32)` — unit via cascade |
| font / weight / slant / color / spacing / line-height / `render_mode` / `shadow_mode` / `font_features` | all `with_*` | — | — |
| `align` | `with_align` (both) | — | — |
| `wrap` / `no_wrap` | — | ✓ | — (forced `None`) |
| `unit`, `with_alpha_mode` | — | ✓ | — (cascade) |
| `anchor` | — | — | ✓ |
| `world_scale` | — | — | ✓ |
| `sidedness` / `lighting` / `unlit` | getters only | — | ✓ setters |

Default differences: `wrap` = `Words` (layout) vs `None` (standalone);
`anchor` = `TopLeft` (layout) vs `Center` (standalone). `color` WHITE,
`align` Left, `lighting` Lit are the same in both.

### Why the split exists

It is a context affordance filter, not different data:

- **`ForLayout`** — text the engine measures and positions inside an element
  box. It needs `wrap` mode, a font `unit` relative to the panel, and
  `alpha_mode` authoring (panel-routed). It does not expose `anchor` (the engine
  positions the run via the element's alignment), `world_scale` (the panel
  defines scale), or per-mesh `sidedness`/`lighting` (the panel handles those).
- **`ForStandalone`** — a free `WorldText` entity placed by `Transform` with no
  box. It needs `anchor` (which point sits on the origin), `world_scale`
  (meters-per-unit, since there is no panel), and per-mesh
  `sidedness`/`lighting`/`unlit`. It cannot `wrap` (no box → forced `None`), and
  `unit`/`alpha` come from the cascade.

## The unification analysis

Going knob by knob:

**1. Construct — accept `impl Into<Dimension>` everywhere.** `Dimension` already
carries its own unit, so an absolute unit (Pt/Mm/In) translates identically in
both contexts: a physical length → meters → world units (world) or → logical px
(screen). The only context-specific rule is what a bare `f32` means:

- ScreenText: 1 unit = 1 logical px, so bare `f32` = pixels.
- WorldText: no pixel grid, so bare `f32` = size in the cascade's `FontUnit`
  (meters by default).

That is a one-line default-unit policy per context, not a type split. The
translation is exactly what the cascade + `meters_per_unit` already do.

**2. Typography (font/weight/color/spacing/line-height/render_mode/
shadow_mode/font_features) — identical.** Already shared; no change.

**3. wrap — unify the mode; width is always the container's.** Panel text has
no `wrap_width`. A run inside a panel wraps against its **containing element's**
width (`El::new().width(...)`). The text style only carries a wrap **mode**
(`Words` / `None`). `TextProps` has no width field at all. So:

- wrap **mode** → on the unified `TextStyle` (shared field).
- width to wrap against → always the container's, never the style's. In a panel
  that is the `El`'s `Sizing`; for the WorldText/ScreenText sugar it is the
  sugar's own width argument — because the sugar *is* a one-element box.

WorldText gets wrapping the same way panel text does: the box supplies a width,
the style supplies the mode. There is no `wrap_width` on text anywhere.

**4. unit / alpha_mode — unify.** Authoring stays available; the cascade fills
the default. Cascade routing is one-way, which is acceptable.

**5. anchor — entity-level, not per-run.** anchor means the same thing in every
context: which point of the box sits at its placement point. A run inside a
multi-element panel has no independent pivot — the element's alignment positions
it, which is why per-run anchor was inert and marker-gated. The rule that
unifies it: **anchor lives on the text component / panel, never on the per-run
`LayoutTextStyle`.** The sugar forwards `.anchor()` to the panel anchor, which
already exists. It is on **both** sugars.

**6. world_scale — world-only, at the component level.** This is the one knob
that is intrinsically 3D: meters-per-font-unit. The screen overlay fixes 1 unit
= 1 px, so there is no per-text scale to set on screen. It is not gated in the
style type — it is not on the shared `TextStyle` at all. Its setter goes on the
**`WorldText` component/builder** (a placement concern), the same move as
`anchor`. `ScreenText` simply does not offer the method. No typestate marker
survives for it. (Even on world text it only matters for bare-`f32` cascade
sizing; pass `Mm`/`Pt` for the font size and `world_scale` drops out.)

**7. sidedness / lighting / unlit — same methods, different defaults.** Both
world and screen text are meshes, so the methods are shared (they already have
shared getters — expose the setters on both). Only the sensible default differs:
WorldText is lit and lives in the scene; ScreenText under the ortho overlay
wants unlit + front-facing (a HUD element).

### Scorecard

| knob | verdict |
|---|---|
| construct (Px/Pt/Mm/In/f32) | unify — bare-`f32` default differs (px vs cascade) |
| typography | unify — identical |
| wrap | unify mode — width is the container's, never the style's |
| unit / alpha_mode | unify — cascade fills default |
| anchor | unify — component-level (forwards to panel anchor), on both sugars |
| sidedness / lighting / unlit | unify methods — differ defaults |
| **world_scale** | **world-only** — component-level setter on `WorldText` only |

Result: one `TextStyle` carrying all typography + wrap-mode + alpha. `anchor`
and `world_scale` move to the text component (one shared, one world-only).
Width-to-wrap-against is always the box. The only genuine asymmetry left is
which component exposes `world_scale`.

## The two primitives vs the panel

- **`WorldText` / `ScreenText`** (sugar): "put this (optionally wrapped) text
  here," sized to Fit, with an optional width argument that doubles as the wrap
  width. No paper, no fixed size, no background plate. One-element box under the
  hood, so it reuses the panel layout + the unified `TextStyle`.
- **`Panel`** (`DiegeticPanel`): a sized surface (paper / fixed / percent /
  grow) holding one or more element children.

### PaperSize stays off the sugar

`PaperSize` (`.paper(PaperSize::A4)`) is a named-dimension shortcut for
`.size(Mm(210), Mm(297))`, with presets for cards, photos, and posters. It
implements `PanelSizing`, so it only makes sense for a **fixed** rectangle of
specific real-world dimensions — the opposite of Fit-to-content text.

The moment you want a fixed paper backing — a literal index card, business card,
or poster prop with text on it — the paper is the artifact and the text is
content laid on it. That is `Panel::world().paper(BusinessCard)` with a text
child, i.e. the full builder. "It would have to be optional" is the tell: an
optional fixed-paper backing is the signal you wanted a `Panel`, not a text
primitive. So `PaperSize` lives on the `Panel` builder only; the sugar never
carries it.

The two primitives stay distinct: text-that-fits vs surface-that-holds.

## Naming note

`WorldText` / `ScreenText` avoid clashing with `bevy::prelude::Text`. A separate
open question (an editor-driven rename, not part of this design) is
`DiegeticPanel` → `Panel` to drop the `bevy_diegetic::DiegeticPanel` stutter,
which would give `Panel::world()` / `Panel::screen()` alongside `WorldText` /
`ScreenText`.

## Open questions for review

1. Does `world_scale` belong as a field on the unified `TextStyle` struct (set
   only via the `WorldText` builder) or as a separate field on the `WorldText`
   component itself? — see **Decision D5** below.
2. Should the sugar's width argument and the wrap behavior be a single optional
   width (present → wrap to it; absent → Fit, no wrap), or two separate inputs?
   — resolved, see **Recorded F** below.
3. Does collapsing the `ForLayout`/`ForStandalone` markers into one `TextStyle`
   break the existing `as_standalone()` / `as_layout_config()` call sites, and
   what replaces them? — resolved, see **Recorded B** below.
4. ScreenText defaults (unlit, front-facing): set on the component, or inherited
   from a screen-space cascade default? — see **Decision D2** below.

---

## Team review findings (cycle 1)

Five expert lenses (architecture, Rust type system, correctness/completeness,
API ergonomics, risk/failure-modes) reviewed this doc against its stated intent
(strengthen posture). Findings split into recorded resolutions (one sensible
in-intent outcome) and surfaced decisions (genuine forks for the author).

### Recorded resolutions (auto-accepted)

- **A. Keep `alpha_mode` on the unified `TextStyle`; do not make it cascade-only.**
  Per-label alpha (`LayoutTextStyle::with_alpha_mode`) is documented, intentional
  public API, and the panel-text reconcile path captures `config.alpha_mode()`
  before spawning the child label to insert `Override<TextAlpha>`. The doc's
  "unify — cascade fills default" line must not be read as removing the field:
  the field stays; the cascade only supplies the default when it is `None`. This
  protects the per-label override path (deleted twice before as "unused", since
  restored). Critical correctness.

- **B. `as_standalone()` / `as_layout_config()` survive as same-type helpers.**
  After the markers collapse they stop being type conversions and become
  field-filtering/defaulting helpers on the one `TextStyle` (e.g. clear `unit`
  / set the measurement wrap mode). Keep them, rename for the new meaning
  (e.g. `for_world_shaping` / `for_panel_shaping`), and audit the call sites in
  `panel_text/reconcile.rs`, `panel_text/shaping.rs`, `world_text/shaping.rs`,
  and `typography_overlay/labels.rs`. They are internal — not user API.

- **C. Disambiguate the `PanelChild` marker.** A sugar `WorldText`/`ScreenText`
  is a one-element panel, so its text child carries `PanelChild` and the
  standalone `render_world_text` path (`Without<PanelChild>`) skips it. Rename
  `PanelChild` → `PanelTextChild` (and/or add a zero-size `SugarText` marker) and
  comment the two meanings so the filter intent is explicit.

- **D. `text_align` stays a shared field on `TextStyle`; `anchor` is
  component-level.** They are distinct: `text_align` positions glyphs within the
  measured run; `anchor` places the run's box at the layout/transform point.
  The doc already moves `anchor` off the style — state explicitly that
  `text_align` does *not* move, so the alignment work stays on the style.

- **E. `PaperSize` stays off the sugar** (as written). If demand appears, a
  `Panel`-level helper (e.g. a single-text card constructor) covers the
  fixed-size-card case without adding `PaperSize` to the text primitives.

- **F. Width/wrap semantics (open question #2): single optional width.**
  Absent → `Fit` + no wrap. Present → fixed width + `Words`. Explicit `\n`
  always breaks regardless of wrap mode (matches the layout engine, which splits
  on `\n` before wrapping). Document this so "absent → no wrap" is not read as
  "newlines are ignored".

- **G. ScreenText render layer/camera: pick a non-colliding default.** Screen
  panels default to render layer 31 / overlay camera order 100. ScreenText must
  render through the same ortho overlay camera (so 1 unit = 1 px holds) without
  silently colliding with user screen panels — document the camera it uses and
  keep the layer configurable.

### Context-specific setters: gate at the builder, not the style (recorded principle)

Multiple lenses noted the unified `TextStyle` would expose setters that are
inert in one context (e.g. `.wrap()` on text headed for a non-wrapping
component). Consensus resolution: the struct holds every field, but the
*setters* for context-specific knobs are exposed on the builders/components
where they do something — `wrap` via `LayoutBuilder`/the sugar's width arg,
`world_scale`/`anchor` via the component — not on the shared `TextStyle`. This
preserves the affordance filter the markers gave without keeping the markers.
(Where `world_scale`'s *field* lives is still a fork — see D5.)

### Dropped

- **PREMISE-CHALLENGE (risk lens): "keep the two marker types, do not unify."**
  Dropped. Its grounds — "unification trades compile-time safety for runtime
  burden", "more code", "the two types work today" — are explicitly inadmissible
  under the strengthen posture, and it offers no proof the unified design cannot
  achieve the intent. The intent enables capability the two-type split does not
  (WorldText wrapping, ScreenText). Its substantive concerns (per-context
  defaults, alpha, wrap, lighting) are captured as A, D2, F, and the gating
  principle above.

## Proposed user decisions

Status legend: `proposed` = awaiting author choice.

- **D1 — Sugar render strategy. (critical, proposed)** The standalone
  `render_world_text` path does not run the layout engine, so "wrapping comes
  for free" only holds if the sugar routes through a real one-element
  `DiegeticPanel`. Options: (a) sugar spawns an internal one-element panel —
  wrapping free, but every label inherits the layout-engine compute (and its
  known freeze fragility + debug-perf cost) and the standalone render path goes
  unused for wrapped text; (b) extend the standalone path with width-based
  wrapping — avoids per-label panel overhead, but is new render code; (c) a
  lightweight single-element fast path that skips full layout passes. This is
  the central architectural fork.
  **→ DECIDED: (a) internal one-element panel.** The sugar spawns a one-element
  `DiegeticPanel`; wrapping is the layout engine's. The standalone
  `render_world_text` (`Without<PanelChild>`) path is unused for sugar text.

- **D2 — Per-context defaults mechanism. (critical, proposed)** One struct
  cannot carry two default sets, yet world text must default Lit/double-sided
  and screen text Unlit/front-facing (and wrap/anchor defaults differ too).
  Pick how defaults apply so neither context renders wrong: (a) each component
  seeds `Override<GlyphLighting/...>` via an observer (mirrors the existing
  `FontUnit`/`TextAlpha` seeding); (b) extend the cascade with
  `CascadeDefault<GlyphLighting/GlyphSidedness>` seeded per coordinate space;
  (c) the builders set explicit defaults at construction. Touches the user's
  standing rule against silently-wrong rendering.
  **→ DECIDED: (b) promote `GlyphLighting` + `GlyphSidedness` to cascade
  attributes.** Global `CascadeDefault` = world values (`Lit` / `DoubleSided`);
  the screen-panel construction bridge stamps `Override<GlyphLighting>(Unlit)` /
  `Override<GlyphSidedness>(OneSided)`, exactly as `CascadeDefaults.panel_font_unit`
  seeds `Override<FontUnit>` today. Children inherit; per-entity override still
  works. This is the documented field-promotion migration in `cascade/mod.rs`
  (lines 89–100), not new infrastructure — covers the listed inventory
  (standalone + panel-label render reads, spawn-seed bridges, the
  `as_standalone()` authoring capture, docs, first-frame `Resolved<A>` tests).

- **D3 — Construct: bare `f32` vs require `Dimension`. (important, proposed)**
  The doc lets a bare `f32` mean px on screen but cascade-meters in world, so
  the same literal `24.0` is two scales. Options: (a) keep bare `f32` with the
  per-context rule (matches the current `WorldTextStyle::new(f32)`, terse); (b)
  require `impl Into<Dimension>` (`Px`/`Pt`/`Mm`/`In`) so the unit is explicit at
  the call site and cannot silently flip.
  **→ DECIDED: (c) hybrid — bare `f32` on the sugar, unit resolved by context.**
  The bare-`f32` footgun only exists on the context-free shared `TextStyle`; the
  sugar type names the context, so `ScreenText`/`WorldText` can take a bare
  `f32`. Mechanism (no new constructors): `Dimension` stays `value` +
  `Option<Unit>`; a bare `f32` → `unit: None` ("resolve from context"),
  `Px`/`Pt`/`Mm`/`In` → `Some(unit)` (always wins). The existing `FontUnit`
  cascade resolves `None`: panel → panel font unit, standalone `WorldText` →
  world cascade (meters), and `ScreenText`'s one-element panel seeds
  `Override<FontUnit>(px)` at construction — the *same* bridge that stamps
  `Unlit`/`OneSided` in D2. Result: `ScreenText` + `24.0` → 24px, `WorldText` +
  `24.0` → cascade unit, `Mm(10.0)` → 10mm everywhere. Explicit units never
  flip. Cost is one extra seed in the D2 construction bridge.

- **D5 — `world_scale`: delete it; scale the sugar via panel
  `world_width`/`world_height`. (important, proposed)**
  **→ DECIDED: delete `world_scale` and the raw standalone `render_world_text`
  path; `WorldText` always means a one-element panel.** `world_scale` was the
  standalone substitute for a panel's world size (it sets meters-per-design-unit
  directly because standalone text had no panel to inherit scale from). Once
  `WorldText` is a one-element panel (D1a), it inherits `world_width` /
  `world_height` — the same mechanism paper panels use — so the text-only knob is
  redundant. The sugar exposes both:
  - neither set → text at its font-unit size (cascade unit; meters for world);
  - `.world_height(m)` → whole label is `m` tall, fonts/spacing scale with it,
    aspect preserved; `.world_width(m)` → scale by width, aspect preserved;
    both → non-uniform.
  Removes the `Without<PanelChild>` standalone render path, the `world_scale`
  field, and its `with_world_scale`/`set_world_scale`/`world_scale` accessors;
  current standalone-`WorldText` users migrate to the panel-backed sugar
  (scaling via `.world_height` / `.world_width`).

- **D6 — Public naming + alias removal. (important, proposed)**
  **→ DECIDED: unified type is `TextStyle`; remove `LayoutTextStyle` /
  `WorldTextStyle` aliases outright (no deprecation — prerelease, no migration
  concern).** Not bare `Style`: the crate already has `ArrowStyle`, and `Style`
  is a glob-collision hazard (historic `bevy_ui::Style`). `TextStyle` is free —
  bevy 0.19 dropped the old `bevy::prelude::TextStyle` (now `TextFont` /
  `TextColor`), confirmed absent from `bevy_text`. Editor-driven rename the
  author runs. Still relates to the separate `DiegeticPanel` → `Panel` rename
  note.

- **D8 — Bundled sugar builder. (important, proposed)** Spawning text today is a
  multi-component tuple (`WorldText` + style + `Transform`). To hit the
  "one-liner" goal, add a `WorldText::builder()` / `ScreenText::builder()` that
  chains text + style + placement (or `#[require(TextStyle)]` to auto-insert a
  default), vs leaving the tuple as-is.
  **→ DECIDED: (a) fluent `WorldText` / `ScreenText` builders.** Text + style +
  placement chain in one expression and return the spawn bundle, e.g.
  `WorldText::new("hi").size(Pt(24.0)).bold().world_height(0.5).anchor(Center)`.
  Delivers the one-liner goal; `.size`/typography setters, `.world_height` /
  `.world_width` (D5), and `.anchor` are all reachable from the chain.

- **D9 — Builder return form. (important, proposed — cycle 1)** The fluent
  `WorldText`/`ScreenText` builder produces a one-element panel plus a text
  child plus a transform — more than one component, with a `ChildOf` link. What
  does the chain return?
  - **(c) `impl Bundle` (no `.build()`) — ELIMINATED (cycle 2).** A flat bundle
    is spawned atomically on one entity and cannot express a parent panel + a
    child text linked by `ChildOf` to a not-yet-existing parent. Type-infeasible
    for the one-element-panel design.
  - **(a) `.build()` → a `#[derive(Bundle)]` struct.** Same problem in weaker
    form: a flat bundle can't carry the parent+child pair, so it forces a
    two-step spawn (`spawn(parent).with_children(...)`) at the call site —
    awkward, and leaks the structure the sugar should hide.
  - **(b) terminal `.spawn(&mut Commands) -> Entity`** *(team recommendation,
    cycle 2)* — the chain ends in a method that spawns the panel + child
    internally (`commands.spawn(panel).with_children(|c| c.spawn(text_child))`)
    and returns the text entity for later queries. Mirrors the existing
    `reconcile.rs` child-spawn pattern; encapsulates the two-entity structure.
    e.g. `WorldText::new("hi").size(Px(200.0)).bold().anchor(Center).spawn(&mut commands)`.

  **→ DECIDED: (b) terminal `.spawn(&mut Commands) -> Entity`.** The chain ends
  in `.spawn(&mut commands)`, which builds the one-element panel + text child
  internally and returns the **text entity** (the child) — the handle callers
  need for `set_text`, marker components, and visibility toggles. `.transform()`
  / `.screen_position()` are part of the chain. (a) was rejected: it leaks the
  panel/child split to the call site and forces every live-updating label to
  descend into `with_children` and fish the child id back out. Acceptable
  tradeoff: the sugar is spawned via `.spawn()`, not dropped into a larger
  `commands.spawn((…))` tuple — fine, since it's always the top-level spawn.

  **→ REVISED (Phase E, user-approved): add `.bundle() -> impl Bundle`; keep
  `.spawn()` as a one-liner over it.** D9's rejection of the bundle form rested
  on the sugar spawning a parent panel *plus* a `ChildOf`-linked child. The
  implementation does not: `.spawn()` creates only the single panel entity, and
  reconcile builds the text child later from the layout tree. So the sugar *is* a
  single-entity bundle — `(DiegeticPanel, TextContent, SugarText, Transform)` —
  and `WorldText::bundle()` / `ScreenText::bundle()` return it as `impl Bundle`.
  `.spawn()` is now `commands.spawn(self.bundle()).id()`. This keeps fairy_dust's
  `cube_face_text`/`cube_face_label` returning `impl Bundle` (composable with the
  `CubeFaceLabel` marker and `with_children`), and live-update examples keep
  `Query<&mut TextContent, With<CubeFaceLabel>>` + `set_text` (the panel root
  carries `TextContent`; `rebuild_sugar_text` propagates). The build `Result`
  (unreachable for the always-`Fit`-height sugar) falls back to
  `DiegeticPanel::default()` with a logged error — no `unwrap`.

---

## Migration inventory

The workspace has 46 example files: 19 in `bevy_diegetic`, 27 in `bevy_lagrange`
(`vendor/clay-layout` is third-party, out of scope). Examples cannot migrate
until the library crates do, since they consume the renamed/removed APIs.

### Prerequisite — library crates first

- **`crates/bevy_diegetic/src`** and **`crates/fairy_dust/src`** use
  `LayoutTextStyle` / `WorldTextStyle` and the deleted internals
  (`as_standalone` / `as_layout_config`, the `Without<PanelChild>` standalone
  render path, `world_scale`). These change before any example.

### Bucket 1 — `WorldTextStyle` / `LayoutTextStyle` → `TextStyle` (mechanical)

Two global renames collapsing to one target; the bulk by line count, the
cheapest by effort (editor-driven). `LayoutBuilder::text(s, style)` is covered
automatically — its argument is the same renamed type.

bevy_diegetic: `aa_text, cascade, dimensions, font_features, font_loading, ime,
panel_rendering, paper_sizes, screen_space, sdf, side_by_side, sizes, slug_text,
text_alpha, text_renderer_gpu_bench, text_stress, typography, units, world_text`.
bevy_lagrange: `animation, focus_bounds, follow_target, render_to_texture,
swapped_axis, showcase/event_log, showcase/policy_panel`.

### Bucket 2 — standalone `WorldText` spawn → fluent builder (substantive)

D5 deletes the raw standalone render path, so every
`(WorldText::new(..), WorldTextStyle::new(..), Transform)` tuple becomes the
fluent builder. Live `Query<&mut WorldText>` + `set_text` is unchanged (the
component persists).

| File | Note | Weight |
|---|---|---|
| `bevy_diegetic/world_text.rs` | canonical standalone demo (ground plane, cube faces, anchor demos); becomes the new sugar showcase | heaviest |
| `bevy_diegetic/{cascade, sizes, typography, side_by_side, sdf, aa_text, text_alpha, font_loading, dimensions, paper_sizes, slug_text, text_renderer_gpu_bench}` | standalone spawns mixed with panels | medium |
| `bevy_lagrange/{swapped_axis, input_keyboard, input_manual, orthographic, pausing}` | cube-face labels: spawn migrates; `&mut WorldText` swap logic unchanged | light–medium |

### Bucket 3 — `world_scale` removal

Single consumer in all examples: **`bevy_diegetic/cascade.rs`** — 4
`.with_world_scale(..)` calls re-expressed as `.world_height(..)` /
`.world_width(..)`.

### Bucket 4 — construct units (D3): transparent

`WorldTextStyle::new(0.04)` → `TextStyle::new(0.04)`; a bare `f32` still resolves
through the cascade as before (world → meters). No behavioral edits — absorbed
into Bucket 1. Explicit `Px/Pt/Mm/In` callers are unaffected.

### New usage (not migration — the new capability)

- `bevy_lagrange/examples/showcase/ui.rs` — the centered `PAUSED` overlay
  (currently bevy_ui) is the motivating consumer for the new `ScreenText`.
- `bevy_diegetic/examples/screen_space.rs` — natural home for a `ScreenText`
  demo alongside the existing screen panels.

---

## Implementation plan (library-first)

### Phase 0 — `bevy_diegetic` core type unification

1. Collapse `TextProps<ForLayout>` / `TextProps<ForStandalone>` into one public
   `TextStyle`; remove the `ForLayout` / `ForStandalone` markers and the
   `LayoutTextStyle` / `WorldTextStyle` aliases (D6).
2. `TextStyle::new(impl Into<Dimension>)`; a bare `f32` maps to `unit: None`
   ("resolve from context") (D3).
3. Keep the `alpha_mode` field and the panel-text reconcile per-label override
   capture (Recorded A). Keep `text_align` on the style (Recorded D).
4. Convert `as_standalone` / `as_layout_config` to same-type helpers, renamed
   for the new meaning (`for_world_shaping` / `for_panel_shaping`); update the
   call sites in `panel_text/{reconcile,shaping}.rs`, `world_text/shaping.rs`,
   `typography_overlay/labels.rs` (Recorded B).
5. Rename `PanelChild` → `PanelTextChild`; comment the filter's two meanings
   (Recorded C).

### Phase 1 — `bevy_diegetic` cascade + sugar + deletions

6. Promote `GlyphLighting` + `GlyphSidedness` to cascade attributes:
   `cascade_attr!` declarations, typed `override_*` / `inherit_*` / `resolved_*`
   wrappers, `CascadePlugin::<_>::default()` lines, render read sites switched to
   `Resolved<_>`. Global `CascadeDefault` = `Lit` / `DoubleSided` (world) (D2).
7. Implement `WorldText` / `ScreenText` as fluent builders that build a
   one-element `DiegeticPanel` internally and return the spawn bundle (D1, D8).
8. The screen-panel construction bridge seeds `Override<FontUnit>(px)`,
   `Override<GlyphLighting>(Unlit)`, `Override<GlyphSidedness>(OneSided)` —
   one bridge covering D2 + D3 screen defaults.
9. Delete `world_scale` (field + `with_world_scale` / `set_world_scale` /
   `world_scale`) and the `Without<PanelChild>` standalone render path; world
   sizing flows through panel `world_width` / `world_height` (D5).

### Phase 2 — `fairy_dust` library

10. Rename `*TextStyle` → `TextStyle` in `help_overlay.rs` and any other
    consumers; confirm it builds against the new `bevy_diegetic`.

### Phase 3 — examples: mechanical rename

11. Apply Bucket 1 renames across all listed example files; `cargo build` the
    workspace examples.

### Phase 4 — examples: standalone → sugar

12. Rewrite Bucket 2 spawns to the fluent builder, starting with
    `world_text.rs` (the canonical demo). Migrate `cascade.rs` `world_scale` →
    `world_height` / `world_width` (Bucket 3).

### Phase 5 — new `ScreenText` usage

13. Replace the showcase `PAUSED` bevy_ui overlay with `ScreenText`
    (`ui.rs`); add a `ScreenText` demo to `screen_space.rs`.

### Status (in progress)

Phases 0–4 done: unified `TextStyle`, lighting/sidedness cascade, `WorldText`/
`ScreenText` sugar (`.bundle()` + `.spawn()`), standalone path + `world_scale`
deleted (overlay dark), fairy_dust + all ~46 example files migrated. Workspace
builds; 225 `bevy_diegetic` tests pass; `cargo +nightly fmt` clean. Remaining:
Phase 5 (new `ScreenText` usage) and Phase 6 (clippy, perf gate, rename handoff).

### Phase 6 — verify

14. `cargo build && cargo +nightly fmt`, `/clippy`. Run the heavy examples
    (`world_text`, `cascade`, `paper_sizes`, `screen_space`, `showcase`) and
    confirm rendering: world text scales/wraps, screen text is unlit + centered,
    the `PAUSED` overlay renders, per-label alpha still honored.

---

## Team review — cycle 1 plan refinements (recorded)

Single-correct-outcome refinements to the plan above (not user forks):

- **R1 — Phase 1 has an internal compile gate.** Step 6 (promote
  `GlyphLighting`/`GlyphSidedness` to cascade) must complete before steps 7–8
  (sugar builders + screen seed bridge) compile, since the bridge calls
  `override_glyph_lighting` / `override_glyph_sidedness`. Split into **Phase 1a**
  (step 6 only) → **Phase 1b** (7–9). Today neither enum is a cascade attribute;
  both already derive `Reflect`/`Eq`, so the `cascade_attr!(…, eq)` promotion is
  mechanical (declarations in `resolved.rs`, wrappers in `attributes.rs`,
  `CascadePlugin::<_>` lines, render reads switched from `style.lighting()` to
  `Resolved<_>` at `world_text/mesh_spawning.rs`, `panel_text/…`).

- **R2 — Library `world_scale` read sites must change with the deletion.**
  Bucket 3 listed only the example (`cascade.rs`). The field is also read in
  `render/world_text/{shaping.rs:88,95,120, rendering.rs:91}` and
  `debug/typography_overlay/pipeline.rs:128,131`. Phase 1b step 9 updates these
  to the panel `world_width`/`world_height` scaling before deleting the field,
  or the workspace won't build.

- **R3 — Recorded B call-site list + names.** Add
  `debug/typography_overlay/pipeline.rs` (`.as_layout_config()`) to the audit
  list. Commit the new names: `as_layout_config` → `for_panel_shaping`,
  `as_standalone` → `for_world_shaping`. Confirm both are crate-internal (they
  are) so removal isn't a breaking change.

- **R4 — Bucket 1 spans the libraries too.** The mechanical `*TextStyle` →
  `TextStyle` rename also covers `fairy_dust` (`camera_control_panel/layout.rs`,
  `primitive.rs`, `screen_panels/{title_bar,description}.rs`) and
  `bevy_diegetic/src` itself — done in Phase 0/2, before the examples.

- **R5 — `PanelChild` rename moves to Phase 0.** Recorded C's
  `PanelChild` → `PanelTextChild` is a no-logic editor rename; do it in Phase 0
  (before behavioral changes) so Phase 1 reviews against the clear name.

- **R6 — Screen defaults are seeded inline at construction, not deferred.** The
  D2/D3 bridge inserts `Override<FontUnit>(Pixels)`, `Override<GlyphLighting>(Unlit)`,
  `Override<GlyphSidedness>(OneSided)` on the panel entity *during* `build()`
  (mirroring the existing `seed_panel_overrides` on `Added<DiegeticPanel>`), not
  via a next-frame observer — else screen text flashes Lit/meters on frame 1.
  Precedence holds: per-entity override > panel override > global default, so a
  user `.with_lighting(Lit)` on a ScreenText still wins. (Confirmed: `Unit` has a
  `Pixels` variant, so the px `FontUnit` override typechecks.)

- **R7 — Sugar builders reuse `DiegeticPanelBuilder`, not a parallel typestate.**
  `WorldText::…` builds `DiegeticPanel::world()…`; `ScreenText::…` builds
  `DiegeticPanel::screen()…` — so ScreenText automatically gets the screen seed
  bridge (R6) and the overlay camera/layer contract. A shared internal helper
  builds the one-element tree for both.

- **R8 — `typography_overlay` depends on the deleted path — retarget it first.**
  `debug/typography_overlay/pipeline.rs` reads `ComputedWorldText`, written only
  by the standalone `render_world_text`. Deleting that path (step 9) silently
  breaks the overlay for panel-backed text. Before deletion, emit the computed
  run from the panel-text path (a `ComputedPanelTextRun`, or unify the computed
  component) and retarget the overlay to read it. Critical — do not drop the
  feature silently.

  **→ SUPERSEDED (user decision, Phase D): defer the overlay retarget.** Rather
  than retarget first, the deletion lands now and the overlay goes dark in the
  interim: the standalone systems that populated `ComputedWorldText` are deleted,
  the `ComputedWorldText` / `ComputedGlyphMetrics` types stay (so the overlay
  still compiles), and the overlay draws nothing until a follow-up populates the
  computed run from the panel-text path. Tracked by `TODO(overlay-retarget)` at
  the overlay read sites. Not silent — the dark state and follow-up are recorded
  here and in code.

  Two dead-code warnings remain from the deletion, both glyph-metric data the
  overlay retarget will likely re-use: `ShapedGlyph.advance` (write-only now) and
  `BASELINE_DEDUP_EPSILON`. Left for the Phase F clippy gate (user-approved
  delete-vs-keep) rather than deleted speculatively. The standalone-only debris
  (`prepare_positioned_run` wrapper, `WORLD_TEXT_DEBUG_LOG_THRESHOLD_MS`) was
  removed.

- **R9 — Phases 1b→4 land contiguously.** Deleting the standalone path + field
  (step 9) leaves the workspace uncompilable until the examples migrate (Bucket
  2/3). Treat Phase 1b → 2 → 3 → 4 as one branch; the full-workspace
  `cargo build` gate is at the end, not between them.

- **R10 — Phase 0 step 1 itemized.** (a) collapse markers into `TextStyle`,
  (b) delete `ForLayout`/`ForStandalone`, (c) delete the
  `LayoutTextStyle`/`WorldTextStyle` alias definitions, (d) export `TextStyle`,
  (e) update `lib.rs` exports.

- **R11 — `world_text.rs` is the Phase 4 validation gate.** Rewrite it first;
  running it (sizes/wrapping/anchors correct) validates the builder before the
  remaining Bucket-2 rewrites follow.

- **R12 — Verification adds automated tests, not just visuals.** Phase 6 gains
  unit/first-frame tests: `Resolved<GlyphLighting>` = Unlit on a spawned
  ScreenText (and Lit on WorldText); per-label `with_alpha_mode` override still
  resolves over the panel default *and* is removed when the label drops it on a
  reconcile update (guards the twice-deleted path); `ScreenText::new(24.0)`
  measures 24 px and `WorldText::new(24.0)` measures 24 cascade-units.

- **R13 — Per-label panel overhead is a verification gate, not a silent
  assumption.** D1 makes every label a one-element panel running the layout
  engine; the freeze path (`project_diegetic_panel_freeze.md`) and debug
  draw-call cost (`project_units_glacial_perf.md`) are known. Before deleting the
  standalone path, run the high-label-count examples (`cascade`, `paper_sizes`,
  `world_text`) in release and confirm no freeze and no large frame-time
  regression. If a regression appears, the lightweight single-element path
  (D1 option c) is the fallback — note it, don't silently cap.

- **R14 — `ScreenText.anchor()` = the panel anchor.** Converged: a screen
  panel's `Anchor` already selects which point sits at its `ScreenPosition`
  (`coordinate_space.rs`), so the sugar's `.anchor()` forwards to it — well
  defined, not inert.

- **R15 — `DiegeticPanel` → `Panel` and the `Diegetic*` export stutter stay out
  of scope.** Tracked as a separate post-migration editor-rename pass; the
  temporary `Panel`/`WorldText`/`ScreenText` vs `Diegetic*` asymmetry is
  accepted for this work.

## Team review — cycle 2 refinements (recorded)

- **R16 — `WorldText`'s `#[require(...)]` must update in Phase 0 (compile gate).**
  `WorldText` is declared `#[require(WorldTextStyle, Transform, Visibility)]`
  (`render/world_text/mod.rs:132`). Deleting the `WorldTextStyle` alias breaks
  this. Phase 0 step 1 gains substep (f): `#[require(TextStyle, Transform,
  Visibility)]`. `Transform`/`Visibility` stay (the spawned root entity needs
  them). NEW, both cycle-2 lenses.

- **R17 — Lighting/sidedness follow the `TextAlpha` dual pattern, not
  cascade-only.** Sharpens D2. `alpha_mode` is the worked precedent: the field
  stays on the style *and* the reconcile/construction path captures it into
  `Override<TextAlpha>`. `GlyphLighting`/`GlyphSidedness` do the same — keep the
  field on `TextStyle` for per-label authoring (e.g. one Unlit label in a Lit
  panel), and capture it into `Override<GlyphLighting>` / `Override<GlyphSidedness>`
  at reconcile/construction; the global `CascadeDefault` fills it when unset. So
  D2's "promote to cascade attribute" = add the cascade layer, *not* remove the
  field. The reconcile capture path mirrors the existing `config.alpha_mode()`
  capture.

- **R18 — Screen seed timing: order render reads after `CascadeSet::Propagate`.**
  Sharpens R6. `seed_panel_overrides` is an observer on `Add<DiegeticPanel>`; it
  fires on the spawn command flush and self-heals the *panel* entity's
  `Resolved<_>` same-frame, but the *child text* resolves through propagation
  (`CascadeSet::Propagate`). Per the cascade contract, a write before Propagate
  is visible to readers scheduled after it in the same frame — so the new
  `GlyphLighting`/`GlyphSidedness` render reads must be ordered after
  `CascadeSet::Propagate`, exactly as `TextAlpha`/`FontUnit` reads are today. No
  first-frame flash if that ordering holds; R12's first-frame test
  (`Resolved<GlyphLighting>` = Unlit on a spawned `ScreenText`) is the guard.

- **R19 — Sugar builders are separate types wrapping the panel builder.**
  Sharpens R7. `WorldTextBuilder` / `ScreenTextBuilder` hold a
  `DiegeticPanelBuilder<Mode, _>` internally and expose only text + placement
  setters (`.size`, `.bold`, `.anchor`, `.world_height`, …) — they are *not*
  `DiegeticPanelBuilder` itself, so the full `.layout()` / `.paper()` surface
  does not leak into the sugar. That keeps the affordance filter the sugar exists
  for.

- **R20 — `as_*` rename audit is 6 sites.** Sharpens R3: add
  `debug/typography_overlay/labels.rs:112` to the list (`pipeline.rs:145`,
  `panel_text/{shaping:73,reconcile:113}.rs`, `world_text/shaping.rs:{62,85}`).
  Confirmed all crate-internal.

- **R21 — `set_text` per-frame cost is part of the R13 gate.** Sharpens R13: the
  static examples (`cascade`, `paper_sizes`, `world_text`) build text once and
  won't exercise the freeze (a resize-path bug) or the per-frame relayout cost.
  The bevy_lagrange cube-face examples call `set_text` every frame; as
  one-element panels they re-run the layout engine per `set_text` per label. The
  perf gate must measure a multi-label, per-frame-`set_text` scene (the
  `input_keyboard`/`orthographic`/`pausing` pattern) in release, not just the
  static demos. If per-label per-frame overhead is large, D1 option (c)
  (lightweight single-element path) is the recorded fallback.

- **R23 — Lighting/sidedness are optional fields + per-context cascade
  defaults (implementation refinement of D2/R17).** The code reality the plan
  didn't capture: panel-text lighting came from the panel's `text_material`
  (not the per-label style), and panel-text sidedness was hardcoded
  `double_sided = true`; only the standalone path read `style.lighting()` /
  `sidedness()`. So R17's literal "unconditionally capture the `TextStyle`
  field into `Override`" would force every existing panel (whose styles default
  `Lit`) to `Lit`, regressing the unlit HUD panels. Resolution (user-approved):
  make `TextStyle` lighting/sidedness `Option` (mirroring `alpha_mode`) —
  `None` = inherit. Global `CascadeDefault` = `Lit` / `DoubleSided` (world);
  `seed_panel_overrides` stamps `Override<TextLighting>(Unlit)` /
  `Override<TextSidedness>(OneSided)` for **screen** panels and
  `Override<TextLighting>(Unlit)` for any panel whose `text_material` is unlit;
  a label that sets `.with_lighting`/`.with_unlit`/`.with_sidedness` is captured
  as its own `Override` in `reconcile_panel_text_children` (spawn + reuse) and
  wins. Cascade attributes are newtypes `TextLighting(GlyphLighting)` /
  `TextSidedness(GlyphSidedness)` (the enum names can't be reused — the
  `cascade_attr!` macro wraps the value, as `FontUnit(Unit)` does). Both render
  paths now read `Resolved<_>`; the forced `double_sided` and the
  material-driven unlit are gone. Per-context default **and** per-label override
  both work; no existing panel regresses. 241 `bevy_diegetic` tests pass.

- **R22 — `WorldText` keeps `Visibility` in `#[require]`.** Minor: the
  builder-spawned root still needs `Transform`/`Visibility`; child labels keep
  their own `Visibility` (the bevy visibility hierarchy culls via the parent).
  No change beyond R16's `TextStyle` swap.
