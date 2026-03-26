# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> This crate is based on the work of [Plonq](https://github.com/Plonq) and his
> [bevy_panorbit_camera](https://github.com/Plonq/bevy_panorbit_camera). Thank you
> for graciously allowing this project to build on your foundation.

## [Unreleased]

### Added
- Pan, orbit, and zoom camera controls with smoothing, customizable sensitivity, and configurable key/mouse bindings
- Orthographic and perspective projection support
- Touch controls (one finger orbit, two finger pan, pinch zoom)
- Multi-viewport and multi-window support
- Render-to-texture camera support
- Optional `bevy_egui` feature to ignore input consumed by egui
- Optional `zoom_overlay` feature: zoom-to-fit, queued camera animations, event-driven camera control (`LookAt`, `ZoomToFit`, `AnimateToFit`, `PlayAnimation`), and debug visualization of fit targets with gizmos and screen-space labels
