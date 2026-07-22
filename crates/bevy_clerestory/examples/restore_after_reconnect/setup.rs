use std::collections::HashSet;

use bevy::camera::RenderTarget;
use bevy::diagnostic::FrameCount;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::ui::UiTargetCamera;
use bevy::window::MonitorSelection;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy::window::WindowRef;
use bevy::window::WindowResolution;
use bevy_clerestory::CancelWindowRecovery;
use bevy_clerestory::CurrentMonitor;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::MonitorInfo;
use bevy_clerestory::Monitors;
use bevy_clerestory::Platform;
use bevy_clerestory::WindowKey;
use bevy_clerestory::WindowRecovery;

use super::ProbeMonitorIndex;
use super::SmokeExitFrame;
use super::constants::*;
use super::selected_window_position;
use super::trace::ProbeTrace;

#[derive(Default, Resource)]
pub(super) struct AcceptedWindowKeys(pub(super) HashSet<WindowKey>);

#[cfg(test)]
#[derive(Resource)]
struct ProbeMonitorOverride(Vec<(Entity, MonitorInfo)>);

#[derive(SystemParam)]
pub(super) struct ProbeMonitors<'w> {
    monitors:         Res<'w, Monitors>,
    #[cfg(test)]
    monitor_override: Option<Res<'w, ProbeMonitorOverride>>,
}

impl ProbeMonitors<'_> {
    pub(super) fn by_entity(&self, entity: Entity) -> Option<MonitorInfo> {
        #[cfg(test)]
        if let Some(monitor_override) = &self.monitor_override {
            return monitor_override
                .0
                .iter()
                .find_map(|(candidate, monitor)| (*candidate == entity).then_some(*monitor));
        }
        self.monitors
            .iter()
            .find_map(|monitor| (monitor.entity == entity).then_some(*monitor.monitor_info))
    }

    fn by_index(&self, index: usize) -> Option<&MonitorInfo> {
        #[cfg(test)]
        if let Some(monitor_override) = &self.monitor_override {
            return monitor_override
                .0
                .iter()
                .find_map(|(_, monitor)| (monitor.index == index).then_some(monitor));
        }
        self.monitors.by_index(index)
    }
}

#[derive(Component)]
pub(super) struct ProbePlacementRequested;

#[derive(Component)]
pub(super) struct ProbeContentAttached;

#[derive(Component)]
pub(super) struct ControlPlacementConfirmed;

#[derive(Component)]
pub(super) struct AutomaticRecoveryCancelled;

#[derive(Component)]
struct ProbeContentRoot;

#[derive(Clone, Copy, Component)]
pub(super) struct ProbeContentOwner {
    window: Entity,
}

#[derive(Component)]
pub(super) struct UnregisteredControl;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitialWindowAction {
    Register(WindowRecovery),
    RequestPlacement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitialControlAction {
    ConfirmAssociation,
    RequestPlacement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ControlPlacementState {
    AwaitingConfirmation,
    Confirmed,
}

fn field(name: &str, value: impl std::fmt::Debug) -> (String, String) {
    (name.into(), format!("{value:?}"))
}

pub(super) fn canonical_window_key(
    primary_window: Option<&PrimaryWindow>,
    managed_window: Option<&ManagedWindow>,
) -> Option<WindowKey> {
    match (primary_window, managed_window) {
        (Some(_), None) => Some(WindowKey::Primary),
        (None, Some(managed_window)) => Some(WindowKey::Managed(managed_window.name.clone())),
        (Some(_), Some(_)) | (None, None) => None,
    }
}

pub(super) fn recovery_policy(window_key: &WindowKey) -> Option<WindowRecovery> {
    match window_key {
        WindowKey::Primary => Some(WindowRecovery::FallbackAndReturn),
        WindowKey::Managed(name) if name == AUTOMATIC_WINDOW_KEY => {
            Some(WindowRecovery::FallbackAndReturn)
        },
        WindowKey::Managed(name) if name == APPLICATION_WINDOW_KEY => {
            Some(WindowRecovery::ApplicationControlled)
        },
        WindowKey::Managed(_) => None,
    }
}

pub(super) fn probe_window(title: &str, position: WindowPosition) -> Window {
    Window {
        title: title.into(),
        position,
        resolution: WindowResolution::new(PROBE_WINDOW_WIDTH, PROBE_WINDOW_HEIGHT),
        ..default()
    }
}

fn managed_content(window_key: &WindowKey) -> Option<&'static str> {
    match window_key {
        WindowKey::Managed(name) if name == AUTOMATIC_WINDOW_KEY => Some(AUTOMATIC_WINDOW_TITLE),
        WindowKey::Managed(name) if name == APPLICATION_WINDOW_KEY => {
            Some(APPLICATION_WINDOW_TITLE)
        },
        WindowKey::Primary | WindowKey::Managed(_) => None,
    }
}

fn attach_content(
    commands: &mut Commands,
    window: Entity,
    window_key: WindowKey,
    label: &str,
    trace: &ProbeTrace,
    frame_count: u32,
) {
    commands.entity(window).insert(ProbeContentAttached);
    let camera = commands
        .spawn((
            Camera2d,
            RenderTarget::Window(WindowRef::Entity(window)),
            ProbeContentOwner { window },
        ))
        .id();
    commands.spawn((
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(CONTENT_FONT_SIZE),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(CONTENT_MARGIN),
            left: Val::Px(CONTENT_MARGIN),
            ..default()
        },
        UiTargetCamera(camera),
        ProbeContentOwner { window },
        ProbeContentRoot,
    ));
    trace.record(
        frame_count,
        PRODUCER_SETUP_CONTENT,
        KIND_CONTENT_ATTACHED,
        vec![
            field(FIELD_WINDOW, window),
            field(FIELD_WINDOW_KEY, window_key),
        ],
    );
}

