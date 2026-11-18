//! Shared physical inputs, diagnostics, and result quantities for CAEKit.

pub mod error;
mod material;
mod pressure;
mod quantity;

pub use error::Error;
pub use material::MaterialSource;
pub use pressure::Pressure;
pub use quantity::{Balance, InterfaceValue, Output, Quantity, SampleLocation, Solution};
