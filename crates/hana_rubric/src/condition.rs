//! Condition support for JSONC keymaps.

use std::collections::HashSet;
use std::marker::PhantomData;

use bevy::app::App;
use bevy::app::Plugin;
use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::prelude::PreUpdate;
use bevy::prelude::Reflect;
use bevy::prelude::Res;
use bevy::prelude::ResMut;
use bevy::prelude::Resource;
use bevy::prelude::State;
use bevy::prelude::States;
use strum::EnumMessage;
use strum::IntoEnumIterator;

use crate::Diagnostic;
use crate::DiagnosticKind;
use crate::DiagnosticSeverity;
use crate::KeymapLoadFailures;
use crate::KeymapSystems;
use crate::keymap_plugin::RegistryValidationFailed;

/// A condition name declared by a [`KeymapContext`] variant.
///
/// Applications define these names by deriving `strum::AsRefStr` with
/// `#[strum(serialize_all = "snake_case")]` on their context enum. Keymap parsing resolves the
/// authored text through [`ConditionRegistry`], so the input path carries only a
/// [`ConditionHandle`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, Reflect)]
#[reflect(opaque)]
pub struct ConditionName(String);

impl ConditionName {
    /// Creates a condition name from application-authored keymap text.
    #[must_use]
    pub(crate) fn new(name: impl Into<String>) -> Self { Self(name.into()) }

    /// Borrows the condition text declared by the application context enum.
    #[must_use]
    pub fn as_str(&self) -> &str { &self.0 }
}

impl AsRef<str> for ConditionName {
    fn as_ref(&self) -> &str { self.as_str() }
}

/// Context-enum requirements shared by resource and state-backed keymaps.
///
/// This is a downstream extension point: each application derives the required traits on its
/// own context enum instead of implementing `KeymapContext` manually. Derive
/// `strum::EnumIter`, `strum::AsRefStr`, and `strum::EnumMessage`; `AsRefStr` supplies the
/// `AsRef<str>` bound below.
pub trait KeymapContext:
    AsRef<str> + Copy + EnumMessage + Eq + IntoEnumIterator + Send + Sync + 'static
{
}

impl<T> KeymapContext for T where
    T: AsRef<str> + Copy + EnumMessage + Eq + IntoEnumIterator + Send + Sync + 'static
{
}

/// The registered condition currently selected by the application context.
///
/// The resource contains only an internal registry handle. It deliberately does not retain a
/// condition string, allowing the input path to select a compiled keymap without parsing or
/// comparing names.
#[derive(Debug, Default, Resource)]
pub struct ActiveCondition {
    handle:         Option<ConditionHandle>,
    is_initialized: bool,
}

impl ActiveCondition {
    /// Returns whether input routing can select the active condition or the global matcher.
    #[must_use]
    pub const fn is_initialized(&self) -> bool { self.is_initialized }

    pub(crate) const fn handle(&self) -> Option<ConditionHandle> { self.handle }

    const fn update(&mut self, condition_handle: ConditionHandle) {
        self.handle = Some(condition_handle);
        self.is_initialized = true;
    }

    pub(crate) const fn await_context(&mut self) {
        self.handle = None;
        self.is_initialized = false;
    }

    pub(crate) const fn enable_global(&mut self) { self.is_initialized = true; }
}

/// A condition's compact registry identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ConditionHandle(usize);

/// Stable condition metadata for schema and companion-file generation.
pub(crate) struct ConditionInfo<'registry> {
    pub(crate) name:        &'registry ConditionName,
    pub(crate) description: &'registry str,
}

/// The application-declared condition names and their resolved runtime handles.
#[derive(Default, Resource)]
pub(crate) struct ConditionRegistry {
    entries: Vec<ConditionEntry>,
}

