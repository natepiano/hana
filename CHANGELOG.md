# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.6] - 2026-05-20

### Changed

- Update `bevy_enhanced_input` from 0.24 to 0.25.0

## [0.0.5] - 2026-04-06

### Added

- `ToU32` impl for `i32`

## [0.0.4] - 2026-04-06

### Added

- `ToF64` impl for `u64`

## [0.0.3] - 2026-03-29

### Added

- `ToU8` cast trait with impls for `f32`, `u32`, `usize`
- `ToU16` cast trait with impls for `usize`, `u32`, `f32`
- `ToF64` cast trait with impls for `usize`, `u32`, `i32`, `f32`
- `ToF32` impl for `f64`
- `ToI32` impl for `f64`
- `ToU32` impl for `u64`
- Prelude re-exports for all new cast traits

## [0.0.2] - 2026-03-25

### Changed

- `input` feature is no longer a default — libraries only need `math` (the default), binaries opt into `input` explicitly with `features = ["input"]`

## [0.0.1] - 2026-03-25

### Added

- Semantic math newtypes (`Position`, `Displacement`, `Velocity`, `ScreenPosition`, `Orientation`) — zero-cost wrappers that act like `Vec3`/`Vec2`/`Quat` but prevent accidental mixing at compile time
- `new()` constructors for all newtypes (`Position::new(x, y, z)`, `ScreenPosition::new(x, y)`)
- Cross-type arithmetic: `Position + Vec3 → Position`, `Position - Vec3 → Position` for natural mixing with Bevy APIs
- `distance`, `distance_squared`, `lerp` methods accepting `impl Into<Self>` — work with both newtypes and raw `Vec3`/`Vec2`
- Numeric cast traits (`ToF32`, `ToI32`, `ToU32`, `ToUsize`) for clean numeric conversions that centralize clippy pedantic cast allows
- Input macros (`action!`, `event!`, `bind_action_system!`) for wiring keyboard actions through `bevy_enhanced_input`
- `Keybindings` builder for modifier-aware keybinding setup with platform-specific Cmd/Ctrl handling
- Feature flags: `math` (default) and `input` (default, requires `bevy_enhanced_input`)
