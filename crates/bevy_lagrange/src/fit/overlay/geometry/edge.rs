use bevy::prelude::*;

/// Screen edge identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum Edge {
    /// Left screen edge.
    Left,
    /// Right screen edge.
    Right,
    /// Top screen edge.
    Top,
    /// Bottom screen edge.
    Bottom,
}
