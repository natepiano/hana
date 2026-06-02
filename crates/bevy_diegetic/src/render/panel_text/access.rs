//! Public get/set access to a panel's text runs, addressed by
//! [`PanelFieldId`].
//!
//! [`PanelText`] is the read-write `SystemParam`; [`PanelTextReader`] is the
//! read-only variant a reader system uses so it does not serialize on the
//! `&mut TextContent` claim. Both resolve a run through the panel's
//! `id → Entity` index ([`DiegeticPanel::text_child`]) and validate liveness via
//! their `PanelTextLayout` query, so a despawned run reads back as `None` rather
//! than a dangling `Entity` (TR-Q).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::PanelTextLayout;
use crate::PanelFieldId;
use crate::panel::DiegeticPanel;
use crate::render::world_text::TextContent;

/// Read-only access to panel text runs by [`PanelFieldId`].
///
/// Reads come from the `El.text` layout cache, which is authoritative for the
/// full run string — a wrapped run materializes as one child per visual line,
/// each holding a per-line slice, so no single child entity owns the whole
/// string. Use this in reader systems; it holds no mutable claim and so runs in
/// parallel with other text readers.
#[derive(SystemParam)]
pub struct PanelTextReader<'w, 's> {
    panels:  Query<'w, 's, (&'static DiegeticPanel, Option<&'static Children>)>,
    layouts: Query<'w, 's, &'static PanelTextLayout>,
}

impl PanelTextReader<'_, '_> {
    /// Resolves a named run to its `line_index == 0` entity, or `None` if no run
    /// carries the id or the mapped entity is no longer a live run.
    #[must_use]
    pub fn entity(&self, panel: Entity, id: &PanelFieldId) -> Option<Entity> {
        let (data, _) = self.panels.get(panel).ok()?;
        self.resolve(data, id)
    }

    /// Reads a named run's full string from the `El.text` cache, never a line
    /// slice, so a wrapped run returns its whole text.
    #[must_use]
    pub fn text(&self, panel: Entity, id: &PanelFieldId) -> Option<&str> {
        let (data, _) = self.panels.get(panel).ok()?;
        let child = self.resolve(data, id)?;
        let layout = self.layouts.get(child).ok()?;
        data.tree().element_text(layout.element_idx)
    }

    /// Resolves a run id to a live entity, the shared chokepoint for `entity` /
    /// `text` / `set_text`.
    ///
    /// `text_child` is an unchecked index read; a despawned child fails the
    /// layout query and resolves to `None` (TR-Q liveness). On a miss, the tree —
    /// authoritative for valid ids at build time, unlike the reconcile-timed
    /// index — discriminates a genuine typo from a run not yet materialized: a
    /// `#[cfg(debug_assertions)]` `warn!` fires only when the id is absent from
    /// the tree, so the first-frame / post-`set_tree` window stays quiet.
    fn resolve(&self, data: &DiegeticPanel, id: &PanelFieldId) -> Option<Entity> {
        // `text_child` is an unchecked index read; the layout query confirms the
        // entity is still a live run (TR-Q). A stale index entry (entity despawned
        // out of flow before reconcile rebuilt the index) falls to the miss path
        // below, but its id is still valid in the tree, so it stays silent.
        if let Some(child) = data.text_child(id)
            && self.layouts.contains(child)
        {
            return Some(child);
        }
        #[cfg(debug_assertions)]
        if !data.tree().contains_text_id(id) {
            warn!("no text run with id {id}");
        }
        None
    }

    /// Reads the lone run of a one-element panel (the runtime form of a
    /// [`DiegeticText`](crate::DiegeticText)), no id needed. Returns `None` if the
    /// panel has zero or more than one run.
    #[must_use]
    pub fn sole_text(&self, panel: Entity) -> Option<&str> {
        let child = self.sole_run_entity(panel)?;
        let (data, _) = self.panels.get(panel).ok()?;
        let layout = self.layouts.get(child).ok()?;
        data.tree().element_text(layout.element_idx)
    }

