//! `bevy_diegetic` — Diegetic UI for Bevy.
//!
//! Provides an in-world UI layout engine inspired by [Clay](https://github.com/nicbarker/clay),
//! implemented in pure Rust with no global state and full thread safety.
//!
//! # Retained-mode layout
//!
//! Clay is immediate-mode: the tree is rebuilt from scratch every frame and layout is computed
//! inline as you build it. `bevy_diegetic` is retained-mode: the [`LayoutTree`] is built once
//! via [`LayoutBuilder`], stored on a component, and the [`LayoutEngine`] only recomputes
//! positions when the tree changes. This is the natural fit for Bevy — the entire ECS is built
//! around doing nothing unless something changed (`Changed<T>`, `Res::is_changed()`,
//! observers). An immediate-mode engine would fight the framework by recomputing
//! unconditionally every frame; retained mode lets Bevy's change detection skip layout
//! entirely on frames where the tree hasn't been touched.
//!
//! # Quick start
//!
//! ```ignore
//! use bevy::prelude::*;
//! use bevy_diegetic::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(DiegeticUiPlugin)
//!     .add_systems(Startup, setup)
//!     .run();
//! ```
//!
//! # Configuration
//!
//! Insert default resources before adding [`DiegeticUiPlugin`] to override
//! construction-time defaults or cascade defaults:
//!
//! ```ignore
//! App::new()
//!     .insert_resource(CascadeDefaults {
//!         panel_font_unit: Unit::Millimeters,
//!         ..default()
//!     })
//!     .insert_resource(CascadeDefault(TextAlpha(AlphaMode::Add)))
//!     .add_plugins(DiegeticUiPlugin);
//! ```

mod callouts;
mod cascade;
mod constants;
#[cfg(feature = "typography_overlay")]
mod debug;
mod fluent;
mod ime;
mod layout;
mod panel;
mod render;
mod screen_space;
mod text;

#[cfg(feature = "bench_support")]
#[doc(hidden)]
/// Internal exports used by feature-gated benchmarks.
pub mod bench_support {
    pub use crate::layout::LayoutEngine;
    pub use crate::layout::LayoutResult;
    pub use crate::layout::LayoutTreeChange;
    pub use crate::layout::MeasureTextFn;
    pub use crate::layout::RectangleSource;
    pub use crate::layout::RenderCommand;
    pub use crate::layout::RenderCommandKind;
}

