mod dispatch;
mod held;
mod key_edge;

#[cfg_attr(
    not(test),
    expect(
        unused_imports,
        reason = "the recovery-chord handler is the caller; it lives in the application crate"
    )
)]
pub(crate) use dispatch::cancel_pending_sequences;
pub(crate) use dispatch::route_input;
pub(crate) use held::KeymapRuntime;
