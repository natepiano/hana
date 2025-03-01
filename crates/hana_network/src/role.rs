/// Represents the role of a Hana network endpoint
pub trait Role {}

/// Controller role - manages and controls visualizations
pub struct HanaRole;
impl Role for HanaRole {}

/// Visualization role - receives and responds to control messages
pub struct VisualizationRole;
impl Role for VisualizationRole {}
