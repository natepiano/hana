//! JSONC keymap decoding with source locations for every authored binding.

#![expect(
    dead_code,
    reason = "later keymap loading and compilation stages consume this document model"
)]

use std::collections::HashSet;
use std::fmt;
use std::ops::Range;

use serde::Deserialize;
use serde::de::MapAccess;
use serde::de::Visitor;
use serde_json_lenient::Value;

use crate::CommandId;
use crate::ConditionName;
use crate::Diagnostic;
use crate::DiagnosticKind;
use crate::DiagnosticSeverity;
use crate::KeystrokeSequence;
use crate::KeystrokeSequenceParseError;

/// A parsed keymap document and the source information needed by later keymap stages.
#[derive(Debug)]
pub(crate) struct KeymapDocument {
    pub(super) schema:      Option<String>,
    pub(super) source_path: String,
    pub(super) blocks:      Vec<KeymapBlock>,
}

impl KeymapDocument {
    /// Parses one JSONC keymap document and retains each binding's authored source location.
    pub(crate) fn parse(
        source_path: &str,
        source: &str,
    ) -> Result<(Self, Vec<Diagnostic>), Vec<Diagnostic>> {
        let wire_document = match serde_json_lenient::from_str::<WireDocument>(source) {
            Ok(wire_document) => wire_document,
            Err(error) => {
                if root_value_starts_with_array(source) {
                    return Err(vec![document_diagnostic(
                        source_path,
                        source,
                        root_value_offset(source),
                        DiagnosticKind::Syntax,
                        "Keymap documents require an object envelope with a `bindings` member."
                            .to_owned(),
                    )]);
                }

                return Err(vec![serde_diagnostic(source_path, source, error)]);
            },
        };
        let source_index = match SourceIndex::parse(source) {
            Ok(source_index) => source_index,
            Err(error) => {
                return Err(vec![document_diagnostic(
                    source_path,
                    source,
                    error.offset,
                    DiagnosticKind::Syntax,
                    error.message,
                )]);
            },
        };

        let mut diagnostics = Vec::new();
        let blocks = parse_blocks(
            source_path,
            source,
            wire_document.bindings,
            source_index.blocks,
            &mut diagnostics,
        )?;

        Ok((
            Self {
                schema: wire_document.schema,
                source_path: source_path.to_owned(),
                blocks,
            },
            diagnostics,
        ))
    }
}

