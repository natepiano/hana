# Type-Safe Reactive Font System

## Context

The MSDF glyph atlas gets permanently poisoned when text references a font that hasn't loaded yet. Five code paths silently fall back to embedded JetBrains Mono data, rasterize glyphs under the *requested* font_id, and those wrong bitmaps persist forever.

**Proven bug chain:** Controls panel text `"EB Garamond"` uses `.with_font(2)` during setup. EB Garamond loads async — not registered yet. Font data falls back to JetBrains Mono. The 'B' in "EB Garamond" (JB Mono glyph index 26) is cached under `GlyphKey { font_id: 2, glyph_index: 26 }`. Later, EB Garamond's 'E' (also index 26) hits that cache entry → displays JB Mono's 'B'.

**Root cause:** The fallback uses the *requested* font_id in the `GlyphKey` while rasterizing with the *embedded* font's data. This permanently associates the wrong bitmap with the requested font's glyph index.

**Goal:** Make this class of bug impossible at the type level, show placeholder text correctly while fonts load, and reactively swap to the real font when it arrives.

---

## Design Principle: Never Write Incorrect Data

The previous plan included atlas/cache purge mechanisms. This plan eliminates the need for purging by ensuring incorrect data is never written in the first place.

**Key insight:** When a font isn't loaded yet, render with the embedded font using the **embedded font's identity** (font_id 0) in all keys and caches. The atlas and shaping cache stay clean because every entry is correctly keyed to the font that actually produced it. When the real font loads, the reactive observer triggers re-shaping and re-rendering with the correct font — fresh cache misses produce correct entries. No stale data to purge.

---

## Plan

### Phase 1: `ResolvedFont` — couple font_id with font_data

**File: `src/text/font_registry.rs`**

Add a type that can only be constructed from a successful `FontRegistry` lookup:

```rust
/// Proof that a `FontId` maps to a loaded font. Constructed only via
/// `FontRegistry::resolve()` or `FontRegistry::resolve_or_embedded()`.
/// Couples font identity with font data so they can never diverge.
pub struct ResolvedFont<'a> {
    id:   FontId,
    font: &'a Font,
}
```

Methods: `id()`, `data() -> &[u8]`, `name() -> &str`, `font() -> &Font`

Add two resolution methods:

```rust
/// Returns `None` if the font isn't registered.
pub fn resolve(&self, id: FontId) -> Option<ResolvedFont<'_>>

/// Always succeeds. Returns the requested font if loaded, or the
/// embedded font (FontId::MONOSPACE) as a placeholder. The returned
/// `ResolvedFont` always carries the *actual* font's identity —
/// never the requested id with mismatched data.
pub fn resolve_or_embedded(&self, id: FontId) -> ResolvedFont<'_>
```

`resolve_or_embedded` is the workhorse for rendering. It provides a placeholder font with honest identity, so all downstream keys (GlyphKey, ShapedCacheKey) reflect the font that actually produced the data.

### Phase 2: `GlyphKey` constructor requires `ResolvedFont`

**File: `src/text/atlas.rs`**

```rust
impl GlyphKey {
    pub fn new(font: &ResolvedFont<'_>, glyph_index: u16) -> Self {
        Self { font_id: font.id().0, glyph_index }
    }
}
```

Keep the raw struct fields for read access (hash, debug, tests), but all rendering code paths use the constructor. This makes it structurally impossible to construct a `GlyphKey` with a font_id that doesn't match the font data used for rasterization.

### Phase 3: Replace all silent fallbacks with `ResolvedFont`

Replace every `map_or(EMBEDDED_FONT, ...)` and `unwrap_or("JetBrains Mono")` with `resolve_or_embedded()`. The resolved font's identity flows through shaping, caching, and rasterization consistently.

