use bevy::prelude::*;

use super::PanelText;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadePanelChild;
use crate::cascade::Override;
use crate::cascade::TextAlpha;

/// Cascading attribute for panel-text alpha mode.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(super) struct PanelTextAlpha(pub AlphaMode);

impl CascadePanelChild for PanelTextAlpha {
    type EntityOverride = PanelText;
    type PanelOverride = Override<TextAlpha>;

    fn entity_value(entity_override: &PanelText) -> Option<Self> {
        entity_override.alpha_mode.map(Self)
    }

    fn panel_value(panel_override: &Override<TextAlpha>) -> Option<Self> {
        Some(Self(panel_override.0.0))
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.text_alpha) }
}
