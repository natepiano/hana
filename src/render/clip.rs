//! Clip rect computation from layout render commands.
//!
//! Walks a `&[RenderCommand]` slice once, maintaining a stack of nested
//! clip regions from [`ScissorStart`]/[`ScissorEnd`] pairs. Returns a
//! parallel `Vec` where each entry is the active clip rect (or `None`
//! if the command is outside any clip region).

use crate::layout::BoundingBox;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;

/// Computes the active clip rect for each render command.
///
/// Returns a `Vec` parallel to `commands` where each entry is the
/// active clip rect at that command's position in the stream, or
/// `None` if the command is not inside any `ScissorStart` region.
///
/// Nested clips are intersected — a child clip region is the overlap
/// of its bounds with the parent's active clip.
pub(super) fn compute_clip_rects(commands: &[RenderCommand]) -> Vec<Option<BoundingBox>> {
    let mut result = Vec::with_capacity(commands.len());
    let mut stack: Vec<BoundingBox> = Vec::new();

    for cmd in commands {
        match cmd.kind {
            RenderCommandKind::ScissorStart => {
                let new_clip = stack
                    .last()
                    .and_then(|current| current.intersect(&cmd.bounds))
                    .unwrap_or(cmd.bounds);
                stack.push(new_clip);
            },
            RenderCommandKind::ScissorEnd => {
                stack.pop();
            },
            _ => {},
        }
        result.push(stack.last().copied());
    }

    result
}

#[cfg(test)]
mod tests {
    use bevy::color::Color;

    use super::*;
    use crate::layout::RectangleSource;

    fn bbox(x: f32, y: f32, width: f32, height: f32) -> BoundingBox {
        BoundingBox {
            x,
            y,
            width,
            height,
        }
    }

    fn rect_cmd(bounds: BoundingBox) -> RenderCommand {
        RenderCommand {
            bounds,
            kind: RenderCommandKind::Rectangle {
                color:  Color::WHITE,
                source: RectangleSource::Background,
            },
            element_idx: 0,
        }
    }

    fn scissor_start(bounds: BoundingBox) -> RenderCommand {
        RenderCommand {
            bounds,
            kind: RenderCommandKind::ScissorStart,
            element_idx: 0,
        }
    }

    fn scissor_end(bounds: BoundingBox) -> RenderCommand {
        RenderCommand {
            bounds,
            kind: RenderCommandKind::ScissorEnd,
            element_idx: 0,
        }
    }

    #[test]
    fn empty_commands() {
        let result = compute_clip_rects(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn no_scissor_commands() {
        let commands = vec![
            rect_cmd(bbox(0.0, 0.0, 100.0, 100.0)),
            rect_cmd(bbox(10.0, 10.0, 50.0, 50.0)),
        ];
        let result = compute_clip_rects(&commands);
        assert!(result.iter().all(Option::is_none));
    }

    #[test]
    fn single_scissor_pair() {
        let clip = bbox(0.0, 0.0, 100.0, 100.0);
        let commands = vec![
            rect_cmd(bbox(0.0, 0.0, 200.0, 200.0)), // before clip
            scissor_start(clip),
            rect_cmd(bbox(10.0, 10.0, 50.0, 50.0)), // inside clip
            scissor_end(clip),
            rect_cmd(bbox(0.0, 0.0, 200.0, 200.0)), // after clip
        ];
        let result = compute_clip_rects(&commands);
        assert!(result[0].is_none());
        assert!(result[1].is_some()); // ScissorStart itself is inside
        assert_eq!(result[2].unwrap(), clip);
        assert!(result[3].is_none()); // ScissorEnd pops
        assert!(result[4].is_none());
    }

    #[test]
    fn nested_clips_intersect() {
        let outer = bbox(0.0, 0.0, 100.0, 100.0);
        let inner = bbox(50.0, 50.0, 100.0, 100.0);
        // Intersection: (50, 50, 50, 50)
        let commands = vec![
            scissor_start(outer),
            scissor_start(inner),
            rect_cmd(bbox(60.0, 60.0, 10.0, 10.0)),
            scissor_end(inner),
            rect_cmd(bbox(10.0, 10.0, 10.0, 10.0)),
            scissor_end(outer),
        ];
        let result = compute_clip_rects(&commands);
        let nested_clip = result[2].unwrap();
        assert!((nested_clip.x - 50.0).abs() < f32::EPSILON);
        assert!((nested_clip.y - 50.0).abs() < f32::EPSILON);
        assert!((nested_clip.width - 50.0).abs() < f32::EPSILON);
        assert!((nested_clip.height - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn pop_restores_parent_clip() {
        let outer = bbox(0.0, 0.0, 100.0, 100.0);
        let inner = bbox(50.0, 50.0, 100.0, 100.0);
        let commands = vec![
            scissor_start(outer),
            scissor_start(inner),
            scissor_end(inner),
            rect_cmd(bbox(10.0, 10.0, 10.0, 10.0)), // should be outer clip
            scissor_end(outer),
        ];
        let result = compute_clip_rects(&commands);
        assert_eq!(result[3].unwrap(), outer);
    }

    #[test]
    fn deeply_nested_three_levels() {
        let a = bbox(0.0, 0.0, 200.0, 200.0);
        let b = bbox(50.0, 50.0, 200.0, 200.0);
        let c = bbox(80.0, 80.0, 200.0, 200.0);
        let commands = vec![
            scissor_start(a),
            scissor_start(b),
            scissor_start(c),
            rect_cmd(bbox(90.0, 90.0, 10.0, 10.0)),
            scissor_end(c),
            scissor_end(b),
            scissor_end(a),
        ];
        let result = compute_clip_rects(&commands);
        // a ∩ b = (50, 50, 150, 150), then ∩ c = (80, 80, 120, 120)
        let deepest = result[3].unwrap();
        assert!((deepest.x - 80.0).abs() < f32::EPSILON);
        assert!((deepest.y - 80.0).abs() < f32::EPSILON);
        assert!((deepest.width - 120.0).abs() < f32::EPSILON);
        assert!((deepest.height - 120.0).abs() < f32::EPSILON);
    }
}
