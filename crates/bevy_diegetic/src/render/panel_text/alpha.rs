use bevy::prelude::*;

use crate::cascade;
use crate::cascade::CascadeDefaults;
use crate::cascade::Override;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::render::world_text::PanelChild;

/// Spawn-time cascade seed for a panel label's text alpha.
///
/// A panel label (`PanelChild` + `WorldText`) is depth-2: it inherits its
/// alpha from the panel it is a `ChildOf`. This observer fires when a label
/// first gains [`PanelChild`] and seeds its [`Resolved<TextAlpha>`] by walking
/// up to the panel's `Override<TextAlpha>` (else the global default), which
/// `build_panel_text_meshes` reads for the glyph material. A label carries no
/// `Override<TextAlpha>` of its own — there is no per-label alpha authoring
/// path — so it always inherits; the standalone `seed_world_text_overrides`
/// bridge skips labels for exactly this reason. Later panel-alpha changes flow
/// to the label through the propagation pass, not this observer.
pub(super) fn seed_panel_child_alpha(
    trigger: On<Add, PanelChild>,
    overrides: Query<&Override<TextAlpha>>,
    parents: Query<&ChildOf>,
    defaults: Res<CascadeDefaults>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let resolved = cascade::resolve_walk::<TextAlpha>(entity, &overrides, &parents, &defaults);
    commands.entity(entity).insert(Resolved(resolved));
}
