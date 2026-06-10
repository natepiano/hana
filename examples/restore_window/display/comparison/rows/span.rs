use bevy::prelude::ChildSpawnerCommands;
use bevy::prelude::Color;
use bevy::prelude::TextColor;
use bevy::prelude::TextFont;
use bevy::prelude::TextSpan;

/// Add a single `TextSpan` child.
pub(super) fn add_span(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    text: &str,
    color: Color,
) {
    child_spawner.spawn((TextSpan(text.to_string()), font.clone(), TextColor(color)));
}
