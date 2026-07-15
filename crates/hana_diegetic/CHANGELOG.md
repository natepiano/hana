# Changelog

## Unreleased

### Breaking Changes

- `AnchoredToPanel` is now an insert-only bundle, not a queryable component. Query `hana_valence::AnchoredTo` for world-space panel attachments that have been lowered into the shared resolver.
- `PanelsAnchoredHere` has moved to `hana_valence::AnchoredHere`, and the diegetic re-export has been removed.
- `PanelAnchorPose` has moved to `hana_valence::AnchorPose`.
- `Anchor::TopLeft` authoring and `AnchoredToPanel::new` call sites are unchanged.
- Apps driving panel hinges with `HingeAngleLens` must order `TweenSystemSet::ApplyTween` before `hana_valence::hinge_to_pose`. Moving `TweenSystemSet::ApplyTween` is app-global and retimes every tween in the app, not just valence lens tweens.