fn parse_blocks(
    source_path: &str,
    source: &str,
    wire_blocks: Vec<WireBlock>,
    indexed_blocks: Vec<IndexedBlock>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<KeymapBlock>, Vec<Diagnostic>> {
    if wire_blocks.len() != indexed_blocks.len() {
        return Err(vec![document_diagnostic(
            source_path,
            source,
            0,
            DiagnosticKind::Syntax,
            format!(
                "Internal keymap parsing inconsistency: serde decoded {} block(s), but source indexing recorded {}.",
                wire_blocks.len(),
                indexed_blocks.len(),
            ),
        )]);
    }

    let mut condition_names = HashSet::new();
    let mut blocks = Vec::with_capacity(wire_blocks.len());

    for (block_index, (wire_block, indexed_block)) in
        wire_blocks.into_iter().zip(indexed_blocks).enumerate()
    {
        blocks.push(parse_block(
            source_path,
            source,
            wire_block,
            indexed_block,
            block_index,
            &mut condition_names,
            diagnostics,
        )?);
    }

    Ok(blocks)
}

fn parse_block(
    source_path: &str,
    source: &str,
    wire_block: WireBlock,
    indexed_block: IndexedBlock,
    block_index: usize,
    condition_names: &mut HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<KeymapBlock, Vec<Diagnostic>> {
    let IndexedBlock {
        context: indexed_context,
        bindings: indexed_bindings,
        unrecognized_members,
    } = indexed_block;
    let (context, context_source) = parse_context(
        source_path,
        wire_block.context,
        indexed_context,
        block_index,
        condition_names,
        diagnostics,
    );
    let context_text =
        context
            .as_ref()
            .map_or_else(String::new, |context_expr| match context_expr {
                ContextExpr::Name(condition_name) => condition_name.as_str().to_owned(),
            });
    let bindings = parse_bindings(
        source_path,
        source,
        wire_block.bindings,
        indexed_bindings,
        block_index,
        context_text,
        diagnostics,
    )?;
    let unrecognized_members = unrecognized_members
        .into_iter()
        .map(|member| UnrecognizedBlockMember {
            name: member.name,
            byte_range: member.location.byte_range,
            line: member.location.line,
            column: member.location.column,
            block_index,
        })
        .collect();

    Ok(KeymapBlock {
        context,
        context_source,
        bindings,
        unrecognized_members,
    })
}

fn parse_context(
    source_path: &str,
    wire_context: WireContext,
    indexed_context: Option<SourceLocation>,
    block_index: usize,
    condition_names: &mut HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Option<ContextExpr>, Option<ContextSource>) {
    let context_source = indexed_context.map(|location| ContextSource::new(location, block_index));
    let context = match wire_context {
        WireContext::Absent => return (None, None),
        WireContext::Null => {
            diagnostics.push(context_diagnostic(
                source_path,
                context_source.as_ref(),
                block_index,
                String::new(),
                DiagnosticKind::Syntax,
                DiagnosticSeverity::Failure,
                "A keymap block `context` value must be a string, not null.".to_owned(),
            ));
            return (None, context_source);
        },
        WireContext::Text(context) => context,
    };

    if !condition_names.insert(context.clone()) {
        diagnostics.push(context_diagnostic(
            source_path,
            context_source.as_ref(),
            block_index,
            context.clone(),
            DiagnosticKind::Context,
            DiagnosticSeverity::Advisory,
            format!("Condition `{context}` appears in more than one keymap block."),
        ));
    }

    (
        Some(ContextExpr::Name(ConditionName::new(context))),
        context_source,
    )
}

fn parse_bindings(
    source_path: &str,
    source: &str,
    wire_bindings: WireBindings,
    locations: Vec<IndexedBinding>,
    block_index: usize,
    context: String,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<Binding>, Vec<Diagnostic>> {
    if wire_bindings.0.len() != locations.len() {
        return Err(vec![document_diagnostic(
            source_path,
            source,
            0,
            DiagnosticKind::Syntax,
            format!(
                "Internal keymap parsing inconsistency: serde decoded {} binding(s) in block {block_index}, but source indexing recorded {}.",
                wire_bindings.0.len(),
                locations.len(),
            ),
        )]);
    }

    let mut binding_names = HashSet::new();
    let mut bindings = Vec::with_capacity(wire_bindings.0.len());

    for ((original_keystroke, wire_value), locations) in wire_bindings.0.into_iter().zip(locations)
    {
        let binding_source = BindingSource::new(
            locations.key,
            locations.value,
            block_index,
            context.clone(),
            original_keystroke.clone(),
        );

        if !binding_names.insert(original_keystroke.clone()) {
            diagnostics.push(binding_source.diagnostic(
                source_path,
                String::new(),
                DiagnosticKind::Syntax,
                DiagnosticSeverity::Advisory,
                format!(
                    "Keymap block {block_index} declares `{original_keystroke}` more than once."
                ),
            ));
        }

        let keystroke_sequence = match original_keystroke.parse::<KeystrokeSequence>() {
            Ok(keystroke_sequence) => keystroke_sequence,
            Err(error) => {
                diagnostics.push(binding_source.keystroke_diagnostic(source_path, source, &error));
                continue;
            },
        };
        let edit = match parse_binding_edit(source_path, &binding_source, wire_value) {
            BindingEditResult::Edit(edit) => edit,
            BindingEditResult::Diagnostic(diagnostic) => {
                diagnostics.push(diagnostic);
                continue;
            },
        };

        bindings.push(Binding {
            keystroke_sequence,
            edit,
            source: binding_source,
        });
    }

    Ok(bindings)
}

/// One ordered block from a [`KeymapDocument`].
#[derive(Debug)]
pub(super) struct KeymapBlock {
    pub(super) context:              Option<ContextExpr>,
    pub(super) context_source:       Option<ContextSource>,
    pub(super) bindings:             Vec<Binding>,
    pub(super) unrecognized_members: Vec<UnrecognizedBlockMember>,
}

/// One unrecognized keymap block member retained for advisory diagnostics.
#[derive(Clone, Debug)]
pub(super) struct UnrecognizedBlockMember {
    pub(super) name:        String,
    pub(super) byte_range:  Range<usize>,
    pub(super) line:        usize,
    pub(super) column:      usize,
    pub(super) block_index: usize,
}

/// One parsed keymap binding and its location in the source document.
#[derive(Debug)]
pub(super) struct Binding {
    pub(super) keystroke_sequence: KeystrokeSequence,
    pub(super) edit:               BindingEdit,
    pub(super) source:             BindingSource,
}

/// One binding change authored in a [`KeymapDocument`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum BindingEdit {
    /// Binds a keystroke sequence to a command.
    Bind(CommandId),
    /// Removes a binding inherited from an earlier block.
    Unbind,
}

/// A keymap context expression.
///
/// A bare condition name remains valid when the keymap grammar later adds boolean operators, so
/// this enum preserves the expression boundary instead of storing a [`ConditionName`] directly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ContextExpr {
    /// A single application-defined condition name.
    Name(ConditionName),
}

/// The source location for one parsed context expression.
#[derive(Clone, Debug)]
pub(super) struct ContextSource {
    pub(super) byte_range:  Range<usize>,
    pub(super) line:        usize,
    pub(super) column:      usize,
    pub(super) block_index: usize,
}

impl ContextSource {
    const fn new(location: SourceLocation, block_index: usize) -> Self {
        Self {
            byte_range: location.byte_range,
            line: location.line,
            column: location.column,
            block_index,
        }
    }
}

/// The retained source locations for one authored binding key and command value.
#[derive(Clone, Debug)]
pub(super) struct BindingSource {
    key:                           SourceLocation,
    value:                         SourceLocation,
    pub(super) block_index:        usize,
    pub(super) context:            String,
    pub(super) original_keystroke: String,
}

impl BindingSource {
    const fn new(
        key: SourceLocation,
        value: SourceLocation,
        block_index: usize,
        context: String,
        original_keystroke: String,
    ) -> Self {
        Self {
            key,
            value,
            block_index,
            context,
            original_keystroke,
        }
    }

    pub(super) fn diagnostic(
        &self,
        source_path: &str,
        command_id: String,
        kind: DiagnosticKind,
        severity: DiagnosticSeverity,
        message: String,
    ) -> Diagnostic {
        self.diagnostic_at(&self.key, source_path, command_id, kind, severity, message)
    }

    pub(super) fn command_diagnostic(
        &self,
        source_path: &str,
        command_id: String,
        kind: DiagnosticKind,
        severity: DiagnosticSeverity,
        message: String,
    ) -> Diagnostic {
        self.diagnostic_at(
            &self.value,
            source_path,
            command_id,
            kind,
            severity,
            message,
        )
    }

    fn diagnostic_at(
        &self,
        location: &SourceLocation,
        source_path: &str,
        command_id: String,
        kind: DiagnosticKind,
        severity: DiagnosticSeverity,
        message: String,
    ) -> Diagnostic {
        Diagnostic {
            source_path: source_path.to_owned(),
            byte_range: location.byte_range.clone(),
            line: location.line,
            column: location.column,
            block_index: self.block_index,
            context: self.context.clone(),
            original_keystroke: self.original_keystroke.clone(),
            command_id,
            kind,
            severity,
            message,
            suggestions: Vec::new(),
        }
    }

    fn keystroke_diagnostic(
        &self,
        source_path: &str,
        source: &str,
        error: &KeystrokeSequenceParseError,
    ) -> Diagnostic {
        match error {
            KeystrokeSequenceParseError::Empty(_) => self.diagnostic(
                source_path,
                String::new(),
                DiagnosticKind::Keystroke,
                DiagnosticSeverity::Failure,
                "A binding key must contain at least one keystroke.".to_owned(),
            ),
            KeystrokeSequenceParseError::Keystroke(error) => {
                let start =
                    (self.key.byte_range.start + error.offset()).min(self.key.byte_range.end);
                let byte_range = start..(start + error.token().len()).min(self.key.byte_range.end);
                let (line, column) = line_and_column(source, start);

                Diagnostic {
                    source_path: source_path.to_owned(),
                    byte_range,
                    line,
                    column,
                    block_index: self.block_index,
                    context: self.context.clone(),
                    original_keystroke: self.original_keystroke.clone(),
                    command_id: String::new(),
                    kind: DiagnosticKind::Keystroke,
                    severity: DiagnosticSeverity::Failure,
                    message: format!("Unrecognized keystroke token `{}`.", error.token()),
                    suggestions: Vec::new(),
                }
            },
        }
    }
}

enum BindingEditResult {
    Edit(BindingEdit),
    Diagnostic(Diagnostic),
}

fn parse_binding_edit(
    source_path: &str,
    binding_source: &BindingSource,
    wire_value: Value,
) -> BindingEditResult {
    match wire_value {
        Value::String(command_id) => match CommandId::try_from(command_id.as_str()) {
            Ok(command_id) => BindingEditResult::Edit(BindingEdit::Bind(command_id)),
            Err(_) => BindingEditResult::Diagnostic(binding_source.command_diagnostic(
                source_path,
                command_id.clone(),
                DiagnosticKind::Command,
                DiagnosticSeverity::Failure,
                format!(
                    "Command ID `{command_id}` requires one :: separator and snake-case segments."
                ),
            )),
        },
        Value::Null => BindingEditResult::Edit(BindingEdit::Unbind),
        Value::Array(_) => BindingEditResult::Diagnostic(binding_source.diagnostic(
            source_path,
            String::new(),
            DiagnosticKind::Syntax,
            DiagnosticSeverity::Failure,
            "Binding arguments are not yet supported.".to_owned(),
        )),
        _ => BindingEditResult::Diagnostic(binding_source.diagnostic(
            source_path,
            String::new(),
            DiagnosticKind::Syntax,
            DiagnosticSeverity::Failure,
            "Binding values must be command ID strings or null.".to_owned(),
        )),
    }
}

fn context_diagnostic(
    source_path: &str,
    context_source: Option<&ContextSource>,
    block_index: usize,
    context: String,
    kind: DiagnosticKind,
    severity: DiagnosticSeverity,
    message: String,
) -> Diagnostic {
    let (byte_range, line, column, block_index) = context_source.map_or_else(
        || (0..0, 0, 0, block_index),
        |context_source| {
            (
                context_source.byte_range.clone(),
                context_source.line,
                context_source.column,
                context_source.block_index,
            )
        },
    );

    Diagnostic {
        source_path: source_path.to_owned(),
        byte_range,
        line,
        column,
        block_index,
        context,
        original_keystroke: String::new(),
        command_id: String::new(),
        kind,
        severity,
        message,
        suggestions: Vec::new(),
    }
}

fn serde_diagnostic(
    source_path: &str,
    source: &str,
    error: serde_json_lenient::Error,
) -> Diagnostic {
    let byte_offset = byte_offset(source, error.line(), error.column());

    document_diagnostic(
        source_path,
        source,
        byte_offset,
        DiagnosticKind::Syntax,
        error.to_string(),
    )
}

fn document_diagnostic(
    source_path: &str,
    source: &str,
    byte_offset: usize,
    kind: DiagnosticKind,
    message: String,
) -> Diagnostic {
    let (line, column) = line_and_column(source, byte_offset);

    Diagnostic {
        source_path: source_path.to_owned(),
        byte_range: byte_offset..byte_offset,
        line,
        column,
        block_index: 0,
        context: String::new(),
        original_keystroke: String::new(),
        command_id: String::new(),
        kind,
        severity: DiagnosticSeverity::Failure,
        message,
        suggestions: Vec::new(),
    }
}

fn root_value_starts_with_array(source: &str) -> bool {
    source
        .get(root_value_offset(source)..)
        .is_some_and(|remaining| remaining.starts_with('['))
}

fn root_value_offset(source: &str) -> usize {
    let mut scanner = JsoncScanner::new(source);
    let _ = scanner.skip_trivia();
    scanner.offset
}

fn byte_offset(source: &str, line: usize, column: usize) -> usize {
    if line == 0 || column == 0 {
        return source.len();
    }

    let mut current_line = 1;
    let mut line_start = 0;

    for (offset, byte) in source.bytes().enumerate() {
        if current_line == line {
            break;
        }
        if byte == b'\n' {
            current_line += 1;
            line_start = offset + 1;
        }
    }

    if current_line != line {
        return source.len();
    }

    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |offset| line_start + offset);
    let mut byte_offset = line_start
        .saturating_add(column - 1)
        .min(line_end)
        .min(source.len());

    while byte_offset > line_start && !source.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }

    byte_offset
}

