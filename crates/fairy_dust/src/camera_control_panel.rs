//! Capability: a screen-space `bevy_diegetic` panel anchored bottom-right
//! that documents `bevy_lagrange::OrbitCam` mouse and trackpad controls.
//!
//! Pulls in `DiegeticUiPlugin` and `MeshPickingPlugin` deduplicated.
//! Intended to pair with [`crate::FairyDustExt::with_orbit_cam_configured`].

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::default_panel_material;

use crate::ensure_plugin;

pub(crate) fn install(app: &mut App) {
    ensure_plugin(app, DiegeticUiPlugin);
    ensure_plugin(app, MeshPickingPlugin);
    app.add_systems(Startup, spawn);
}

#[derive(Component)]
struct CameraControlPanel;

const RADIUS: Px = Px(15.0);
const FRAME_PAD: Px = Px(2.0);
const BORDER: Px = Px(2.0);
const INSET: Px = Px(FRAME_PAD.0 + BORDER.0);
const INNER_RADIUS: Px = Px(RADIUS.0 - INSET.0);

const TITLE_SIZE: Pt = Pt(16.0);
const HEADER_SIZE: Pt = Pt(13.0);
const LABEL_SIZE: Pt = Pt(11.0);

const FRAME_BG: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const INNER_BG: Color = Color::srgba(0.02, 0.03, 0.07, 0.80);
const BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const DIVIDER: Color = Color::srgba(0.15, 0.4, 0.6, 0.25);
const TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HEADER_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
const LABEL_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);

fn spawn(mut commands: Commands) {
    let unlit = StandardMaterial {
        unlit: true,
        ..default_panel_material()
    };
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomRight)
        .material(unlit.clone())
        .text_material(unlit)
        .layout(build_layout)
        .build();

    match panel {
        Ok(p) => {
            commands.spawn((CameraControlPanel, p, Transform::default()));
        },
        Err(e) => {
            error!("fairy_dust: failed to build camera control panel: {e}");
        },
    }
}

fn build_layout(b: &mut LayoutBuilder) {
    let title = LayoutTextStyle::new(TITLE_SIZE).with_color(TITLE_COLOR);
    let header = LayoutTextStyle::new(HEADER_SIZE).with_color(HEADER_COLOR);
    let label = LayoutTextStyle::new(LABEL_SIZE).with_color(LABEL_COLOR);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(FRAME_PAD))
            .corner_radius(CornerRadius::new(RADIUS, Px(0.0), RADIUS, Px(0.0)))
            .background(FRAME_BG)
            .border(Border::all(BORDER, BORDER_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(Px(10.0)))
                    .child_gap(Px(6.0))
                    .corner_radius(CornerRadius::new(
                        INNER_RADIUS,
                        Px(0.0),
                        INNER_RADIUS,
                        Px(0.0),
                    ))
                    .background(INNER_BG)
                    .border(Border::all(Px(1.0), BORDER_DIM)),
                |b| {
                    b.text("CAMERA", title);

                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::LeftToRight)
                            .child_gap(Px(12.0)),
                        |b| {
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .direction(Direction::TopToBottom)
                                    .child_gap(Px(4.0)),
                                |b| {
                                    b.text("Mouse", header.clone());
                                    b.text("MMB drag \u{2192} Orbit", label.clone());
                                    b.text("Shift+MMB \u{2192} Pan", label.clone());
                                    b.text("Scroll \u{2192} Zoom", label.clone());
                                },
                            );

                            b.with(
                                El::new()
                                    .width(Sizing::fixed(Px(1.0)))
                                    .height(Sizing::GROW)
                                    .background(DIVIDER),
                                |_| {},
                            );

                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .direction(Direction::TopToBottom)
                                    .child_gap(Px(4.0)),
                                |b| {
                                    b.text("Trackpad", header.clone());
                                    b.text("Scroll \u{2192} Orbit", label.clone());
                                    b.text("Shift+Scroll \u{2192} Pan", label.clone());
                                    b.text("Ctrl+Scroll \u{2192} Zoom", label.clone());
                                    b.text("Pinch \u{2192} Zoom", label.clone());
                                },
                            );
                        },
                    );
                },
            );
        },
    );
}
