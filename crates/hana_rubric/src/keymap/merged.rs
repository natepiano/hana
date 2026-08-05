//! Semantic keymap layering before matcher construction.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::CompiledKeymap;
use super::Generation;
use super::KeymapDocument;
use super::document::BindingEdit;
use super::document::BindingSource;
use super::document::ContextExpr;
use super::document::ContextSource;
use crate::Capability;
use crate::CommandId;
use crate::CommandLookup;
use crate::CommandRegistry;
use crate::Diagnostic;
use crate::DiagnosticKind;
use crate::DiagnosticOrigin;
use crate::DiagnosticSeverity;
use crate::Keystroke;
use crate::KeystrokeSequence;
use crate::PrimaryTrigger;
use crate::condition::ConditionHandle;
use crate::condition::ConditionLookup;
use crate::condition::ConditionRegistry;

pub(super) const RECOGNIZED_BLOCK_MEMBERS: [&str; 2] = ["bindings", "context"];

/// The scope a keymap block's bindings take effect in.
///
/// [`BindingScope::Global`] is declared first so the derived [`Ord`] sorts every global binding
/// ahead of every conditioned one, which is what makes [`MergedKeymap::bindings`] report a
/// command's global keystroke when the same command is also bound inside a condition.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum BindingScope {
    Global,
    Condition(ConditionHandle),
}

/// The exact valid edits that remain after defaults and user keymaps are layered.
#[derive(Default)]
pub(crate) struct ResolvedEdits {
    entries: HashMap<BindingScope, HashMap<KeystrokeSequence, LayeredEdit>>,
}

impl ResolvedEdits {
    fn apply(
        &mut self,
        binding_scope: BindingScope,
        keystroke_sequence: KeystrokeSequence,
        source_layer: BindingSourceLayer,
        resolved_edit: ResolvedEdit,
    ) {
        match self
            .entries
            .entry(binding_scope)
            .or_default()
            .entry(keystroke_sequence)
        {
            Entry::Occupied(mut occupied) => occupied.get_mut().apply(source_layer, resolved_edit),
            Entry::Vacant(vacant) => {
                vacant.insert(LayeredEdit::from_layer(source_layer, resolved_edit));
            },
        }
    }

    /// Drops one layer's edit at a keystroke identity, restoring the layer beneath it.
    ///
    /// Repeated rejections of the same identity and layer are a no-op: a held binding that
    /// collides with several longer sequences is recorded once per collision.
    fn reject(
        &mut self,
        binding_scope: BindingScope,
        keystroke_sequence: &KeystrokeSequence,
        source_layer: BindingSourceLayer,
    ) {
        let Some(edits) = self.entries.get_mut(&binding_scope) else {
            return;
        };
        let Entry::Occupied(mut occupied) = edits.entry(keystroke_sequence.clone()) else {
            return;
        };

        if matches!(
            occupied.get_mut().reject_layer(source_layer),
            BindingRetention::Unbound
        ) {
            occupied.remove();
        }
    }

    fn global(&self) -> Option<&HashMap<KeystrokeSequence, LayeredEdit>> {
        self.entries.get(&BindingScope::Global)
    }

    fn for_condition(
        &self,
        condition_handle: ConditionHandle,
    ) -> Option<&HashMap<KeystrokeSequence, LayeredEdit>> {
        self.entries.get(&BindingScope::Condition(condition_handle))
    }
}

/// Every layer's edit at one keystroke identity, newest layer first.
///
/// A shipped default stays reachable underneath a user edit at the same identity so that
/// rejecting the user edit — as [`MergedKeymap::reject_held_prefixes`] does for a held prefix
/// that collides with a longer user sequence — restores the shipped binding instead of leaving
/// the identity unbound. Only a user [`ResolvedEdit::Tombstone`] removes the shipped binding.
#[derive(Clone)]
enum LayeredEdit {
    ShippedDefault(ResolvedEdit),
    User(ResolvedEdit),
    UserOverShippedDefault {
        user:            ResolvedEdit,
        shipped_default: ResolvedEdit,
    },
}

impl LayeredEdit {
    const fn from_layer(source_layer: BindingSourceLayer, resolved_edit: ResolvedEdit) -> Self {
        match source_layer {
            BindingSourceLayer::ShippedDefault => Self::ShippedDefault(resolved_edit),
            BindingSourceLayer::User => Self::User(resolved_edit),
        }
    }

    fn apply(&mut self, source_layer: BindingSourceLayer, resolved_edit: ResolvedEdit) {
        *self = match (source_layer, &*self) {
            (BindingSourceLayer::ShippedDefault, Self::ShippedDefault(_) | Self::User(_)) => {
                Self::ShippedDefault(resolved_edit)
            },
            (BindingSourceLayer::ShippedDefault, Self::UserOverShippedDefault { user, .. }) => {
                Self::UserOverShippedDefault {
                    user:            user.clone(),
                    shipped_default: resolved_edit,
                }
            },
            (BindingSourceLayer::User, Self::User(_)) => Self::User(resolved_edit),
            (
                BindingSourceLayer::User,
                Self::ShippedDefault(shipped_default)
                | Self::UserOverShippedDefault {
                    shipped_default, ..
                },
            ) => Self::UserOverShippedDefault {
                user:            resolved_edit,
                shipped_default: shipped_default.clone(),
            },
        };
    }

    /// The edit the keymap acts on: the user's when the user authored one, else the shipped one.
    const fn live(&self) -> &ResolvedEdit {
        match self {
            Self::ShippedDefault(resolved_edit) | Self::User(resolved_edit) => resolved_edit,
            Self::UserOverShippedDefault { user, .. } => user,
        }
    }

    /// Drops `source_layer`'s edit, leaving whatever the layer underneath still binds.
    fn reject_layer(&mut self, source_layer: BindingSourceLayer) -> BindingRetention {
        match (source_layer, &mut *self) {
            (BindingSourceLayer::ShippedDefault, Self::ShippedDefault(_))
            | (BindingSourceLayer::User, Self::User(_)) => BindingRetention::Unbound,
            (
                BindingSourceLayer::User,
                Self::UserOverShippedDefault {
                    shipped_default, ..
                },
            ) => {
                *self = Self::ShippedDefault(shipped_default.clone());
                BindingRetention::Layered
            },
            (BindingSourceLayer::ShippedDefault, Self::UserOverShippedDefault { user, .. }) => {
                *self = Self::User(user.clone());
                BindingRetention::Layered
            },
            (BindingSourceLayer::ShippedDefault, Self::User(_))
            | (BindingSourceLayer::User, Self::ShippedDefault(_)) => BindingRetention::Layered,
        }
    }
}

/// Whether a keystroke identity still binds a command once one layer's edit is rejected.
enum BindingRetention {
    Layered,
    Unbound,
}

#[derive(Clone)]
enum ResolvedEdit {
    Bind(ResolvedBinding),
    Tombstone,
}

#[derive(Clone)]
struct ResolvedBinding {
    command_id:        CommandId,
    source:            BindingSource,
    source_layer:      BindingSourceLayer,
    diagnostic_origin: DiagnosticOrigin,
}

#[derive(Clone)]
struct ScopedBinding {
    keystroke_sequence: KeystrokeSequence,
    binding:            ResolvedBinding,
    binding_scope:      BindingScope,
}

/// One held-prefix collision participant to drop, named down to the layer that authored it so
/// the layer beneath survives.
struct RejectedBinding {
    keystroke_sequence: KeystrokeSequence,
    binding_scope:      BindingScope,
    source_layer:       BindingSourceLayer,
}

impl From<&ScopedBinding> for RejectedBinding {
    fn from(scoped_binding: &ScopedBinding) -> Self {
        Self {
            keystroke_sequence: scoped_binding.keystroke_sequence.clone(),
            binding_scope:      scoped_binding.binding_scope,
            source_layer:       scoped_binding.binding.source_layer,
        }
    }
}