pub(super) fn attach_primary_content(
    windows: Query<Entity, (Added<PrimaryWindow>, Without<ProbeContentAttached>)>,
    mut commands: Commands,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
) {
    for window in &windows {
        attach_content(
            &mut commands,
            window,
            WindowKey::Primary,
            PRIMARY_WINDOW_TITLE,
            &trace,
            frame_count.0,
        );
    }
}

pub(super) fn remove_window_content(
    removed: On<Remove, Window>,
    content: Query<(Entity, &ProbeContentOwner)>,
    mut commands: Commands,
) {
    for (entity, owner) in &content {
        if owner.window == removed.entity {
            commands.entity(entity).despawn();
        }
    }
}

pub(super) fn control_automatic_window_mode(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut windows: Query<(&ManagedWindow, &mut Window), Without<PrimaryWindow>>,
) {
    let window_mode = match (
        keyboard.just_pressed(KeyCode::KeyB),
        keyboard.just_pressed(KeyCode::KeyW),
    ) {
        (true, false) => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        (false, true) => WindowMode::Windowed,
        (true, true) | (false, false) => return,
    };
    for (managed_window, mut window) in &mut windows {
        if managed_window.name == AUTOMATIC_WINDOW_KEY && window.focused {
            window.mode = window_mode;
        }
    }
}

pub(super) fn cancel_automatic_window_recovery(
    keyboard: Res<ButtonInput<KeyCode>>,
    windows: Query<
        (Entity, &ManagedWindow, &Window),
        (Without<PrimaryWindow>, Without<AutomaticRecoveryCancelled>),
    >,
    mut commands: Commands,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
) {
    if !keyboard.just_pressed(KeyCode::KeyC) {
        return;
    }
    for (entity, managed_window, window) in &windows {
        if managed_window.name != AUTOMATIC_WINDOW_KEY || !window.focused {
            continue;
        }
        let window_key = WindowKey::Managed(AUTOMATIC_WINDOW_KEY.into());
        commands.entity(entity).insert(AutomaticRecoveryCancelled);
        commands.trigger(CancelWindowRecovery {
            window: window_key.clone(),
        });
        trace.record(
            frame_count.0,
            PRODUCER_AUTOMATIC_RECOVERY_CANCELLATION_REQUESTED,
            KIND_RECOVERY_CANCELLATION_REQUESTED,
            vec![
                field(FIELD_WINDOW, entity),
                field(FIELD_WINDOW_KEY, window_key),
            ],
        );
    }
}

pub(super) fn attach_managed_content(
    windows: Query<
        (Entity, &ManagedWindow),
        (
            Added<ManagedWindow>,
            Without<PrimaryWindow>,
            Without<ProbeContentAttached>,
        ),
    >,
    mut commands: Commands,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
) {
    for (window, managed_window) in &windows {
        let window_key = WindowKey::Managed(managed_window.name.clone());
        let Some(label) = managed_content(&window_key) else {
            continue;
        };
        attach_content(
            &mut commands,
            window,
            window_key,
            label,
            &trace,
            frame_count.0,
        );
    }
}

pub(super) fn exit_after_smoke_frame(
    exit_frame: Option<Res<SmokeExitFrame>>,
    frame_count: Res<FrameCount>,
    mut app_exit: MessageWriter<AppExit>,
) {
    if exit_frame.is_some_and(|exit_frame| frame_count.0 >= exit_frame.0) {
        app_exit.write(AppExit::Success);
    }
}

