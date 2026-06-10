mod current_only;
mod restored;
mod span;

use bevy::prelude::ChildSpawnerCommands;
use bevy::prelude::Color;
use bevy::prelude::TextFont;

use super::values::CurrentValues;
use crate::events::CachedMismatchState;
use crate::events::CachedRestoredState;

pub(super) fn build_current_only_spans(
    child_spawner: &mut ChildSpawnerCommands,
    current_values: &CurrentValues,
    font: &TextFont,
) {
    current_only::build_current_only_spans(child_spawner, current_values, font);
}

pub(super) fn build_restored_spans(
    child_spawner: &mut ChildSpawnerCommands,
    cached_restored_state: &CachedRestoredState,
    cached_mismatch_state: Option<&CachedMismatchState>,
    current_values: &CurrentValues,
    font: &TextFont,
) {
    restored::build_restored_spans(
        child_spawner,
        cached_restored_state,
        cached_mismatch_state,
        current_values,
        font,
    );
}

pub(super) fn add_span(
    child_spawner: &mut ChildSpawnerCommands,
    font: &TextFont,
    text: &str,
    color: Color,
) {
    span::add_span(child_spawner, font, text, color);
}
