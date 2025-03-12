use bevy::color::palettes::tailwind::*;
use bevy::prelude::*;

pub struct OscillatingGizmoPlugin;

const DEFAULT_MIN_SCALE: f32 = 0.4;
const DEFAULT_MAX_SCALE: f32 = 0.45;
const DEFAULT_FREQUENCY: f32 = 5.0;

impl Plugin for OscillatingGizmoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, draw_oscillating_circle);
    }
}

#[derive(Resource)]
struct CircleGizmoParams {
    min_scale: f32,
    max_scale: f32,
    frequency: f32,
}

fn setup(mut commands: Commands) {
    // Parameters for our oscillating circle
    commands.insert_resource(CircleGizmoParams {
        min_scale: DEFAULT_MIN_SCALE,
        max_scale: DEFAULT_MAX_SCALE,
        frequency: DEFAULT_FREQUENCY,
    });
}

fn draw_oscillating_circle(time: Res<Time>, params: Res<CircleGizmoParams>, mut gizmos: Gizmos) {
    let t = time.elapsed_secs() * params.frequency;
    let scale_factor = (t.sin() + 1.0) / 2.0; // Value between 0 and 1

    // Calculate radius between min and max
    let current_radius = params.min_scale + (params.max_scale - params.min_scale) * scale_factor;

    // Draw a circle at the origin (center of the screen from camera's perspective)
    gizmos
        .circle(
            Vec3::ZERO,     // Position at origin
            current_radius, // Current oscillating radius
            RED_500,        // Color of the circle
        )
        .resolution(64);
}
