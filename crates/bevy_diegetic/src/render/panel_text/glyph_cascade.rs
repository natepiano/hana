use bevy::prelude::*;

use crate::cascade;
use crate::cascade::CascadeDefault;
use crate::cascade::Override;
use crate::cascade::Resolved;
use crate::cascade::TextLighting;
use crate::cascade::TextSidedness;
use crate::render::world_text::PanelChild;

/// Spawn-time cascade seed for a panel label's glyph lighting and sidedness.
///
/// Fires when a label first gains [`PanelChild`] and seeds its
/// `Resolved<TextLighting>` / `Resolved<TextSidedness>` via
/// [`resolve_walk`](cascade::resolve_walk). The walk honors the label's own
/// override first — `reconcile_panel_text_children` inserts one when the label
/// authored `TextStyle::with_lighting` / `with_sidedness` — then climbs
/// `ChildOf` to the panel's override (seeded by `seed_panel_overrides` for
/// screen panels and unlit-material panels), else the global default (`Lit` /
/// `DoubleSided`). `update_panel_text_geometry` reads these for the glyph
/// material. Later changes flow through the propagation pass, not this observer.
/// The glyph-render twin of `seed_panel_child_alpha`.
pub(super) fn seed_panel_child_glyph(
    trigger: On<Add, PanelChild>,
    lighting_overrides: Query<&Override<TextLighting>>,
    sidedness_overrides: Query<&Override<TextSidedness>>,
    parents: Query<&ChildOf>,
    lighting_default: Res<CascadeDefault<TextLighting>>,
    sidedness_default: Res<CascadeDefault<TextSidedness>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let lighting = cascade::resolve_walk::<TextLighting>(
        entity,
        &lighting_overrides,
        &parents,
        lighting_default.0,
    );
    let sidedness = cascade::resolve_walk::<TextSidedness>(
        entity,
        &sidedness_overrides,
        &parents,
        sidedness_default.0,
    );
    commands
        .entity(entity)
        .insert((Resolved(lighting), Resolved(sidedness)));
}
