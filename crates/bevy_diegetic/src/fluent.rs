//! Fluent text primitives: [`WorldText`] and [`ScreenText`].
//!
//! Each builder produces a one-element [`DiegeticPanel`] holding a single text
//! child, so wrapping, lighting, and the unified [`TextStyle`] come from the
//! panel-text pipeline rather than a separate standalone render path. The chain
//! ends in [`WorldText::spawn`] / [`ScreenText::spawn`], which builds the panel
//! plus its text child and returns the **panel** entity — the handle callers
//! query for [`TextContent`] to change the string at runtime.
//!
//! ```ignore
//! // World text, fit to content, anchored at its center:
//! let panel = WorldText::new("Hello")
//!     .size(0.16)
//!     .color(Color::WHITE)
//!     .anchor(Anchor::Center)
//!     .transform(Transform::from_xyz(0.0, 2.0, 0.0))
//!     .spawn(&mut commands);
//!
//! // Screen text, wrapped to 320 px, positioned in pixels:
//! ScreenText::new("Paused")
//!     .size(48.0)
//!     .width(320.0)
//!     .screen_position(40.0, 40.0)
//!     .spawn(&mut commands);
//!
//! // Change the string later — query the panel handle:
//! fn update(mut q: Query<&mut TextContent, With<DiegeticPanel>>) {
//!     for mut content in &mut q {
//!         content.set_text("new text");
//!     }
//! }
//! ```

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::layout::Anchor;
use crate::layout::Dimension;
use crate::layout::FontFeatures;
use crate::layout::FontSlant;
use crate::layout::FontWeight;
use crate::layout::GlyphLighting;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
use crate::layout::LayoutBuilder;
use crate::layout::LayoutTree;
use crate::layout::Px;
use crate::layout::TextAlign;
use crate::layout::TextStyle;
use crate::layout::TextWrap;
use crate::layout::Unit;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPanelCommands;
use crate::panel::Fit;
use crate::panel::PanelSystems;
use crate::render::TextContent;

/// Style and wrap width stored on a one-element panel spawned by
/// [`WorldText`] / [`ScreenText`].
///
/// [`rebuild_fluent_text`] reads these to rebuild the single-element layout tree
/// whenever the panel's [`TextContent`] changes, so callers can change the
/// string at runtime through the panel handle.
#[derive(Component, Clone, Debug)]
pub(crate) struct FluentText {
    style:      TextStyle,
    wrap_width: Option<f32>,
}

/// Generates the shared typography setters as inherent methods, so callers reach
/// `.size()` / `.bold()` / `.color()` directly without importing a trait.
macro_rules! text_style_setters {
    ($ty:ty) => {
        impl $ty {
            /// Sets the font size. Accepts [`Pt`](crate::Pt), [`Mm`](crate::Mm),
            /// [`In`](crate::In), [`Px`], or a bare `f32` (resolved from the
            /// contextual unit: world meters here, pixels for [`ScreenText`]).
            #[must_use]
            pub fn size(mut self, size: impl Into<Dimension>) -> Self {
                self.style.set_dimension(size);
                self
            }

            /// Sets the font by identifier.
            #[must_use]
            pub const fn font(mut self, font_id: u16) -> Self {
                self.style.set_font_id(font_id);
                self
            }

            /// Sets the font weight.
            #[must_use]
            pub const fn weight(mut self, weight: FontWeight) -> Self {
                self.style.set_weight(weight);
                self
            }

            /// Shorthand for [`Self::weight`] with [`FontWeight::BOLD`].
            #[must_use]
            pub const fn bold(mut self) -> Self {
                self.style.set_weight(FontWeight::BOLD);
                self
            }

            /// Sets the font slant.
            #[must_use]
            pub const fn slant(mut self, slant: FontSlant) -> Self {
                self.style.set_slant(slant);
                self
            }

            /// Shorthand for [`Self::slant`] with [`FontSlant::Italic`].
            #[must_use]
            pub const fn italic(mut self) -> Self {
                self.style.set_slant(FontSlant::Italic);
                self
            }

            /// Sets the line height in the font's unit. `0.0` = derive from size.
            #[must_use]
            pub const fn line_height(mut self, line_height: f32) -> Self {
                self.style.set_line_height(line_height);
                self
            }

            /// Sets extra spacing between characters.
            #[must_use]
            pub const fn letter_spacing(mut self, spacing: f32) -> Self {
                self.style.set_letter_spacing(spacing);
                self
            }

            /// Sets extra spacing between words.
            #[must_use]
            pub const fn word_spacing(mut self, spacing: f32) -> Self {
                self.style.set_word_spacing(spacing);
                self
            }

            /// Sets the text color.
            #[must_use]
            pub const fn color(mut self, color: Color) -> Self {
                self.style.set_color(color);
                self
            }

            /// Sets horizontal alignment of glyphs within the measured run.
            #[must_use]
            pub const fn align(mut self, align: TextAlign) -> Self {
                self.style.set_align(align);
                self
            }

            /// Sets the glyph render mode.
            #[must_use]
            pub const fn render_mode(mut self, mode: GlyphRenderMode) -> Self {
                self.style.set_render_mode(mode);
                self
            }

            /// Sets the glyph shadow mode.
            #[must_use]
            pub const fn shadow_mode(mut self, mode: GlyphShadowMode) -> Self {
                self.style.set_shadow_mode(mode);
                self
            }

            /// Overrides glyph sidedness for this text (else inherits the
            /// context default via the `TextSidedness` cascade attribute).
            #[must_use]
            pub const fn sidedness(mut self, sidedness: GlyphSidedness) -> Self {
                self.style.set_sidedness(sidedness);
                self
            }

            /// Overrides glyph lighting for this text (else inherits the context
            /// default via the `TextLighting` cascade attribute).
            #[must_use]
            pub const fn lighting(mut self, lighting: GlyphLighting) -> Self {
                self.style.set_lighting(lighting);
                self
            }

            /// Shorthand for [`Self::lighting`] with [`GlyphLighting::Unlit`].
            #[must_use]
            pub const fn unlit(mut self) -> Self {
                self.style.set_lighting(GlyphLighting::Unlit);
                self
            }

            /// Sets OpenType font feature overrides.
            #[must_use]
            pub const fn font_features(mut self, features: FontFeatures) -> Self {
                self.style.set_font_features(features);
                self
            }

            /// Sets the per-text [`AlphaMode`] override.
            #[must_use]
            pub const fn alpha_mode(mut self, alpha_mode: AlphaMode) -> Self {
                self.style.set_alpha_mode(alpha_mode);
                self
            }

            /// Replaces the whole [`TextStyle`] at once. Later chained setters
            /// still apply on top of it.
            #[must_use]
            pub const fn style(mut self, style: TextStyle) -> Self {
                self.style = style;
                self
            }

            /// Wraps the text to this fixed width (in the font's unit). Absent →
            /// fit-to-content with no wrapping. Explicit `\n` always breaks
            /// regardless of wrap mode.
            #[must_use]
            pub const fn width(mut self, width: f32) -> Self {
                self.wrap_width = Some(width);
                self
            }
        }
    };
}