pub(super) fn spawn_probe_windows(mut commands: Commands) {
    commands.spawn((
        probe_window(AUTOMATIC_WINDOW_TITLE, WindowPosition::Automatic),
        ManagedWindow {
            name: AUTOMATIC_WINDOW_KEY.into(),
        },
    ));
    commands.spawn((
        probe_window(APPLICATION_WINDOW_TITLE, WindowPosition::Automatic),
        ManagedWindow {
            name: APPLICATION_WINDOW_KEY.into(),
        },
    ));
    commands.spawn((
        probe_window(CONTROL_WINDOW_TITLE, WindowPosition::Automatic),
        UnregisteredControl,
    ));
}

fn matching_selected_monitor(
    installed_monitor: Option<MonitorInfo>,
    current_monitor: &CurrentMonitor,
    selected_monitor_index: usize,
) -> Option<MonitorInfo> {
    let installed_monitor = installed_monitor?;
    (installed_monitor.index == selected_monitor_index
        && current_monitor.monitor_info == installed_monitor)
        .then_some(installed_monitor)
}

fn initial_window_action(
    accepted_window_keys: &AcceptedWindowKeys,
    window_key: &WindowKey,
    installed_monitor: Option<MonitorInfo>,
    current_monitor: &CurrentMonitor,
    selected_monitor_index: usize,
    platform: Platform,
    placement_request: Option<&ProbePlacementRequested>,
    selected_monitor: Option<&MonitorInfo>,
) -> Option<InitialWindowAction> {
    if accepted_window_keys.0.contains(window_key) {
        return None;
    }
    let window_recovery = recovery_policy(window_key)?;
    if matching_selected_monitor(installed_monitor, current_monitor, selected_monitor_index)
        .is_none()
    {
        return (platform.position_available()
            && placement_request.is_none()
            && selected_monitor.is_some())
        .then_some(InitialWindowAction::RequestPlacement);
    }
    Some(InitialWindowAction::Register(window_recovery))
}

fn initial_control_action(
    control_placement_state: ControlPlacementState,
    installed_monitor: Option<MonitorInfo>,
    selected_monitor_index: usize,
    platform: Platform,
    placement_request: Option<&ProbePlacementRequested>,
    selected_monitor: Option<&MonitorInfo>,
) -> Option<InitialControlAction> {
    if control_placement_state == ControlPlacementState::Confirmed {
        return None;
    }
    let installed_monitor = installed_monitor?;
    if installed_monitor.index == selected_monitor_index {
        return Some(InitialControlAction::ConfirmAssociation);
    }
    (platform.position_available() && placement_request.is_none() && selected_monitor.is_some())
        .then_some(InitialControlAction::RequestPlacement)
}

fn confirm_control_association(
    entity: Entity,
    window: &Window,
    on_monitor: Entity,
    installed_monitor: MonitorInfo,
    commands: &mut Commands,
    trace: &ProbeTrace,
    frame_count: u32,
) {
    commands.entity(entity).insert(ControlPlacementConfirmed);
    trace.record(
        frame_count,
        PRODUCER_CONTROL_ASSOCIATION,
        KIND_CONTROL_ASSOCIATION_CONFIRMED,
        vec![
            field(FIELD_WINDOW, entity),
            field(FIELD_WINDOW_TITLE, &window.title),
            field(FIELD_MONITOR_ENTITY, on_monitor),
            field(FIELD_MONITOR, installed_monitor),
        ],
    );
}

pub(super) fn place_and_register_probe_windows(
    monitor_index: Res<ProbeMonitorIndex>,
    platform: Res<Platform>,
    monitors: ProbeMonitors,
    windows: Query<(
        Entity,
        &OnMonitor,
        &CurrentMonitor,
        Option<&PrimaryWindow>,
        Option<&ManagedWindow>,
        Option<&ProbePlacementRequested>,
    )>,
    mut accepted_window_keys: ResMut<AcceptedWindowKeys>,
    mut commands: Commands,
) {
    for (entity, on_monitor, current_monitor, primary_window, managed_window, placement_request) in
        &windows
    {
        let Some(window_key) = canonical_window_key(primary_window, managed_window) else {
            continue;
        };
        if accepted_window_keys.0.contains(&window_key) {
            continue;
        }
        let installed_monitor = monitors.by_entity(on_monitor.0);
        match initial_window_action(
            &accepted_window_keys,
            &window_key,
            installed_monitor,
            current_monitor,
            monitor_index.0,
            *platform,
            placement_request,
            monitors.by_index(monitor_index.0),
        ) {
            Some(InitialWindowAction::RequestPlacement) => {
                commands.entity(entity).insert(ProbePlacementRequested);
            },
            Some(InitialWindowAction::Register(window_recovery)) => {
                accepted_window_keys.0.insert(window_key);
                commands.entity(entity).insert(window_recovery);
            },
            None => {},
        }
    }
}

