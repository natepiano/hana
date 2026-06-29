# Precompose API Review

2026-06-27 adhoc review of the proposed precompose API types from the HDR text discussion.

## Decisions

- `PanelElementId` is the shared panel-local identity type. `El::id(...)`
  applies to any layout element; `Text::id(...)` remains the text-focused
  convenience path and stores the same element id on the text leaf.
- Named ids share one namespace across normal element ids, text element ids,
  and editable field ids. A container can have one id and a child text element
  can have another id; reusing the same named id anywhere in the panel tree is
  a build error.
- The first public precompose API is `El::precompose_ldr()`. No public enum is
  exposed until there is a second mode with real semantics.
- `precompose_ldr()` flattens that element subtree through an LDR render target
  and draws the cached output as one image in the source panel. The source
  panel's normal render command stream stops at that element, so descendant text,
  fills, borders, and shapes do not also draw through the standard path.
- The cached image is owned per source element. Its allocation is reused while
  the computed boundary pixel size is unchanged and resized when the boundary
  changes.
- The output behaves like a single alpha-blended image in the parent panel. It
  does not preserve per-descendant depth, ordering against scene geometry, or
  interactive descendants.
