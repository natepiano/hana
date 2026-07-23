# Widgets V2: Presets and Themes

> **Status: DEFERRED.** Design this only after applications have used the direct button, slider, and tooltip authoring APIs together. The current widget plan remains in [`widgets.md`](widgets.md).

## Why this is deferred

The first widget API should expose the behavior and state-specific visual inputs each widget actually needs. A preset or theme system designed before several widgets are in use would guess too early about shared structure, naming, inheritance, and customization.

Widgets v1 therefore provides:

- headless widget behavior and events;
- ordinary `El`/`LayoutTree` authoring for normal appearance and rich content;
- direct widget-builder inputs for state-specific retained presentation;
- private retained visual slots that update appearance without relayout.

No `ButtonPreset`, `ButtonStyle`, `SliderPreset`, `SliderStyle`, `TooltipPreset`, or `TooltipStyle` type is part of widgets v1. Those names are historical placeholders, not approved future API names.

## Questions for widgets v2

Design the preset/theme layer from real button, slider, tooltip, and later widget usage. Revisit:

- whether the public abstraction should be a preset, a theme, a scene/template helper, or some combination;
- whether applications choose small variants such as `Normal`, `Primary`, and `Plain`, theme keys, complete per-instance values, or layered overrides;
- which values belong globally in a theme and which belong on one widget instance;
- how state-dependent child text, images, icons, shapes, and animations are addressed without restricting rich layout content;
- how focus, hover, press, disabled, selected, drag, and validation states compose;
- whether materials stay limited to `Handle<StandardMaterial>` or gain a public custom/extended-material contract;
- whether a slider preset includes a variable-length fill and, if so, what retained geometry operation resizes it;
- whether tooltips need any default presentation beyond their application-authored `TooltipTemplate` tree.

## Constraints inherited from widgets v1

Any later convenience layer should build on the direct widget APIs rather than replace them:

- behavior must remain usable without a preset or theme;
- normal structure and content remain ordinary layout authoring;
- runtime state presentation must reuse retained visual slots and avoid relayout when geometry is unchanged;
- preset/theme code must not become a second authority for click, drag, focus, disabled, tooltip, or application-owned value state;
- applications must remain free to build custom widget layout and presentation from the same state and events.

