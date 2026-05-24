//! Per-verb IPC handlers. Each module owns one verb family. The
//! dispatcher in [`super::dispatch`] just routes by verb and reports
//! back whether the response should trigger a notifier.

pub mod bind;
pub mod layout;
pub mod mode;
pub mod misc;
pub mod tag;
pub mod window;