/// Fluent builder for a world-space text label.
///
/// Spawns a one-element world [`DiegeticPanel`] sized to fit (or to
/// [`Self::width`]) and returns the panel entity from [`Self::spawn`].
pub struct WorldText {
    text:         String,
    style:        TextStyle,
    wrap_width:   Option<f32>,
    world_width:  Option<f32>,
    world_height: Option<f32>,
    anchor:       Option<Anchor>,
    transform:    Transform,
}

text_style_setters!(WorldText);

impl WorldText {
    /// Starts a world-text builder with the given string and default style.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text:         text.into(),
            style:        TextStyle::default(),
            wrap_width:   None,
            world_width:  None,
            world_height: None,
            anchor:       None,
            transform:    Transform::IDENTITY,
        }
    }

    /// Sets the anchor point that places the label box at its transform
    /// (default [`Anchor::TopLeft`]). [`Anchor::Center`] centers the label on
    /// the transform.
    #[must_use]
    pub const fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Scales the label so its whole world width matches `meters`; height
    /// follows the aspect ratio. Mutually combines with [`Self::world_height`]
    /// for non-uniform scaling.
    #[must_use]
    pub const fn world_width(mut self, meters: f32) -> Self {
        self.world_width = Some(meters);
        self
    }

    /// Scales the label so its whole world height matches `meters`; width
    /// follows the aspect ratio.
    #[must_use]
    pub const fn world_height(mut self, meters: f32) -> Self {
        self.world_height = Some(meters);
        self
    }

    /// Places the label at this world transform.
    #[must_use]
    pub const fn transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    /// Builds the one-element world panel as a [`Bundle`], for composing with
    /// marker components or spawning under `with_children`. [`Self::spawn`] is
    /// the terminal one-liner over this.
    ///
    /// A degenerate size (unreachable while the height is always `Fit`) logs and
    /// falls back to a default panel rather than panicking.
    #[must_use]
    pub fn bundle(self) -> impl Bundle {
        let tree = build_one_element_tree(&self.text, &self.style, self.wrap_width);

        let sized = match self.wrap_width {
            Some(width) => DiegeticPanel::world().size(width, Fit),
            None => DiegeticPanel::world().size(Fit, Fit),
        }
        .font_unit(Unit::Meters);
        let scaled_width = match self.world_width {
            Some(meters) => sized.world_width(meters),
            None => sized,
        };
        let scaled = match self.world_height {
            Some(meters) => scaled_width.world_height(meters),
            None => scaled_width,
        };
        let anchored = match self.anchor {
            Some(anchor) => scaled.anchor(anchor),
            None => scaled,
        };
        let panel = anchored.with_tree(tree).build().unwrap_or_else(|error| {
            error!("WorldText: {error}; falling back to a default panel");
            DiegeticPanel::default()
        });

        (
            panel,
            TextContent::new(self.text),
            FluentText {
                style:      self.style,
                wrap_width: self.wrap_width,
            },
            self.transform,
        )
    }

    /// Spawns the one-element world panel, returning the panel entity (the
    /// handle callers query for [`TextContent`] to change the string later).
    pub fn spawn(self, commands: &mut Commands) -> Entity { commands.spawn(self.bundle()).id() }
}