/// The keymap layer that authored a resolved binding.
#[derive(Clone, Copy, Eq, PartialEq)]
enum BindingSourceLayer {
    ShippedDefault,
    User,
}

/// Whether the two participants in a held-prefix collision were authored by the same keymap layer.
///
/// A valid hold takes precedence over a longer sequence from the *other* layer, whichever layer the
/// hold is in, so only the sequence is rejected. A collision inside one layer rejects every
/// participant, because neither edit can be read as overriding the other.
enum HeldPrefixConflict {
    AcrossLayers,
    WithinOneLayer,
}

impl HeldPrefixConflict {
    const fn between(held: BindingSourceLayer, other: BindingSourceLayer) -> Self {
        match (held, other) {
            (BindingSourceLayer::ShippedDefault, BindingSourceLayer::User)
            | (BindingSourceLayer::User, BindingSourceLayer::ShippedDefault) => Self::AcrossLayers,
            (BindingSourceLayer::ShippedDefault, BindingSourceLayer::ShippedDefault)
            | (BindingSourceLayer::User, BindingSourceLayer::User) => Self::WithinOneLayer,
        }
    }
}

/// Which scope a keymap block's `context` member resolved to, or that it named no usable one.
enum ContextResolution {
    Resolved(BindingScope),
    Invalid,
}

/// The user keymap layered over the embedded defaults, or its absence.
///
/// [`UserKeymap::DefaultsOnly`] covers every situation that leaves nothing to layer — a
/// defaults-only startup commit, a missing user keymap file, and a user keymap file that is not
/// UTF-8 — because the merge treats all three the same way.
pub(crate) enum UserKeymap {
    Layered {
        origin:   DiagnosticOrigin,
        contents: String,
    },
    DefaultsOnly,
}

/// The parsed user keymap layered over the parsed defaults, or its absence.
///
/// The document form of [`UserKeymap`], named the same way because it carries the same two states
/// one parse later.
pub(crate) enum UserKeymapDocument<'document> {
    Layered(&'document KeymapDocument),
    DefaultsOnly,
}

/// Live command bindings after global fallback and per-condition tombstones are resolved.
pub(crate) struct MergedKeymap {
    global:                Vec<(KeystrokeSequence, CommandId)>,
    pub(super) conditions: HashMap<ConditionHandle, Vec<(KeystrokeSequence, CommandId)>>,
}

impl MergedKeymap {
    const MAX_COMMAND_EDIT_DISTANCE: usize = 3;
    const MAX_COMMAND_SUGGESTIONS: usize = 3;

    /// Parses, validates, and layers defaults followed by an optional user keymap.
    ///
    /// # Errors
    ///
    /// Returns every diagnostic collected before a document-level parse failure, followed by the
    /// failing document's own diagnostics.
    /// Per-binding diagnostics are returned with the successfully merged keymap.
    pub(crate) fn from_sources(
        defaults_diagnostic_origin: &DiagnosticOrigin,
        defaults_source: &str,
        user_keymap: &UserKeymap,
        command_registry: &CommandRegistry,
        condition_registry: &ConditionRegistry,
        protected_keystrokes: &[Keystroke],
    ) -> Result<(Self, Vec<Diagnostic>), Vec<Diagnostic>> {
        let (defaults, mut diagnostics) =
            KeymapDocument::parse(defaults_diagnostic_origin, defaults_source)?;
        let user = match user_keymap {
            UserKeymap::Layered { origin, contents } => {
                let (user, user_diagnostics) = match KeymapDocument::parse(origin, contents) {
                    Ok(parsed) => parsed,
                    Err(mut user_diagnostics) => {
                        diagnostics.append(&mut user_diagnostics);
                        return Err(diagnostics);
                    },
                };
                diagnostics.extend(user_diagnostics);
                Some(user)
            },
            UserKeymap::DefaultsOnly => None,
        };
        let user_keymap_document = user.as_ref().map_or(
            UserKeymapDocument::DefaultsOnly,
            UserKeymapDocument::Layered,
        );
        let (merged_keymap, merge_diagnostics) = Self::from_documents(
            &defaults,
            user_keymap_document,
            command_registry,
            condition_registry,
            protected_keystrokes,
        );
        diagnostics.extend(merge_diagnostics);

        Ok((merged_keymap, diagnostics))
    }

    /// Validates and layers already parsed defaults followed by an optional user keymap.
    #[must_use]
    pub(crate) fn from_documents(
        defaults: &KeymapDocument,
        user_keymap_document: UserKeymapDocument<'_>,
        command_registry: &CommandRegistry,
        condition_registry: &ConditionRegistry,
        protected_keystrokes: &[Keystroke],
    ) -> (Self, Vec<Diagnostic>) {
        let mut diagnostics = Vec::new();
        let mut resolved_edits = ResolvedEdits::default();

        Self::apply_document(
            defaults,
            BindingSourceLayer::ShippedDefault,
            &mut resolved_edits,
            &mut diagnostics,
            command_registry,
            condition_registry,
            protected_keystrokes,
        );
        if let UserKeymapDocument::Layered(user) = user_keymap_document {
            Self::apply_document(
                user,
                BindingSourceLayer::User,
                &mut resolved_edits,
                &mut diagnostics,
                command_registry,
                condition_registry,
                protected_keystrokes,
            );
        }
        Self::reject_held_prefixes(
            &mut resolved_edits,
            command_registry,
            condition_registry,
            &mut diagnostics,
        );

        (
            Self::from_resolved_edits(resolved_edits, condition_registry),
            diagnostics,
        )
    }

    /// Constructs the matchers and command table for one replacement generation.
    #[must_use]
    pub(crate) fn compile(
        &self,
        generation: Generation,
        command_registry: &CommandRegistry,
    ) -> CompiledKeymap {
        CompiledKeymap::from_merged(generation, self, command_registry)
    }

    fn apply_document(
        document: &KeymapDocument,
        source_layer: BindingSourceLayer,
        resolved_edits: &mut ResolvedEdits,
        diagnostics: &mut Vec<Diagnostic>,
        command_registry: &CommandRegistry,
        condition_registry: &ConditionRegistry,
        protected_keystrokes: &[Keystroke],
    ) {
        for block in &document.blocks {
            diagnostics.extend(block.unrecognized_members.iter().map(|member| {
                let suggestion = Self::closest_block_member(&member.name);

                Diagnostic {
                    origin:             document.diagnostic_origin.clone(),
                    byte_range:         member.byte_range.clone(),
                    line:               member.line,
                    column:             member.column,
                    block_index:        member.block_index,
                    context:            String::new(),
                    original_keystroke: String::new(),
                    command_id:         String::new(),
                    kind:               DiagnosticKind::Syntax,
                    severity:           DiagnosticSeverity::Advisory,
                    message:            format!(
                        "Unrecognized keymap block member `{}`. Did you mean `{suggestion}`?",
                        member.name
                    ),
                    suggestions:        vec![suggestion.to_owned()],
                }
            }));

            let binding_scope = match Self::resolve_context(
                block.context.as_ref(),
                block.context_source.as_ref(),
                document,
                condition_registry,
                diagnostics,
            ) {
                ContextResolution::Resolved(binding_scope) => binding_scope,
                ContextResolution::Invalid => continue,
            };

            for binding in &block.bindings {
                let Some(resolved_edit) = Self::resolve_binding(
                    binding.edit.clone(),
                    &binding.keystroke_sequence,
                    &binding.source,
                    source_layer,
                    &document.diagnostic_origin,
                    command_registry,
                    protected_keystrokes,
                    diagnostics,
                ) else {
                    continue;
                };

                resolved_edits.apply(
                    binding_scope,
                    binding.keystroke_sequence.clone(),
                    source_layer,
                    resolved_edit,
                );
            }
        }
    }

