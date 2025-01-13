Hana Spike Plan
Overview
Building a minimal proof of concept for a Bevy-based visualization system with dynamically loadable plugins.
Core Requirements

Create a main Bevy application that can load plugins
Create a basic plugin interface that integrates with Bevy's plugin system
Create a simple example plugin with a 3D visualization
Demonstrate dynamic loading of plugin at runtime

Project Structure
hana/
├── Cargo.toml                  # Workspace manifest
├── crates/
│   └── hana/                  # main app
│   └── hana_network/          # main app
│   └── hana_visualization/    # Core plugin interface
├── examples/
│   └── basic_visualization/   # Example visualization plugin

Technology Stack

Rust (stable)
Bevy 0.15.1

Implementation Plan

- [x] Create workspace and basic crate structure
- [x] Implement minimal visualization in hana_visualization
- [x] Build main app to load and run plugin
- [x] Verify dynamic loading works - **it does not**, so we had to switch to a different approach - using separate binaries and networking to communicate between them
