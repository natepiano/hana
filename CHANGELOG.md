# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.0.2-rc.1] - 2026-06-08

### Changed

- Updated to Bevy 0.19

## [0.0.1] - 2026-04-06

### Added

- Three outline methods: JumpFlood (screen-space silhouette), WorldHull (world-unit extrusion), ScreenHull (pixel-width extrusion)
- Type-safe builder API: `Outline::jump_flood()`, `Outline::screen_hull()`, `Outline::world_hull()`
- Overlap modes for hull methods: Merged, Grouped, PerMesh
- Smoothed outline normal generation for correct extrusion on concave and hard-edged meshes
- HDR glow via intensity multiplier (works with Bevy's bloom)
- Automatic outline propagation from parent to descendant `Mesh3d` entities
- `NoOutline` marker to exclude specific children from propagation
- Skinning and morph target support
- Depth-aware rendering via depth prepass