fn line_and_column(source: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = &source[..byte_offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |offset| offset + 1);
    let column = source[line_start..byte_offset.min(source.len())]
        .chars()
        .count()
        + 1;

    (line, column)
}

#[derive(Deserialize)]
struct WireDocument {
    #[serde(rename = "$schema")]
    schema:   Option<String>,
    bindings: Vec<WireBlock>,
}

#[derive(Deserialize)]
struct WireBlock {
    #[serde(default)]
    context:  WireContext,
    bindings: WireBindings,
}

#[derive(Default)]
enum WireContext {
    #[default]
    Absent,
    Null,
    Text(String),
}

impl<'de> Deserialize<'de> for WireContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)
            .map(|context| context.map_or(Self::Null, Self::Text))
    }
}

struct WireBindings(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for WireBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WireBindingsVisitor;

        impl<'de> Visitor<'de> for WireBindingsVisitor {
            type Value = WireBindings;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a keymap bindings object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut bindings = Vec::new();

                while let Some(binding) = map.next_entry()? {
                    bindings.push(binding);
                }

                Ok(WireBindings(bindings))
            }
        }

        deserializer.deserialize_map(WireBindingsVisitor)
    }
}

struct SourceIndex {
    blocks: Vec<IndexedBlock>,
}

impl SourceIndex {
    fn parse(source: &str) -> Result<Self, SourceIndexError> {
        let mut scanner = JsoncScanner::new(source);
        scanner.skip_trivia()?;
        scanner.expect_byte(b'{', "Keymap documents must start with an object.")?;

        let mut blocks = None;
        scanner.skip_trivia()?;

        if scanner.consume_byte(b'}') {
            return Ok(Self { blocks: Vec::new() });
        }

        loop {
            let key = scanner.parse_string()?;
            scanner.skip_trivia()?;
            scanner.expect_byte(b':', "Expected `:` after an object key.")?;
            scanner.skip_trivia()?;

            if scanner.string_value(&key)? == "bindings" && scanner.peek_byte() == Some(b'[') {
                blocks = Some(scanner.parse_blocks()?);
            } else {
                scanner.skip_value()?;
            }

            scanner.skip_trivia()?;
            if scanner.consume_byte(b'}') {
                break;
            }
            scanner.expect_byte(b',', "Expected `,` between object members.")?;
            scanner.skip_trivia()?;
            if scanner.consume_byte(b'}') {
                break;
            }
        }

        scanner.skip_trivia()?;
        if scanner.peek_byte().is_some() {
            return Err(scanner.error("Expected the end of the keymap document."));
        }

        Ok(Self {
            blocks: blocks.unwrap_or_default(),
        })
    }
}

