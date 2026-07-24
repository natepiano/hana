use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::window::MonitorSelection;
use bevy::window::PrimaryWindow;
use bevy::window::VideoModeSelection;
use bevy::window::WindowMode;
use bevy::window::WindowPosition;
use bevy::window::WindowResolution;
use bevy_clerestory::CancelWindowRecovery;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::WindowKey;
use serde::Deserialize;
use serde::Serialize;

use super::constants::*;
use super::setup;
use super::setup::AutomaticRecoveryCancelled;
use super::setup::UnregisteredControl;
use super::trace::ProbeTrace;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ProbeWindowSelector {
    Primary,
    Automatic,
    Application,
    Control,
}

impl ProbeWindowSelector {
    fn matches(
        self,
        primary: bool,
        managed_window: Option<&ManagedWindow>,
        unregistered_control: bool,
    ) -> bool {
        match self {
            Self::Primary => primary,
            Self::Automatic => managed_window
                .is_some_and(|managed_window| managed_window.name == AUTOMATIC_WINDOW_KEY),
            Self::Application => managed_window
                .is_some_and(|managed_window| managed_window.name == APPLICATION_WINDOW_KEY),
            Self::Control => unregistered_control,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum RequestedWindowMode {
    Windowed,
    Borderless,
    Exclusive,
}

impl RequestedWindowMode {
    const fn window_mode(self) -> WindowMode {
        match self {
            Self::Windowed => WindowMode::Windowed,
            Self::Borderless => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            Self::Exclusive => {
                WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current)
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(super) enum ProbeCommand {
    SetMode {
        window: ProbeWindowSelector,
        mode:   RequestedWindowMode,
    },
    Move {
        window:   ProbeWindowSelector,
        position: [i32; 2],
    },
    Resize {
        window: ProbeWindowSelector,
        size:   [u32; 2],
    },
    CancelRecovery,
    ReplaceApplication,
    Close {
        window: ProbeWindowSelector,
    },
}

#[derive(Clone, Debug, Event)]
pub(super) struct ProbeCommandIntent {
    pub(super) command_id: String,
    pub(super) command:    ProbeCommand,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum CommandStatus {
    Applied,
    Rejected,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct CommandReceipt {
    pub(super) command_id: String,
    pub(super) status:     CommandStatus,
    pub(super) detail:     String,
}

#[derive(Default, Resource)]
pub(super) struct CommandReceipts(pub(super) BTreeMap<String, CommandReceipt>);

type WindowQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static mut Window,
        Has<PrimaryWindow>,
        Option<&'static ManagedWindow>,
        Has<UnregisteredControl>,
    ),
>;

type SelectedWindow<'a> = (
    Entity,
    Mut<'a, Window>,
    bool,
    Option<&'a ManagedWindow>,
    bool,
);

fn select_window<'a>(
    windows: &'a mut WindowQuery,
    selector: ProbeWindowSelector,
) -> Option<SelectedWindow<'a>> {
    windows
        .iter_mut()
        .find(|(_, _, primary, managed, control)| selector.matches(*primary, *managed, *control))
}

fn mutate_window(
    windows: &mut WindowQuery,
    selector: ProbeWindowSelector,
    apply: impl FnOnce(&mut Window) -> &'static str,
) -> Result<&'static str, &'static str> {
    select_window(windows, selector).map_or(
        Err("target window is unavailable"),
        |(_, mut target, ..)| Ok(apply(&mut target)),
    )
}

pub(super) fn apply_probe_command(
    event: On<ProbeCommandIntent>,
    mut commands: Commands,
    mut windows: WindowQuery,
    mut receipts: ResMut<CommandReceipts>,
    trace: Res<ProbeTrace>,
    frame_count: Res<bevy::diagnostic::FrameCount>,
) {
    if receipts.0.contains_key(&event.command_id) {
        return;
    }
    let result = match event.command.clone() {
        ProbeCommand::SetMode { window, mode } => mutate_window(&mut windows, window, |target| {
            target.mode = mode.window_mode();
            "window mode updated"
        }),
        ProbeCommand::Move { window, position } => mutate_window(&mut windows, window, |target| {
            target.position = WindowPosition::At(IVec2::from_array(position));
            "window position updated"
        }),
        ProbeCommand::Resize { window, size } => mutate_window(&mut windows, window, |target| {
            target.resolution = WindowResolution::new(size[0], size[1]);
            "window size updated"
        }),
        ProbeCommand::CancelRecovery => windows
            .iter_mut()
            .find(|(_, _, _, managed, _)| {
                managed.is_some_and(|managed| managed.name == AUTOMATIC_WINDOW_KEY)
            })
            .map_or(
                Err("managed automatic window is unavailable"),
                |(entity, ..)| {
                    commands.entity(entity).insert(AutomaticRecoveryCancelled);
                    let window_key = WindowKey::Managed(AUTOMATIC_WINDOW_KEY.into());
                    commands.trigger(CancelWindowRecovery {
                        window: window_key.clone(),
                    });
                    trace.record(
                        frame_count.0,
                        PRODUCER_AUTOMATIC_RECOVERY_CANCELLATION_REQUESTED,
                        KIND_RECOVERY_CANCELLATION_REQUESTED,
                        vec![(FIELD_WINDOW_KEY.into(), format!("{window_key:?}"))],
                    );
                    Ok("automatic recovery cancelled")
                },
            ),
        ProbeCommand::ReplaceApplication => {
            let application_exists = windows.iter().any(|(_, _, _, managed, _)| {
                managed.is_some_and(|managed| managed.name == APPLICATION_WINDOW_KEY)
            });
            if application_exists {
                Ok("application-controlled window already exists")
            } else {
                commands.spawn((
                    setup::probe_window(APPLICATION_WINDOW_TITLE, WindowPosition::Automatic),
                    ManagedWindow {
                        name: APPLICATION_WINDOW_KEY.into(),
                    },
                ));
                Ok("application-controlled replacement requested")
            }
        },
        ProbeCommand::Close { window } => select_window(&mut windows, window).map_or(
            Err("target window is unavailable"),
            |(entity, _, primary, managed_window, _)| {
                let window_key = if primary {
                    Some(WindowKey::Primary)
                } else {
                    managed_window
                        .map(|managed_window| WindowKey::Managed(managed_window.name.clone()))
                };
                if let Some(window_key) = window_key {
                    commands.trigger(CancelWindowRecovery { window: window_key });
                }
                commands.entity(entity).despawn();
                Ok("window close requested")
            },
        ),
    };
    let (status, detail) = match result {
        Ok(detail) => (CommandStatus::Applied, detail),
        Err(detail) => (CommandStatus::Rejected, detail),
    };
    receipts.0.insert(
        event.command_id.clone(),
        CommandReceipt {
            command_id: event.command_id.clone(),
            status,
            detail: detail.into(),
        },
    );
}
