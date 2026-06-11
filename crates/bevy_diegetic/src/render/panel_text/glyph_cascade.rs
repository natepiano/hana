use bevy::prelude::*;

use crate::cascade;
use crate::cascade::CascadeDefault;
use crate::cascade::Override;
use crate::cascade::Resolved;
use crate::cascade::TextLighting;
use crate::cascade::TextSidedness;
use crate::render::TextAntiAlias;
use crate::render::world_text::TextContent;

/// Spawn-time cascade seed for a panel label's glyph lighting, sidedness, and
/// anti-alias mode.
///
/// Fires when a label first gains [`TextContent`] and seeds its
/// `Resolved<TextLighting>` / `Resolved<TextSidedness>` /
/// `Resolved<TextAntiAlias>` via [`resolve_walk`](cascade::resolve_walk). The
/// walk honors the label's own override first —
/// `reconcile_panel_text_children` inserts one when the label authored
/// `TextStyle::with_lighting` / `with_sidedness`, and `override_text_anti_alias`
/// authors the anti-alias one — then climbs `ChildOf` to the panel's override
/// (seeded by `seed_panel_overrides` for screen panels and unlit-material
/// panels), else the global default (`Lit` / `DoubleSided` / `Both`).
/// `update_panel_text_batches` reads lighting and sidedness as batch-key
/// fields and the anti-alias mode as a per-run record field. Later changes
/// flow through the propagation pass, not this observer. The glyph-render
/// twin of `seed_panel_child_alpha`.
pub(super) fn seed_panel_text_child_glyph(
    trigger: On<Add, TextContent>,
    lighting_overrides: Query<&Override<TextLighting>>,
    sidedness_overrides: Query<&Override<TextSidedness>>,
    anti_alias_overrides: Query<&Override<TextAntiAlias>>,
    parents: Query<&ChildOf>,
    lighting_default: Res<CascadeDefault<TextLighting>>,
    sidedness_default: Res<CascadeDefault<TextSidedness>>,
    anti_alias_default: Res<CascadeDefault<TextAntiAlias>>,
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
    let anti_alias = cascade::resolve_walk::<TextAntiAlias>(
        entity,
        &anti_alias_overrides,
        &parents,
        anti_alias_default.0,
    );
    commands.entity(entity).insert((
        Resolved(lighting),
        Resolved(sidedness),
        Resolved(anti_alias),
    ));
}