impl ConditionRegistry {
    /// Registers every variant of `C` in declaration order.
    ///
    /// # Errors
    ///
    /// Returns diagnostics when a variant has no description or its name duplicates a previously
    /// registered name. A failed registration leaves the registry unchanged.
    pub(crate) fn register<C: KeymapContext>(&mut self) -> Result<(), Vec<Diagnostic>> {
        let mut declarations = Vec::new();
        let mut diagnostics = Vec::new();
        let mut names = HashSet::new();

        for condition in C::iter() {
            let condition_name = ConditionName::new(condition.as_ref());
            let description = condition
                .get_message()
                .filter(|message| !message.is_empty());
            let is_duplicate = !names.insert(condition_name.clone())
                || self
                    .entries
                    .iter()
                    .any(|condition_entry| condition_entry.name == condition_name);

            if is_duplicate {
                diagnostics.push(condition_diagnostic(
                    condition_name.as_str(),
                    DiagnosticSeverity::Failure,
                    format!(
                        "Condition `{}` is registered more than once.",
                        condition_name.as_str()
                    ),
                ));
            }

            match description {
                Some(description) => declarations.push(ConditionEntry {
                    name: condition_name,
                    description,
                }),
                None => diagnostics.push(condition_diagnostic(
                    condition_name.as_str(),
                    DiagnosticSeverity::Failure,
                    format!(
                        "Condition `{}` has no description. Add #[strum(message = \"…\")].",
                        condition_name.as_str()
                    ),
                )),
            }
        }

        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }

        self.entries.extend(declarations);

        Ok(())
    }

    /// Resolves authored condition text to its compact runtime handle.
    #[must_use]
    pub(crate) fn resolve(&self, condition_name: &str) -> Option<ConditionHandle> {
        self.iter()
            .position(|condition_info| condition_info.name.as_str() == condition_name)
            .map(ConditionHandle)
    }

    /// Iterates over registered condition metadata in declaration order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = ConditionInfo<'_>> {
        self.entries.iter().map(|condition_entry| ConditionInfo {
            name:        &condition_entry.name,
            description: condition_entry.description,
        })
    }
}

struct ConditionEntry {
    name:        ConditionName,
    description: &'static str,
}

pub(crate) fn sync_resource_condition<C: KeymapContext + Resource>(
    context: Option<Res<C>>,
    condition_registry: Res<ConditionRegistry>,
    mut active_condition: ResMut<ActiveCondition>,
) {
    let Some(context) = context else {
        return;
    };

    if context.is_changed() {
        update_active_condition(*context, &condition_registry, &mut active_condition);
    }
}

pub(crate) fn sync_state_condition<C: KeymapContext + States>(
    state: Option<Res<State<C>>>,
    condition_registry: Res<ConditionRegistry>,
    mut active_condition: ResMut<ActiveCondition>,
) {
    let Some(state) = state else {
        return;
    };

    if state.is_changed() {
        update_active_condition(*state.get(), &condition_registry, &mut active_condition);
    }
}

fn update_active_condition<C: KeymapContext>(
    context: C,
    condition_registry: &ConditionRegistry,
    active_condition: &mut ActiveCondition,
) {
    if let Some(condition_handle) = condition_registry.resolve(context.as_ref()) {
        active_condition.update(condition_handle);
    }
}

pub(crate) fn condition_diagnostic(
    condition_name: &str,
    severity: DiagnosticSeverity,
    message: String,
) -> Diagnostic {
    Diagnostic {
        source_path: String::new(),
        byte_range: 0..0,
        line: 0,
        column: 0,
        block_index: 0,
        context: condition_name.to_owned(),
        original_keystroke: String::new(),
        command_id: String::new(),
        kind: DiagnosticKind::Context,
        severity,
        message,
        suggestions: Vec::new(),
    }
}

pub(crate) struct ResourceContextPlugin<C> {
    keymap_plugin: crate::KeymapPlugin,
    marker:        PhantomData<fn() -> C>,
}

impl<C> ResourceContextPlugin<C> {
    pub(crate) fn new(keymap_plugin: crate::KeymapPlugin) -> Self {
        Self {
            keymap_plugin,
            marker: PhantomData,
        }
    }
}

impl<C> Plugin for ResourceContextPlugin<C>
where
    C: KeymapContext + Resource,
{
    fn build(&self, app: &mut App) {
        self.keymap_plugin.install(app);
        if register_context::<C>(app, ContextSource::Resource).is_ok() {
            app.add_systems(
                PreUpdate,
                sync_resource_condition::<C>.in_set(KeymapSystems::UpdateActiveCondition),
            );
        }
    }

    fn finish(&self, app: &mut App) { crate::KeymapPlugin::finish_assembly(app); }

    fn is_unique(&self) -> bool { false }
}

