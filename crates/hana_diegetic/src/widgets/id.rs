use std::collections::HashMap;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::Button;
use super::Slider;
use super::WidgetOf;
use crate::PanelBuildError;
use crate::PanelElementId;
use crate::cascade::Cascade;
use crate::layout::LayoutTree;
use crate::panel::DiegeticPanel;

/// Runtime identity of a reified panel widget.
#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct PanelWidget {
    id: PanelElementId,
}

impl PanelWidget {
    pub(crate) const fn new(id: PanelElementId) -> Self { Self { id } }

    /// Returns the widget's panel-local authored id.
    #[must_use]
    pub const fn id(&self) -> &PanelElementId { &self.id }
}

/// Read-only lookup from panel-local widget identity to a live widget entity.
#[derive(SystemParam)]
pub struct PanelWidgetReader<'w, 's> {
    panels:  Query<'w, 's, &'static PanelWidgetIndex, With<DiegeticPanel>>,
    widgets: Query<'w, 's, (&'static PanelWidget, &'static WidgetOf)>,
}

impl PanelWidgetReader<'_, '_> {
    /// Resolves `id` within `panel` to its live reified widget entity.
    ///
    /// Returns `None` when the panel or id is missing, the widget has not been
    /// reified, or the panel-local index points at a stale or mismatched entity.
    #[must_use]
    pub fn entity(&self, panel: Entity, id: &PanelElementId) -> Option<Entity> {
        let entity = self.panels.get(panel).ok()?.entity(id)?;
        let (widget, widget_of) = self.widgets.get(entity).ok()?;
        (widget.id() == id && widget_of.panel() == panel).then_some(entity)
    }
}

#[derive(Component, Default)]
pub(crate) struct PanelWidgetIndex(HashMap<PanelElementId, Entity>);

impl PanelWidgetIndex {
    pub(crate) fn clear(&mut self) { self.0.clear(); }

    fn entity(&self, id: &PanelElementId) -> Option<Entity> { self.0.get(id).copied() }

    pub(crate) fn replace(&mut self, index: HashMap<PanelElementId, Entity>) { self.0 = index; }
}

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub(crate) enum WidgetKind {
    Button,
    Slider,
}

#[derive(Clone, Component, Debug, PartialEq)]
pub(crate) enum WidgetSpec {
    Button(Button),
    Slider(Slider),
}

impl WidgetSpec {
    pub(crate) const fn kind(&self) -> WidgetKind {
        match self {
            Self::Button(_) => WidgetKind::Button,
            Self::Slider(_) => WidgetKind::Slider,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComputedWidgetRecord {
    id:            PanelElementId,
    kind:          WidgetKind,
    preorder:      usize,
    authored:      WidgetSpec,
    interactivity: Cascade<super::WidgetInteractivity>,
}

impl ComputedWidgetRecord {
    pub(crate) const fn new(
        id: PanelElementId,
        preorder: usize,
        authored: WidgetSpec,
        interactivity: Cascade<super::WidgetInteractivity>,
    ) -> Self {
        let kind = authored.kind();
        Self {
            id,
            kind,
            preorder,
            authored,
            interactivity,
        }
    }

    pub(crate) const fn id(&self) -> &PanelElementId { &self.id }

    pub(crate) const fn kind(&self) -> WidgetKind { self.kind }

    pub(crate) const fn preorder(&self) -> usize { self.preorder }

    pub(crate) const fn authored(&self) -> &WidgetSpec { &self.authored }

    pub(crate) const fn interactivity(&self) -> Cascade<super::WidgetInteractivity> {
        self.interactivity
    }
}

pub(crate) fn validate_tree(tree: &LayoutTree) -> Result<(), PanelBuildError> {
    if let Some(duplicate) = tree.duplicate_named_element_id() {
        return Err(PanelBuildError::DuplicateElementId(duplicate.clone()));
    }
    tree.validate_widgets()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;

    use super::PanelWidget;
    use super::PanelWidgetIndex;
    use super::PanelWidgetReader;
    use crate::DiegeticPanel;
    use crate::PanelElementId;
    use crate::WidgetOf;

    #[test]
    fn reader_rejects_index_after_owner_panel_component_is_removed() {
        let mut app = App::new();
        let panel = app.world_mut().spawn(DiegeticPanel::default()).id();
        let id = PanelElementId::named("action");
        let widget = app
            .world_mut()
            .spawn((PanelWidget::new(id.clone()), WidgetOf::new(panel)))
            .id();
        let index = app.world_mut().get_mut::<PanelWidgetIndex>(panel);
        assert!(index.is_some());
        let Some(mut index) = index else {
            return;
        };
        index.replace(HashMap::from([(id.clone(), widget)]));

        let before = app.world_mut().run_system_once({
            let id = id.clone();
            move |reader: PanelWidgetReader| reader.entity(panel, &id)
        });
        assert!(before.is_ok());
        assert_eq!(before.ok().flatten(), Some(widget));

        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        assert!(app.world().get_entity(panel).is_ok());
        assert!(app.world().get::<PanelWidgetIndex>(panel).is_some());

        let after = app
            .world_mut()
            .run_system_once(move |reader: PanelWidgetReader| reader.entity(panel, &id));
        assert!(after.is_ok());
        assert_eq!(after.ok().flatten(), None);
    }
}
