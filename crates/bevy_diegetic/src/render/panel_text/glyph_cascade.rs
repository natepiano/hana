use bevy::prelude::*;

use crate::cascade;
use crate::cascade::CascadeDefault;
use crate::cascade::HdrTextCoverageBias;
use crate::cascade::Override;
use crate::cascade::Resolved;
use crate::cascade::TextMaterial;
use crate::layout::GlyphShadowMode;
use crate::layout::Lighting;
use crate::layout::ShadowCasting;
use crate::layout::Sidedness;
use crate::render::AntiAlias;
use crate::render::world_text::TextContent;

/// Spawn-time cascade seed for a panel label's glyph attributes.
///
/// Fires when a label first gains [`TextContent`] and seeds its
/// `Resolved<TextMaterial>` / `Resolved<Lighting>` / `Resolved<Sidedness>` /
/// `Resolved<AntiAlias>` / `Resolved<HdrTextCoverageBias>` via
/// [`resolve_walk`](cascade::resolve_walk). The walk
/// honors the label's own override first — `reconcile_panel_text_children`
/// inserts one when the label authored `TextStyle::with_material` /
/// `with_lighting` / `with_sidedness` /
/// `with_hdr_text_coverage_bias`, and `override_anti_alias` authors anti-alias
/// state — then climbs `ChildOf` to the panel's override (seeded by
/// `seed_panel_overrides` for screen panels and panel fields), else the global
/// default. `update_panel_text_batches` reads the material handle, lighting,
/// and sidedness as material-table inputs and anti-alias mode plus HDR coverage
/// bias as per-run record fields.
/// Later changes flow through the propagation pass, not this observer. The
/// glyph-render twin of `seed_panel_child_alpha`.
pub(super) fn seed_panel_text_child_glyph(
    trigger: On<Add, TextContent>,
    material_overrides: Query<&Override<TextMaterial>>,
    lighting_overrides: Query<&Override<Lighting>>,
    sidedness_overrides: Query<&Override<Sidedness>>,
    shadow_casting_overrides: Query<&Override<ShadowCasting>>,
    glyph_shadow_mode_overrides: Query<&Override<GlyphShadowMode>>,
    anti_alias_overrides: Query<&Override<AntiAlias>>,
    hdr_text_coverage_bias_overrides: Query<&Override<HdrTextCoverageBias>>,
    parents: Query<&ChildOf>,
    material_default: Res<CascadeDefault<TextMaterial>>,
    lighting_default: Res<CascadeDefault<Lighting>>,
    sidedness_default: Res<CascadeDefault<Sidedness>>,
    shadow_casting_default: Res<CascadeDefault<ShadowCasting>>,
    glyph_shadow_mode_default: Res<CascadeDefault<GlyphShadowMode>>,
    anti_alias_default: Res<CascadeDefault<AntiAlias>>,
    hdr_text_coverage_bias_default: Res<CascadeDefault<HdrTextCoverageBias>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let material = cascade::resolve_walk::<TextMaterial>(
        entity,
        &material_overrides,
        &parents,
        material_default.0.clone(),
    );
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
    let shadow_casting = cascade::resolve_walk::<ShadowCasting>(
        entity,
        &shadow_casting_overrides,
        &parents,
        shadow_casting_default.0,
    );
    let glyph_shadow_mode = cascade::resolve_walk::<GlyphShadowMode>(
        entity,
        &glyph_shadow_mode_overrides,
        &parents,
        glyph_shadow_mode_default.0,
    );
    let anti_alias = cascade::resolve_walk::<AntiAlias>(
        entity,
        &anti_alias_overrides,
        &parents,
        anti_alias_default.0,
    );
    let hdr_text_coverage_bias = cascade::resolve_walk::<HdrTextCoverageBias>(
        entity,
        &hdr_text_coverage_bias_overrides,
        &parents,
        hdr_text_coverage_bias_default.0,
    );
    commands.entity(entity).insert((
        Resolved(material),
        Resolved(lighting),
        Resolved(sidedness),
        Resolved(shadow_casting),
        Resolved(glyph_shadow_mode),
        Resolved(anti_alias),
        Resolved(hdr_text_coverage_bias),
    ));
}
