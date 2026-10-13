//! rocketviz core: hosts simulation engines, runs Monte Carlo analyses, and turns trajectories into the data the
//! app displays and exports.

pub mod analysis;
pub mod engine;
pub mod error;
pub mod export;
pub mod montecarlo;
pub mod tiles;

pub use error::{Error, Result};
