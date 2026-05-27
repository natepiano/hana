//! Standalone world-space text component and rendering system.

mod mesh_spawning;
mod readiness;
mod rendering;
mod shaping;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use mesh_spawning::WorldTextMesh;
pub(super) use mesh_spawning::free_run_storage_on_world_mesh_removal;
pub(super) use mesh_spawning::update_world_text_alpha;
pub(super) use readiness::AwaitingReady;
pub use readiness::WorldTextReady;
pub(super) use readiness::emit_world_text_ready;

use super::text_shaping::TextShapingContext;
use crate::cascade::CascadeDefault;
use crate::cascade::FontUnit;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::layout::ShapedTextCache;
use crate::layout::WorldTextStyle;
use crate::text::FontRegistry;
use crate::text::GlyphCache;
use crate::text::TextMaterial;

pub(super) fn render_world_text(
    changed_texts: Query<
        Entity,
        (
            With<WorldText>,
            Without<PanelChild>,
            Or<(
                Changed<WorldText>,
                Changed<WorldTextStyle>,
                Changed<Resolved<FontUnit>>,
            )>,
        ),
    >,
    texts: Query<(&WorldText, &WorldTextStyle), Without<PanelChild>>,
    resolved_alphas: Query<&Resolved<TextAlpha>, Without<PanelChild>>,
    resolved_units: Query<&Resolved<FontUnit>, Without<PanelChild>>,
    old_meshes: Query<(Entity, &ChildOf), With<WorldTextMesh>>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    cache: ResMut<ShapedTextCache>,
    meshes: ResMut<Assets<Mesh>>,
    backend_services: BackendRenderServices,
    font_default: Res<CascadeDefault<FontUnit>>,
    alpha_default: Res<CascadeDefault<TextAlpha>>,
    commands: Commands,
) {
    rendering::render_world_text(
        changed_texts,
        texts,
        resolved_alphas,
        resolved_units,
        old_meshes,
        font_registry,
        shaping_cx,
        cache,
        meshes,
        backend_services,
        font_default,
        alpha_default,
        commands,
    );
}

#[derive(SystemParam)]
pub(super) struct BackendRenderServices<'w> {
    backend:         ResMut<'w, GlyphCache>,
    materials:       ResMut<'w, Assets<TextMaterial>>,
    storage_buffers: ResMut<'w, Assets<ShaderBuffer>>,
}

/// Computed layout data for a [`WorldText`] entity, populated by the
/// renderer. Used by the typography debug overlay to draw glyph bounding
/// boxes and metric lines aligned with the rendered text.
///
/// Only available when the `typography_overlay` feature is enabled.
#[cfg(feature = "typography_overlay")]
#[derive(Component, Clone, Debug)]
pub struct ComputedWorldText {
    /// `Anchor` offset Y in layout units (matches the renderer's anchor).
    pub anchor_y: f32,
    /// Per-visible-glyph metrics aligned with the rendered text.
    pub glyphs:   Vec<ComputedGlyphMetrics>,
}

/// Overlay-only metrics for one visible glyph in a shaped [`WorldText`] run.
#[cfg(feature = "typography_overlay")]
#[derive(Clone, Debug)]
pub struct ComputedGlyphMetrics {
    /// Ink bounding box `[x, y, width, height]` in world units.
    pub rect:      [f32; 4],
    /// Glyph origin X for overlay callouts, in world units.
    ///
    /// Usually this is the shaped glyph origin. When a substituted glyph draws
    /// before that origin, as `JetBrains` Mono coding alternates do, this shifts
    /// left to the visible overhang so the origin/advance callout tracks the
    /// displayed glyph cell.
    pub origin_x:  f32,
    /// Glyph baseline origin Y in world units.
    pub origin_y:  f32,
    /// Shaped horizontal advance in world units.
    pub advance_x: f32,
}

/// Standalone text rendered in world space.
///
/// Attach to any entity with a [`Transform`] to place text in the 3D scene.
/// Style is controlled by the required [`TextStyle`] component (added
/// automatically with defaults if not specified).
///
/// ```ignore
/// commands.spawn((
///     WorldText::new("Hello, world!"),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
///
/// // With custom style:
/// commands.spawn((
///     WorldText::new("Styled"),
///     WorldTextStyle::new(24.0).with_color(Color::RED),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
/// ```
#[derive(Component, Clone, Debug, Reflect)]
#[require(WorldTextStyle, Transform, Visibility)]
pub struct WorldText {
    text: String,
}

impl WorldText {
    /// Creates a new world text with the given string.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self { Self { text: text.into() } }

    /// Text contents.
    #[must_use]
    pub fn text(&self) -> &str { &self.text }