pub(super) fn place_and_confirm_unregistered_control(
    monitor_index: Res<ProbeMonitorIndex>,
    platform: Res<Platform>,
    monitors: ProbeMonitors,
    controls: Query<
        (
            Entity,
            &Window,
            &OnMonitor,
            Option<&ProbePlacementRequested>,
            Option<&ControlPlacementConfirmed>,
        ),
        With<UnregisteredControl>,
    >,
    mut commands: Commands,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
) {
    for (entity, window, on_monitor, placement_request, confirmation) in &controls {
        let installed_monitor = monitors.by_entity(on_monitor.0);
        match initial_control_action(
            confirmation.map_or(ControlPlacementState::AwaitingConfirmation, |_| {
                ControlPlacementState::Confirmed
            }),
            installed_monitor,
            monitor_index.0,
            *platform,
            placement_request,
            monitors.by_index(monitor_index.0),
        ) {
            Some(InitialControlAction::ConfirmAssociation) => {
                let Some(installed_monitor) = installed_monitor else {
                    continue;
                };
                confirm_control_association(
                    entity,
                    window,
                    on_monitor.0,
                    installed_monitor,
                    &mut commands,
                    &trace,
                    frame_count.0,
                );
            },
            Some(InitialControlAction::RequestPlacement) => {
                commands.entity(entity).insert(ProbePlacementRequested);
            },
            None => {},
        }
    }
}

pub(super) fn request_probe_window_placement(
    monitor_index: Res<ProbeMonitorIndex>,
    platform: Res<Platform>,
    mut windows: Query<&mut Window, Added<ProbePlacementRequested>>,
) {
    for mut window in &mut windows {
        window.position = selected_window_position(*platform, monitor_index.0);
    }
}