    /// Resolves a one-element panel's lone `line_index == 0` run entity. The
    /// run's `Auto` id is not caller-addressable, so the lone run is found by
    /// walking the panel's children rather than the id index. Returns `None` when
    /// the panel has no run or more than one (the call is ambiguous then).
    fn sole_run_entity(&self, panel: Entity) -> Option<Entity> {
        let (_, children) = self.panels.get(panel).ok()?;
        let children = children?;
        let mut found = None;
        for child in children {
            let Ok(layout) = self.layouts.get(*child) else {
                continue;
            };
            if layout.line_index == 0 {
                if found.is_some() {
                    return None;
                }
                found = Some(*child);
            }
        }
        found
    }
}

/// Read-write access to panel text runs by [`PanelFieldId`].
///
/// `set_text` writes the run's `line_index == 0` child `TextContent`; the
/// `Changed<TextContent>` reactor then syncs it into the `El.text` cache and
/// relayout re-wraps it, so passing the whole string works for both single-line
/// and wrapped runs. Reads delegate to the embedded [`PanelTextReader`] and so
/// come from the `El.text` cache.
///
/// The `&mut TextContent` claim serializes against any other `TextContent`
/// accessor: one system should own the `PanelText` writes per frame; reader
/// systems take [`PanelTextReader`] instead.
#[derive(SystemParam)]
pub struct PanelText<'w, 's> {
    reader:  PanelTextReader<'w, 's>,
    content: Query<'w, 's, &'static mut TextContent, With<PanelTextLayout>>,
}

impl PanelText<'_, '_> {
    /// Resolves a named run to its `line_index == 0` entity. See
    /// [`PanelTextReader::entity`].
    #[must_use]
    pub fn entity(&self, panel: Entity, id: &PanelFieldId) -> Option<Entity> {
        self.reader.entity(panel, id)
    }

    /// Reads a named run's full string. See [`PanelTextReader::text`].
    #[must_use]
    pub fn text(&self, panel: Entity, id: &PanelFieldId) -> Option<&str> {
        self.reader.text(panel, id)
    }

    /// Reads the lone run of a one-element panel. See
    /// [`PanelTextReader::sole_text`].
    #[must_use]
    pub fn sole_text(&self, panel: Entity) -> Option<&str> { self.reader.sole_text(panel) }

    /// Sets a named run's text, returning whether a run was found and written.
    ///
    /// Writes the `line_index == 0` child; the reactor re-wraps from the new
    /// string, so a wrapped run is retext by passing the whole replacement.
    pub fn set_text(&mut self, panel: Entity, id: &PanelFieldId, text: impl Into<String>) -> bool {
        let Some(child) = self.reader.entity(panel, id) else {
            return false;
        };
        let Ok(mut content) = self.content.get_mut(child) else {
            return false;
        };
        content.set_text(text);
        true
    }