| File | Location | Current | Replacement |
|------|----------|---------|-------------|
| `src/render/text_renderer.rs` | `shape_text_cached` ~644 | `family_name(...).unwrap_or("JetBrains Mono")` | `resolved.name()` from `resolve_or_embedded()` |
| `src/render/text_renderer.rs` | `shape_text_to_quads` ~754 | `font(...).map_or(EMBEDDED_FONT, Font::data)` | `resolved.data()` from `resolve_or_embedded()` |
| `src/render/world_text.rs` | `shape_world_text` ~318 | `font(...).map_or(EMBEDDED_FONT, Font::data)` | `resolved.data()` from `resolve_or_embedded()` |
| `src/render/world_text.rs` | line height calc ~377 | `font_registry.font(FontId(style.font_id())).map_or(style.size(), \|f\| f.metrics(...).line_height)` | `resolved.font().metrics(style.size()).line_height` |
| `src/text/atlas.rs` | `preload` ~469 | silent early return | Change signature to take `&ResolvedFont` |

**Critical change in shaping:** The `ShapedCacheKey` must use the *resolved* font_id (possibly 0 if falling back), not the *requested* font_id. This means `shape_text_cached` receives the `ResolvedFont` and uses `resolved.id()` for the cache key. When the real font loads, the cache key changes → cache miss → re-shapes with the correct font.

**Critical change in quad generation:** `shape_text_to_quads` uses `resolved.id()` for `GlyphKey` construction and `resolved.data()` for rasterization. Both come from the same `ResolvedFont`, so they cannot diverge.

### Phase 4: Shared family names for measurer

**File: `src/text/font_registry.rs`** — add `family_names: Arc<RwLock<Vec<String>>>` field
- `register_font()` pushes the new name to the shared vec
- Add `shared_family_names() -> Arc<RwLock<Vec<String>>>`

**File: `src/text/measurer.rs`** — change `families: Vec<String>` → `Arc<RwLock<Vec<String>>>`
- Acquire read lock for name lookup, drop before shaping
- Fallback to "JetBrains Mono" when font_id not in list (measurement with placeholder is consistent with rendering with placeholder)

**File: `src/plugin/mod.rs`** — update measurer construction to pass `shared_family_names()`

### Phase 5: Reactive `FontRegistered` observer

**File: `src/plugin/mod.rs`** — add observer:

```rust
fn on_font_loaded(
    trigger: On<FontRegistered>,
    mut cache: ResMut<ShapedTextCache>,
    mut world_texts: Query<&mut TextStyle, With<WorldText>>,
    mut panels: Query<&mut DiegeticPanel>,
)
```

Actions (no atlas purge needed — atlas was never poisoned):
1. `cache.invalidate_font(trigger.id.0)` — remove shaping results produced with the placeholder font so they get re-shaped with the real font
2. For each `WorldText` with matching *requested* `font_id` → touch `TextStyle` via `DerefMut` to trigger change detection
3. For each `DiegeticPanel` whose tree uses that font → touch via `DerefMut` to trigger change detection and re-layout

Register with `app.add_observer(on_font_loaded)`.

**Note on panel invalidation:** Accessing `&mut DiegeticPanel` via `DerefMut` triggers Bevy's change detection without needing `Clone`. The system `compute_panel_layouts` checks `is_changed()` and will recompute layout.

### Phase 6: `ShapedTextCache::invalidate_font()`

**File: `src/render/text_renderer.rs`**

```rust
impl ShapedTextCache {
    /// Removes all cached shaping results and measurements for glyphs
    /// that were shaped with the given font_id as a placeholder.
    /// Called when the real font loads so text gets re-shaped correctly.
    pub fn invalidate_font(&mut self, font_id: u16) {
        // Remove entries where the requested font matches but the
        // resolved font was the embedded fallback (font_id 0).
        // Also remove direct entries for this font_id to force re-shaping.
        self.entries.retain(|key, _| key.font_id != FontId::MONOSPACE.0 || ...);
        self.measurements.retain(|key, _| ...);
    }
}
```

**Design note:** The cache key currently stores the *resolved* font_id (the actual font used). When font 2 wasn't loaded, entries were keyed with font_id 0. We need a way to identify "entries that were placeholder shaping for font 2." Two options:

**Option A — Simple invalidation:** Clear all entries keyed with font_id 0 when *any* new font loads. JB Mono text will be re-shaped on next render (cheap, one-time cost).

