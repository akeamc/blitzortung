//! Blitzortung.org client.
//!
//! ## Live data
//!
//! For realtime data, the Blitzortung.org websocket servers are used.
//! **This requires the `live` feature to be enabled.**
#![warn(
    clippy::pedantic,
    clippy::nursery,
    missing_docs,
    missing_debug_implementations
)]
#![allow(clippy::missing_panics_doc)]
#![cfg_attr(docsrs, feature(doc_auto_cfg, doc_cfg))]

#[cfg(feature = "live")]
pub mod live;
