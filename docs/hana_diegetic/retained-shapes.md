# Retained Shapes

Goal: support retained per-shape updates for panel primitives, with transform,
material, and record channels comparable to text runs.

The motivating case is a clock face. Rotating a second hand should update one
retained member transform without rebuilding the path atlas or replaying the
full panel command stream.

## Constraints

1. Future per-member update channels use `BatchStore::member_batch_mut`; no new
   generic store API is required.
2. The path atlas stays local to the panel-shape store. A later
   member-keyed-to-content-keyed outline swap can be local: identical outlines
   can share one atlas entry, and transform-only member updates touch the atlas
   zero times. Today the atlas rebuild is wholesale and marks every `ShapeBatch`
   geometry-dirty; content-keyed entries address that in the follow-on.
3. Incremental authoring is out of scope for this bookkeeping phase. Updating
   one authored shape without full panel relayout still needs retained
   transform, material, and record channels, but those channels should reuse the
   per-member routing added here.
