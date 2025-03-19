use bevy::color::palettes::tailwind::*;
use bevy::prelude::*;
use error_stack::Report;
use hana_viz::PendingConnections;

use crate::error::{Error, Severity};

const ERROR_MESSAGE_LEFT_OFFSET: f32 = 10.0;
const ERROR_MESSAGE_BOTTOM_BASE: f32 = 10.0;
const ERROR_MESSAGE_SPACING: f32 = 20.0;
const ERROR_MESSAGE_FONT_SIZE: f32 = 16.0;

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

/// System to process visualization errors
pub fn process_visualization_errors(
    mut error_state: ResMut<ErrorState>,
    pending_connections: Res<PendingConnections>,
) {
    // Only process if the PendingConnections resource has changed
    if !pending_connections.is_changed() {
        return;
    }

    // Process any newly failed connections
    for (details, error_msg) in &pending_connections.failed {
        // Log the error
        error!(
            "Visualization '{}' failed with error: {}",
            details.name, error_msg
        );

        // Create an error report
        let report = Report::new(Error::Visualization).attach_printable(format!(
            "Visualization '{}' failed with error: {}",
            details.name, error_msg
        ));

        // Add the report to our error history
        error_state.errors.push(report);
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
                bottom: Val::Px(ERROR_MESSAGE_SPACING + (idx as f32 * ERROR_MESSAGE_BOTTOM_BASE)),
                left: Val::Px(ERROR_MESSAGE_LEFT_OFFSET),
                ..default()
            },
            // Text color component
            TextColor(color.into()),
            // Text font component
            TextFont {
                font_size: ERROR_MESSAGE_FONT_SIZE,
                ..default()
            },
            // Our marker component
            ErrorMessage { created_for: idx },
        ));
    }
}