/// Fluent builder for a screen-space (overlay) text label.
///
/// Spawns a one-element screen [`DiegeticPanel`]; its construction bridge seeds
/// unlit / front-facing / pixel defaults. Returns the panel entity from
/// [`Self::spawn`].
pub struct ScreenText {
    text:          String,
    style:         TextStyle,
    wrap_width:    Option<f32>,
    anchor:        Option<Anchor>,
    position:      Option<Vec2>,
    render_layers: Option<RenderLayers>,
    camera_order:  Option<isize>,
}

text_style_setters!(ScreenText);

impl ScreenText {
    /// Starts a screen-text builder with the given string and default style.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text:          text.into(),
            style:         TextStyle::default(),
            wrap_width:    None,
            anchor:        None,
            position:      None,
            render_layers: None,
            camera_order:  None,
        }
    }

    /// Sets the anchor that places the label within the window (default
    /// [`Anchor::TopLeft`]). With no [`Self::screen_position`], the anchor
    /// positions the label against the window edges — [`Anchor::Center`]
    /// centers it on screen.
    #[must_use]
    pub const fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Places the label at an explicit pixel position (top-left origin, y-down).
    #[must_use]
    pub const fn screen_position(mut self, x: f32, y: f32) -> Self {
        self.position = Some(Vec2::new(x, y));
        self
    }

    /// Sets the overlay camera render order (default `100`).
    #[must_use]
    pub const fn camera_order(mut self, order: isize) -> Self {
        self.camera_order = Some(order);
        self
    }

    /// Sets the render layers for camera isolation (default layer 31).
    #[must_use]
    pub fn render_layers(mut self, layers: RenderLayers) -> Self {
        self.render_layers = Some(layers);
        self
    }

    /// Builds the one-element screen panel as a [`Bundle`], for composing with
    /// marker components. [`Self::spawn`] is the terminal one-liner over this.
    ///
    /// A degenerate size (unreachable while the height is always `Fit`) logs and
    /// falls back to a default panel rather than panicking.
    #[must_use]
    pub fn bundle(self) -> impl Bundle {
        let tree = build_one_element_tree(&self.text, &self.style, self.wrap_width);

        let sized = match self.wrap_width {
            Some(width) => DiegeticPanel::screen().size(Px(width), Fit),
            None => DiegeticPanel::screen().size(Fit, Fit),
        };
        let anchored = match self.anchor {
            Some(anchor) => sized.anchor(anchor),
            None => sized,
        };
        let positioned = match self.position {
            Some(pos) => anchored.screen_position(pos.x, pos.y),
            None => anchored,
        };
        let ordered = match self.camera_order {
            Some(order) => positioned.camera_order(order),
            None => positioned,
        };
        let layered = match self.render_layers {
            Some(layers) => ordered.render_layers(layers),
            None => ordered,
        };
        let panel = layered.with_tree(tree).build().unwrap_or_else(|error| {
            error!("ScreenText: {error}; falling back to a default panel");
            DiegeticPanel::default()
        });

        (
            panel,
            TextContent::new(self.text),
            FluentText {
                style:      self.style,
                wrap_width: self.wrap_width,
            },
        )
    }

    /// Spawns the one-element screen panel, returning the panel entity (the
    /// handle callers query for [`TextContent`] to change the string later).
    pub fn spawn(self, commands: &mut Commands) -> Entity { commands.spawn(self.bundle()).id() }
}

/// Builds the single-element layout tree for a [`WorldText`] / [`ScreenText`]
/// panel. Applies [`TextWrap::Words`] when a wrap width is set and
/// [`TextWrap::None`] otherwise.
fn build_one_element_tree(text: &str, style: &TextStyle, wrap_width: Option<f32>) -> LayoutTree {
    let mut style = style.clone();
    style.set_wrap(if wrap_width.is_some() {
        TextWrap::Words
    } else {
        TextWrap::None
    });
    let mut builder = LayoutBuilder::new(wrap_width.unwrap_or(0.0), 0.0);
    builder.text(text.to_string(), style);
    builder.build()
}

/// Rebuilds a [`WorldText`] / [`ScreenText`] panel's single-element tree when its
/// [`TextContent`] changes, so callers can change the string at runtime through
/// the panel handle. The deferred [`set_tree`](DiegeticPanelCommands::set_tree) flushes at
/// [`PanelSystems::ApplyTreeChanges`], before layout reads the tree.
pub(crate) fn rebuild_fluent_text(
    panels: Query<(Entity, &TextContent, &FluentText), Changed<TextContent>>,
    mut commands: Commands,
) {
    for (entity, content, spec) in &panels {
        let tree = build_one_element_tree(content.text(), &spec.style, spec.wrap_width);
        commands.set_tree(entity, tree);
    }
}

/// Registers the runtime-text rebuild for [`WorldText`] / [`ScreenText`] panels.
pub(crate) struct FluentTextPlugin;

impl Plugin for FluentTextPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            rebuild_fluent_text.before(PanelSystems::ApplyTreeChanges),
        );
    }
}
