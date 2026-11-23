//! Covering designs: an exact checker, a text form with a stable digest, the Schoenheim lower bound,
//! and a local search that looks for coverings with fewer blocks than the best known.
//!
//! The checker is the part everything else leans on. It is small, has no tuning and no randomness,
//! and the same code runs natively and, built for `wasm32`, in a browser.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod bound;
pub mod check;
pub mod design;
#[cfg(not(target_arch = "wasm32"))]
pub mod lab;
pub mod rng;
pub mod search;

// The browser ABI hands raw pointers across the boundary, which needs `unsafe`.
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
mod wasm;

pub use check::{check, Fault, Report};
pub use design::{Covering, ParseError};
