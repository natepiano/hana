use bevy::prelude::Component;

/// Marker on a [`WorldText`](super::WorldText) entity spawned as a child of a
/// [`DiegeticPanel`](crate::DiegeticPanel).
///
/// Standalone-text systems filter `Without<PanelChild>` to skip panel labels
/// (the panel-text systems render those); panel-text systems filter
/// `With<PanelChild>`. The layout payload lives in
/// [`PanelTextLayout`](crate::render::panel_text::PanelTextLayout).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PanelChild;
