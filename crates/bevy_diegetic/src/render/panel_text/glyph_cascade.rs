use bevy::prelude::*;

use crate::cascade;
use crate::cascade::CascadeDefault;
use crate::cascade::Override;
use crate::cascade::Resolved;
use crate::layout::Lighting;
use crate::layout::Sidedness;
use crate::render::AntiAlias;
use crate::render::world_text::TextContent;

/// Spawn-time cascade seed for a panel label's glyph attributes.
///
/// Fires when a label first gains [`TextContent`] and seeds its
/// `Resolved<Lighting>` / `Resolved<Sidedness>` / `Resolved<AntiAlias>` via
/// [`resolve_walk`](cascade::resolve_walk). The walk honors the label's own
/// override first — `reconcile_panel_text_children` inserts one when the label
/// authored `TextStyle::with_lighting` / `with_sidedness`, and
/// `override_anti_alias` authors anti-alias state — then climbs `ChildOf` to
/// the panel's override (seeded by `seed_panel_overrides` for screen panels
/// and unlit-material panels), else the global default (`Lit` / `DoubleSided`
/// / `Both`). `update_panel_text_batches` reads lighting and sidedness as
/// batch-key fields and anti-alias mode as a per-run record field.
/// Later changes flow through the propagation pass, not this observer. The
/// glyph-render twin of `seed_panel_child_alpha`.
pub(super) fn seed_panel_text_child_glyph(
    trigger: On<Add, TextContent>,
    lighting_overrides: Query<&Override<Lighting>>,
    sidedness_overrides: Query<&Override<Sidedness>>,
    anti_alias_overrides: Query<&Override<AntiAlias>>,
    parents: Query<&ChildOf>,
    lighting_default: Res<CascadeDefault<Lighting>>,
    sidedness_default: Res<CascadeDefault<Sidedness>>,
    anti_alias_default: Res<CascadeDefault<AntiAlias>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let lighting = cascade::resolve_walk::<Lighting>(
        entity,
        &lighting_overrides,
        &parents,
        lighting_default.0,
    );
    let sidedness = cascade::resolve_walk::<Sidedness>(
        entity,
        &sidedness_overrides,
        &parents,
        sidedness_default.0,
    );
    let anti_alias = cascade::resolve_walk::<AntiAlias>(
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
