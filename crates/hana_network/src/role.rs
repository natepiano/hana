/// Represents the role of a Hana network endpoint
pub trait Role {}

/// Controller role - manages and controls visualizations
#[derive(Debug)]
pub struct HanaRole;
impl Role for HanaRole {}

/// Visualization role - receives and responds to control messages
#[derive(Debug)]
pub struct VisualizationRole;
impl Role for VisualizationRole {}
