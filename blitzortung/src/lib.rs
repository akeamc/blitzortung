#![warn(
    clippy::pedantic,
    clippy::nursery,
    missing_docs,
    missing_debug_implementations,
    unreachable_pub
)]

#[cfg(feature = "live")]
pub mod live;

#[cfg(feature = "live")]
mod stream;
