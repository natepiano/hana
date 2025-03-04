// //! VisualizationControl bevy plugin for use in bevy based visualizations
use bevy::prelude::*;
use hana_network::Instruction;
use tracing::debug;

use crate::InstructionReceiver;

pub struct HanaPlugin;

impl Plugin for HanaPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(InstructionReceiver::spawn())
            .add_event::<HanaEvent>()
            .add_systems(Update, handle_hana_instruction);
    }
}

#[derive(Event)]
pub enum HanaEvent {
    Ping,
    // We can add more events here as needed
}

fn handle_hana_instruction(
    mut adapter: ResMut<InstructionReceiver>,
    mut viz_events: EventWriter<HanaEvent>,
) {
    while let Some(instruction) = adapter.try_recv() {
        match instruction {
            Instruction::Ping => {
                debug!("Received ping instruction");
                viz_events.send(HanaEvent::Ping);
            }
            _ => debug!("Received unhandled instruction"),
        }
    }
}
