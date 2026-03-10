//! Minimal diegetic UI panel example.
//!
//! Spawns a floating panel in 3D space with a status-panel-style layout,
//! rendered as debug wireframes via gizmos.
//!
//! # Retained-mode layout
//!
//! Unlike Clay (immediate-mode, rebuilds the tree every frame), `bevy_diegetic` is
//! retained-mode: the [`LayoutTree`] is built once here in `setup` and stored on the
//! [`DiegeticPanel`] component. The plugin's layout system only recomputes positions when
//! Bevy's change detection sees the component has been modified. On frames where nothing
//! changes, layout is skipped entirely — which is the natural pattern in ECS.
//!
//! # `El` vs `Element`
//!
//! [`El`] is the ergonomic builder facade. Under the hood, each `El` converts into an
//! [`Element`] — the canonical struct stored in the arena-based [`LayoutTree`]. You never
//! need to construct `Element` directly; `El` exposes the same properties as a fluent chain.
//! The separation keeps the storage format clean while giving users a nice API.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_panel
//! ```

use bevy::color::Color;
use bevy::prelude::*;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextConfig;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DiegeticUiPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // Camera.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Build a status panel layout.
    //
    // This tree is constructed once and handed to the ECS. The layout engine will compute
    // positions from it, and only recompute if the `DiegeticPanel` component changes.
    // In an immediate-mode engine you'd rebuild this every frame — here you don't.
    let mut builder = LayoutBuilder::new(160.0, 120.0);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(4.0))
            .direction(Direction::TopToBottom)
            .child_gap(4.0)
            .background(Color::srgb_u8(40, 44, 52))
            // Border shorthand: uniform width and color in one call.
            .border(Border::all(2.0, Color::srgb_u8(120, 130, 140))),
        |b| {
            // Header.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(20.0))
                    .background(Color::srgb_u8(60, 130, 180))
                    // Border fluent builder: per-side widths, set color separately.
                    // Same struct, just a different entry point — no sub-builder or `.end()`.
                    .border(
                        Border::new()
                            .bottom(1.0)
                            .color(Color::srgb_u8(40, 100, 160)),
                    ),
                |b| {
                    b.text("STATUS", TextConfig::new(12));
                },
            );

            // Body rows.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_gap(2.0),
                |b| {
                    key_value_row(b, "FPS", "60");
                    key_value_row(b, "Entities", "128");
                    key_value_row(b, "Draw Calls", "42");
                },
            );

            // Footer.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(16.0))
                    .background(Color::srgb_u8(80, 80, 90)),
                |b| {
                    b.text("v0.1.0", TextConfig::new(8));
                },
            );
        },
    );
    let tree = builder.build();

    // Spawn the panel as a Bevy entity. The `LayoutTree` lives on this component and
    // persists across frames. Layout is recomputed only when this component changes.
    commands.spawn((
        DiegeticPanel {
            tree,
            layout_width: 160.0,
            layout_height: 120.0,
            world_width: 1.0,
            world_height: 0.75,
        },
        Transform::default(),
    ));
}

/// Adds a key-value row: label on left, value pushed right by a grow spacer.
fn key_value_row(b: &mut LayoutBuilder, label: &str, value: &str) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(14.0))
            .direction(Direction::LeftToRight),
        |b| {
            b.text(label, TextConfig::new(9));
            b.with(
                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                |_| {},
            );
            b.text(value, TextConfig::new(9));
        },
    );
}
