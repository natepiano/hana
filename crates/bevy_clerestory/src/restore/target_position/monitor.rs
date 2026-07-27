use crate::monitors::MonitorInfo;
use crate::monitors::Monitors;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MonitorResolutionSource {
    Requested,
    FallbackToPrimary,
}

pub(crate) struct ResolvedMonitor<'a> {
    pub monitor_info:              &'a MonitorInfo,
    pub monitor_resolution_source: MonitorResolutionSource,
}

/// Resolve the target monitor from a saved monitor index.
///
/// A saved position is only meaningful relative to the monitor it was measured from, so a
/// `FallbackToPrimary` resolution discards it — the caller must not carry a position past a
/// monitor it could not resolve.
#[must_use]
pub(crate) fn resolve_target_monitor(
    saved_monitor_index: usize,
    monitors: &Monitors,
) -> ResolvedMonitor<'_> {
    monitors.by_index(saved_monitor_index).map_or_else(
        || ResolvedMonitor {
            monitor_info:              monitors.first(),
            monitor_resolution_source: MonitorResolutionSource::FallbackToPrimary,
        },
        |monitor_info| ResolvedMonitor {
            monitor_info,
            monitor_resolution_source: MonitorResolutionSource::Requested,
        },
    )
}
