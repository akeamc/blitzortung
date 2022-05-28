#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

#[cfg(feature = "live")]
pub mod live;

#[cfg(feature = "live")]
mod stream;