use bevy::asset::embedded_asset;
use bevy::prelude::*;
pub use callouts::ArrowStyle;
pub use callouts::CalloutCap;
pub use callouts::CalloutLine;
use callouts::CalloutPlugin;
pub use cascade::CascadeDefault;
pub use cascade::CascadeDefaults;
pub use cascade::CascadeEntityCommandsExt;
pub use cascade::CascadeProperty;
pub use cascade::CascadeSet;
pub use cascade::FontUnit;
pub use cascade::TextAlpha;
pub use cascade::TextLighting;
pub use cascade::TextSidedness;
pub use cascade::resolved_font_unit;
pub use cascade::resolved_text_alpha;
pub use cascade::resolved_text_lighting;
pub use cascade::resolved_text_sidedness;
#[cfg(feature = "typography_overlay")]
pub use debug::GlyphMetricVisibility;
#[cfg(feature = "typography_overlay")]
pub use debug::OverlayBoundingBox;
#[cfg(feature = "typography_overlay")]
pub use debug::TypographyOverlay;
use debug::TypographyOverlayPlugin;
pub use fluent::DiegeticText;
pub use fluent::DiegeticTextBuilder;
use fluent::DiegeticTextPlugin;
pub use ime::ImeAcceptCommit;
pub use ime::ImeAppInputContext;
pub use ime::ImeAppInputDisposition;
pub use ime::ImeAppInputDispositionHook;
pub use ime::ImeAppOwnedFieldSpec;
pub use ime::ImeApplied;
pub use ime::ImeAppliedResult;
pub use ime::ImeBufferBoundary;
pub use ime::ImeBufferRange;
pub use ime::ImeBufferSnapshot;
pub use ime::ImeBuiltInApplied;
pub use ime::ImeBuiltInFieldKind;
pub use ime::ImeBuiltInFieldSpec;
pub use ime::ImeBuiltInValue;
pub use ime::ImeCancelCause;
pub use ime::ImeCanceled;
pub use ime::ImeCommitAttemptId;
pub use ime::ImeCommitAuthority;
pub use ime::ImeCommitAuthorityToken;
pub use ime::ImeCommitCause;
pub use ime::ImeCommitRequested;
pub use ime::ImeCursorState;
pub use ime::ImeEditableFieldSpec;
pub use ime::ImeInputBlocker;
pub use ime::ImeOpenSession;
pub use ime::ImePanelField;
use ime::ImePlugin;
pub use ime::ImePreedit;
pub use ime::ImePreeditBoundary;
pub use ime::ImeRejectCommit;
pub use ime::ImeRejection;
pub use ime::ImeRequestCancel;
pub use ime::ImeRequestCommit;
pub use ime::ImeSelectionSnapshot;
pub use ime::ImeSessionAnchor;
pub use ime::ImeSessionId;
pub use ime::ImeStarted;
pub use ime::ImeSystemSet;
pub use ime::ImeTarget;
pub use ime::ImeTextChanged;
pub use ime::ImeValidationRejected;
pub use ime::ImeValueRevision;
pub use ime::PanelFieldId;
pub use layout::AlignX;
pub use layout::AlignY;
pub use layout::Anchor;
pub use layout::Border;
pub use layout::BoundingBox;
pub use layout::CornerRadius;
pub use layout::Dimension;
pub use layout::DimensionMatch;
pub use layout::Direction;
pub use layout::El;
pub use layout::FontFeatureFlags;
pub use layout::FontFeatures;
pub use layout::FontSlant;
pub use layout::FontWeight;
pub use layout::GlyphLighting;
pub use layout::GlyphRenderMode;
pub use layout::GlyphShadowMode;
pub use layout::GlyphSidedness;
pub use layout::HasUnit;
pub use layout::In;
pub use layout::InvalidSize;
pub use layout::LayoutBuilder;
pub use layout::LayoutTree;
/// Function signature for custom text measurement. Takes a text string and
/// a [`TextMeasure`] describing the font configuration, returns
/// [`TextDimensions`]. See [`DiegeticTextMeasurer`] and the `side_by_side`
/// example for usage.
pub use layout::MeasureTextFn;
pub use layout::Mm;
pub use layout::Padding;
pub use layout::PanelSize;
pub use layout::PaperSize;
pub use layout::Pt;
pub use layout::Px;
pub use layout::Sizing;
pub use layout::TextAlign;
/// Measured width and height of a text string, returned by [`MeasureTextFn`].
pub use layout::TextDimensions;
/// Font configuration passed to [`MeasureTextFn`]: font ID, size, weight,
/// slant, line height, letter/word spacing. See the `side_by_side` example
/// for a real-world custom measurer that bridges clay-layout to our
/// parley-backed measurement via this type.
pub use layout::TextMeasure;
pub use layout::TextStyle;
pub use layout::TextWrap;
pub use layout::Unit;
pub use panel::AnyUnit;
pub use panel::CompatibleUnits;
pub use panel::ComputedDiegeticPanel;
pub use panel::CoordinateSpace;
pub use panel::DiegeticPanel;
pub use panel::DiegeticPanelBuilder;
pub use panel::DiegeticPanelCommands;
pub use panel::DiegeticPanelGizmoGroup;
pub use panel::DiegeticPerfStats;
pub use panel::Fit;
pub use panel::FitMax;
pub use panel::FitRange;
pub use panel::Grow;
pub use panel::GrowMax;
pub use panel::GrowRange;
pub use panel::HeadlessLayoutPlugin;
pub use panel::Inches;
pub use panel::Millimeters;
pub use panel::PanelBuildError;
pub use panel::PanelFieldRecord;
use panel::PanelPlugin;
pub use panel::PanelSizing;
pub use panel::PanelSystems;
pub use panel::PanelTextPerfStats;
pub use panel::Percent;
pub use panel::Pixels;
pub use panel::Points;
pub use panel::ScreenPosition;
pub use panel::ShowTextGizmos;
pub use panel::SurfaceShadow;
pub use render::DiegeticTextMut;
pub use render::PanelText;
pub use render::PanelTextLayout;
pub use render::PanelTextReader;
pub use render::PanelTextRuns;
use render::RenderPlugin;
pub use render::StableTransparency;
pub use render::TextAntiAlias;
pub use render::TextContent;
pub use render::TextRunOf;
pub use render::WorldTextReady;
pub use render::default_panel_material;
pub use screen_space::ScreenSpaceCamera;
pub use screen_space::ScreenSpaceLight;
use screen_space::ScreenSpacePlugin;
pub use text::DiegeticTextMeasurer;
pub use text::Font;
pub use text::FontId;
pub use text::FontLoadFailed;
pub use text::FontMetrics;
pub use text::FontRegistered;
pub use text::FontRegistry;
pub use text::FontSource;
#[cfg(feature = "typography_overlay")]
pub use text::GlyphBounds;
#[cfg(feature = "typography_overlay")]
pub use text::GlyphTypographyMetrics;
use text::TextPlugin;

/// Bevy plugin that adds diegetic UI panel support.
///
/// Composes layout, rendering, text, callouts, and screen-space overlay
/// support into a single plugin. Insert [`CascadeDefaults`] or
/// [`CascadeDefault<A>`](CascadeDefault) resources before adding this plugin;
/// they take effect through the child plugins at build time.
///
/// # Quick start
///
/// ```ignore
/// App::new().add_plugins(DiegeticUiPlugin)
/// ```
pub struct DiegeticUiPlugin;

impl Plugin for DiegeticUiPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/sdf_panel.wgsl");

        app.init_resource::<CascadeDefaults>();
        app.add_plugins((
            TextPlugin,
            PanelPlugin,
            ImePlugin,
            ScreenSpacePlugin,
            RenderPlugin,
            CalloutPlugin,
            DiegeticTextPlugin,
            #[cfg(feature = "typography_overlay")]
            TypographyOverlayPlugin,
        ));
    }
}