    fn resolve_context(
        context: Option<&ContextExpr>,
        context_source: Option<&ContextSource>,
        document: &KeymapDocument,
        condition_registry: &ConditionRegistry,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> ContextResolution {
        match (context, context_source) {
            (None, None) => ContextResolution::Resolved(BindingScope::Global),
            (None, Some(context_source)) => {
                diagnostics.push(Diagnostic {
                    origin: document.diagnostic_origin.clone(),
                    byte_range:         context_source.byte_range.clone(),
                    line:               context_source.line,
                    column:             context_source.column,
                    block_index:        context_source.block_index,
                    context:            String::new(),
                    original_keystroke: String::new(),
                    command_id:         String::new(),
                    kind:               DiagnosticKind::Context,
                    severity:           DiagnosticSeverity::Advisory,
                    message: String::from(
                        "A keymap block `context` value must be a string naming a registered condition.",
                    ),
                    suggestions:        Vec::new(),
                });

                ContextResolution::Invalid
            },
            (Some(ContextExpr::Name(condition_name)), Some(context_source)) => {
                if let ConditionLookup::Registered { handle, .. } =
                    condition_registry.lookup(condition_name.as_str())
                {
                    ContextResolution::Resolved(BindingScope::Condition(handle))
                } else {
                    let names = condition_registry
                        .iter()
                        .map(|condition_info| condition_info.name.as_str())
                        .collect::<Vec<_>>();
                    let message = if names.is_empty() {
                        format!(
                            "Keymap context `{}` is not registered by the application.",
                            condition_name.as_str()
                        )
                    } else {
                        format!(
                            "Keymap context `{}` is not registered. Registered contexts: {}.",
                            condition_name.as_str(),
                            names.join(", ")
                        )
                    };

                    diagnostics.push(Diagnostic {
                        origin: document.diagnostic_origin.clone(),
                        byte_range: context_source.byte_range.clone(),
                        line: context_source.line,
                        column: context_source.column,
                        block_index: context_source.block_index,
                        context: condition_name.as_str().to_owned(),
                        original_keystroke: String::new(),
                        command_id: String::new(),
                        kind: DiagnosticKind::Context,
                        severity: DiagnosticSeverity::Failure,
                        message,
                        suggestions: names.into_iter().map(str::to_owned).collect(),
                    });

                    ContextResolution::Invalid
                }
            },
            (Some(ContextExpr::Name(condition_name)), None) => {
                diagnostics.push(Diagnostic {
                    origin:             document.diagnostic_origin.clone(),
                    byte_range:         0..0,
                    line:               0,
                    column:             0,
                    block_index:        0,
                    context:            condition_name.as_str().to_owned(),
                    original_keystroke: String::new(),
                    command_id:         String::new(),
                    kind:               DiagnosticKind::Context,
                    severity:           DiagnosticSeverity::Failure,
                    message:            format!(
                        "Keymap context `{}` has no source location.",
                        condition_name.as_str()
                    ),
                    suggestions:        Vec::new(),
                });

                ContextResolution::Invalid
            },
        }
    }

    fn resolve_binding(
        edit: BindingEdit,
        keystroke_sequence: &KeystrokeSequence,
        binding_source: &BindingSource,
        source_layer: BindingSourceLayer,
        diagnostic_origin: &DiagnosticOrigin,
        command_registry: &CommandRegistry,
        protected_keystrokes: &[Keystroke],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<ResolvedEdit> {
        if let Some(protected_keystroke) = protected_keystrokes
            .iter()
            .find(|protected_keystroke| **protected_keystroke == keystroke_sequence.first())
        {
            diagnostics.push(
                binding_source.diagnostic(
                    diagnostic_origin,
                    String::new(),
                    DiagnosticKind::ReservedKeystroke,
                    DiagnosticSeverity::Failure,
                    format!(
                        "Keystroke 1 `{protected_keystroke}` is reserved for the application's recovery command and cannot start a keymap sequence."
                    ),
                ),
            );

            return None;
        }

        match edit {
            BindingEdit::Unbind => Some(ResolvedEdit::Tombstone),
            BindingEdit::Bind(command_id) => {
                let CommandLookup::Found(command_info) = command_registry.lookup(&command_id)
                else {
                    let suggestions = Self::closest_command_ids(&command_id, command_registry);
                    let message = suggestions.first().map_or_else(
                        || format!("Command `{command_id}` is not registered."),
                        |suggestion| {
                            format!(
                                "Command `{command_id}` is not registered. Did you mean `{suggestion}`?"
                            )
                        },
                    );
                    let mut diagnostic = binding_source.command_diagnostic(
                        diagnostic_origin,
                        command_id.to_string(),
                        DiagnosticKind::Command,
                        DiagnosticSeverity::Failure,
                        message,
                    );
                    diagnostic.suggestions = suggestions;
                    diagnostics.push(diagnostic);

                    return None;
                };
                let capability = command_info.capability;

                if let Some((index, modifier_family)) = keystroke_sequence
                    .iter()
                    .enumerate()
                    .find_map(|(index, keystroke)| match keystroke.primary_trigger() {
                        PrimaryTrigger::ModifierFamily(modifier_family) => {
                            Some((index, modifier_family))
                        },
                        PrimaryTrigger::OrdinaryKey(_) => None,
                    })
                    && (capability != Capability::Held || keystroke_sequence.len() != 1)
                {
                    diagnostics.push(binding_source.diagnostic(
                        diagnostic_origin,
                        command_id.to_string(),
                        DiagnosticKind::BareModifierRequiresHeldCommand,
                        DiagnosticSeverity::Failure,
                        format!(
                            "Bare modifier keystroke {} `{modifier_family}` can only be the sole keystroke bound to a hold-to-act command.",
                            index + 1
                        ),
                    ));

                    return None;
                }

                if capability == Capability::Held && keystroke_sequence.len() > 1 {
                    diagnostics.push(binding_source.diagnostic(
                        diagnostic_origin,
                        command_id.to_string(),
                        DiagnosticKind::HeldCommandInSequence,
                        DiagnosticSeverity::Failure,
                        format!(
                            "Hold-to-act command `{command_id}` must use exactly one keystroke."
                        ),
                    ));

                    return None;
                }

                if capability == Capability::Unremappable {
                    diagnostics.push(binding_source.diagnostic(
                        diagnostic_origin,
                        command_id.to_string(),
                        DiagnosticKind::UnremappableCommand,
                        DiagnosticSeverity::Failure,
                        format!("Command `{command_id}` is reserved for recovery."),
                    ));

                    return None;
                }

                Some(ResolvedEdit::Bind(ResolvedBinding {
                    command_id,
                    source: binding_source.clone(),
                    source_layer,
                    diagnostic_origin: diagnostic_origin.clone(),
                }))
            },
        }
    }

    fn reject_held_prefixes(
        resolved_edits: &mut ResolvedEdits,
        command_registry: &CommandRegistry,
        condition_registry: &ConditionRegistry,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let global_bindings = Self::scoped_bindings(resolved_edits.global(), BindingScope::Global);
        let mut rejected_bindings = Vec::new();
        Self::collect_held_prefix_rejections(
            &global_bindings,
            command_registry,
            false,
            diagnostics,
            &mut rejected_bindings,
        );

        for condition_info in condition_registry.iter() {
            let ConditionLookup::Registered {
                handle: condition_handle,
                ..
            } = condition_registry.lookup(condition_info.name.as_str())
            else {
                continue;
            };
            let mut bindings = global_bindings.clone();

            if let Some(condition_edits) = resolved_edits.for_condition(condition_handle) {
                for (keystroke_sequence, layered_edit) in condition_edits {
                    bindings
                        .retain(|candidate| candidate.keystroke_sequence != *keystroke_sequence);

                    if let ResolvedEdit::Bind(binding) = layered_edit.live() {
                        bindings.push(ScopedBinding {
                            keystroke_sequence: keystroke_sequence.clone(),
                            binding:            binding.clone(),
                            binding_scope:      BindingScope::Condition(condition_handle),
                        });
                    }
                }
            }

            Self::collect_held_prefix_rejections(
                &bindings,
                command_registry,
                true,
                diagnostics,
                &mut rejected_bindings,
            );
        }

        for rejected_binding in rejected_bindings {
            resolved_edits.reject(
                rejected_binding.binding_scope,
                &rejected_binding.keystroke_sequence,
                rejected_binding.source_layer,
            );
        }
    }

    fn scoped_bindings(
        edits: Option<&HashMap<KeystrokeSequence, LayeredEdit>>,
        binding_scope: BindingScope,
    ) -> Vec<ScopedBinding> {
        edits.map_or_else(Vec::new, |edits| {
            edits
                .iter()
                .filter_map(
                    |(keystroke_sequence, layered_edit)| match layered_edit.live() {
                        ResolvedEdit::Bind(binding) => Some(ScopedBinding {
                            keystroke_sequence: keystroke_sequence.clone(),
                            binding: binding.clone(),
                            binding_scope,
                        }),
                        ResolvedEdit::Tombstone => None,
                    },
                )
                .collect()
        })
    }

    fn collect_held_prefix_rejections(
        bindings: &[ScopedBinding],
        command_registry: &CommandRegistry,
        is_condition_scope: bool,
        diagnostics: &mut Vec<Diagnostic>,
        rejected_bindings: &mut Vec<RejectedBinding>,
    ) {
        for held_binding in bindings {
            let is_held = matches!(
                command_registry.lookup(&held_binding.binding.command_id),
                CommandLookup::Found(command_info) if command_info.capability == Capability::Held
            );
            if !is_held || held_binding.keystroke_sequence.len() != 1 {
                continue;
            }

            for other_binding in bindings {
                if (is_condition_scope
                    && held_binding.binding_scope == BindingScope::Global
                    && other_binding.binding_scope == BindingScope::Global)
                    || other_binding.keystroke_sequence.len() <= 1
                    || other_binding.keystroke_sequence.first()
                        != held_binding.keystroke_sequence.first()
                {
                    continue;
                }

                match HeldPrefixConflict::between(
                    held_binding.binding.source_layer,
                    other_binding.binding.source_layer,
                ) {
                    HeldPrefixConflict::AcrossLayers => {
                        diagnostics.push(other_binding.binding.source.diagnostic(
                        &other_binding.binding.diagnostic_origin,
                        other_binding.binding.command_id.to_string(),
                        DiagnosticKind::HeldCommandInSequence,
                        DiagnosticSeverity::Failure,
                        format!(
                            "Multi-stroke binding `{}` in `{}` shares its prefix with hold-to-act binding `{}` in `{}`.",
                            other_binding.binding.command_id,
                            other_binding.binding.diagnostic_origin,
                            held_binding.binding.command_id,
                            held_binding.binding.diagnostic_origin,
                        ),
                    ));
                    },
                    HeldPrefixConflict::WithinOneLayer => {
                        diagnostics.push(held_binding.binding.source.diagnostic(
                        &held_binding.binding.diagnostic_origin,
                        held_binding.binding.command_id.to_string(),
                        DiagnosticKind::HeldCommandInSequence,
                        DiagnosticSeverity::Failure,
                        format!(
                            "Hold-to-act command `{}` in `{}` shares its keystroke with multi-stroke binding `{}` in `{}`.",
                            held_binding.binding.command_id,
                            held_binding.binding.diagnostic_origin,
                            other_binding.binding.command_id,
                            other_binding.binding.diagnostic_origin,
                        ),
                    ));
                        rejected_bindings.push(RejectedBinding::from(held_binding));
                    },
                }
                rejected_bindings.push(RejectedBinding::from(other_binding));
            }
        }
    }

    fn from_resolved_edits(
        resolved_edits: ResolvedEdits,
        condition_registry: &ConditionRegistry,
    ) -> Self {
        let global = Self::live_bindings(resolved_edits.global());
        let mut conditions = HashMap::new();

        for condition_info in condition_registry.iter() {
            let ConditionLookup::Registered {
                handle: condition_handle,
                ..
            } = condition_registry.lookup(condition_info.name.as_str())
            else {
                continue;
            };
            let mut bindings = global.clone();

            if let Some(condition_edits) = resolved_edits.for_condition(condition_handle) {
                for (keystroke_sequence, layered_edit) in condition_edits {
                    match layered_edit.live() {
                        ResolvedEdit::Bind(binding) => {
                            bindings.insert(keystroke_sequence.clone(), binding.command_id.clone());
                        },
                        ResolvedEdit::Tombstone => {
                            bindings.remove(keystroke_sequence);
                        },
                    }
                }
            }

            conditions.insert(condition_handle, bindings.into_iter().collect());
        }

        Self {
            global: global.into_iter().collect(),
            conditions,
        }
    }

    fn live_bindings(
        edits: Option<&HashMap<KeystrokeSequence, LayeredEdit>>,
    ) -> HashMap<KeystrokeSequence, CommandId> {
        edits.map_or_else(HashMap::new, |edits| {
            edits
                .iter()
                .filter_map(
                    |(keystroke_sequence, layered_edit)| match layered_edit.live() {
                        ResolvedEdit::Bind(binding) => {
                            Some((keystroke_sequence.clone(), binding.command_id.clone()))
                        },
                        ResolvedEdit::Tombstone => None,
                    },
                )
                .collect()
        })
    }

    fn closest_block_member(member_name: &str) -> &'static str {
        RECOGNIZED_BLOCK_MEMBERS
            .iter()
            .min_by_key(|candidate| levenshtein(member_name, candidate))
            .copied()
            .unwrap_or("bindings")
    }

    fn closest_command_ids(
        command_id: &CommandId,
        command_registry: &CommandRegistry,
    ) -> Vec<String> {
        let mut candidates = command_registry
            .iter()
            .filter_map(|command_info| {
                bounded_levenshtein(
                    command_id.as_str(),
                    command_info.id.as_str(),
                    Self::MAX_COMMAND_EDIT_DISTANCE,
                )
                .map(|distance| (distance, command_info.id.as_str()))
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();

        candidates
            .into_iter()
            .take(Self::MAX_COMMAND_SUGGESTIONS)
            .map(|(_, candidate)| candidate.to_owned())
            .collect()
    }

    pub(super) fn global(&self) -> &[(KeystrokeSequence, CommandId)] { &self.global }

    /// Every resolved binding, global bindings first and then each condition's
    /// bindings in the order the condition registry issued its handles.
    ///
    /// A command bound both globally and inside a condition therefore reports
    /// its global keystroke, and a command bound only under two conditions
    /// reports the one registered first rather than whichever the condition map
    /// happened to hash ahead of the other.
    pub(super) fn bindings(&self) -> impl Iterator<Item = (&KeystrokeSequence, &CommandId)> {
        let mut binding_scopes = std::iter::once(BindingScope::Global)
            .chain(self.conditions.keys().copied().map(BindingScope::Condition))
            .collect::<Vec<_>>();
        binding_scopes.sort_unstable();

        binding_scopes
            .into_iter()
            .flat_map(|binding_scope| match binding_scope {
                BindingScope::Global => self.global.as_slice(),
                BindingScope::Condition(condition_handle) => self
                    .conditions
                    .get(&condition_handle)
                    .map_or(&[][..], Vec::as_slice),
            })
            .map(|(keystroke_sequence, command_id)| (keystroke_sequence, command_id))
    }

    #[cfg(test)]
    pub(super) fn for_condition(
        &self,
        condition_handle: ConditionHandle,
    ) -> Option<&[(KeystrokeSequence, CommandId)]> {
        self.conditions.get(&condition_handle).map(Vec::as_slice)
    }
}

fn levenshtein(left: &str, right: &str) -> usize {
    bounded_levenshtein(left, right, usize::MAX).unwrap_or(usize::MAX)
}

fn bounded_levenshtein(left: &str, right: &str, maximum_distance: usize) -> Option<usize> {
    if left.len().abs_diff(right.len()) > maximum_distance {
        return None;
    }

    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];

    for (left_index, left_byte) in left.bytes().enumerate() {
        current[0] = left_index + 1;
        let mut smallest = current[0];

        for (right_index, right_byte) in right.bytes().enumerate() {
            let substitution_cost = usize::from(left_byte != right_byte);
            let insertion = current[right_index] + 1;
            let deletion = previous[right_index + 1] + 1;
            let substitution = previous[right_index] + substitution_cost;
            let distance = insertion.min(deletion).min(substitution);
            current[right_index + 1] = distance;
            smallest = smallest.min(distance);
        }

        if smallest > maximum_distance {
            return None;
        }

        std::mem::swap(&mut previous, &mut current);
    }

    let distance = previous[right.len()];
    (distance <= maximum_distance).then_some(distance)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::time::Duration;
    use std::time::Instant;

    use bevy::prelude::Event;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy::reflect::TypeRegistry;
    use bevy_enhanced_input::prelude::CustomInputs;
    use strum::AsRefStr;
    use strum::EnumIter;
    use strum::EnumMessage;

    use super::Generation;
    use super::MergedKeymap;
    use super::UserKeymap;
    use crate::Capability;
    use crate::CommandId;
    use crate::CommandKeystroke;
    use crate::CommandRegistry;
    use crate::Diagnostic;
    use crate::DiagnosticKind;
    use crate::DiagnosticOrigin;
    use crate::DiagnosticSeverity;
    use crate::HoldPhase;
    use crate::KeymapBindings;
    use crate::KeymapCommand;
    use crate::Keystroke;
    use crate::KeystrokeSequence;
    use crate::MatchOutcome;
    use crate::ReflectKeymapCommand;
    use crate::TimeoutOutcome;
    use crate::condition::ConditionHandle;
    use crate::condition::ConditionLookup;
    use crate::condition::ConditionRegistry;

    const DEFAULTS_PATH: &str = "defaults.jsonc";
    const USER_PATH: &str = "keymap.jsonc";
    const MATCH_TIMEOUT: Duration = Duration::from_secs(1);

    fn defaults_keymap_file() -> DiagnosticOrigin {
        DiagnosticOrigin::KeymapFile(PathBuf::from(DEFAULTS_PATH))
    }

    fn user_keymap_file() -> DiagnosticOrigin {
        DiagnosticOrigin::KeymapFile(PathBuf::from(USER_PATH))
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq)]
    #[strum(serialize_all = "snake_case")]
    enum TestContext {
        #[strum(message = "While a dimension lock is active")]
        DimensionLock,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq)]
    #[strum(serialize_all = "snake_case")]
    enum TestPaletteContext {
        #[strum(message = "While the command palette is open")]
        PaletteOpen,
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct CameraHome;

