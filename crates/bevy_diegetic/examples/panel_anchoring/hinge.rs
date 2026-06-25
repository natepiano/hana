//! The hinge-chain capability (`3`): the shared tile set re-anchored edge-to-edge
//! into a numbered strip that folds into a coplanar stack and unwraps back (`R`),
//! plus the fold-control state and the per-link hinge-pose writer.
//!
//! The tiles are the same persistent set the anchor fan uses; switching to the
//! hinge chain morphs them into the flat strip (see [`crate::scene`]). Tile `0` is
//! the fixed top of the strip; every later tile anchors its top edge onto its
//! parent's bottom edge and carries a [`PanelAnchorPose`] that [`drive_hinge_pose`]
//! writes as the hinge angle, so the chain collapses toward tile `0` and unwraps
//! back. `A`/`C` pick accordion vs coil, `F`/`B` the lean toward or away from the
//! camera, `G`/`S` glide vs step travel, and `U`/`D`/`R` fold, mirror-fold, and
//! unwrap. The link count is shared with the anchor fan ([`AnchorChain`]); `+`/`-`
//! grow and shrink it in every mode.

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Border;
use bevy_diegetic::El;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::PanelAnchorPose;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use fairy_dust::ControlActivation;
use fairy_dust::TitleChipActivation;

use crate::anchor_demo::AnchorChain;
use crate::anchor_demo::AnchorTile;
use crate::constants::*;
use crate::presentation::smoothstep;
use crate::scene::ActiveCapability;
use crate::scene::ModeMorph;

/// Direction a hinge ease is moving the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HingePhase {
    /// Easing toward the coplanar strip (`unwrap` → `1`).
    Unwrapping,
    /// Easing back to the folded chain (`unwrap` → `0`).
    Folding,
}

/// Which fold pattern the chain collapses with (`A`/`C`). Selecting one only
/// relights the control; the next `U`/`D`/`R` is what moves the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FoldPattern {
    /// Adjacent creases bend opposite ways (the current fold).
    Accordion,
    /// Each link rolls onto its neighbor the same way.
    Coil,
}

/// Which way the folded stack leans relative to the camera (`F`/`B`). Selecting
/// one only relights the control; the next `U`/`D`/`R` is what moves the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FoldDirection {
    /// The stack leans toward the camera.
    Front,
    /// The stack leans away from the camera.
    Back,
}

/// The last fold action performed, kept lit until the next `U`/`D` (`R` relights
/// `Down` without latching itself). With a single fixed top, `Up` and `Down` fold
/// the chain toward tile `0` from opposite leans rather than pinning opposite ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FoldAction {
    /// Fold leaning one way.
    Up,
    /// Fold leaning the mirror way.
    Down,
}

/// How far one `U`/`D` press carries the chain (`G`/`S`). Selecting one only
/// relights the control; the next `U`/`D` is what moves the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FoldTravel {
    /// One press sweeps continuously through flat to the far collapse.
    Glide,
    /// One press advances one rest-stage (collapsed ↔ flat) and stops.
    Step,
}

/// The pattern, direction, and action a fold is built from. The lit selection is
/// what the panel shows; the committed copy is what the current pose reflects, so
/// relighting a different selection while folded does not jump the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FoldSpec {
    pub(crate) pattern:   FoldPattern,
    pub(crate) direction: FoldDirection,
    pub(crate) action:    FoldAction,
}

/// State of the hinge-chain fold (capability `3`). The chain starts unwrapped as a
/// flat strip; `U`/`D` fold it into a coplanar stack and `R` unwraps it back. `lit`
/// is the panel selection set by `A`/`C`/`F`/`B`/`U`/`D`; `folded` is what the pose
/// is built from, captured when a fold ease begins. The link count lives in
/// [`AnchorChain`] — the one tile set shared with the anchor fan.
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct HingeChain {
    /// The selection shown lit in the panel; only `U`/`D`/`R` act on it.
    lit:         FoldSpec,
    /// The fold the current pose reflects, captured when a fold ease begins.
    folded:      FoldSpec,
    /// Whether `U`/`D` sweep through flat to the far side or stop at each stage.
    travel:      FoldTravel,
    /// Current ease, or `None` when settled (fully folded or unwrapped).
    phase:       Option<HingePhase>,
    /// Whether the fold ease is frozen (`P`/`Space`); drives the title-bar chip.
    paused:      bool,
    /// Seconds into the current ease.
    timer:       f32,
    /// `0` fully folded .. `1` fully unwrapped; the rest value when settled.
    unwrap:      f32,
    /// `unwrap` when the current ease began.
    from_unwrap: f32,
    /// Set when an action is requested while not flat: the chain unwraps first,
    /// then commits `lit` and folds — the unfold-first / unfold-then-redo rule.
    refold:      bool,
}

