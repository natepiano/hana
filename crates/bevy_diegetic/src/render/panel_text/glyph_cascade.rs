use bevy::prelude::*;

use crate::cascade;
use crate::cascade::CascadeDefault;
use crate::cascade::DrawLayer;
use crate::cascade::Override;
use crate::cascade::Resolved;
use crate::cascade::TextLighting;
use crate::cascade::TextSidedness;
use crate::render::AntiAlias;
use crate::render::world_text::TextContent;

/// Spawn-time cascade seed for a panel label's glyph attributes.
///
/// Fires when a label first gains [`TextContent`] and seeds its
/// `Resolved<TextLighting>` / `Resolved<TextSidedness>` /
/// `Resolved<DrawLayer>` / `Resolved<AntiAlias>` via
/// [`resolve_walk`](cascade::resolve_walk). The walk honors the label's own
/// override first — `reconcile_panel_text_children` inserts one when the label
/// authored `TextStyle::with_lighting` / `with_sidedness` /
/// `with_draw_layer`, and `override_text_anti_alias` authors anti-alias state
/// — then climbs `ChildOf` to the panel's override (seeded by
/// `seed_panel_overrides` for screen panels and unlit-material panels), else
/// the global default (`Lit` / `DoubleSided` / the default draw layer /
/// `Both`). `update_panel_text_batches` reads lighting, sidedness, and draw
/// layer as batch-key fields and anti-alias mode as a per-run record field.
/// Later changes flow through the propagation pass, not this observer. The
/// glyph-render twin of `seed_panel_child_alpha`.
pub(super) fn seed_panel_text_child_glyph(
    trigger: On<Add, TextContent>,
    lighting_overrides: Query<&Override<TextLighting>>,
    sidedness_overrides: Query<&Override<TextSidedness>>,
    draw_layer_overrides: Query<&Override<DrawLayer>>,
    anti_alias_overrides: Query<&Override<AntiAlias>>,
    parents: Query<&ChildOf>,
    lighting_default: Res<CascadeDefault<TextLighting>>,
    sidedness_default: Res<CascadeDefault<TextSidedness>>,
    draw_layer_default: Res<CascadeDefault<DrawLayer>>,
    anti_alias_default: Res<CascadeDefault<AntiAlias>>,
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
    let draw_layer = cascade::resolve_walk::<DrawLayer>(
        entity,
        &draw_layer_overrides,
        &parents,
        draw_layer_default.0,
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
        Resolved(draw_layer),
        Resolved(anti_alias),
    ));
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::Mm;
    use crate::TextStyle;
    use crate::cascade::CascadeEntityCommandsExt;
    use crate::cascade::CascadePlugin;
    use crate::cascade::DEFAULT_DRAW_LAYER;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::render::panel_text::reconcile;
    use crate::text::DiegeticTextMeasurer;

    fn measurer() -> DiegeticTextMeasurer {
        DiegeticTextMeasurer {
            measure_fn: Arc::new(|_text: &str, measure: &TextMeasure| TextDimensions {
                width:       measure.size,
                height:      measure.size,
                line_height: measure.size,
            }),
        }
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(measurer())
            .add_plugins(HeadlessLayoutPlugin)
            .add_plugins(CascadePlugin::<TextLighting>::default())
            .add_plugins(CascadePlugin::<TextSidedness>::default())
            .add_plugins(CascadePlugin::<DrawLayer>::default())
            .add_observer(seed_panel_text_child_glyph)
            .add_systems(PostUpdate, reconcile::reconcile_panel_text_children);
        app
    }

    /// One-label tree, optionally authoring a draw layer on the label.
    fn label_tree(draw_layer: Option<DrawLayer>) -> LayoutTree {
        let mut style = TextStyle::new(13.0);
        if let Some(layer) = draw_layer {
            style = style.with_draw_layer(layer);
        }
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text("Hi", style);
        builder.build()
    }

    /// Resolved draw layer of the scene's single panel label.
    fn single_label_draw_layer(app: &mut App) -> DrawLayer {
        let mut query = app
            .world_mut()
            .query_filtered::<&Resolved<DrawLayer>, With<TextContent>>();
        let resolved: Vec<DrawLayer> = query.iter(app.world()).map(|r| r.0).collect();
        assert_eq!(resolved.len(), 1, "expected exactly one panel label");
        resolved[0]
    }

    #[test]
    fn label_without_draw_layer_resolves_to_global_default() {
        let mut app = test_app();
        let panel = DiegeticPanel::world()
            .size(Mm(50.0), Mm(30.0))
            .with_tree(label_tree(None))
            .build()
            .expect("test panel should build");
        app.world_mut().spawn(panel);

        for _ in 0..3 {
            app.update();
        }

        assert_eq!(
            single_label_draw_layer(&mut app),
            DrawLayer(DEFAULT_DRAW_LAYER)
        );
    }

    #[test]
    fn with_draw_layer_lands_the_override_on_the_label_entity() {
        let mut app = test_app();
        let panel = DiegeticPanel::world()
            .size(Mm(50.0), Mm(30.0))
            .with_tree(label_tree(Some(DrawLayer(10))))
            .build()
            .expect("test panel should build");
        app.world_mut().spawn(panel);

        for _ in 0..3 {
            app.update();
        }

        // `reconcile` captured the style's draw layer and inserted the
        // label's `Override<DrawLayer>`; the walk resolves it ahead of
        // the global default.
        let mut overrides = app
            .world_mut()
            .query_filtered::<&Override<DrawLayer>, With<TextContent>>();
        let authored: Vec<DrawLayer> = overrides.iter(app.world()).map(|o| o.0).collect();
        assert_eq!(authored, vec![DrawLayer(10)]);
        assert_eq!(single_label_draw_layer(&mut app), DrawLayer(10));
    }

    #[test]
    fn label_draw_layer_change_reinherits_default_through_reconcile() {
        let mut app = test_app();
        let panel = DiegeticPanel::world()
            .size(Mm(50.0), Mm(30.0))
            .with_tree(label_tree(Some(DrawLayer(10))))
            .build()
            .expect("test panel should build");
        let panel_entity = app.world_mut().spawn(panel).id();

        for _ in 0..3 {
            app.update();
        }
        assert_eq!(single_label_draw_layer(&mut app), DrawLayer(10));

        // The label drops its own draw layer. `reconcile` removes the label's
        // `Override<DrawLayer>` (its update arm), and the propagation pass
        // re-inherits the global default.
        app.world_mut()
            .commands()
            .set_tree(panel_entity, label_tree(None));
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            single_label_draw_layer(&mut app),
            DrawLayer(DEFAULT_DRAW_LAYER)
        );
    }

    #[test]
    fn override_and_inherit_entity_commands_round_trip() {
        let mut app = test_app();
        let entity = app.world_mut().spawn_empty().id();

        app.world_mut()
            .commands()
            .entity(entity)
            .override_draw_layer(DrawLayer(-3));
        app.update();
        assert_eq!(
            app.world()
                .get::<Resolved<DrawLayer>>(entity)
                .expect("override self-heals Resolved<DrawLayer>")
                .0,
            DrawLayer(-3)
        );

        app.world_mut()
            .commands()
            .entity(entity)
            .inherit_draw_layer();
        app.update();
        assert_eq!(
            app.world()
                .get::<Resolved<DrawLayer>>(entity)
                .expect("inherit re-heals Resolved<DrawLayer>")
                .0,
            DrawLayer(DEFAULT_DRAW_LAYER)
        );
    }
}