    impl KeymapCommand for CameraHome {
        const ID: &'static str = "camera::home";
        const TITLE: &'static str = "Camera Home";
        const DESCRIPTION: &'static str = "Returns the camera to its home position.";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct CameraReset;

    impl KeymapCommand for CameraReset {
        const ID: &'static str = "camera::reset";
        const TITLE: &'static str = "Camera Reset";
        const DESCRIPTION: &'static str = "Resets the camera position.";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct CameraHold;

    impl KeymapCommand for CameraHold {
        const ID: &'static str = "camera::hold";
        const TITLE: &'static str = "Camera Hold";
        const DESCRIPTION: &'static str = "Holds the camera action active.";
        const CAPABILITY: Capability = Capability::Held;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { Some(HoldPhase::Begin) }
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct RecoveryOpen;

    impl KeymapCommand for RecoveryOpen {
        const ID: &'static str = "recovery::open";
        const TITLE: &'static str = "Recovery Open";
        const DESCRIPTION: &'static str = "Opens the recovery command.";
        const CAPABILITY: Capability = Capability::Unremappable;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    fn command_registry() -> Result<CommandRegistry, String> {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<CameraHold>();
        type_registry.register::<CameraHome>();
        type_registry.register::<CameraReset>();
        type_registry.register::<RecoveryOpen>();
        let mut custom_inputs = CustomInputs::default();

        CommandRegistry::build(&type_registry, &mut custom_inputs)
            .map_err(|diagnostics| format!("command registry errors: {diagnostics:?}"))
    }

