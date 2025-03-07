use bevy::color::palettes::tailwind::*;
use bevy::prelude::*;
use error_stack::Report;
use hana_viz::VisualizationStateChanged;

use crate::error::{Error, Severity};

pub struct ErrorHandlingPlugin;

impl Plugin for ErrorHandlingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ErrorState>()
            // Run error processing before display updates
            .add_systems(Update, process_visualization_errors)
            .add_systems(
                Update,
                (update_error_display,).after(process_visualization_errors),
            );
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
pub fn process_visualization_errors(
    mut error_state: ResMut<ErrorState>,
    mut state_events: EventReader<VisualizationStateChanged>,
    _exit: EventWriter<AppExit>,
) {
    for event in state_events.read() {
        // Only process events with errors
        if let Some(error_msg) = &event.error {
            // Log the error
            error!(
                "Visualization {:?} error in state {}: {}",
                event.entity, event.new_state, error_msg
            );

            // Create an error report
            let report = Report::new(Error::Visualization).attach_printable(format!(
                "Visualization {:?} error in state {}: {}",
                event.entity, event.new_state, error_msg
            ));

            // Add the report to our error history
            error_state.errors.push(report);

            // Check for critical errors
            if event.new_state == "Error" || event.new_state == "Disconnected" {
                // These are errors but not critical - application can continue
                // If we had critical errors that should exit the app, we would handle them here
                // exit.send(AppExit::Error(std::num::NonZeroU8::new(1).unwrap()));
            }
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
