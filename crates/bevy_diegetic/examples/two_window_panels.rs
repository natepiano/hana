//! Smoke test for multi-window screen-space panels.
//!
//! Spawns two OS windows and one [`DiegeticPanel::screen()`] panel per
//! window, each pinned with [`.window_entity(..)`]. Each panel renders
//! distinct text so the two windows are visually distinguishable.
//!
//! Closing one window should leave the other window's panel working.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;
use bevy::window::WindowResolution;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;

const PANEL_WIDTH: f32 = 360.0;
const PANEL_HEIGHT: f32 = 140.0;
const TITLE_SIZE: f32 = 22.0;
const BODY_SIZE: f32 = 15.0;

const PANEL_BACKGROUND: Color = Color::srgba(0.08, 0.08, 0.12, 0.92);
const BORDER_COLOR: Color = Color::srgba(0.3, 0.5, 0.9, 0.7);
const TITLE_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);
const BODY_COLOR: Color = Color::srgb(0.85, 0.85, 0.9);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Window A — Primary".to_owned(),
                    resolution: WindowResolution::new(720_u32, 480_u32),
                    ..default()
                }),
                ..default()
            }),
            DiegeticUiPlugin,
        ))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, primary: Query<Entity, With<PrimaryWindow>>) {
    let secondary = commands
        .spawn(Window {
            title: "Window B — Secondary".to_owned(),
            resolution: WindowResolution::new(720_u32, 480_u32),
            ..default()
        })
        .id();

    // Camera for the secondary window so non-overlay content (if any) has
    // somewhere to render. The screen-space overlay camera is spawned
    // automatically by the panel observer.
    commands.spawn((
        Camera3d::default(),
        Camera::default(),
        bevy::camera::RenderTarget::Window(WindowRef::Entity(secondary)),
        Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Primary-window scene camera.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let Ok(primary_entity) = primary.single() else {
        return;
    };

    spawn_panel(&mut commands, "Window A", WindowRef::Entity(primary_entity));
    spawn_panel(&mut commands, "Window B", WindowRef::Entity(secondary));
}

fn spawn_panel(commands: &mut Commands, title: &'static str, window: WindowRef) {
    let panel = DiegeticPanel::screen()
        .size(
            Sizing::fixed(Px(PANEL_WIDTH)),
            Sizing::fixed(Px(PANEL_HEIGHT)),
        )
        .anchor(Anchor::Center)
        .window(window)
        .layout(|b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(16.0))
                    .child_gap(8.0)
                    .background(PANEL_BACKGROUND)
                    .border(Border::all(2.0, BORDER_COLOR)),
                |b| {
                    b.text(
                        title,
                        LayoutTextStyle::new(TITLE_SIZE).with_color(TITLE_COLOR),
                    );
                    b.text(
                        "Close one window — the other keeps rendering.",
                        LayoutTextStyle::new(BODY_SIZE).with_color(BODY_COLOR),
                    );
                },
            );
        })
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn(panel);
        },
        Err(err) => {
            error!("failed to build screen panel for {title}: {err:?}");
        },
    }
}
