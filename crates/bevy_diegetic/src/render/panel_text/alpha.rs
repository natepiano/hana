use bevy::prelude::*;

use crate::cascade;
use crate::cascade::CascadeDefault;
use crate::cascade::Override;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::render::world_text::PanelChild;

/// Spawn-time cascade seed for a panel label's text alpha.
///
/// A panel label (`PanelChild` + `TextContent`) is depth-2 under its panel. This
/// observer fires when a label first gains [`PanelChild`] and seeds its
/// [`Resolved<TextAlpha>`] via [`resolve_walk`](cascade::resolve_walk), which
/// `update_panel_text_geometry` reads for the glyph material (and
/// `update_panel_text_alpha` reads on a later alpha-only change). The walk honors the
/// label's own `Override<TextAlpha>` first — `reconcile_panel_text_children`
/// inserts one when the label authored its alpha
/// (`LayoutTextStyle::with_alpha_mode`), in the same bundle as [`PanelChild`]
/// so it is present here — then climbs `ChildOf` to the panel's
/// `Override<TextAlpha>`, else the global default. The standalone
/// `seed_world_text_overrides` bridge skips labels (its `Without<PanelChild>`
/// filter), so they are seeded only here. Later alpha changes flow through the
/// propagation pass, not this observer.
pub(super) fn seed_panel_child_alpha(
    trigger: On<Add, PanelChild>,
    overrides: Query<&Override<TextAlpha>>,
    parents: Query<&ChildOf>,
    default: Res<CascadeDefault<TextAlpha>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let resolved = cascade::resolve_walk::<TextAlpha>(entity, &overrides, &parents, default.0);
    commands.entity(entity).insert(Resolved(resolved));
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::LayoutBuilder;
    use crate::LayoutTextStyle;
    use crate::Mm;
    use crate::TextContent;
    use crate::cascade::CascadePlugin;
    use crate::layout::LayoutTree;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::render::panel_text::reconcile;
    use crate::text::DiegeticTextMeasurer;

    /// Panel that the label-spawning system parents its label under.
    #[derive(Resource)]
    struct TestPanel(Entity);

    /// Records the alpha the reader observed for the label on the first frame
    /// the label exists.
    #[derive(Resource, Default)]
    struct SeenLabelAlpha(Option<AlphaMode>);

    fn measurer() -> DiegeticTextMeasurer {
        DiegeticTextMeasurer {
            measure_fn: Arc::new(|_text: &str, measure: &TextMeasure| TextDimensions {
                width:       measure.size,
                height:      measure.size,
                line_height: measure.size,
            }),
        }
    }

    /// Stand-in for `reconcile_panel_text_children`: parents one label under
    /// the panel during a command flush, once the panel carries its
    /// `Override<TextAlpha>`.
    fn spawn_label_once(
        panel: Res<TestPanel>,
        ready: Query<(), (With<DiegeticPanel>, With<Override<TextAlpha>>)>,
        labels: Query<(), With<PanelChild>>,
        mut commands: Commands,
    ) {
        if !labels.is_empty() || ready.get(panel.0).is_err() {
            return;
        }
        commands
            .entity(panel.0)
            .with_child((TextContent::new("label"), PanelChild));
    }

    /// Stand-in for `update_panel_text_geometry`'s alpha read: the cached
    /// `Resolved<TextAlpha>` else the global-default fallback. Records the
    /// first observation so the assertion sees the spawn frame, not a later
    /// settled one.
    fn read_label_alpha(
        resolved_alphas: Query<&Resolved<TextAlpha>, With<PanelChild>>,
        labels: Query<Entity, With<PanelChild>>,
        default: Res<CascadeDefault<TextAlpha>>,
        mut seen: ResMut<SeenLabelAlpha>,
    ) {
        if seen.0.is_some() {
            return;
        }
        for entity in &labels {
            seen.0 = Some(
                resolved_alphas
                    .get(entity)
                    .map_or(default.0.0, |resolved| resolved.0.0),
            );
        }
    }

    #[test]
    fn label_inherits_overridden_panel_alpha_same_frame_as_spawn() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(measurer())
            .add_plugins(HeadlessLayoutPlugin)
            .add_plugins(CascadePlugin::<TextAlpha>::default())
            .add_observer(seed_panel_child_alpha)
            .init_resource::<SeenLabelAlpha>();

        // Panel override is `Add`; the global default stays `Blend`, so the
        // reader's fallback would mask an ordering regression only if the two
        // were equal — they are not.
        let panel = DiegeticPanel::world()
            .size(Mm(50.0), Mm(30.0))
            .text_alpha_mode(AlphaMode::Add)
            .layout(|b| {
                b.text("Hi", LayoutTextStyle::new(Mm(6.0)));
            })
            .build()
            .expect("test panel should build");
        let panel_entity = app.world_mut().spawn(panel).id();
        app.insert_resource(TestPanel(panel_entity));
        app.add_systems(
            Update,
            (spawn_label_once, read_label_alpha.after(spawn_label_once)),
        );

        for _ in 0..5 {
            app.update();
        }

        // The label's `Resolved<TextAlpha>` was seeded by `seed_panel_child_alpha`
        // during the same flush that spawned the label, so the reader saw the
        // panel's overridden `Add` — not the `Blend` fallback.
        assert_eq!(
            app.world().resource::<SeenLabelAlpha>().0,
            Some(AlphaMode::Add)
        );
    }

    #[test]
    fn label_resolves_to_global_default_when_panel_has_no_override() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(measurer())
            // Set the global default before `HeadlessLayoutPlugin` init-defaults it.
            .insert_resource(CascadeDefault(TextAlpha(AlphaMode::Multiply)))
            .add_plugins(HeadlessLayoutPlugin)
            .add_plugins(CascadePlugin::<TextAlpha>::default())
            .add_observer(seed_panel_child_alpha);

        // Panel authors no `text_alpha_mode`, so it carries no `Override<TextAlpha>`.
        let panel = DiegeticPanel::world()
            .size(Mm(50.0), Mm(30.0))
            .layout(|b| {
                b.text("Hi", LayoutTextStyle::new(Mm(6.0)));
            })
            .build()
            .expect("test panel should build");
        let panel_entity = app.world_mut().spawn(panel).id();

        // The label climbs `ChildOf` past the panel (no override there) to the
        // root, resolving to the global default — the designed tree-following
        // rule, not a stop-at-panel boundary.
        let label = app
            .world_mut()
            .spawn((TextContent::new("label"), PanelChild, ChildOf(panel_entity)))
            .id();
        app.update();

        assert_eq!(
            app.world()
                .get::<Resolved<TextAlpha>>(label)
                .expect("label should carry Resolved<TextAlpha>")
                .0
                .0,
            AlphaMode::Multiply
        );
    }

    #[test]
    fn label_with_own_alpha_overrides_inherited_panel_alpha() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(measurer())
            .add_plugins(HeadlessLayoutPlugin)
            .add_plugins(CascadePlugin::<TextAlpha>::default())
            .add_observer(seed_panel_child_alpha)
            .add_systems(PostUpdate, reconcile::reconcile_panel_text_children);

        // Panel sets `Add` for its labels; the one label authors its own
        // `Multiply` via `LayoutTextStyle::with_alpha_mode`. `reconcile` inserts
        // the label's `Override<TextAlpha>`, which the walk resolves ahead of
        // the panel's inherited alpha.
        let panel = DiegeticPanel::world()
            .size(Mm(50.0), Mm(30.0))
            .text_alpha_mode(AlphaMode::Add)
            .layout(|b| {
                b.text(
                    "Hi",
                    LayoutTextStyle::new(Mm(6.0)).with_alpha_mode(AlphaMode::Multiply),
                );
            })
            .build()
            .expect("test panel should build");
        app.world_mut().spawn(panel);

        for _ in 0..3 {
            app.update();
        }

        assert_eq!(single_label_alpha(&mut app), AlphaMode::Multiply);
    }

    #[test]
    fn label_alpha_change_reinherits_panel_alpha_through_reconcile() {
        fn label_tree(alpha: Option<AlphaMode>) -> LayoutTree {
            let mut style = LayoutTextStyle::new(13.0);
            if let Some(mode) = alpha {
                style = style.with_alpha_mode(mode);
            }
            let mut builder = LayoutBuilder::new(100.0, 50.0);
            builder.text("Hi", style);
            builder.build()
        }

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(measurer())
            .add_plugins(HeadlessLayoutPlugin)
            .add_plugins(CascadePlugin::<TextAlpha>::default())
            .add_observer(seed_panel_child_alpha)
            .add_systems(PostUpdate, reconcile::reconcile_panel_text_children);

        // Panel inherits `Add`; the label initially authors its own `Multiply`.
        let panel = DiegeticPanel::world()
            .size(Mm(50.0), Mm(30.0))
            .text_alpha_mode(AlphaMode::Add)
            .with_tree(label_tree(Some(AlphaMode::Multiply)))
            .build()
            .expect("test panel should build");
        let panel_entity = app.world_mut().spawn(panel).id();

        for _ in 0..3 {
            app.update();
        }
        assert_eq!(single_label_alpha(&mut app), AlphaMode::Multiply);

        // The label drops its own alpha. `reconcile` removes the label's
        // `Override<TextAlpha>` (its update arm), and the propagation pass
        // re-inherits the panel's `Add`.
        app.world_mut()
            .commands()
            .set_tree(panel_entity, label_tree(None));
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(single_label_alpha(&mut app), AlphaMode::Add);
    }

    /// Resolved alpha of the scene's single panel label.
    fn single_label_alpha(app: &mut App) -> AlphaMode {
        let mut query = app
            .world_mut()
            .query_filtered::<&Resolved<TextAlpha>, With<PanelChild>>();
        let resolved: Vec<AlphaMode> = query.iter(app.world()).map(|r| r.0.0).collect();
        assert_eq!(resolved.len(), 1, "expected exactly one panel label");
        resolved[0]
    }
}
