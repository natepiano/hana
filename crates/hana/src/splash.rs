use bevy::prelude::*;

pub struct SplashPlugin;

impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Text::new(
            "Welcome to hana!\n\
            Press f1 to start the basic visualization\n\
            Press shift-f1 to stop it\n\
            Press p to Ping it\n\
            \n\
            Press f2 to viz start the basic visualization\n\
            Press shift-f2 to viz stop it\n\
            Press v to viz Ping it\n\
            ",
        ),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}
