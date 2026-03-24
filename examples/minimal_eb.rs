//! Minimal EB Garamond test — increasing text count in a `DiegeticPanel`.

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Font;
use bevy_diegetic::FontRegistered;
use bevy_diegetic::GlyphLoadingPolicy;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextConfig;

/// Number of text elements to spawn.
const TEXT_COUNT: usize = 20;

#[derive(Resource, Default)]
struct FontHandles(Vec<Handle<Font>>);

#[derive(Component)]
struct TestPanel;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
        ))
        .init_resource::<FontHandles>()
        .add_observer(on_font)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fh: ResMut<FontHandles>) {
    fh.0.push(asset_server.load("fonts/EBGaramond-Regular.ttf"));
    commands.spawn(Camera3d::default());
    commands.spawn(AmbientLight {
        color:                      Color::WHITE,
        brightness:                 500.0,
        affects_lightmapped_meshes: false,
    });
}

fn on_font(trigger: On<FontRegistered>, mut commands: Commands) {
    let samples = [
        "fi", "fl", "ffi", "ffl", "Th", "st", "ct", "AV", "To", "Wa", "fi", "fl", "ffi", "ffl",
        "Th", "st", "ct", "AV", "To", "Wa",
    ];

    info!("Building panel with {TEXT_COUNT} EB Garamond text elements...");
    let start = std::time::Instant::now();

    let mut builder = LayoutBuilder::new(800.0, 600.0);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom)
            .child_gap(4.0),
        |b| {
            for sample in samples.iter().take(TEXT_COUNT) {
                b.text(
                    *sample,
                    TextConfig::new(36.0)
                        .with_font(trigger.id.0)
                        .with_loading_policy(GlyphLoadingPolicy::Progressive),
                );
            }
        },
    );

    let tree = builder.build();
    info!("Tree built in {:?}", start.elapsed());

    commands.spawn((
        TestPanel,
        DiegeticPanel {
            tree,
            layout_width: 800.0,
            layout_height: 600.0,
            world_width: 4.0,
            world_height: 3.0,
        },
        Transform::from_xyz(0.0, 0.0, -3.0),
    ));
    info!("Panel spawned in {:?}", start.elapsed());
}