pub(super) fn trace_probe_session(
    monitor_index: Res<ProbeMonitorIndex>,
    platform: Res<Platform>,
    monitors: Res<Monitors>,
    trace: Res<ProbeTrace>,
    frame_count: Res<FrameCount>,
) {
    trace.record(
        frame_count.0,
        PRODUCER_STARTUP_SESSION,
        KIND_PROBE_SESSION,
        vec![
            field(FIELD_PLATFORM, *platform),
            field(FIELD_SELECTED_MONITOR_INDEX, monitor_index.0),
            field(FIELD_PLACEMENT_CAPABILITY, platform.position_available()),
            field(
                FIELD_MONITOR,
                monitors
                    .iter()
                    .map(|monitor| (monitor.entity, *monitor.monitor_info))
                    .collect::<Vec<_>>(),
            ),
        ],
    );
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
pub(super) mod tests {
    use bevy::ecs::schedule::common_conditions::not;
    use bevy::ecs::schedule::common_conditions::resource_exists;
    use bevy::reflect::tuple_struct::DynamicTupleStruct;
    use bevy::window::Monitor;
    use bevy::window::WindowMode;
    use bevy::winit::WinitMonitors;
    use bevy_clerestory::MonitorId;
    use bevy_clerestory::MonitorIdentity;
    use bevy_clerestory::WindowManagerPlugin;
    use bevy_kana::ToI32;

    use super::*;

    const EXPECTED_CONTENT_ENTITIES_PER_WINDOW: usize = 2;
    const EXPECTED_CONTENT_PER_WINDOW: usize = 1;
    const INITIAL_MONITOR_INDEX: usize = 3;
    const MONITOR_PHYSICAL_SIZE: UVec2 = UVec2::new(1_920, 1_080);
    const MONITOR_SCALE: f64 = 1.0;
    const SELECTED_MONITOR_ID_RAW: u64 = 17;
    const SELECTED_MONITOR_INDEX: usize = 0;
    const SMOKE_EXIT_FRAME: u32 = 3;

    #[derive(Default, Resource)]
    struct CancellationRequests(Vec<WindowKey>);

    #[derive(Clone, Copy)]
    pub(crate) struct ProbeTestMonitors {
        pub(crate) mismatched_entity:  Entity,
        pub(crate) mismatched_monitor: MonitorInfo,
        pub(crate) selected_entity:    Entity,
        pub(crate) selected_monitor:   MonitorInfo,
    }

    fn content_app() -> App {
        let mut app = App::new();
        app.init_resource::<FrameCount>()
            .insert_resource(ProbeTrace::default())
            .init_resource::<AcceptedWindowKeys>()
            .add_observer(remove_window_content)
            .add_systems(Update, (attach_primary_content, attach_managed_content));
        app
    }

    fn current_monitor(index: usize) -> CurrentMonitor {
        CurrentMonitor {
            monitor_info:          MonitorInfo {
                identity: MonitorIdentity::Unverified,
                index,
                scale: MONITOR_SCALE,
                physical_position: IVec2::ZERO,
                physical_size: MONITOR_PHYSICAL_SIZE,
            },
            effective_window_mode: WindowMode::Windowed,
        }
    }

    fn monitor_component(physical_position: IVec2) -> Monitor {
        Monitor {
            name: None,
            physical_height: MONITOR_PHYSICAL_SIZE.y,
            physical_width: MONITOR_PHYSICAL_SIZE.x,
            physical_position,
            refresh_rate_millihertz: None,
            scale_factor: MONITOR_SCALE,
            video_modes: Vec::new(),
        }
    }

    fn selected_monitor_id() -> MonitorId {
        let mut reflected_monitor_id = DynamicTupleStruct::default();
        reflected_monitor_id.insert(SELECTED_MONITOR_ID_RAW);
        MonitorId::from_reflect(&reflected_monitor_id)
            .expect("reflected monitor identifier should be constructible")
    }

    pub(crate) fn production_system_app() -> (App, ProbeTestMonitors) {
        let test_directory = tempfile::tempdir().expect("test directory should be available");
        let mut topology_app = App::new();
        topology_app
            .init_resource::<FrameCount>()
            .insert_resource(WinitMonitors::default())
            .add_plugins(WindowManagerPlugin::with_path(
                test_directory.path().join("windows.ron"),
            ));
        let first_monitor_entity = topology_app
            .world_mut()
            .spawn(monitor_component(IVec2::ZERO))
            .id();
        let second_monitor_entity = topology_app
            .world_mut()
            .spawn(monitor_component(IVec2::new(
                MONITOR_PHYSICAL_SIZE.x.to_i32(),
                0,
            )))
            .id();
        topology_app.world_mut().run_schedule(PreStartup);

        let monitors = topology_app
            .world_mut()
            .remove_resource::<Monitors>()
            .expect("production monitor initialization should install Monitors");
        let selected = monitors
            .iter()
            .find(|monitor| monitor.monitor_info.index == SELECTED_MONITOR_INDEX)
            .expect("selected test monitor should be installed");
        let selected_entity = selected.entity;
        let selected_monitor = MonitorInfo {
            identity: MonitorIdentity::Verified(selected_monitor_id()),
            ..*selected.monitor_info
        };
        let mismatched = monitors
            .iter()
            .find(|monitor| monitor.entity != selected_entity)
            .expect("mismatched test monitor should be installed");
        let mismatched_entity = mismatched.entity;
        let mismatched_monitor = *mismatched.monitor_info;

        let mut app = App::new();
        assert!(app.world_mut().spawn_empty_at(first_monitor_entity).is_ok());
        app.world_mut()
            .entity_mut(first_monitor_entity)
            .insert(monitor_component(IVec2::ZERO));
        assert!(
            app.world_mut()
                .spawn_empty_at(second_monitor_entity)
                .is_ok()
        );
        app.world_mut()
            .entity_mut(second_monitor_entity)
            .insert(monitor_component(IVec2::new(
                MONITOR_PHYSICAL_SIZE.x.to_i32(),
                0,
            )));
        app.init_resource::<FrameCount>()
            .init_resource::<AcceptedWindowKeys>()
            .init_resource::<super::super::recovery_trace::PreUnplugReadiness>()
            .insert_resource(ProbeMonitorIndex(SELECTED_MONITOR_INDEX))
            .insert_resource(Platform::Windows)
            .insert_resource(ProbeTrace::default())
            .insert_resource(ProbeMonitorOverride(vec![
                (selected_entity, selected_monitor),
                (mismatched_entity, mismatched_monitor),
            ]))
            .insert_resource(monitors)
            .add_systems(
                Update,
                (
                    request_probe_window_placement,
                    (
                        place_and_register_probe_windows,
                        place_and_confirm_unregistered_control,
                    ),
                    super::super::recovery_trace::record_recovery_readiness,
                )
                    .chain_ignore_deferred(),
            );
        (
            app,
            ProbeTestMonitors {
                mismatched_entity,
                mismatched_monitor,
                selected_entity,
                selected_monitor,
            },
        )
    }

    fn spawn_content_windows(
        world: &mut World,
        monitor_entity: Entity,
        monitor_index: usize,
    ) -> [Entity; 3] {
        let primary = world
            .spawn((
                Window::default(),
                PrimaryWindow,
                OnMonitor(monitor_entity),
                current_monitor(monitor_index),
            ))
            .id();
        let automatic = world
            .spawn((
                Window::default(),
                ManagedWindow {
                    name: AUTOMATIC_WINDOW_KEY.into(),
                },
                OnMonitor(monitor_entity),
                current_monitor(monitor_index),
            ))
            .id();
        let application = world
            .spawn((
                Window::default(),
                ManagedWindow {
                    name: APPLICATION_WINDOW_KEY.into(),
                },
                OnMonitor(monitor_entity),
                current_monitor(monitor_index),
            ))
            .id();
        [primary, automatic, application]
    }

    fn owned_content(world: &mut World, window: Entity) -> Vec<Entity> {
        let mut query = world.query::<(Entity, &ProbeContentOwner)>();
        query
            .iter(world)
            .filter_map(|(entity, owner)| (owner.window == window).then_some(entity))
            .collect()
    }

    fn assert_window_content(world: &mut World, window: Entity) {
        let content = owned_content(world, window);
        assert_eq!(content.len(), EXPECTED_CONTENT_ENTITIES_PER_WINDOW);
        let cameras: Vec<_> = content
            .iter()
            .copied()
            .filter(|entity| world.get::<Camera2d>(*entity).is_some())
            .collect();
        let roots: Vec<_> = content
            .iter()
            .copied()
            .filter(|entity| world.get::<ProbeContentRoot>(*entity).is_some())
            .collect();
        assert_eq!(cameras.len(), EXPECTED_CONTENT_PER_WINDOW);
        assert_eq!(roots.len(), EXPECTED_CONTENT_PER_WINDOW);

        let Some(camera) = cameras.first().copied() else {
            return;
        };
        let Some(root) = roots.first().copied() else {
            return;
        };
        assert!(matches!(
            world.get::<RenderTarget>(camera),
            Some(RenderTarget::Window(WindowRef::Entity(target))) if *target == window
        ));
        assert!(world.get::<Node>(root).is_some());
        assert!(world.get::<ChildOf>(root).is_none());
        assert_eq!(
            world.get::<UiTargetCamera>(root).map(|target| target.0),
            Some(camera)
        );
    }

    fn automatic_mode_app(smoke_exit_frame: Option<u32>) -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<FrameCount>()
            .init_resource::<CancellationRequests>()
            .insert_resource(ProbeTrace::default())
            .add_observer(
                |event: On<CancelWindowRecovery>, mut requests: ResMut<CancellationRequests>| {
                    requests.0.push(event.window.clone());
                },
            )
            .add_systems(
                Update,
                (
                    control_automatic_window_mode,
                    cancel_automatic_window_recovery,
                )
                    .run_if(not(resource_exists::<SmokeExitFrame>)),
            );
        if let Some(frame) = smoke_exit_frame {
            app.insert_resource(SmokeExitFrame(frame));
        }
        app
    }

    #[test]
    fn mismatched_control_is_placed_one_update_after_the_request() {
        let (mut app, monitors) = production_system_app();
        let control = app
            .world_mut()
            .spawn((
                probe_window(CONTROL_WINDOW_TITLE, WindowPosition::Automatic),
                UnregisteredControl,
                OnMonitor(monitors.mismatched_entity),
            ))
            .id();

        app.update();

        assert!(matches!(
            app.world()
                .get::<Window>(control)
                .map(|window| &window.position),
            Some(WindowPosition::Automatic)
        ));
        assert!(
            app.world()
                .get::<ProbePlacementRequested>(control)
                .is_some()
        );

        app.update();

        assert_eq!(
            app.world()
                .get::<Window>(control)
                .map(|window| &window.position),
            Some(&WindowPosition::Centered(MonitorSelection::Index(
                SELECTED_MONITOR_INDEX,
            )))
        );
    }

    #[test]
    fn matching_control_records_one_confirmation_and_ignores_later_association_changes() {
        let (mut app, monitors) = production_system_app();
        let control = app
            .world_mut()
            .spawn((
                probe_window(CONTROL_WINDOW_TITLE, WindowPosition::Automatic),
                UnregisteredControl,
                OnMonitor(monitors.selected_entity),
            ))
            .id();

        app.update();
        app.world_mut()
            .entity_mut(control)
            .insert(OnMonitor(monitors.mismatched_entity));
        app.update();
        app.world_mut()
            .insert_resource(ProbeMonitorIndex(monitors.mismatched_monitor.index));
        app.update();

        assert!(
            app.world()
                .get::<ControlPlacementConfirmed>(control)
                .is_some()
        );
        assert!(app.world().get::<WindowRecovery>(control).is_none());
        assert!(
            app.world()
                .get::<ProbePlacementRequested>(control)
                .is_none()
        );
        assert!(app.world().resource::<AcceptedWindowKeys>().0.is_empty());
        let records = app.world().resource::<ProbeTrace>().records();
        assert_eq!(
            records
                .iter()
                .filter(|record| record.kind == KIND_CONTROL_ASSOCIATION_CONFIRMED)
                .count(),
            1,
        );
        assert!(records.iter().any(|record| {
            record.kind == KIND_CONTROL_ASSOCIATION_CONFIRMED
                && record.fields.iter().any(|(name, value)| {
                    name == FIELD_WINDOW_TITLE && value == &format!("{CONTROL_WINDOW_TITLE:?}")
                })
                && record.fields.iter().any(|(name, value)| {
                    name == FIELD_MONITOR_ENTITY
                        && value == &format!("{:?}", monitors.selected_entity)
                })
        }));
        assert!(
            records
                .iter()
                .all(|record| record.kind != KIND_RECOVERY_ACCEPTED)
        );
    }

    #[test]
    fn unregistered_control_stays_outside_recovery() {
        let mut app = App::new();
        app.insert_resource(ProbeMonitorIndex(INITIAL_MONITOR_INDEX))
            .insert_resource(Platform::Windows)
            .insert_resource(ProbeTrace::default())
            .init_resource::<AcceptedWindowKeys>()
            .add_systems(Startup, spawn_probe_windows);
        app.update();

        let mut controls_query = app
            .world_mut()
            .query_filtered::<Entity, With<UnregisteredControl>>();
        let controls: Vec<_> = controls_query.iter(app.world()).collect();
        assert_eq!(controls.len(), 1);
        let control = controls[0];
        assert!(matches!(
            app.world()
                .get::<Window>(control)
                .map(|window| &window.position),
            Some(WindowPosition::Automatic)
        ));
        assert!(app.world().get::<WindowRecovery>(control).is_none());
        assert!(app.world().get::<PrimaryWindow>(control).is_none());
        assert!(app.world().get::<ManagedWindow>(control).is_none());
        assert!(app.world().resource::<AcceptedWindowKeys>().0.is_empty());
        assert_eq!(
            app.world()
                .resource::<ProbeTrace>()
                .records()
                .iter()
                .filter(|record| record.kind == KIND_RECOVERY_ACCEPTED)
                .count(),
            0,
        );
    }

    #[test]
    fn canonical_content_is_an_unparented_ui_root_targeted_to_its_window_camera_once() {
        let mut app = content_app();
        let monitor_entity = app.world_mut().spawn_empty().id();
        let windows = spawn_content_windows(app.world_mut(), monitor_entity, INITIAL_MONITOR_INDEX);
        app.update();
        app.update();

        for window in windows {
            assert_window_content(app.world_mut(), window);
        }

        app.world_mut()
            .entity_mut(windows[0])
            .remove::<PrimaryWindow>();
        app.world_mut().entity_mut(windows[0]).insert(PrimaryWindow);
        app.world_mut()
            .entity_mut(windows[1])
            .remove::<ManagedWindow>();
        app.world_mut()
            .entity_mut(windows[1])
            .insert(ManagedWindow {
                name: AUTOMATIC_WINDOW_KEY.into(),
            });
        app.update();

        for window in windows {
            assert_window_content(app.world_mut(), window);
        }
    }

    #[test]
    fn window_removal_cleans_up_its_camera_and_ui_root() {
        let mut app = content_app();
        let monitor_entity = app.world_mut().spawn_empty().id();
        let windows = spawn_content_windows(app.world_mut(), monitor_entity, INITIAL_MONITOR_INDEX);
        app.update();

        let removed_content = owned_content(app.world_mut(), windows[1]);
        assert_eq!(removed_content.len(), EXPECTED_CONTENT_ENTITIES_PER_WINDOW);
        app.world_mut().entity_mut(windows[1]).remove::<Window>();
        app.world_mut().flush();

        assert!(
            removed_content
                .into_iter()
                .all(|entity| app.world().get_entity(entity).is_err())
        );
        for window in [windows[0], windows[2]] {
            assert_window_content(app.world_mut(), window);
        }
    }

    #[test]
    fn accepted_reconstructed_windows_receive_content_without_registration_or_placement() {
        for (window_key, title) in [
            (WindowKey::Primary, PRIMARY_WINDOW_TITLE),
            (
                WindowKey::Managed(AUTOMATIC_WINDOW_KEY.into()),
                AUTOMATIC_WINDOW_TITLE,
            ),
        ] {
            let (mut app, monitors) = production_system_app();
            app.add_systems(PreUpdate, (attach_primary_content, attach_managed_content));
            app.world_mut()
                .resource_mut::<AcceptedWindowKeys>()
                .0
                .insert(window_key.clone());
            let reconstructed = app
                .world_mut()
                .spawn((
                    probe_window(title, WindowPosition::Automatic),
                    OnMonitor(monitors.mismatched_entity),
                    CurrentMonitor {
                        monitor_info:          monitors.mismatched_monitor,
                        effective_window_mode: WindowMode::Windowed,
                    },
                ))
                .id();
            match &window_key {
                WindowKey::Primary => {
                    app.world_mut()
                        .entity_mut(reconstructed)
                        .insert(PrimaryWindow);
                },
                WindowKey::Managed(name) => {
                    app.world_mut()
                        .entity_mut(reconstructed)
                        .insert(ManagedWindow { name: name.clone() });
                },
            }

            app.update();

            assert!(
                app.world()
                    .get::<ProbeContentAttached>(reconstructed)
                    .is_some()
            );
            assert_window_content(app.world_mut(), reconstructed);
            assert!(app.world().get::<WindowRecovery>(reconstructed).is_none());
            assert!(
                app.world()
                    .get::<ProbePlacementRequested>(reconstructed)
                    .is_none()
            );
            assert!(matches!(
                app.world()
                    .get::<Window>(reconstructed)
                    .map(|window| &window.position),
                Some(WindowPosition::Automatic)
            ));

            app.update();

            assert_window_content(app.world_mut(), reconstructed);
            assert!(app.world().get::<WindowRecovery>(reconstructed).is_none());
            assert!(
                app.world()
                    .get::<ProbePlacementRequested>(reconstructed)
                    .is_none()
            );
        }
    }

    #[test]
    fn keyboard_mode_control_changes_only_the_managed_automatic_window() {
        let mut app = automatic_mode_app(None);
        let automatic = app
            .world_mut()
            .spawn((
                Window {
                    focused: true,
                    ..default()
                },
                ManagedWindow {
                    name: AUTOMATIC_WINDOW_KEY.into(),
                },
            ))
            .id();
        let application = app
            .world_mut()
            .spawn((
                Window {
                    focused: true,
                    ..default()
                },
                ManagedWindow {
                    name: APPLICATION_WINDOW_KEY.into(),
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyB);
        app.update();

        assert_eq!(
            app.world()
                .get::<Window>(automatic)
                .map(|window| &window.mode),
            Some(&WindowMode::BorderlessFullscreen(MonitorSelection::Current)),
        );
        assert_eq!(
            app.world()
                .get::<Window>(application)
                .map(|window| &window.mode),
            Some(&WindowMode::Windowed),
        );

        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.clear();
        keyboard.press(KeyCode::KeyW);
        app.update();
        assert_eq!(
            app.world()
                .get::<Window>(automatic)
                .map(|window| &window.mode),
            Some(&WindowMode::Windowed),
        );
    }

    #[test]
    fn keyboard_cancellation_targets_the_focused_managed_automatic_window_once() {
        let mut app = automatic_mode_app(None);
        let automatic = app
            .world_mut()
            .spawn((
                Window {
                    focused: true,
                    ..default()
                },
                ManagedWindow {
                    name: AUTOMATIC_WINDOW_KEY.into(),
                },
            ))
            .id();
        app.world_mut().spawn((
            Window {
                focused: true,
                ..default()
            },
            ManagedWindow {
                name: APPLICATION_WINDOW_KEY.into(),
            },
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyC);

        app.update();

        assert_eq!(
            app.world().resource::<CancellationRequests>().0,
            [WindowKey::Managed(AUTOMATIC_WINDOW_KEY.into())],
        );
        assert!(
            app.world()
                .get::<AutomaticRecoveryCancelled>(automatic)
                .is_some()
        );
        let cancellation_records: Vec<_> = app
            .world()
            .resource::<ProbeTrace>()
            .records()
            .into_iter()
            .filter(|record| record.kind == KIND_RECOVERY_CANCELLATION_REQUESTED)
            .collect();
        assert_eq!(cancellation_records.len(), 1);
        assert_eq!(
            cancellation_records[0].producer,
            PRODUCER_AUTOMATIC_RECOVERY_CANCELLATION_REQUESTED,
        );

        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.clear();
        keyboard.press(KeyCode::KeyC);
        app.update();
        assert_eq!(app.world().resource::<CancellationRequests>().0.len(), 1);
    }

    #[test]
    fn keyboard_mode_control_is_inert_during_smoke_without_focus_and_without_its_window() {
        let mut smoke_app = automatic_mode_app(Some(SMOKE_EXIT_FRAME));
        let automatic = smoke_app
            .world_mut()
            .spawn((
                Window {
                    focused: true,
                    ..default()
                },
                ManagedWindow {
                    name: AUTOMATIC_WINDOW_KEY.into(),
                },
            ))
            .id();
        smoke_app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyB);
        smoke_app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyC);
        smoke_app.update();
        assert_eq!(
            smoke_app
                .world()
                .get::<Window>(automatic)
                .map(|window| &window.mode),
            Some(&WindowMode::Windowed),
        );
        assert!(
            smoke_app
                .world()
                .resource::<CancellationRequests>()
                .0
                .is_empty()
        );

        let mut unfocused_app = automatic_mode_app(None);
        let automatic = unfocused_app
            .world_mut()
            .spawn((
                Window {
                    focused: false,
                    ..default()
                },
                ManagedWindow {
                    name: AUTOMATIC_WINDOW_KEY.into(),
                },
            ))
            .id();
        unfocused_app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyB);
        unfocused_app.update();
        assert_eq!(
            unfocused_app
                .world()
                .get::<Window>(automatic)
                .map(|window| &window.mode),
            Some(&WindowMode::Windowed),
        );

        let mut absent_app = automatic_mode_app(None);
        absent_app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyB);
        absent_app.update();
    }
}
