use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::Button;
use super::Slider;
use super::WidgetOf;
use crate::PanelBuildError;
use crate::PanelElementId;
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
    panels:  Query<'w, 's, &'static DiegeticPanel>,
    widgets: Query<'w, 's, (&'static PanelWidget, &'static WidgetOf)>,
}

impl PanelWidgetReader<'_, '_> {
    /// Resolves `id` within `panel` to its live reified widget entity.
    ///
    /// Returns `None` when the panel or id is missing, the widget has not been
    /// reified, or the panel-local index points at a stale or mismatched entity.
    #[must_use]
    pub fn entity(&self, panel: Entity, id: &PanelElementId) -> Option<Entity> {
        let entity = self.panels.get(panel).ok()?.widget_entity(id)?;
        let (widget, widget_of) = self.widgets.get(entity).ok()?;
        (widget.id() == id && widget_of.panel() == panel).then_some(entity)
    }
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
    id:       PanelElementId,
    kind:     WidgetKind,
    preorder: usize,
    authored: WidgetSpec,
}

impl ComputedWidgetRecord {
    pub(crate) const fn new(id: PanelElementId, preorder: usize, authored: WidgetSpec) -> Self {
        let kind = authored.kind();
        Self {
            id,
            kind,
            preorder,
            authored,
        }
    }

    pub(crate) const fn id(&self) -> &PanelElementId { &self.id }

    pub(crate) const fn kind(&self) -> WidgetKind { self.kind }

    pub(crate) const fn preorder(&self) -> usize { self.preorder }

    pub(crate) const fn authored(&self) -> &WidgetSpec { &self.authored }
}

pub(crate) fn validate_tree(tree: &LayoutTree) -> Result<(), PanelBuildError> {
    if let Some(duplicate) = tree.duplicate_named_element_id() {
        return Err(PanelBuildError::DuplicateElementId(duplicate.clone()));
    }
    tree.validate_widgets()
}
