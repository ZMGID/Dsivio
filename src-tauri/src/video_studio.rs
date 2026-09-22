//! Native video provider protocols. Shared media task handling lives in media_generation.
pub(crate) mod providers;
pub(crate) mod migration;
pub use providers::{RequestPreview, VideoInput, VideoResult};