    /// Mutates the text contents.
    pub fn set_text(&mut self, text: impl Into<String>) { self.text = text.into(); }
}

/// Marker on a [`WorldText`] entity spawned as a child of a
/// [`DiegeticPanel`](crate::DiegeticPanel).
///
/// Standalone-text systems filter `Without<PanelChild>` to skip panel labels
/// (the panel-text systems render those); panel-text systems filter
/// `With<PanelChild>`. The layout payload lives in
/// [`PanelTextLayout`](crate::render::panel_text::PanelTextLayout).
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PanelChild;

/// Spawn-time bridge for standalone world-text cascade values.
///
/// A newly-added [`WorldTextStyle`] is no longer a cascade authoring source, so
/// the bridge only seeds [`Resolved<FontUnit>`] and [`Resolved<TextAlpha>`] to
/// their global defaults. Non-default per-entity values come from the typed
/// cascade commands (`override_font_unit`, `override_text_alpha`), which insert
/// overrides and self-heal the directly targeted entity on command flush.
///
/// Filters `Without<PanelChild>`: panel labels share the [`WorldTextStyle`]
/// component but their alpha is seeded by the panel-label path (and they never
/// read a font unit of their own), so they must not be seeded as standalones.
/// Labels are seeded by `seed_panel_child_alpha` instead.
pub(super) fn seed_world_text_overrides(
    trigger: On<Add, WorldTextStyle>,
    styles: Query<(), (With<WorldTextStyle>, Without<PanelChild>)>,
    font_default: Res<CascadeDefault<FontUnit>>,
    alpha_default: Res<CascadeDefault<TextAlpha>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if styles.get(entity).is_err() {
        return;
    }
    commands
        .entity(entity)
        .insert((Resolved(font_default.0), Resolved(alpha_default.0)));
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use super::*;
    use crate::cascade::CascadeDefault;
    use crate::cascade::CascadeDefaults;
    use crate::cascade::CascadeEntityCommandsExt;
    use crate::cascade::CascadePlugin;
    use crate::cascade::CascadeSet;
    use crate::cascade::Override;
    use crate::layout::Unit;
    use crate::resolved_text_alpha;

    fn standalone_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CascadeDefaults>()
            .add_plugins(CascadePlugin::<TextAlpha>::default())
            .add_plugins(CascadePlugin::<FontUnit>::default())
            .add_observer(seed_world_text_overrides);
        app
    }

    fn resolved_alpha(app: &App, entity: Entity) -> AlphaMode {
        app.world()
            .get::<Resolved<TextAlpha>>(entity)
            .expect("standalone should carry Resolved<TextAlpha>")
            .0
            .0
    }

    fn resolved_unit(app: &App, entity: Entity) -> Unit {
        app.world()
            .get::<Resolved<FontUnit>>(entity)
            .expect("standalone should carry Resolved<FontUnit>")
            .0
            .0
    }

    #[derive(Resource)]
    struct Target(Entity);

    #[derive(Resource, Default)]
    struct SeenAlpha(Option<AlphaMode>);

    fn override_target_before_propagate(target: Res<Target>, mut commands: Commands) {
        commands
            .entity(target.0)
            .override_text_alpha(AlphaMode::Add);
    }

    fn override_target_after_propagate(target: Res<Target>, mut commands: Commands) {
        commands
            .entity(target.0)
            .override_text_alpha(AlphaMode::Add);
    }

    fn read_target_alpha_after_propagate(world: &mut World) {
        let entity = world.resource::<Target>().0;
        let alpha = resolved_text_alpha(world, entity);
        world.resource_mut::<SeenAlpha>().0 = Some(alpha);
    }

    #[test]
    fn no_override_standalone_seeds_resolved_to_global_defaults() {
        let mut app = standalone_app();
        // Default `WorldTextStyle` authors neither unit nor alpha, so the
        // standalone carries no `Override<A>` and resolves to the globals.
        let entity = app.world_mut().spawn(WorldText::new("hi")).id();
        app.update();

        assert_eq!(resolved_unit(&app, entity), Unit::Meters);
        assert_eq!(resolved_alpha(&app, entity), AlphaMode::Blend);
        assert!(app.world().get::<Override<FontUnit>>(entity).is_none());
        assert!(app.world().get::<Override<TextAlpha>>(entity).is_none());
    }

    #[test]
    fn font_unit_override_command_updates_standalone() {
        let mut app = standalone_app();
        let entity = app
            .world_mut()
            .commands()
            .spawn((WorldText::new("hi"), WorldTextStyle::new(11.0)))
            .override_font_unit(Unit::Points)
            .id();
        app.world_mut().flush();

        assert_eq!(resolved_unit(&app, entity), Unit::Points);
        let node_override = app
            .world()
            .get::<Override<FontUnit>>(entity)
            .expect("explicit unit override should carry Override<FontUnit>");
        assert_eq!(node_override.0.0, Unit::Points);
    }

    #[test]
    fn text_alpha_override_command_updates_standalone() {
        let mut app = standalone_app();
        let entity = app
            .world_mut()
            .commands()
            .spawn((WorldText::new("hi"), WorldTextStyle::new(0.22)))
            .override_text_alpha(AlphaMode::Add)
            .id();
        app.world_mut().flush();

        assert_eq!(resolved_alpha(&app, entity), AlphaMode::Add);
        let node_override = app
            .world()
            .get::<Override<TextAlpha>>(entity)
            .expect("explicit alpha override should carry Override<TextAlpha>");
        assert_eq!(node_override.0.0, AlphaMode::Add);
    }

    #[test]
    fn no_override_standalone_alpha_follows_runtime_default_change() {
        let mut app = standalone_app();
        // No `Override<TextAlpha>` exists for standalone text (alpha has no
        // authoring path), so its resolved alpha is purely the global default —
        // the path the now-deleted render-side alpha re-resolve used to cover,
        // now driven by the propagation pass's `default_changed` branch.
        let entity = app.world_mut().spawn(WorldText::new("hi")).id();
        app.update();
        assert_eq!(resolved_alpha(&app, entity), AlphaMode::Blend);

        app.world_mut()
            .resource_mut::<CascadeDefault<TextAlpha>>()
            .0 = TextAlpha(AlphaMode::Add);
        app.update();
        assert_eq!(resolved_alpha(&app, entity), AlphaMode::Add);
    }

    #[test]
    fn override_and_inherit_text_alpha_commands_update_standalone() {
        let mut app = standalone_app();
        let entity = app.world_mut().spawn(WorldText::new("hi")).id();
        app.update();
        assert_eq!(resolved_text_alpha(app.world(), entity), AlphaMode::Blend);

        app.world_mut()
            .commands()
            .entity(entity)
            .override_text_alpha(AlphaMode::Add);
        app.update();
        assert_eq!(resolved_text_alpha(app.world(), entity), AlphaMode::Add);

        app.world_mut()
            .commands()
            .entity(entity)
            .inherit_text_alpha();
        app.update();
        assert_eq!(resolved_text_alpha(app.world(), entity), AlphaMode::Blend);
    }

    #[test]
    fn spawn_frame_override_self_heals_resolved_alpha() {
        let mut app = standalone_app();
        let entity = app
            .world_mut()
            .commands()
            .spawn(WorldText::new("hi"))
            .override_text_alpha(AlphaMode::Add)
            .id();
        app.world_mut().flush();

        assert_eq!(resolved_text_alpha(app.world(), entity), AlphaMode::Add);
    }

    #[test]
    fn parent_override_before_propagate_updates_existing_child_same_frame() {
        let mut app = standalone_app();
        let parent = app.world_mut().spawn_empty().id();
        let child = app
            .world_mut()
            .spawn((WorldText::new("hi"), ChildOf(parent)))
            .id();
        app.update();
        assert_eq!(resolved_text_alpha(app.world(), child), AlphaMode::Blend);

        app.insert_resource(Target(parent));
        app.init_resource::<SeenAlpha>();
        app.add_systems(
            Update,
            (
                override_target_before_propagate.before(CascadeSet::Propagate),
                read_target_alpha_after_propagate.after(CascadeSet::Propagate),
            ),
        );
        app.update();

        assert_eq!(resolved_text_alpha(app.world(), child), AlphaMode::Add);
        assert_eq!(app.world().resource::<SeenAlpha>().0, Some(AlphaMode::Add));
    }

    #[test]
    fn parent_override_after_propagate_updates_existing_child_next_frame() {
        let mut app = standalone_app();
        let parent = app.world_mut().spawn_empty().id();
        let child = app
            .world_mut()
            .spawn((WorldText::new("hi"), ChildOf(parent)))
            .id();
        app.update();
        assert_eq!(resolved_text_alpha(app.world(), child), AlphaMode::Blend);

        app.insert_resource(Target(parent));
        app.add_systems(
            Update,
            override_target_after_propagate.after(CascadeSet::Propagate),
        );
        app.update();
        assert_eq!(resolved_text_alpha(app.world(), child), AlphaMode::Blend);

        app.update();
        assert_eq!(resolved_text_alpha(app.world(), child), AlphaMode::Add);
    }
}