    fn condition_registry() -> Result<ConditionRegistry, String> {
        let mut condition_registry = ConditionRegistry::default();
        condition_registry
            .register::<TestContext>()
            .map_err(|diagnostics| format!("condition registry errors: {diagnostics:?}"))?;

        Ok(condition_registry)
    }

    fn merged_keymap(
        defaults: &str,
        user: Option<&str>,
        protected_keystrokes: &[Keystroke],
    ) -> Result<
        (
            MergedKeymap,
            Vec<Diagnostic>,
            CommandRegistry,
            ConditionRegistry,
        ),
        String,
    > {
        let command_registry = command_registry()?;
        let condition_registry = condition_registry()?;
        let user_keymap = user.map_or(UserKeymap::DefaultsOnly, |source| UserKeymap::Layered {
            origin:   user_keymap_file(),
            contents: source.to_owned(),
        });
        let (merged_keymap, diagnostics) = MergedKeymap::from_sources(
            &defaults_keymap_file(),
            defaults,
            &user_keymap,
            &command_registry,
            &condition_registry,
            protected_keystrokes,
        )
        .map_err(|diagnostics| format!("keymap parse errors: {diagnostics:?}"))?;

        Ok((
            merged_keymap,
            diagnostics,
            command_registry,
            condition_registry,
        ))
    }

    fn dimension_lock_handle(
        condition_registry: &ConditionRegistry,
    ) -> Result<ConditionHandle, String> {
        match condition_registry.lookup("dimension_lock") {
            ConditionLookup::Registered { handle, .. } => Ok(handle),
            ConditionLookup::UnregisteredName => {
                Err(String::from("dimension_lock condition was not registered"))
            },
        }
    }

    fn command_for_sequence<'bindings>(
        bindings: &'bindings [(KeystrokeSequence, crate::CommandId)],
        source: &str,
    ) -> Result<Option<&'bindings str>, String> {
        let keystroke_sequence = source
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid test sequence: {error}"))?;

        Ok(bindings
            .iter()
            .find(|(binding_sequence, _)| *binding_sequence == keystroke_sequence)
            .map(|(_, command_id)| command_id.as_str()))
    }

    fn condition_bindings(
        merged_keymap: &MergedKeymap,
        condition_handle: ConditionHandle,
    ) -> Result<&[(KeystrokeSequence, crate::CommandId)], String> {
        merged_keymap
            .for_condition(condition_handle)
            .ok_or_else(|| String::from("condition matcher bindings were not compiled"))
    }

    fn keystroke_sequence(source: &str) -> Result<KeystrokeSequence, String> {
        source
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid test sequence: {error}"))
    }

    /// The keystroke [`MergedKeymap::bindings`] reports first for `camera::home`,
    /// which the defaults bind under two conditions and nowhere globally.
    fn first_reported_home_keystroke(
        condition_registry: &ConditionRegistry,
    ) -> Result<KeystrokeSequence, String> {
        let defaults = r#"{
            "bindings": [
                { "context": "dimension_lock", "bindings": { "space": "camera::home" }},
                { "context": "palette_open", "bindings": { "enter": "camera::home" }}
            ]
        }"#;
        let command_registry = command_registry()?;
        let (merged_keymap, _) = MergedKeymap::from_sources(
            &defaults_keymap_file(),
            defaults,
            &UserKeymap::DefaultsOnly,
            &command_registry,
            condition_registry,
            &[],
        )
        .map_err(|diagnostics| format!("keymap parse errors: {diagnostics:?}"))?;