impl Default for HingeChain {
    /// Launches as a flat unwrapped strip with `A`/`F`/`D` lit.
    fn default() -> Self {
        let lit = FoldSpec {
            pattern:   FoldPattern::Accordion,
            direction: FoldDirection::Front,
            action:    FoldAction::Down,
        };
        Self {
            lit,
            folded: lit,
            travel: FoldTravel::Step,
            phase: None,
            paused: false,
            timer: 0.0,
            unwrap: 1.0,
            from_unwrap: 1.0,
            refold: false,
        }
    }
}

impl HingeChain {
    /// Hinge angle (radians) each dependent link tilts about its pinned edge at the
    /// current unwrap progress; zero when fully unwrapped.
    fn link_angle(self) -> f32 { HINGE_FOLD_ANGLE_RAD * (1.0 - self.unwrap) }

    /// The lit fold pattern (`A`/`C`).
    pub(crate) const fn pattern(self) -> FoldPattern { self.lit.pattern }

    /// The lit fold direction (`F`/`B`).
    pub(crate) const fn direction(self) -> FoldDirection { self.lit.direction }

    /// The lit fold action (`U`/`D`).
    pub(crate) const fn action(self) -> FoldAction { self.lit.action }

    /// The lit travel mode (`G`/`S`).
    pub(crate) const fn travel(self) -> FoldTravel { self.travel }

    /// Selects the lit travel mode (`G`/`S`); does not move the chain.
    pub(crate) const fn set_travel(&mut self, travel: FoldTravel) { self.travel = travel; }

    /// Selects the lit fold pattern (`A`/`C`); does not move the chain.
    pub(crate) const fn set_pattern(&mut self, pattern: FoldPattern) { self.lit.pattern = pattern; }

    /// Selects the lit fold direction (`F`/`B`); does not move the chain.
    pub(crate) const fn set_direction(&mut self, direction: FoldDirection) {
        self.lit.direction = direction;
    }

    /// Folds the chain leaning one way (`U`).
    pub(crate) fn up(&mut self) { self.act(FoldAction::Up); }

    /// Folds the chain leaning the mirror way (`D`).
    pub(crate) fn down(&mut self) { self.act(FoldAction::Down); }

    /// Freezes or resumes the fold ease while one is in flight (`P`/`Space`).
    pub(crate) const fn toggle_pause(&mut self) {
        if self.phase.is_some() {
            self.paused = !self.paused;
        }
    }

    /// Latches the action lit and moves the chain toward it. From the flat strip
    /// both travel modes fold straight toward `action`. Off the flat strip, `Glide`
    /// unwraps then refolds to the far side in one sweep (the unfold-first /
    /// unfold-then-redo rule), while `Step` advances a single rest-stage toward
    /// `action` and stops.
    fn act(&mut self, action: FoldAction) {
        self.lit.action = action;
        self.paused = false;
        if self.is_flat() {
            self.folded = self.lit;
            self.refold = false;
            self.ease_to(HingePhase::Folding);
            return;
        }
        match self.travel {
            FoldTravel::Glide => {
                self.refold = true;
                self.ease_to(HingePhase::Unwrapping);
            },
            FoldTravel::Step => {
                self.refold = false;
                if action == self.folded.action {
                    self.ease_to(HingePhase::Folding);
                } else {
                    self.ease_to(HingePhase::Unwrapping);
                }
            },
        }
    }

    /// Unwraps the chain back to the flat strip and relights `Down` (`R`).
    pub(crate) fn reset(&mut self) {
        self.lit.action = FoldAction::Down;
        self.refold = false;
        self.paused = false;
        self.ease_to(HingePhase::Unwrapping);
    }

    /// Whether the chain is settled flat (the spread-out strip), the only state a
    /// fold may start straight from.
    fn is_flat(self) -> bool { self.phase.is_none() && self.unwrap >= 1.0 }

    /// Starts an ease toward `phase`'s rest state from the current `unwrap`, ignored
    /// if that ease is already running.
    fn ease_to(&mut self, phase: HingePhase) {
        if self.phase == Some(phase) {
            return;
        }
        self.phase = Some(phase);
        self.from_unwrap = self.unwrap;
        self.timer = 0.0;
    }
}

impl TitleChipActivation for HingeChain {
    /// Highlights the title-bar `Pause` word while a hinge fold is frozen.
    fn activation(&self) -> ControlActivation {
        if self.paused {
            ControlActivation::Active
        } else {
            ControlActivation::Inactive
        }
    }
}

/// Per-crease fold sign for a folding link `steps` links from the fixed top under
/// `pattern`. An accordion alternates the sign so adjacent creases bend opposite
/// ways (zigzag); a coil keeps it constant so each crease bends the same way and
/// the links roll onto one another.
const fn crease_sign(pattern: FoldPattern, steps: usize) -> f32 {
    match pattern {
        FoldPattern::Accordion if steps.is_multiple_of(2) => -1.0,
        FoldPattern::Accordion | FoldPattern::Coil => 1.0,
    }
}

