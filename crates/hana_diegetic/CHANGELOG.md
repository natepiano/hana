# Changelog

## Unreleased

### Breaking Changes

- `Cascade<T>` has moved to `bevy_kana` and is no longer re-exported by
  `hana_diegetic`. Public authored-state getters that returned `Cascade<T>` are
  now internal; use the existing domain builders, typed `override_*` /
  `inherit_*` commands, and `resolved_*` readers. Panel cascade builders now
  seed components only at spawn. Runtime panel material updates use
  `override_sdf_material` / `inherit_sdf_material`,
  `override_text_material` / `inherit_text_material`, and
  `override_shape_material` / `inherit_shape_material` on entity commands;
  the former `&mut DiegeticPanel` material mutators have been removed. The
  compatibility `DiegeticPanel::surface_shadow` seed getter has also been
  removed; use `resolved_shadow_casting` for runtime inspection.
- Cascade propagation now uses `bevy_kana`'s shared `CascadePlugin<A>` and
  explicit `CascadeFrom` relationship. `ChildOf` no longer implies inherited
  diegetic attributes; panel text construction inserts both relationships for
  their independent responsibilities.
- `LineStyle::hairline_fade_value` is replaced by
  `LineStyle::hairline_fade_override` for public override inspection.
- `AnchoredToPanel` has been removed. Attach, retarget, and detach through
  `DiegeticPanelCommands` on Bevy `Commands` using same-space `PanelEntity<Space>` and
  `WidgetEntity<Space>` handles. Query `hana_valence::AnchoredTo` only for
  world-space attachments after they have been lowered into the shared
  resolver.
- `PanelsAnchoredHere` has moved to `hana_valence::AnchoredHere`, and the diegetic re-export has been removed.
- `PanelAnchorPose` has moved to `hana_valence::AnchorPose`.
- Public world/screen conversion now uses `DiegeticPanelCommands` methods on the
  same Bevy `Commands` value as attachment changes. Calls retain written order,
  and conversion is rejected while the panel, or one of its widgets,
  participates in an attachment.
- Apps driving panel hinges with `HingeAngleLens` must order `TweenSystemSet::ApplyTween` before `hana_valence::hinge_to_pose`. Moving `TweenSystemSet::ApplyTween` is app-global and retimes every tween in the app, not just valence lens tweens.