pub(crate) struct StateContextPlugin<C> {
    keymap_plugin: crate::KeymapPlugin,
    marker:        PhantomData<fn() -> C>,
}

impl<C> StateContextPlugin<C> {
    pub(crate) fn new(keymap_plugin: crate::KeymapPlugin) -> Self {
        Self {
            keymap_plugin,
            marker: PhantomData,
        }
    }
}

impl<C> Plugin for StateContextPlugin<C>
where
    C: KeymapContext + States,
{
    fn build(&self, app: &mut App) {
        self.keymap_plugin.install(app);
        if register_context::<C>(app, ContextSource::State).is_ok() {
            app.add_systems(
                PreUpdate,
                sync_state_condition::<C>.in_set(KeymapSystems::UpdateActiveCondition),
            );
        }
    }

    fn finish(&self, app: &mut App) { crate::KeymapPlugin::finish_assembly(app); }

    fn is_unique(&self) -> bool { false }
}

#[derive(Resource)]
struct ContextSourceInstalled(ContextSource);

#[derive(Clone, Copy)]
pub(crate) enum ContextSource {
    Resource,
    State,
    Derived,
}

impl ContextSource {
    const fn description(self) -> &'static str {
        match self {
            Self::Resource => "resource-backed",
            Self::State => "state-backed",
            Self::Derived => "derived",
        }
    }
}

pub(crate) fn register_context<C: KeymapContext>(
    app: &mut App,
    context_source: ContextSource,
) -> Result<(), Vec<Diagnostic>> {
    crate::KeymapPlugin::install_runtime(app);
    if let Some(previous_context_source) = app.world().get_resource::<ContextSourceInstalled>() {
        let diagnostics = vec![condition_diagnostic(
            "",
            DiagnosticSeverity::Failure,
            format!(
                "A {} keymap context is already registered; a keymap accepts exactly one context source.",
                previous_context_source.0.description()
            ),
        )];
        retain_context_diagnostics(app, &diagnostics);
        return Err(diagnostics);
    }
    app.world_mut()
        .insert_resource(ContextSourceInstalled(context_source));
    app.world_mut()
        .resource_mut::<ActiveCondition>()
        .await_context();
    app.init_resource::<ConditionRegistry>();

    let result = app
        .world_mut()
        .resource_mut::<ConditionRegistry>()
        .register::<C>();

    if let Err(diagnostics) = &result {
        app.world_mut().insert_resource(RegistryValidationFailed);
        retain_context_diagnostics(app, diagnostics);
    }

    result
}

pub(crate) fn retain_context_diagnostics(app: &mut App, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        match diagnostic.severity {
            DiagnosticSeverity::Failure => bevy::log::error!("{}", diagnostic.message),
            DiagnosticSeverity::Advisory => bevy::log::warn!("{}", diagnostic.message),
        }
    }
    app.init_resource::<KeymapLoadFailures>();
    app.world_mut()
        .resource_mut::<KeymapLoadFailures>()
        .retained_diagnostics
        .extend(diagnostics.iter().cloned());
}

#[cfg(test)]
mod tests {
    use bevy::prelude::App;
    use bevy::prelude::NextState;
    use bevy::prelude::Resource;
    use bevy::prelude::States;
    use bevy::state::app::AppExtStates;
    use bevy::state::app::StatesPlugin;
    use strum::AsRefStr;
    use strum::EnumIter;
    use strum::EnumMessage;

