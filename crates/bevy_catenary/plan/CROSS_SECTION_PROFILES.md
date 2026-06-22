# Pluggable Cross-Section Profiles for bevy_catenary

## Context

The tube mesh generation currently hardcodes a circular cross-section. The user wants arbitrary profile shapes (circle, rectangle/ribbon, custom polygon) swept along cable paths. This enables ribbons, flat arrows, custom-shaped conduits, and other non-circular swept geometry — all using the same path computation (catenary, linear, routed) and mesh pipeline (RMF, elbows, caps, inside normals).

The circle is baked in at exactly **two spots** in `src/plugin/mesh.rs`:
1. `generate_tube_rings()` — the ring vertex loop (cos/sin circle)
2. `add_hemisphere_cap()` — cap generation assumes circular equator

Everything else (RMF, quad connection, elbows, trimming, inside normals, flat caps) is already profile-agnostic.

## Approach

### New Types

**`CrossSection` enum** — defines the 2D profile shape:
- `Circle { radius, sides }` — current behavior
- `Rectangle { half_width, half_height, corner_subdivisions, corner_radius }` — ribbon/flat strip
- `Custom { vertices: Vec<Vec2>, normal_style: NormalStyle }` — user polygon

**`NormalStyle` enum** — how vertex normals are computed:
- `Smooth` — average adjacent edge normals (circles, rounded shapes)
- `Faceted` — per-edge normals for hard edges (rectangles, sharp polygons)

**`ResolvedProfile` struct** (internal, not public) — pre-computed 2D vertices, normals, UV v-coordinates, and bounding radius from any `CrossSection` variant. Computed once per mesh generation via `resolve_profile()`.

### CableMeshConfig Change

Add `profile: Option<CrossSection>` with `None` default. When `None`, falls back to existing `radius` + `sides` as a circle. **100% backward compatible** — no existing code needs changes.

### Pipeline Modification

1. `generate_tube_mesh()` calls `resolve_profile(config)` once at the top
2. `generate_tube_rings()` uses `ResolvedProfile` vertices/normals instead of cos/sin loop
3. `add_end_caps()` receives `&ResolvedProfile`, gates `Round` caps on `supports_round_cap` (true only for circles)
4. `ElbowParams::from_config()` uses `bounding_radius` from profile
5. `sides` references throughout become `profile.vertex_count()`

### Cap Rules by Profile
- **Circle**: `Round`, `Flat`, `None` all work (unchanged)
- **Rectangle / Custom**: `Flat` and `None` work. `Round` degrades to `Flat` with a warning log

## Critical Files

- `src/plugin/mesh.rs` — all ring/cap generation, `CableMeshConfig`, new types
- `src/plugin/mod.rs` — export `CrossSection`, `NormalStyle`
- `src/lib.rs` — re-export for public API
- `examples/playground.rs` — optional: add a ribbon demo to section 2 or a new section

## Implementation Steps

1. Add `NormalStyle` and `CrossSection` enums (both `Clone, Debug, Reflect`)
2. Add `ResolvedProfile` struct and `resolve_profile()` function with unit tests verifying circle output matches old hardcoded math exactly
3. Add `profile: Option<CrossSection>` to `CableMeshConfig` with `None` default
4. Refactor `generate_tube_rings()` to use `ResolvedProfile` instead of circle loop
5. Refactor `add_end_caps()` / `add_single_cap()` to accept `&ResolvedProfile` and gate round caps
6. Update `ElbowParams` to use `bounding_radius`
7. Update exports in `mod.rs` and `lib.rs`
8. Regression test: default config produces identical mesh output
9. Add tests for rectangle and custom profiles

## Verification

1. `cargo build && cargo +nightly fmt` — must compile clean
2. `cargo nextest run` — all existing tests pass (regression)
3. Run playground example — all 9 sections look identical to before (no visual regression from the refactor)
4. Test a `CrossSection::Rectangle` in a new playground cable to verify ribbon rendering
5. Test `CapStyle::Round` with rectangle profile degrades gracefully to `Flat`

## Notes

- Custom profiles must be convex for flat caps (triangle fan). Non-convex would need ear-clipping triangulation — defer to future work
- Camera-facing billboard ribbons are a runtime system concern, not a profile concern — out of scope
- UV v-coordinate for non-circular profiles should be proportional to arc length around the perimeter to avoid texture stretching
