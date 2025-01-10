use bevy::prelude::*;
use hana_plugin::{HanaPlugin, PluginError, PluginFactory};

/// Basic plugin component to identify our entities
#[derive(Component)]
struct ExampleCube;

/// The plugin that will be exposed to Hana
pub struct ExamplePlugin;

/// The actual Bevy plugin that contains our visualization logic
pub struct ExampleBevyPlugin;

impl Plugin for ExampleBevyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_cube)
            .add_systems(Update, rotate_cube);
    }
}

impl HanaPlugin for ExamplePlugin {
    fn create_bevy_plugin(&self) -> Box<dyn Plugin> {
        Box::new(ExampleBevyPlugin)
    }
}

impl PluginFactory for ExamplePlugin {
    fn create() -> Box<dyn HanaPlugin> {
        Box::new(ExamplePlugin)
    }
}

// Required for dynamic loading
#[no_mangle]
pub extern "C" fn create_plugin() -> Box<dyn HanaPlugin> {
    ExamplePlugin::create()
}

// Bevy systems for our visualization
fn setup_cube(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Spawn a cube
    commands.spawn((
        ExampleCube,
        Mesh3d(meshes.add(Cuboid::default())),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));

    // Add a light
    commands.spawn((
        PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
}

fn rotate_cube(time: Res<Time>, mut query: Query<&mut Transform, With<ExampleCube>>) {
    for mut transform in &mut query {
        transform.rotate_y(time.delta_secs());
    }
}