    use super::ActiveCondition;
    use super::ConditionHandle;
    use super::ConditionRegistry;
    use super::KeymapContext;
    use crate::DiagnosticKind;
    use crate::KeymapPlugin;

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum ResourceContext {
        #[strum(message = "While flying the ship")]
        Flying,
        #[strum(message = "While the pause menu is open")]
        Paused,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, Hash, PartialEq, States)]
    #[strum(serialize_all = "snake_case")]
    enum StateContext {
        #[strum(message = "While flying the ship")]
        Flying,
        #[strum(message = "While the pause menu is open")]
        Paused,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum MissingDescriptionContext {
        Flying,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq)]
    #[strum(serialize_all = "snake_case")]
    enum FirstDuplicateContext {
        #[strum(message = "While flying the first context")]
        Flying,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq)]
    #[strum(serialize_all = "snake_case")]
    enum SecondDuplicateContext {
        #[strum(message = "While flying the second context")]
        Flying,
    }

    #[test]
    fn derived_state_context_satisfies_keymap_context_without_a_resource_impl() {
        assert_keymap_context::<StateContext>();
    }

    #[test]
    fn registry_preserves_variant_names_descriptions_and_declaration_order() {
        let mut condition_registry = ConditionRegistry::default();

        assert!(condition_registry.register::<ResourceContext>().is_ok());

        let conditions = condition_registry
            .iter()
            .map(|condition_info| (condition_info.name.as_str(), condition_info.description))
            .collect::<Vec<_>>();
        assert_eq!(
            conditions,
            vec![
                ("flying", "While flying the ship"),
                ("paused", "While the pause menu is open"),
            ]
        );
    }

    #[test]
    fn missing_condition_description_produces_a_context_diagnostic() {
        let mut app = App::new();
        app.add_plugins(KeymapPlugin::new().for_context::<MissingDescriptionContext>());

        let diagnostics = &app
            .world()
            .resource::<crate::KeymapLoadFailures>()
            .retained_diagnostics;

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::Context
                && diagnostic.context == "flying"
                && diagnostic.message.contains("flying")
        }));
        assert_eq!(
            app.world().resource::<ConditionRegistry>().iter().count(),
            0
        );
    }

    #[test]
    fn duplicate_condition_names_are_rejected_without_replacing_the_first_registration() {
        let mut condition_registry = ConditionRegistry::default();

        assert!(
            condition_registry
                .register::<FirstDuplicateContext>()
                .is_ok()
        );
        let diagnostics = condition_registry
            .register::<SecondDuplicateContext>()
            .err();

        assert!(diagnostics.as_ref().is_some_and(|diagnostics| {
            diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == DiagnosticKind::Context
                    && diagnostic.context == "flying"
                    && diagnostic.message.contains("registered more than once")
            })
        }));
        assert_eq!(
            condition_registry
                .iter()
                .map(|condition_info| condition_info.description)
                .collect::<Vec<_>>(),
            vec!["While flying the first context"]
        );
    }

    #[test]
    fn resource_context_changes_update_the_active_condition_handle() {
        let mut app = App::new();
        app.insert_resource(ResourceContext::Flying)
            .add_plugins(KeymapPlugin::new().for_context::<ResourceContext>());

        app.update();
        assert_active_condition(&app, "flying");

        *app.world_mut().resource_mut::<ResourceContext>() = ResourceContext::Paused;
        app.update();
        assert_active_condition(&app, "paused");
    }

    #[test]
    fn state_context_changes_update_the_active_condition_handle() {
        let mut app = App::new();
        app.add_plugins(StatesPlugin)
            .insert_state(StateContext::Flying)
            .add_plugins(KeymapPlugin::new().for_state_context::<StateContext>());

        app.update();
        assert_active_condition(&app, "flying");

        app.world_mut()
            .resource_mut::<NextState<StateContext>>()
            .set(StateContext::Paused);
        app.update();
        app.update();
        assert_active_condition(&app, "paused");
    }

    #[test]
    fn missing_state_resource_leaves_the_active_condition_unchanged() -> Result<(), String> {
        let mut app = App::new();
        app.add_plugins(KeymapPlugin::new().for_state_context::<StateContext>());
        let paused = app
            .world()
            .resource::<ConditionRegistry>()
            .resolve("paused")
            .ok_or_else(|| "state context did not register paused".to_owned())?;
        app.world_mut()
            .resource_mut::<ActiveCondition>()
            .update(paused);

        app.update();

        assert_eq!(
            app.world().resource::<ActiveCondition>().handle(),
            Some(paused)
        );
        Ok(())
    }

    fn assert_keymap_context<C: KeymapContext>() {}

    fn assert_active_condition(app: &App, condition_name: &str) {
        let active_condition = app.world().resource::<ActiveCondition>();
        let condition_registry = app.world().resource::<ConditionRegistry>();
        let _: Option<ConditionHandle> = active_condition.handle;
        let expected_handle = condition_registry.resolve(condition_name);

        assert!(active_condition.is_initialized());
        assert!(expected_handle.is_some());
        assert_ne!(
            condition_registry.resolve("flying"),
            condition_registry.resolve("paused")
        );

        assert_eq!(active_condition.handle, expected_handle);
    }
}
