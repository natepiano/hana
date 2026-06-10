//! Generic atlas for non-text analytic paths.

use std::collections::HashMap;
use std::hash::Hash;

use bevy::prelude::Assets;
use bevy::render::storage::ShaderBuffer;
use bevy_kana::ToU32;

use super::BandRecord;
use super::CurveRecord;
use super::GlyphRecord;
use super::PathAtlasHandles;
use super::PathOutline;
use super::build_packed_path;

/// Compact atlas keyed by caller-owned stable path identities.
#[derive(Debug)]
pub(crate) struct PathAtlas<K> {
    indices:           HashMap<K, u32>,
    curves:            Vec<CurveRecord>,
    bands:             Vec<BandRecord>,
    path_records:      Vec<GlyphRecord>,
    revision:          u32,
    uploaded_revision: u32,
    handles:           Option<PathAtlasHandles>,
}

impl<K> Default for PathAtlas<K> {
    fn default() -> Self {
        Self {
            indices:           HashMap::new(),
            curves:            Vec::new(),
            bands:             Vec::new(),
            path_records:      Vec::new(),
            revision:          0,
            uploaded_revision: u32::MAX,
            handles:           None,
        }
    }
}

impl<K> PathAtlas<K>
where
    K: Clone + Eq + Hash,
{
    /// Rebuilds the atlas from the caller's live path set.
    pub fn rebuild<I>(&mut self, paths: I, band_count: usize)
    where
        I: IntoIterator<Item = (K, PathOutline)>,
    {
        self.indices.clear();
        self.curves.clear();
        self.bands.clear();
        self.path_records.clear();

        for (key, path) in paths {
            let packed = build_packed_path(path, band_count);
            let record_index = self.path_records.len().to_u32();
            let curve_start = self.curves.len().to_u32();
            let band_start = self.bands.len().to_u32();
            let axis_band_count = packed.bands().len().to_u32() / 2;

            self.curves.extend_from_slice(packed.curves());
            self.bands
                .extend(packed.bands().iter().map(|band| BandRecord {
                    start: band.start + curve_start,
                    ..*band
                }));
            self.path_records.push(GlyphRecord::new(
                packed.bounds(),
                band_start,
                axis_band_count,
                band_start + axis_band_count,
                axis_band_count,
            ));
            self.indices.insert(key, record_index);
        }

        self.revision = self.revision.wrapping_add(1);
        if self.path_records.is_empty() {
            self.handles = None;
            self.uploaded_revision = u32::MAX;
        }
    }

    /// Atlas slot for `key`, if the last rebuild included it.
    #[must_use]
    pub fn index(&self, key: &K) -> Option<u32> { self.indices.get(key).copied() }

    /// Uploads atlas storage buffers when the compact atlas changed.
    pub fn upload(
        &mut self,
        storage_buffers: &mut Assets<ShaderBuffer>,
    ) -> Option<(PathAtlasHandles, bool)> {
        if self.path_records.is_empty() {
            return None;
        }
        if let Some(handles) = self.handles.clone()
            && self.uploaded_revision == self.revision
        {
            return Some((handles, false));
        }

        let handles = PathAtlasHandles {
            curves: storage_buffers.add(ShaderBuffer::from(self.curves.clone())),
            bands:  storage_buffers.add(ShaderBuffer::from(self.bands.clone())),
            glyphs: storage_buffers.add(ShaderBuffer::from(self.path_records.clone())),
        };
        self.uploaded_revision = self.revision;
        self.handles = Some(handles.clone());
        Some((handles, true))
    }
}
