//! rocketviz core: hosts simulation engines, runs Monte Carlo analyses, and turns trajectories into the data the
//! app displays and exports.

pub mod engine;
pub mod error;

pub use error::{Error, Result};