    /// Sets the lone run of a one-element panel (a
    /// [`DiegeticText`](crate::DiegeticText)), no id needed. Returns whether a
    /// single run was found and written.
    pub fn set_sole_text(&mut self, panel: Entity, text: impl Into<String>) -> bool {
        let Some(child) = self.reader.sole_run_entity(panel) else {
            return false;
        };
        let Ok(mut content) = self.content.get_mut(child) else {
            return false;
        };
        content.set_text(text);
        true
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::sync::Arc;

    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use bevy_kana::ToF32;

    use super::PanelText;
    use super::PanelTextReader;
    use crate::Mm;
    use crate::PanelFieldId;
    use crate::PanelSystems;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::TextStyle;
    use crate::panel::ComputedDiegeticPanel;
    use crate::panel::DiegeticPanel;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::render::panel_text::PanelTextLayout;
    use crate::render::panel_text::reconcile;
    use crate::text::DiegeticTextMeasurer;

    fn monospace_measurer() -> DiegeticTextMeasurer {
        DiegeticTextMeasurer {
            measure_fn: Arc::new(|text: &str, measure: &TextMeasure| {
                let char_width = measure.size * MONOSPACE_WIDTH_RATIO;
                let width = text
                    .lines()
                    .map(|line| line.chars().count().to_f32() * char_width)
                    .fold(0.0_f32, f32::max);
                let line_count = text.lines().count().max(1).to_f32();
                TextDimensions {
                    width,
                    height: measure.size * line_count,
                    line_height: measure.size,
                }
            }),
        }
    }

    /// Headless layout plus the Phase 2 cache sync + marker clear (`Update`) and
    /// the reconcile pass (`PostUpdate`) in their real ordering, so the public
    /// `PanelText` get/set is exercised against a live index and the relayout
    /// reactor.
    fn access_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(monospace_measurer());
        app.add_plugins(HeadlessLayoutPlugin);
        app.add_systems(
            Update,
            (
                reconcile::sync_run_text_to_cache.before(PanelSystems::ApplyTreeChanges),
                reconcile::clear_reconcile_owned.after(reconcile::sync_run_text_to_cache),
            ),
        );
        app.add_systems(PostUpdate, reconcile::reconcile_panel_text_children);
        app
    }