struct IndexedBlock {
    context:              Option<SourceLocation>,
    bindings:             Vec<IndexedBinding>,
    unrecognized_members: Vec<IndexedBlockMember>,
}

struct IndexedBinding {
    key:   SourceLocation,
    value: SourceLocation,
}

struct IndexedBlockMember {
    name:     String,
    location: SourceLocation,
}

#[derive(Clone, Debug)]
struct SourceLocation {
    byte_range: Range<usize>,
    line:       usize,
    column:     usize,
}

impl SourceLocation {
    fn from_token(source: &str, token: StringToken) -> Self {
        Self::from_range(source, token.content_range)
    }

    fn from_range(source: &str, byte_range: Range<usize>) -> Self {
        let (line, column) = line_and_column(source, byte_range.start);

        Self {
            byte_range,
            line,
            column,
        }
    }
}

struct JsoncScanner<'source> {
    source: &'source str,
    offset: usize,
}

impl<'source> JsoncScanner<'source> {
    const fn new(source: &'source str) -> Self { Self { source, offset: 0 } }

    fn parse_blocks(&mut self) -> Result<Vec<IndexedBlock>, SourceIndexError> {
        self.expect_byte(b'[', "Expected the `bindings` member to be an array.")?;
        self.skip_trivia()?;
        let mut blocks = Vec::new();

        if self.consume_byte(b']') {
            return Ok(blocks);
        }

        loop {
            blocks.push(self.parse_block()?);
            self.skip_trivia()?;
            if self.consume_byte(b']') {
                break;
            }
            self.expect_byte(b',', "Expected `,` between keymap blocks.")?;
            self.skip_trivia()?;
            if self.consume_byte(b']') {
                break;
            }
        }

        Ok(blocks)
    }

    fn parse_block(&mut self) -> Result<IndexedBlock, SourceIndexError> {
        self.expect_byte(b'{', "Expected a keymap block object.")?;
        self.skip_trivia()?;
        let mut context = None;
        let mut bindings = None;
        let mut unrecognized_members = Vec::new();

        if self.consume_byte(b'}') {
            return Ok(IndexedBlock {
                context,
                bindings: Vec::new(),
                unrecognized_members,
            });
        }

        loop {
            let key = self.parse_string()?;
            self.skip_trivia()?;
            self.expect_byte(b':', "Expected `:` after a block member name.")?;
            self.skip_trivia()?;

            let member_name = self.string_value(&key)?;
            match member_name.as_str() {
                "context" => context = Some(self.parse_context_location()?),
                "bindings" if self.peek_byte() == Some(b'{') => {
                    bindings = Some(self.parse_binding_locations()?);
                },
                _ => {
                    unrecognized_members.push(IndexedBlockMember {
                        name:     member_name,
                        location: SourceLocation::from_token(self.source, key),
                    });
                    self.skip_value()?;
                },
            }

            self.skip_trivia()?;
            if self.consume_byte(b'}') {
                break;
            }
            self.expect_byte(b',', "Expected `,` between block members.")?;
            self.skip_trivia()?;
            if self.consume_byte(b'}') {
                break;
            }
        }

        Ok(IndexedBlock {
            context,
            bindings: bindings.unwrap_or_default(),
            unrecognized_members,
        })
    }

    fn parse_binding_locations(&mut self) -> Result<Vec<IndexedBinding>, SourceIndexError> {
        self.expect_byte(b'{', "Expected a bindings object.")?;
        self.skip_trivia()?;
        let mut bindings = Vec::<IndexedBinding>::new();

        if self.consume_byte(b'}') {
            return Ok(bindings);
        }

        loop {
            let key = self.parse_string()?;
            self.skip_trivia()?;
            self.expect_byte(b':', "Expected `:` after a binding key.")?;
            self.skip_trivia()?;
            let value = if self.peek_byte() == Some(b'"') {
                SourceLocation::from_token(self.source, self.parse_string()?)
            } else {
                let start = self.offset;
                self.skip_value()?;
                SourceLocation::from_range(self.source, start..self.offset)
            };
            bindings.push(IndexedBinding {
                key: SourceLocation::from_token(self.source, key),
                value,
            });
            self.skip_trivia()?;
            if self.consume_byte(b'}') {
                break;
            }
            self.expect_byte(b',', "Expected `,` between bindings.")?;
            self.skip_trivia()?;
            if self.consume_byte(b'}') {
                break;
            }
        }

        Ok(bindings)
    }

    fn parse_context_location(&mut self) -> Result<SourceLocation, SourceIndexError> {
        if self.peek_byte() == Some(b'\"') {
            return Ok(SourceLocation::from_token(
                self.source,
                self.parse_string()?,
            ));
        }

        let start = self.offset;
        self.skip_value()?;
        Ok(SourceLocation::from_range(self.source, start..self.offset))
    }

    fn skip_value(&mut self) -> Result<(), SourceIndexError> {
        self.skip_trivia()?;

        match self.peek_byte() {
            Some(b'{') => self.skip_object(),
            Some(b'[') => self.skip_array(),
            Some(b'\"') => self.parse_string().map(|_| ()),
            Some(b't') => self.consume_keyword(b"true"),
            Some(b'f') => self.consume_keyword(b"false"),
            Some(b'n') => self.consume_keyword(b"null"),
            Some(b'-' | b'0'..=b'9') => {
                self.skip_number();
                Ok(())
            },
            _ => Err(self.error("Expected a JSON value.")),
        }
    }

    fn skip_object(&mut self) -> Result<(), SourceIndexError> {
        self.expect_byte(b'{', "Expected an object.")?;
        self.skip_trivia()?;

        if self.consume_byte(b'}') {
            return Ok(());
        }

        loop {
            self.parse_string()?;
            self.skip_trivia()?;
            self.expect_byte(b':', "Expected `:` after an object key.")?;
            self.skip_value()?;
            self.skip_trivia()?;
            if self.consume_byte(b'}') {
                return Ok(());
            }
            self.expect_byte(b',', "Expected `,` between object members.")?;
            self.skip_trivia()?;
            if self.consume_byte(b'}') {
                return Ok(());
            }
        }
    }

    fn skip_array(&mut self) -> Result<(), SourceIndexError> {
        self.expect_byte(b'[', "Expected an array.")?;
        self.skip_trivia()?;

        if self.consume_byte(b']') {
            return Ok(());
        }

        loop {
            self.skip_value()?;
            self.skip_trivia()?;
            if self.consume_byte(b']') {
                return Ok(());
            }
            self.expect_byte(b',', "Expected `,` between array values.")?;
            self.skip_trivia()?;
            if self.consume_byte(b']') {
                return Ok(());
            }
        }
    }

    fn skip_number(&mut self) {
        while self.peek_byte().is_some_and(|byte| {
            !byte.is_ascii_whitespace() && !matches!(byte, b',' | b']' | b'}' | b'/')
        }) {
            self.offset += 1;
        }
    }

    fn consume_keyword(&mut self, keyword: &[u8]) -> Result<(), SourceIndexError> {
        if self.source.as_bytes()[self.offset..].starts_with(keyword) {
            self.offset += keyword.len();
            Ok(())
        } else {
            Err(self.error("Expected a JSON keyword."))
        }
    }

    fn parse_string(&mut self) -> Result<StringToken, SourceIndexError> {
        self.expect_byte(b'\"', "Expected a JSON string.")?;
        let start = self.offset - 1;
        let content_start = self.offset;

        while let Some(byte) = self.peek_byte() {
            match byte {
                b'\"' => {
                    let content_end = self.offset;
                    self.offset += 1;
                    return Ok(StringToken {
                        token_range:   start..self.offset,
                        content_range: content_start..content_end,
                    });
                },
                b'\\' => {
                    self.offset += 1;
                    if self.peek_byte().is_none() {
                        return Err(
                            self.error("A JSON string escape must have a following character.")
                        );
                    }
                    self.offset += 1;
                },
                byte if byte.is_ascii_control() => {
                    return Err(self.error("JSON strings cannot contain control characters."));
                },
                _ => self.offset += 1,
            }
        }

        Err(self.error("JSON strings must end with a quote."))
    }

    fn string_value(&self, token: &StringToken) -> Result<String, SourceIndexError> {
        serde_json_lenient::from_str(&self.source[token.token_range.clone()]).map_err(|error| {
            SourceIndexError {
                offset:  token.token_range.start,
                message: error.to_string(),
            }
        })
    }

    fn expect_byte(&mut self, expected: u8, message: &str) -> Result<(), SourceIndexError> {
        self.skip_trivia()?;

        if self.consume_byte(expected) {
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek_byte() == Some(expected) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn skip_trivia(&mut self) -> Result<(), SourceIndexError> {
        loop {
            while self
                .peek_byte()
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                self.offset += 1;
            }

            let remaining = &self.source.as_bytes()[self.offset..];
            if remaining.starts_with(b"//") {
                self.offset += 2;
                while self.peek_byte().is_some_and(|byte| byte != b'\n') {
                    self.offset += 1;
                }
                continue;
            }
            if remaining.starts_with(b"/*") {
                self.offset += 2;
                while self.source.as_bytes().get(self.offset..self.offset + 2) != Some(b"*/") {
                    if self.peek_byte().is_none() {
                        return Err(self.error("Block comments must end with `*/`."));
                    }
                    self.offset += 1;
                }
                self.offset += 2;
                continue;
            }

            return Ok(());
        }
    }

    fn peek_byte(&self) -> Option<u8> { self.source.as_bytes().get(self.offset).copied() }

    fn error(&self, message: &str) -> SourceIndexError {
        SourceIndexError {
            offset:  self.offset,
            message: message.to_owned(),
        }
    }
}

struct StringToken {
    token_range:   Range<usize>,
    content_range: Range<usize>,
}

#[derive(Debug)]
struct SourceIndexError {
    offset:  usize,
    message: String,
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected keymap parse failures"
)]
mod tests {
    use super::BindingEdit;
    use super::ContextExpr;
    use super::DiagnosticKind;
    use super::KeymapDocument;
    use super::SourceIndex;