/// Whether the stack leans toward the camera (`Front`) or away (`Back`), negating
/// the fold angle so the same creases close to the opposite side.
const fn fold_lean(direction: FoldDirection) -> f32 {
    match direction {
        FoldDirection::Back => 1.0,
        FoldDirection::Front => -1.0,
    }
}

/// Advances the hinge unwrap/fold ease by `dt`, lerping `unwrap` from its value
/// when the ease began toward the target rest state. When an unwrap that was queued
/// for a refold reaches the flat strip, it commits the lit selection and folds;
/// otherwise the phase clears.
pub(crate) fn advance_hinge(hinge: &mut HingeChain, dt: f32) {
    let Some(phase) = hinge.phase else {
        return;
    };
    hinge.timer += dt;
    let progress = smoothstep((hinge.timer / HINGE_EASE_SECS).clamp(0.0, 1.0));
    let target = match phase {
        HingePhase::Unwrapping => 1.0,
        HingePhase::Folding => 0.0,
    };
    hinge.unwrap = (target - hinge.from_unwrap).mul_add(progress, hinge.from_unwrap);
    if hinge.timer < HINGE_EASE_SECS {
        return;
    }
    hinge.unwrap = target;
    hinge.phase = None;
    if phase == HingePhase::Unwrapping && hinge.refold {
        hinge.refold = false;
        hinge.folded = hinge.lit;
        hinge.ease_to(HingePhase::Folding);
    }
}

/// Whether the hinge fold ease is currently frozen by `P`/`Space`.
pub(crate) const fn hinge_paused(hinge: &HingeChain) -> bool { hinge.paused }

/// Writes the right-axis hinge rotation onto every folding tile in
/// `PanelSystems::AnimateAnchorPose`, before the world resolver. Tile `0` is the
/// fixed top and carries no pose; every later tile tilts about its pinned top edge
/// by the hinge angle times its crease sign (alternating for an accordion, constant
/// for a coil) and the committed lean, so the chain collapses toward tile `0` and
/// unwraps to a coplanar strip. Runs only while the hinge chain is active and no
/// mode morph is in flight (the morph owns the poses then).
pub(crate) fn drive_hinge_pose(
    active: Res<ActiveCapability>,
    morph: Res<ModeMorph>,
    hinge: Res<HingeChain>,
    chain: Res<AnchorChain>,
    mut poses: Query<(&mut PanelAnchorPose, &AnchorTile)>,
) {
    if morph.active() || active.index != HINGE_CHAIN_INDEX {
        return;
    }
    let angle = hinge.link_angle();
    let count = chain.count();
    let fold = hinge.folded;
    let down = matches!(fold.action, FoldAction::Down);
    let lean = fold_lean(fold.direction) * if down { -1.0 } else { 1.0 };
    for (mut pose, tile) in &mut poses {
        if tile.order == 0 || tile.order >= count {
            continue;
        }
        let sign = crease_sign(fold.pattern, tile.order);
        pose.rotation = Quat::from_rotation_x(angle * sign * lean);
        pose.translation = Vec3::ZERO;
    }
}

/// A hinge-chain tile's layout tree: its link number centered, accented per
/// `order`/`count` on the shared color wheel. Built at the shared tile size.
pub(crate) fn build_hinge_number_tree(order: usize, count: usize) -> LayoutTree {
    let accent = hinge_accent(order, count);
    let mut builder = LayoutBuilder::new(Mm(PANEL_WIDTH), Mm(PANEL_HEIGHT));
    builder.with(
        El::column()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(HINGE_BACKGROUND)
            .border(Border::all(HINGE_BORDER_WIDTH, accent))
            .alignment(AlignX::Center, AlignY::Center),
        |builder| {
            builder.text(((order + 1).to_string(), hinge_label_style(accent)));
        },
    );
    builder.build()
}

/// The link's accent, hue-walked around the color wheel by its index over the
/// current count, so a fuller chain spreads its accents across more of the wheel.
/// Shared with the anchor fan, whose tiles follow the same color wheel.
pub(crate) fn hinge_accent(order: usize, count: usize) -> Color {
    let index = steps_to_f32(order);
    let scale = steps_to_f32(count.max(1)).max(f32::EPSILON);
    let hue = (HINGE_ACCENT_HUE_START + 360.0 * index / scale).rem_euclid(360.0);
    Color::hsl(hue, HINGE_ACCENT_SATURATION, HINGE_ACCENT_LIGHTNESS)
}

fn steps_to_f32(count: usize) -> f32 {
    let mut steps = 0.0;
    for _ in 0..count {
        steps += 1.0;
    }
    steps
}

fn hinge_label_style(accent: Color) -> TextStyle {
    TextStyle::new(HINGE_LABEL_SIZE)
        .with_color(accent)
        .with_shadow_mode(GlyphShadowMode::None)
}