    fn auto_tree(text: &str) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(text, TextStyle::new(10.0));
        builder.build()
    }

    fn named_tree(id: &PanelFieldId, text: &str) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text_id(id.clone(), text, TextStyle::new(10.0));
        builder.build()
    }

    fn spawn_panel(app: &mut App, tree: LayoutTree) -> Entity {
        app.world_mut()
            .spawn(
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .with_tree(tree)
                    .build()
                    .expect("panel should build"),
            )
            .id()
    }

    /// Spawns and runs two frames: frame 1 materializes the run (tagged
    /// `ReconcileOwned`), frame 2 clears the marker so a later edit reads as
    /// out-of-flow.
    fn settled_panel(app: &mut App, tree: LayoutTree) -> Entity {
        let panel = spawn_panel(app, tree);
        app.update();
        app.update();
        panel
    }

    fn content_width(app: &App, panel: Entity) -> f32 {
        app.world()
            .get::<ComputedDiegeticPanel>(panel)
            .expect("computed panel should exist")
            .content_bounds()
            .expect("content bounds should exist")
            .width
    }

    fn first_run_entity(app: &mut App) -> Entity {
        let mut state = app
            .world_mut()
            .query_filtered::<Entity, With<PanelTextLayout>>();
        let runs: Vec<Entity> = state.iter(app.world()).collect();
        assert_eq!(runs.len(), 1, "expected exactly one run child");
        runs[0]
    }

    #[test]
    fn reader_resolves_a_named_run_and_reads_its_text() {
        let mut app = access_app();
        let id = PanelFieldId::named("title");
        let panel = settled_panel(&mut app, named_tree(&id, "Hi"));

        let lookup = id.clone();
        let resolved = app
            .world_mut()
            .run_system_once(move |reader: PanelTextReader| reader.entity(panel, &lookup))
            .expect("system runs");
        assert!(
            resolved.is_some(),
            "a named run should resolve to an entity"
        );

        let lookup = id;
        let text = app
            .world_mut()
            .run_system_once(move |reader: PanelTextReader| {
                reader.text(panel, &lookup).map(str::to_owned)
            })
            .expect("system runs");
        assert_eq!(text.as_deref(), Some("Hi"));
    }

    #[test]
    fn unknown_id_resolves_to_none() {
        let mut app = access_app();
        let id = PanelFieldId::named("title");
        let panel = settled_panel(&mut app, named_tree(&id, "Hi"));

        let resolved = app
            .world_mut()
            .run_system_once(move |reader: PanelTextReader| {
                reader.entity(panel, &PanelFieldId::named("missing"))
            })
            .expect("system runs");
        assert!(resolved.is_none(), "an unknown id must not resolve");
    }

    #[test]
    fn auto_id_run_is_not_addressable_but_sole_text_reads_it() {
        let mut app = access_app();
        let panel = settled_panel(&mut app, auto_tree("Hi"));

        // No public `PanelFieldId` names an auto run, so a named lookup misses.
        let by_name = app
            .world_mut()
            .run_system_once(move |reader: PanelTextReader| {
                reader.entity(panel, &PanelFieldId::named("Hi"))
            })
            .expect("system runs");
        assert!(by_name.is_none(), "an auto-id run is not name-addressable");

        // The one-element `sole_text` path reaches it without an id.
        let sole = app
            .world_mut()
            .run_system_once(move |reader: PanelTextReader| {
                reader.sole_text(panel).map(str::to_owned)
            })
            .expect("system runs");
        assert_eq!(sole.as_deref(), Some("Hi"));
    }

    #[test]
    fn set_text_through_panel_text_relayouts() {
        let mut app = access_app();
        let id = PanelFieldId::named("title");
        let panel = settled_panel(&mut app, named_tree(&id, "Hi"));
        let before = content_width(&app, panel);

        let target = id.clone();
        let wrote = app
            .world_mut()
            .run_system_once(move |mut text: PanelText| {
                text.set_text(panel, &target, "Hello World")
            })
            .expect("system runs");
        assert!(wrote, "a named run should accept the write");
        app.update();

        let read = id;
        let after_text = app
            .world_mut()
            .run_system_once(move |reader: PanelTextReader| {
                reader.text(panel, &read).map(str::to_owned)
            })
            .expect("system runs");
        assert_eq!(after_text.as_deref(), Some("Hello World"));

        let after = content_width(&app, panel);
        assert!(
            after > before,
            "the wider string should relayout: width {before} -> {after}",
        );
    }

    #[test]
    fn set_text_unknown_id_is_a_noop() {
        let mut app = access_app();
        let id = PanelFieldId::named("title");
        let panel = settled_panel(&mut app, named_tree(&id, "Hi"));

        let wrote = app
            .world_mut()
            .run_system_once(move |mut text: PanelText| {
                text.set_text(panel, &PanelFieldId::named("missing"), "ignored")
            })
            .expect("system runs");
        assert!(!wrote, "an unknown id must not write");

        let read = id;
        let still = app
            .world_mut()
            .run_system_once(move |reader: PanelTextReader| {
                reader.text(panel, &read).map(str::to_owned)
            })
            .expect("system runs");
        assert_eq!(still.as_deref(), Some("Hi"), "the run text is unchanged");
    }

    #[test]
    fn orphaned_run_resolves_to_none_through_the_system_param() {
        let mut app = access_app();
        let id = PanelFieldId::named("title");
        let panel = settled_panel(&mut app, named_tree(&id, "Hi"));

        // Despawn the run out of flow. The panel's `text_index` still maps the id
        // to the now-dead entity until the next reconcile rebuilds it, so the
        // liveness check inside the SystemParam is what must yield `None`.
        let run = first_run_entity(&mut app);
        app.world_mut().entity_mut(run).despawn();

        let lookup = id;
        let resolved = app
            .world_mut()
            .run_system_once(move |reader: PanelTextReader| reader.entity(panel, &lookup))
            .expect("system runs");
        assert!(
            resolved.is_none(),
            "a despawned run must resolve to None, not a dangling entity",
        );
    }

    #[test]
    fn set_sole_text_retexts_a_one_element_panel() {
        let mut app = access_app();
        let panel = settled_panel(&mut app, auto_tree("Hi"));

        let wrote = app
            .world_mut()
            .run_system_once(move |mut text: PanelText| text.set_sole_text(panel, "Bye"))
            .expect("system runs");
        assert!(wrote, "the lone run should accept the write");
        app.update();

        let sole = app
            .world_mut()
            .run_system_once(move |reader: PanelTextReader| {
                reader.sole_text(panel).map(str::to_owned)
            })
            .expect("system runs");
        assert_eq!(sole.as_deref(), Some("Bye"));
    }
}