    const SOURCE_PATH: &str = "keymap.jsonc";

    fn assert_binding_key_slice(source: &str, binding_index: usize, expected_key: &str) {
        let source_index = SourceIndex::parse(source).expect("source index parses binding source");
        let binding_location = &source_index.blocks[0].bindings[binding_index];

        assert_eq!(
            &source[binding_location.key.byte_range.clone()],
            expected_key,
        );
    }

    #[test]
    fn envelope_parses_and_preserves_block_order() {
        let source = r#"
            {
                "$schema": "./keymap.schema.json",
                "bindings": [
                    { "bindings": { "space": "transport::toggle_playback" } },
                    {
                        "context": "dimension_lock",
                        "bindings": { "b": "dimension_lock::toggle_coupling" }
                    }
                ]
            }
        "#;

        let (document, diagnostics) =
            KeymapDocument::parse(SOURCE_PATH, source).expect("valid keymap envelope");

        assert!(diagnostics.is_empty());
        assert_eq!(document.schema.as_deref(), Some("./keymap.schema.json"));
        assert_eq!(document.blocks.len(), 2);
        assert!(matches!(
            document.blocks[0].bindings[0].edit,
            BindingEdit::Bind(ref command_id) if command_id.as_str() == "transport::toggle_playback"
        ));
        assert!(matches!(
            document.blocks[1].context,
            Some(ContextExpr::Name(ref condition_name)) if condition_name.as_str() == "dimension_lock"
        ));
        assert!(matches!(
            document.blocks[1].bindings[0].edit,
            BindingEdit::Bind(ref command_id) if command_id.as_str() == "dimension_lock::toggle_coupling"
        ));
        assert_eq!(document.blocks[0].bindings[0].source.block_index, 0);
        assert_eq!(document.blocks[1].bindings[0].source.block_index, 1);
    }