        merged_keymap
            .bindings()
            .find(|(_, command_id)| command_id.as_str() == CameraHome::ID)
            .map(|(keystroke_sequence, _)| keystroke_sequence.clone())
            .ok_or_else(|| String::from("camera::home was not reported by any binding"))
    }

    #[test]
    fn a_command_bound_only_under_conditions_reports_the_first_registered_condition()
    -> Result<(), String> {
        let mut dimension_lock_first = ConditionRegistry::default();
        dimension_lock_first
            .register::<TestContext>()
            .map_err(|diagnostics| format!("condition registry errors: {diagnostics:?}"))?;
        dimension_lock_first
            .register::<TestPaletteContext>()
            .map_err(|diagnostics| format!("condition registry errors: {diagnostics:?}"))?;
        let mut palette_open_first = ConditionRegistry::default();
        palette_open_first
            .register::<TestPaletteContext>()
            .map_err(|diagnostics| format!("condition registry errors: {diagnostics:?}"))?;
        palette_open_first
            .register::<TestContext>()
            .map_err(|diagnostics| format!("condition registry errors: {diagnostics:?}"))?;

        assert_eq!(
            first_reported_home_keystroke(&dimension_lock_first)?,
            keystroke_sequence("space")?
        );
        assert_eq!(
            first_reported_home_keystroke(&palette_open_first)?,
            keystroke_sequence("enter")?
        );
        Ok(())
    }

    #[test]
    fn repeated_construction_reports_the_same_conditioned_keystroke() -> Result<(), String> {
        let mut condition_registry = ConditionRegistry::default();
        condition_registry
            .register::<TestContext>()
            .map_err(|diagnostics| format!("condition registry errors: {diagnostics:?}"))?;
        condition_registry
            .register::<TestPaletteContext>()
            .map_err(|diagnostics| format!("condition registry errors: {diagnostics:?}"))?;

        assert_eq!(
            first_reported_home_keystroke(&condition_registry)?,
            first_reported_home_keystroke(&condition_registry)?
        );
        Ok(())
    }

    /// [`BindingScope::Global`] sorting ahead of [`BindingScope::Condition`] is what makes
    /// [`KeymapBindings`] report the global keystroke; reversing the two variants' declaration
    /// order reverses the derived [`Ord`] and fails this assertion.
    #[test]
    fn a_command_bound_globally_and_under_a_condition_reports_its_global_keystroke()
    -> Result<(), String> {
        let defaults = r#"{
            "bindings": [
                { "bindings": { "space": "camera::home" }},
                { "context": "dimension_lock", "bindings": { "enter": "camera::home" }}
            ]
        }"#;
        let (merged_keymap, _, _, _) = merged_keymap(defaults, None, &[])?;
        let keymap_bindings = KeymapBindings::from_bindings(merged_keymap.bindings());
        let command_id = CommandId::from_str(CameraHome::ID)
            .map_err(|error| format!("invalid test command id: {error}"))?;
        let global_keystroke = keystroke_sequence("space")?;

        assert_eq!(
            keymap_bindings.keystroke(&command_id),
            CommandKeystroke::BoundTo(&global_keystroke)
        );
        Ok(())
    }

