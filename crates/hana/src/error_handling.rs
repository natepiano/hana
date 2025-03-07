use bevy::color::palettes::tailwind::*;
use bevy::prelude::*; // Add this import
use error_stack::Report;

use crate::error::{Error, Severity};
use crate::tokio_runtime::{BevyReceiver, BevyVisualizationEvent};

pub struct ErrorHandlingPlugin;

impl Plugin for ErrorHandlingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ErrorState>()
            // Run error processing before display updates
            .add_systems(Update, process_error_events)
            .add_systems(Update, (update_error_display,).after(process_error_events));
    }
}

/// Resource to store error state and history
#[derive(Resource, Default)]
pub struct ErrorState {
    pub errors: Vec<Report<Error>>,
}

/// Component marking an entity as an error message
#[derive(Component)]
pub struct ErrorMessage {
    pub created_for: usize, // Index in the ErrorState.errors array
}

/// System to process errors from Tokio
pub fn process_error_events(
    mut error_state: ResMut<ErrorState>,
    bevy_receiver: Res<BevyReceiver>,
    mut exit: EventWriter<AppExit>,
) {
    // Process all pending messages from Tokio
    while let Ok(event) = bevy_receiver.0.try_recv() {
        if let BevyVisualizationEvent::Failed(report) = event {
            // Log the entire error report
            error!("Error received: {report:?}");

            // Check severity and exit on critical errors
            let is_critical = matches!(report.current_context(), Error::TokioRuntimeChannelClosed);
            if is_critical {
                error!("CRITICAL ERROR detected - application will exit");
                exit.send(AppExit::Error(std::num::NonZeroU8::new(1).unwrap()));
            }

            // Add the report to our error history (taking ownership)
            error_state.errors.push(report);
        }
    }
}

/// System to display error messages in the UI
pub fn update_error_display(
    error_state: Res<ErrorState>,
    mut commands: Commands,
    existing_messages: Query<(Entity, &ErrorMessage)>,
) {
    // Only run if there are errors and the resource changed
    if !error_state.is_changed() && error_state.errors.is_empty() {
        return;
    }

    // Clear existing messages that no longer match errors in the state
    for (entity, message) in existing_messages.iter() {
        if message.created_for >= error_state.errors.len() {
            commands.entity(entity).despawn_recursive();
        }
    }

    // Create or update error messages
    for (idx, report) in error_state.errors.iter().enumerate() {
        // Skip if this error already has a message entity
        if existing_messages
            .iter()
            .any(|(_, msg)| msg.created_for == idx)
        {
            continue;
        }

        // Get error type
        let error = report.current_context();
        let severity = match error {
            Error::Visualization => Severity::Error,
            Error::TokioRuntimeChannelClosed => Severity::Critical,
        };

        // Get color based on severity
        let color = match severity {
            Severity::Critical => RED_500,
            Severity::Error => ORANGE_500,
            Severity::Warning => YELLOW_500,
        };

        // Create message text from the report
        let severity_str = match severity {
            Severity::Critical => "CRITICAL",
            Severity::Error => "ERROR",
            Severity::Warning => "WARNING",
        };
        let message = format!("[{}] {}", severity_str, report);

        // Spawn error message UI using Bevy 0.15's approach
        commands.spawn((
            // Base text component
            Text::new(message),
            // Position with Node component
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(10.0 + (idx as f32 * 20.0)),
                left: Val::Px(10.0),
                ..default()
            },
            // Text color component
            TextColor(color.into()),
            // Font size component
            TextFont {
                font_size: 16.0,
                ..default()
            },
            // Our marker component
            ErrorMessage { created_for: idx },
        ));
    }
}
