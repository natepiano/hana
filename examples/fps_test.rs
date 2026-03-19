//! Minimal FPS test — isolate MSDF rendering overhead.

use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextConfig;

const FONT_SIZE: f32 = 7.0;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(DiegeticUiPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, update_fps)
        .run();
}

#[derive(Component)]
struct FpsPanel;

fn setup(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let tree = build_panel("--");
    commands.spawn((
        FpsPanel,
        DiegeticPanel {
            tree,
            layout_width: 160.0,
            layout_height: 224.0,
            world_width: 1.5,
            world_height: 2.1,
        },
        Transform::IDENTITY,
    ));
}

fn update_fps(
    diagnostics: Res<DiagnosticsStore>,
    mut panels: Query<&mut DiegeticPanel, With<FpsPanel>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed)
        .map_or_else(|| "--".to_string(), |v| format!("{v:.0}"));

    for mut panel in &mut panels {
        panel.tree = build_panel(&fps);
    }
}

fn build_panel(fps: &str) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(160.0))
            .height(Sizing::fixed(224.0))
            .padding(Padding::all(8.0))
            .direction(Direction::TopToBottom)
            .background(bevy::color::Color::srgb_u8(40, 44, 52))
            .border(Border::all(2.0, bevy::color::Color::srgb_u8(120, 130, 140))),
    );

    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(5.0))
            .direction(Direction::TopToBottom)
            .child_gap(2.0),
        |b| {
            b.text("STATUS", TextConfig::new(FONT_SIZE));

            // Same rows as side_by_side.
            for (label, value) in [
                ("panel size:", "medium"),
                ("layout units:", "160"),
                ("renderer:", "msdf"),
                ("radius:", "3.0"),
                ("fps:", fps),
                ("frame ms:", "--"),
            ] {
                b.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::FIT)
                        .direction(Direction::LeftToRight),
                    |b| {
                        b.text(label, TextConfig::new(FONT_SIZE));
                        b.with(
                            El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                            |_| {},
                        );
                        b.text(value, TextConfig::new(FONT_SIZE));
                    },
                );
            }

            b.with(
                El::new().width(Sizing::GROW).height(Sizing::fixed(4.0)),
                |_| {},
            );

            b.text(
                "Lorem ipsum dolor sit amet, consectetur adipiscing elit",
                TextConfig::new(FONT_SIZE),
            );
        },
    );

    builder.build()
}