    #[test]
    fn user_edit_overrides_the_default_at_the_same_conditioned_identity() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "context": "dimension_lock",
                "bindings": { "space": "camera::home" }
            }]
        }"#;
        let user = r#"{
            "bindings": [{
                "context": "dimension_lock",
                "bindings": { "space": "camera::reset" }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, condition_registry) =
            merged_keymap(defaults, Some(user), &[])?;
        let condition_handle = dimension_lock_handle(&condition_registry)?;

        assert!(diagnostics.is_empty());
        assert_eq!(
            command_for_sequence(
                condition_bindings(&merged_keymap, condition_handle)?,
                "space"
            )?,
            Some("camera::reset")
        );

        Ok(())
    }

    #[test]
    fn invalid_user_edit_leaves_the_default_binding_in_place() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "g": "camera::home" } }] }"#;
        let user = r#"{ "bindings": [{ "bindings": { "g": "camera::missing" } }] }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, Some(user), &[])?;

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::Command
                && diagnostic.severity == DiagnosticSeverity::Failure
                && diagnostic.command_id == "camera::missing"
        }));
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "g")?,
            Some("camera::home")
        );

        Ok(())
    }

    #[test]
    fn user_document_failure_keeps_earlier_defaults_diagnostics() -> Result<(), String> {
        let command_registry = command_registry()?;
        let condition_registry = condition_registry()?;
        let defaults = r#"{ "bindings": [{ "bindings": { "g": "camera:missing" } }] }"#;
        let user = r#"{ "bindings": [}"#;
        let Err(diagnostics) = MergedKeymap::from_sources(
            &defaults_keymap_file(),
            defaults,
            &UserKeymap::Layered {
                origin:   user_keymap_file(),
                contents: user.to_owned(),
            },
            &command_registry,
            &condition_registry,
            &[],
        ) else {
            return Err(String::from("invalid user document unexpectedly loaded"));
        };

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].origin, defaults_keymap_file());
        assert_eq!(diagnostics[0].kind, DiagnosticKind::Command);
        assert_eq!(diagnostics[0].command_id, "camera:missing");
        assert_eq!(diagnostics[1].origin, user_keymap_file());
        assert_eq!(diagnostics[1].kind, DiagnosticKind::Syntax);

        Ok(())
    }

    #[test]
    fn context_tombstone_suppresses_global_fallback() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "space": "camera::home" } }] }"#;
        let user = r#"{
            "bindings": [{
                "context": "dimension_lock",
                "bindings": { "space": null }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, condition_registry) =
            merged_keymap(defaults, Some(user), &[])?;
        let condition_handle = dimension_lock_handle(&condition_registry)?;

        assert!(diagnostics.is_empty());
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "space")?,
            Some("camera::home")
        );
        assert_eq!(
            command_for_sequence(
                condition_bindings(&merged_keymap, condition_handle)?,
                "space"
            )?,
            None
        );

        Ok(())
    }

    #[test]
    fn sequence_tombstones_change_only_the_complete_sequence() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "bindings": {
                    "g": "camera::home",
                    "g g": "camera::reset"
                }
            }]
        }"#;
        let user = r#"{ "bindings": [{ "bindings": { "g": null } }] }"#;
        let (merged_keymap, diagnostics, command_registry, condition_registry) =
            merged_keymap(defaults, Some(user), &[])?;
        let condition_handle = dimension_lock_handle(&condition_registry)?;
        let mut compiled_keymap = merged_keymap.compile(Generation(1), &command_registry);
        let matcher = compiled_keymap
            .matchers
            .get_mut(&condition_handle)
            .ok_or_else(|| String::from("dimension lock matcher is missing"))?;
        let now = Instant::now();
        let first = "g"
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid test sequence: {error}"))?
            .first();
        let second = first;

        assert!(diagnostics.is_empty());
        assert_eq!(command_for_sequence(merged_keymap.global(), "g")?, None);
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "g g")?,
            Some("camera::reset")
        );
        assert!(matches!(
            matcher.match_keystroke(first, now, MATCH_TIMEOUT),
            MatchOutcome::Pending
        ));
        assert!(matches!(
            matcher.match_keystroke(second, now, MATCH_TIMEOUT),
            MatchOutcome::Matched(_)
        ));

        Ok(())
    }

    #[test]
    fn tombstoning_a_longer_sequence_leaves_the_shorter_sequence_immediate() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "bindings": {
                    "g": "camera::home",
                    "g g": "camera::reset"
                }
            }]
        }"#;
        let user = r#"{ "bindings": [{ "bindings": { "g g": null } }] }"#;
        let (merged_keymap, diagnostics, command_registry, condition_registry) =
            merged_keymap(defaults, Some(user), &[])?;
        let condition_handle = dimension_lock_handle(&condition_registry)?;
        let mut compiled_keymap = merged_keymap.compile(Generation(1), &command_registry);
        let matcher = compiled_keymap
            .matchers
            .get_mut(&condition_handle)
            .ok_or_else(|| String::from("dimension lock matcher is missing"))?;
        let keystroke = "g"
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid test sequence: {error}"))?
            .first();

        assert!(diagnostics.is_empty());
        assert_eq!(command_for_sequence(merged_keymap.global(), "g g")?, None);
        assert!(matches!(
            matcher.match_keystroke(keystroke, Instant::now(), MATCH_TIMEOUT),
            MatchOutcome::Matched(_)
        ));

        Ok(())
    }

    #[test]
    fn timeout_carries_a_short_binding_through_an_unbound_intermediate_prefix() -> Result<(), String>
    {
        let defaults = r#"{
            "bindings": [{
                "bindings": {
                    "g": "camera::home",
                    "g g g": "camera::reset"
                }
            }]
        }"#;
        let (merged_keymap, diagnostics, command_registry, condition_registry) =
            merged_keymap(defaults, None, &[])?;
        let condition_handle = dimension_lock_handle(&condition_registry)?;
        let mut compiled_keymap = merged_keymap.compile(Generation(1), &command_registry);
        let matcher = compiled_keymap
            .matchers
            .get_mut(&condition_handle)
            .ok_or_else(|| String::from("dimension lock matcher is missing"))?;
        let keystroke = "g"
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid test sequence: {error}"))?
            .first();
        let now = Instant::now();

        assert!(diagnostics.is_empty());
        assert!(matches!(
            matcher.match_keystroke(keystroke, now, MATCH_TIMEOUT),
            MatchOutcome::Deferred(_)
        ));
        assert!(matches!(
            matcher.match_keystroke(keystroke, now, MATCH_TIMEOUT),
            MatchOutcome::Pending
        ));
        assert!(matches!(
            matcher.resolve_timeout(now + MATCH_TIMEOUT, MATCH_TIMEOUT),
            TimeoutOutcome::Resolved(_)
        ));

        Ok(())
    }

    #[test]
    fn conditioned_binding_replaces_an_unconditioned_binding() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "g": "camera::home" } }] }"#;
        let user = r#"{
            "bindings": [{
                "context": "dimension_lock",
                "bindings": { "g": "camera::reset" }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, condition_registry) =
            merged_keymap(defaults, Some(user), &[])?;
        let condition_handle = dimension_lock_handle(&condition_registry)?;

        assert!(diagnostics.is_empty());
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "g")?,
            Some("camera::home")
        );
        assert_eq!(
            command_for_sequence(condition_bindings(&merged_keymap, condition_handle)?, "g")?,
            Some("camera::reset")
        );

        Ok(())
    }

    #[test]
    fn unknown_commands_offer_nearby_registered_ids() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "g": "camera::hume" } }] }"#;
        let (_, diagnostics, _, _) = merged_keymap(defaults, None, &[])?;
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.command_id == "camera::hume")
            .ok_or_else(|| String::from("unknown command diagnostic is missing"))?;

        assert_eq!(diagnostic.kind, DiagnosticKind::Command);
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Failure);
        assert!(diagnostic.suggestions.iter().any(|id| id == "camera::home"));
        assert_eq!(&defaults[diagnostic.byte_range.clone()], "camera::hume");

        Ok(())
    }

    #[test]
    fn held_and_unremappable_commands_are_rejected() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "bindings": {
                    "g f": "camera::hold",
                    "r": "recovery::open"
                }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, None, &[])?;

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::HeldCommandInSequence
                && diagnostic.command_id == "camera::hold"
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::UnremappableCommand
                && diagnostic.command_id == "recovery::open"
        }));
        assert!(merged_keymap.global().is_empty());

        Ok(())
    }

    #[test]
    fn sole_bare_modifier_binding_is_valid_for_a_held_command() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "shift": "camera::hold" } }] }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, None, &[])?;

        assert!(diagnostics.is_empty());
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "shift")?,
            Some("camera::hold")
        );
        Ok(())
    }

    #[test]
    fn bare_modifier_binding_is_rejected_for_a_one_shot_command() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "shift": "camera::home" } }] }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, None, &[])?;
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind == DiagnosticKind::BareModifierRequiresHeldCommand)
            .ok_or_else(|| String::from("bare modifier diagnostic is missing"))?;

        assert_eq!(diagnostic.severity, DiagnosticSeverity::Failure);
        assert_eq!(diagnostic.command_id, "camera::home");
        assert!(diagnostic.message.contains("sole keystroke"));
        assert!(diagnostic.message.contains("hold-to-act command"));
        assert_eq!(command_for_sequence(merged_keymap.global(), "shift")?, None);
        Ok(())
    }

    #[test]
    fn sequence_containing_a_bare_modifier_is_rejected() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "g shift": "camera::home" } }] }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, None, &[])?;
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind == DiagnosticKind::BareModifierRequiresHeldCommand)
            .ok_or_else(|| String::from("bare modifier sequence diagnostic is missing"))?;

        assert_eq!(diagnostic.severity, DiagnosticSeverity::Failure);
        assert!(diagnostic.message.contains("keystroke 2 `shift`"));
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "g shift")?,
            None
        );
        Ok(())
    }

    #[test]
    fn modified_ordinary_key_remains_valid_and_routable() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "shift-f": "camera::home" } }] }"#;
        let (merged_keymap, diagnostics, command_registry, _) = merged_keymap(defaults, None, &[])?;
        let mut compiled_keymap = merged_keymap.compile(Generation(1), &command_registry);
        let keystroke = "shift-f"
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid modified key test sequence: {error}"))?
            .first();

        assert!(diagnostics.is_empty());
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "shift-f")?,
            Some("camera::home")
        );
        assert!(matches!(
            compiled_keymap.match_global(keystroke, Instant::now(), MATCH_TIMEOUT),
            MatchOutcome::Matched(_)
        ));
        Ok(())
    }

    #[test]
    fn same_source_held_prefix_and_longer_binding_are_both_rejected() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "bindings": {
                    "g": "camera::hold",
                    "g h": "camera::reset"
                }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, None, &[])?;
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind == DiagnosticKind::HeldCommandInSequence)
            .ok_or_else(|| String::from("held-prefix diagnostic is missing"))?;

        assert_eq!(diagnostic.severity, DiagnosticSeverity::Failure);
        assert_eq!(diagnostic.command_id, "camera::hold");
        assert!(diagnostic.message.contains("camera::hold"));
        assert!(diagnostic.message.contains("camera::reset"));
        assert_eq!(command_for_sequence(merged_keymap.global(), "g")?, None);
        assert_eq!(command_for_sequence(merged_keymap.global(), "g h")?, None);

        Ok(())
    }

    #[test]
    fn later_user_sequence_does_not_replace_a_shipped_held_prefix() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "bindings": { "f": "camera::hold" }
            }]
        }"#;
        let user = r#"{
            "bindings": [{
                "bindings": { "f g": "camera::reset" }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, Some(user), &[])?;
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind == DiagnosticKind::HeldCommandInSequence)
            .ok_or_else(|| String::from("user held-prefix diagnostic is missing"))?;

        assert_eq!(diagnostic.origin, user_keymap_file());
        assert_eq!(diagnostic.command_id, "camera::reset");
        assert!(diagnostic.message.contains("camera::hold"));
        assert!(diagnostic.message.contains("camera::reset"));
        assert!(diagnostic.message.contains(DEFAULTS_PATH));
        assert!(diagnostic.message.contains(USER_PATH));
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "f")?,
            Some("camera::hold")
        );
        assert_eq!(command_for_sequence(merged_keymap.global(), "f g")?, None);
        Ok(())
    }

    #[test]
    fn user_hold_displaces_a_shipped_sequence_sharing_its_prefix() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "bindings": { "f g": "camera::reset" }
            }]
        }"#;
        let user = r#"{
            "bindings": [{
                "bindings": { "f": "camera::hold" }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, Some(user), &[])?;
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind == DiagnosticKind::HeldCommandInSequence)
            .ok_or_else(|| String::from("shipped held-prefix diagnostic is missing"))?;

        assert_eq!(diagnostic.origin, defaults_keymap_file());
        assert_eq!(diagnostic.command_id, "camera::reset");
        assert!(diagnostic.message.contains("camera::hold"));
        assert!(diagnostic.message.contains("camera::reset"));
        assert!(diagnostic.message.contains(DEFAULTS_PATH));
        assert!(diagnostic.message.contains(USER_PATH));
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "f")?,
            Some("camera::hold")
        );
        assert_eq!(command_for_sequence(merged_keymap.global(), "f g")?, None);
        Ok(())
    }

    #[test]
    fn user_tombstone_removes_a_shipped_sequence_before_a_user_hold_conflicts_with_it()
    -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "bindings": { "f g": "camera::reset" }
            }]
        }"#;
        let user = r#"{
            "bindings": [{
                "bindings": {
                    "f g": null,
                    "f": "camera::hold"
                }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, Some(user), &[])?;

        assert!(diagnostics.is_empty());
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "f")?,
            Some("camera::hold")
        );
        assert_eq!(command_for_sequence(merged_keymap.global(), "f g")?, None);
        Ok(())
    }

    #[test]
    fn identical_user_override_and_longer_user_sequence_fall_back_to_the_shipped_hold()
    -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "bindings": { "f": "camera::hold" }
            }]
        }"#;
        let user = r#"{
            "bindings": [{
                "bindings": {
                    "f": "camera::hold",
                    "f g": "camera::reset"
                }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, Some(user), &[])?;
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind == DiagnosticKind::HeldCommandInSequence)
            .ok_or_else(|| String::from("same-source held-prefix diagnostic is missing"))?;

        assert_eq!(diagnostic.origin, user_keymap_file());
        assert_eq!(diagnostic.command_id, "camera::hold");
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "f")?,
            Some("camera::hold")
        );
        assert_eq!(command_for_sequence(merged_keymap.global(), "f g")?, None);
        Ok(())
    }

    #[test]
    fn user_tombstone_allows_a_sequence_after_a_shipped_held_prefix() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "bindings": { "f": "camera::hold" }
            }]
        }"#;
        let user = r#"{
            "bindings": [{
                "bindings": {
                    "f": null,
                    "f g": "camera::reset"
                }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, Some(user), &[])?;

        assert!(diagnostics.is_empty());
        assert_eq!(command_for_sequence(merged_keymap.global(), "f")?, None);
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "f g")?,
            Some("camera::reset")
        );
        Ok(())
    }

    #[test]
    fn unresolved_context_does_not_override_global_bindings() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "space": "camera::home" } }] }"#;
        let user = r#"{
            "bindings": [{
                "context": "unknown_context",
                "bindings": { "space": "camera::reset" }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, Some(user), &[])?;

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::Context
                && diagnostic.severity == DiagnosticSeverity::Failure
                && diagnostic.context == "unknown_context"
        }));
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "space")?,
            Some("camera::home")
        );

        Ok(())
    }

    #[test]
    fn null_context_does_not_override_global_bindings() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "space": "camera::home" } }] }"#;
        let user = r#"{
            "bindings": [{
                "context": null,
                "bindings": { "space": "camera::reset" }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, Some(user), &[])?;

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::Syntax
                && diagnostic.severity == DiagnosticSeverity::Failure
                && diagnostic.message.contains("must be a string")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DiagnosticKind::Context
                && diagnostic.severity == DiagnosticSeverity::Advisory
                && diagnostic.message.contains("registered condition")
        }));
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "space")?,
            Some("camera::home")
        );

        Ok(())
    }

    #[test]
    fn unrecognized_members_and_duplicate_binding_keys_are_advisories() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{
                "contxt": "dimension_lock",
                "bindings": {
                    "g": "camera::home",
                    "g": "camera::reset"
                }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) = merged_keymap(defaults, None, &[])?;

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == DiagnosticSeverity::Advisory
                && diagnostic.message.contains("contxt")
                && diagnostic.suggestions == ["context"]
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == DiagnosticSeverity::Advisory
                && diagnostic.original_keystroke == "g"
        }));
        assert_eq!(
            command_for_sequence(merged_keymap.global(), "g")?,
            Some("camera::reset")
        );

        Ok(())
    }

    #[test]
    fn protected_keystroke_aliases_reject_bare_and_prefix_bindings() -> Result<(), String> {
        let protected_keystroke = "secondary-p"
            .parse::<Keystroke>()
            .map_err(|error| format!("invalid protected keystroke: {error}"))?;
        let defaults = if cfg!(target_os = "macos") {
            r#"{
                "bindings": [{
                    "bindings": {
                        "cmd-p": "camera::home",
                        "super-p x": "camera::home",
                        "win-p": "camera::home",
                        "cmd-left-p": "camera::home",
                        "cmd-right-p": "camera::home"
                    }
                }]
            }"#
        } else {
            r#"{
                "bindings": [{
                    "bindings": {
                        "ctrl-p": "camera::home",
                        "control-p x": "camera::home",
                        "ctrlleft-p": "camera::home",
                        "ctrl-right-p": "camera::home",
                        "controlleft-p": "camera::home"
                    }
                }]
            }"#
        };
        let (merged_keymap, diagnostics, _, _) =
            merged_keymap(defaults, None, &[protected_keystroke])?;

        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.kind == DiagnosticKind::ReservedKeystroke)
                .count(),
            5
        );
        let recovery_diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind == DiagnosticKind::ReservedKeystroke)
            .ok_or_else(|| String::from("reserved keystroke diagnostic is missing"))?;
        if cfg!(target_os = "macos") {
            assert_eq!(
                recovery_diagnostic.message,
                "Keystroke 1 `cmd-p` is reserved for the application's recovery command and cannot start a keymap sequence."
            );
        } else {
            assert_eq!(
                recovery_diagnostic.message,
                "Keystroke 1 `ctrl-p` is reserved for the application's recovery command and cannot start a keymap sequence."
            );
        }
        assert!(merged_keymap.global().is_empty());

        Ok(())
    }

    #[test]
    fn protected_keystroke_modifier_order_aliases_are_rejected() -> Result<(), String> {
        let protected_keystroke = "cmd-shift-p"
            .parse::<Keystroke>()
            .map_err(|error| format!("invalid protected keystroke: {error}"))?;
        let defaults = r#"{
            "bindings": [{
                "bindings": {
                    "shift-cmd-p": "camera::home",
                    "shift-super-p x": "camera::home",
                    "win-shift-p": "camera::home"
                }
            }]
        }"#;
        let (merged_keymap, diagnostics, _, _) =
            merged_keymap(defaults, None, &[protected_keystroke])?;

        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.kind == DiagnosticKind::ReservedKeystroke)
                .count(),
            3
        );
        assert!(merged_keymap.global().is_empty());

        Ok(())
    }

    #[test]
    fn control_p_is_platform_specific_against_a_protected_platform_chord() -> Result<(), String> {
        let protected_keystroke = "secondary-p"
            .parse::<Keystroke>()
            .map_err(|error| format!("invalid protected keystroke: {error}"))?;
        let defaults = r#"{ "bindings": [{ "bindings": { "ctrl-p": "camera::home" } }] }"#;
        let (merged_keymap, diagnostics, _, _) =
            merged_keymap(defaults, None, &[protected_keystroke])?;

        if cfg!(target_os = "macos") {
            assert!(diagnostics.is_empty());
            assert_eq!(
                command_for_sequence(merged_keymap.global(), "ctrl-p")?,
                Some("camera::home")
            );
        } else {
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == DiagnosticKind::ReservedKeystroke
                    && diagnostic.severity == DiagnosticSeverity::Failure
            }));
            assert!(merged_keymap.global().is_empty());
        }

        Ok(())
    }
}