    #[test]
    fn bare_root_array_is_rejected_with_envelope_message() {
        let source = r#"[{ "bindings": { "space": "transport::toggle_playback" } }]"#;

        let diagnostics =
            KeymapDocument::parse(SOURCE_PATH, source).expect_err("a root array must be rejected");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].kind, DiagnosticKind::Syntax);
        assert_eq!(
            diagnostics[0].message,
            "Keymap documents require an object envelope with a `bindings` member."
        );
    }

    #[test]
    fn comments_and_trailing_commas_parse() {
        let source = r#"
            {
                // The schema keeps editor completions available.
                "$schema": "./keymap.schema.json",
                "bindings": [
                    {
                        "bindings": {
                            "space": "transport::toggle_playback",
                        },
                    },
                ],
            }
        "#;

        let (document, diagnostics) =
            KeymapDocument::parse(SOURCE_PATH, source).expect("JSONC keymap document");

        assert!(diagnostics.is_empty());
        assert_eq!(document.blocks.len(), 1);
        assert_eq!(document.blocks[0].bindings.len(), 1);
    }

    #[test]
    fn null_binding_creates_an_unbind_edit() {
        let source = r#"{ "bindings": [{ "bindings": { "space": null } }] }"#;

        let (document, diagnostics) =
            KeymapDocument::parse(SOURCE_PATH, source).expect("null binding tombstone");

        assert!(diagnostics.is_empty());
        assert_eq!(document.blocks.len(), 1);
        assert_eq!(document.blocks[0].bindings.len(), 1);
        assert_eq!(document.blocks[0].bindings[0].edit, BindingEdit::Unbind);
    }

    #[test]
    fn binding_arguments_are_rejected_until_they_are_supported() {
        let source = r#"{
            "bindings": [{ "bindings": {
                "space": ["transport::toggle_playback", {}],
                "enter": "editor::copy"
            } }]
        }"#;

        let (document, diagnostics) = KeymapDocument::parse(SOURCE_PATH, source)
            .expect("binding argument diagnostics retain sibling bindings");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].kind, DiagnosticKind::Syntax);
        assert_eq!(
            diagnostics[0].message,
            "Binding arguments are not yet supported."
        );
        assert_eq!(document.blocks[0].bindings.len(), 1);
    }

    #[test]
    fn numeric_binding_arguments_followed_by_comments_are_rejected() {
        let source = r#"{
            "bindings": [{ "bindings": {
                "space": ["transport::toggle_playback", 1/* explanation */],
                "enter": "editor::copy"
            } }]
        }"#;

        let (document, diagnostics) = KeymapDocument::parse(SOURCE_PATH, source)
            .expect("binding argument diagnostics retain sibling bindings");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].kind, DiagnosticKind::Syntax);
        assert_eq!(
            diagnostics[0].message,
            "Binding arguments are not yet supported."
        );
        assert_eq!(document.blocks[0].bindings.len(), 1);
    }

    #[test]
    fn null_context_is_rejected_without_discarding_the_block_bindings() {
        let source = r#"{
            "bindings": [{
                "context": null,
                "bindings": { "space": "transport::toggle_playback" }
            }]
        }"#;

        let (document, diagnostics) = KeymapDocument::parse(SOURCE_PATH, source)
            .expect("null context diagnostics retain block bindings");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].kind, DiagnosticKind::Syntax);
        assert!(document.blocks[0].context.is_none());
        assert_eq!(document.blocks[0].bindings.len(), 1);
    }

    #[test]
    fn bad_keystrokes_leave_sibling_bindings_available() {
        let source = r#"{
            "bindings": [{ "bindings": {
                "a": "editor::select_all",
                "unknown": "editor::copy",
                "b": "editor::bold"
            } }]
        }"#;

        let (document, diagnostics) = KeymapDocument::parse(SOURCE_PATH, source)
            .expect("keystroke diagnostics retain sibling bindings");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].kind, DiagnosticKind::Keystroke);
        assert_eq!(diagnostics[0].original_keystroke, "unknown");
        assert_eq!(document.blocks[0].bindings.len(), 2);
    }

    #[test]
    fn duplicate_context_names_across_blocks_are_diagnosed() {
        let source = r#"
            {
                "bindings": [
                    { "context": "editing", "bindings": { "a": "editor::select_all" } },
                    { "context": "editing", "bindings": { "b": "editor::bold" } }
                ]
            }
        "#;

        let (document, diagnostics) = KeymapDocument::parse(SOURCE_PATH, source)
            .expect("duplicate context diagnostics retain block bindings");
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind == DiagnosticKind::Context)
            .expect("duplicate context diagnostic");

        assert_eq!(diagnostic.context, "editing");
        assert_eq!(diagnostic.block_index, 1);
        assert!(diagnostic.message.contains("more than one keymap block"));
        assert_eq!(document.blocks[0].bindings.len(), 1);
        assert_eq!(document.blocks[1].bindings.len(), 1);
    }

    #[test]
    fn duplicate_binding_keys_within_one_block_are_diagnosed() {
        let source = r#"
            { "bindings": [{ "bindings": {
                "space": "transport::toggle_playback",
                "space": "transport::stop"
            } }] }
        "#;

        let (document, diagnostics) = KeymapDocument::parse(SOURCE_PATH, source)
            .expect("duplicate binding diagnostics retain both bindings");
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.kind == DiagnosticKind::Syntax
                    && diagnostic
                        .message
                        .contains("declares `space` more than once")
            })
            .expect("duplicate binding diagnostic");

        assert_eq!(diagnostic.block_index, 0);
        assert_eq!(diagnostic.original_keystroke, "space");
        let bindings = &document.blocks[0].bindings;
        assert_eq!(bindings.len(), 2);
        assert!(matches!(
            bindings[0].edit,
            BindingEdit::Bind(ref command_id) if command_id.as_str() == "transport::toggle_playback"
        ));
        assert!(matches!(
            bindings[1].edit,
            BindingEdit::Bind(ref command_id) if command_id.as_str() == "transport::stop"
        ));
    }

    #[test]
    fn invalid_bindings_in_repeated_context_blocks_keep_individual_locations() {
        let source = r#"{"bindings":[{"context":"editing","bindings":{"unknown":"editor::select_all","invalid":"editor::bold","a":"editor::copy"}},{"context":"editing","bindings":{"broken":"editor::copy","unmapped":"editor::paste","b":"editor::cut"}}]}"#;
        let (document, diagnostics) = KeymapDocument::parse(SOURCE_PATH, source)
            .expect("keystroke diagnostics retain valid bindings");

        for (original_keystroke, block_index) in [
            ("unknown", 0),
            ("invalid", 0),
            ("broken", 1),
            ("unmapped", 1),
        ] {
            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| {
                    diagnostic.kind == DiagnosticKind::Keystroke
                        && diagnostic.original_keystroke == original_keystroke
                })
                .expect("keystroke diagnostic for every invalid binding");
            let byte_start = source
                .find(&format!("\"{original_keystroke}\""))
                .expect("binding key in test source")
                + 1;
            let column = source[..byte_start].chars().count() + 1;

            assert_eq!(diagnostic.byte_range.start, byte_start);
            assert_eq!(diagnostic.line, 1);
            assert_eq!(diagnostic.column, column);
            assert_eq!(diagnostic.block_index, block_index);
        }

        assert_eq!(document.blocks[0].bindings.len(), 1);
        assert_eq!(document.blocks[1].bindings.len(), 1);
    }

    #[test]
    fn escaped_binding_keys_keep_keystroke_diagnostics_inside_their_source_span() {
        let source =
            r#"{ "bindings": [{ "bindings": { "ctrl-\u0078 unknown": "editor::copy" } }] }"#;

        let (document, diagnostics) = KeymapDocument::parse(SOURCE_PATH, source)
            .expect("keystroke diagnostics retain document data");
        let binding_start = source
            .find(r"ctrl-\u0078 unknown")
            .expect("escaped binding key in source");
        let binding_range = binding_start..binding_start + r"ctrl-\u0078 unknown".len();

        assert!(document.blocks[0].bindings.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].byte_range.start >= binding_range.start);
        assert!(diagnostics[0].byte_range.end <= binding_range.end);
    }

    #[test]
    fn syntax_diagnostics_use_byte_offsets_after_multibyte_text() {
        let source = r#"{ "bindings": [], /* café */ ! }"#;
        let diagnostics = KeymapDocument::parse(SOURCE_PATH, source)
            .expect_err("invalid JSONC must reject the whole document");
        let error_offset = source.find('!').expect("syntax error marker in source");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].byte_range, error_offset..error_offset);
    }

    #[test]
    fn source_index_skips_nested_binding_object_and_array_values() {
        let source = r#"{
            "bindings": [{ "bindings": {
                "object": { "nested": [true, { "number": 1 }] },
                "array": [{ "item": null }, [1, 2, 3]]
            } }]
        }"#;

        assert_binding_key_slice(source, 0, "object");
        assert_binding_key_slice(source, 1, "array");
    }

    #[test]
    fn source_index_skips_comments_around_binding_punctuation() {
        let source = r#"{
            "bindings": [{ "bindings": {
                "line" // between the key and colon
                : "editor::copy" // between the value and comma
                ,
                "block" /* between the key and colon */ : "editor::paste" /* between the value and comma */,
            } }]
        }"#;

        assert_binding_key_slice(source, 0, "line");
        assert_binding_key_slice(source, 1, "block");
    }

    #[test]
    fn source_index_preserves_escaped_quotes_in_binding_key_ranges() {
        let source = r#"{ "bindings": [{ "bindings": { "button\"quote": "editor::copy" } }] }"#;

        assert_binding_key_slice(source, 0, r#"button\"quote"#);
    }

    #[test]
    fn source_index_finds_bindings_after_unknown_block_members() {
        let source = r#"{
            "bindings": [{
                "before_context": { "nested": [1] },
                "context": "editing",
                "before_bindings": [true, false],
                "bindings": { "named": "editor::copy" }
            }]
        }"#;

        assert_binding_key_slice(source, 0, "named");
    }
}