**Option B — Targeted invalidation:** Add a `requested_font_id` field to `ShapedCacheKey` so we can invalidate only entries where `requested_font_id == trigger.id && resolved_font_id == 0`. More precise but adds a field to every cache key.

Recommend **Option A** for simplicity. Font loading is a rare one-time event.

### Phase 7: `LayoutTree::uses_font()` helper

**File: `src/layout/element.rs`**

```rust
impl LayoutTree {
    pub fn uses_font(&self, font_id: u16) -> bool {
        self.elements.iter().any(|el| matches!(
            &el.content,
            ElementContent::Text { config, .. } if config.font_id() == font_id
        ))
    }
}
```

Used by the `on_font_loaded` observer to filter panels that need re-layout.

### Phase 8: Exports

- `src/text/mod.rs` — `pub use font_registry::ResolvedFont;`
- `src/lib.rs` — `pub use text::ResolvedFont;`

---

## How it works end-to-end

**Before font loads:**
1. Text configured with `font_id: 2` → `resolve_or_embedded(FontId(2))` returns `ResolvedFont { id: FontId(0), font: &jb_mono }`
2. Shaping uses "JetBrains Mono" name → parley produces JB Mono glyph indices
3. `ShapedCacheKey` uses font_id 0 → correctly cached under embedded font
4. `GlyphKey::new(&resolved, glyph_index)` → `GlyphKey { font_id: 0, glyph_index: <JB Mono index> }`
5. Atlas rasterizes with JB Mono data under font_id 0 → correctly cached
6. Text renders in JB Mono as placeholder — measurement and rendering are consistent

**When font loads:**
1. `consume_loaded_fonts` registers "EB Garamond", triggers `FontRegistered { id: FontId(2) }`
2. `on_font_loaded` observer fires:
   - `cache.invalidate_font(2)` → clears placeholder shaping entries
   - Touches `TextStyle`/`DiegeticPanel` for affected entities → change detection triggered
   - Family names already updated (shared `Arc<RwLock<Vec<String>>>`)
3. Next frame:
   - `compute_panel_layouts` runs on changed panels → measurer uses "EB Garamond" name → correct measurements
   - `resolve_or_embedded(FontId(2))` returns `ResolvedFont { id: FontId(2), font: &eb_garamond }`
   - Shaping uses "EB Garamond" → correct glyph indices
   - `GlyphKey { font_id: 2, glyph_index: <EB Garamond index> }` → atlas cache miss → rasterized with correct data
4. `WhenReady` waits for async rasterization → text appears in EB Garamond

**No poisoning at any stage. No purge needed.**

---

## Files changed

| File | Changes |
|------|---------|
| `src/text/font_registry.rs` | `ResolvedFont` struct, `resolve()`, `resolve_or_embedded()`, `Arc<RwLock<Vec<String>>>` shared families |
| `src/text/atlas.rs` | `GlyphKey::new()` constructor |
| `src/text/measurer.rs` | `families` → `Arc<RwLock<Vec<String>>>` |
| `src/text/mod.rs` | Export `ResolvedFont` |
| `src/render/text_renderer.rs` | Replace 2 fallbacks with `resolve_or_embedded()`, add `ShapedTextCache::invalidate_font()` |
| `src/render/world_text.rs` | Replace 1 fallback with `resolve_or_embedded()` |
| `src/layout/element.rs` | `LayoutTree::uses_font()` |
| `src/plugin/mod.rs` | `on_font_loaded` observer, shared families wiring |
| `src/lib.rs` | Export `ResolvedFont` |

---

## Verification

1. `cargo build && cargo +nightly fmt` — compiles clean
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo nextest run` — all existing tests pass
4. `cargo run --example typography --features typography_overlay` — verify:
   - Controls panel shows text in JB Mono on startup, swaps to correct fonts when loaded
   - Switch to EB Garamond (press 3), verify 'E' in WAVEFORM renders correctly as a serif 'E'
5. `cargo mend` — no warnings
