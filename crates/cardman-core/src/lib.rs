//! Platform-agnostic core logic for Cardman.
//!
//! Provides domain models, card-to-column mapping, GitHub API client traits,
//! and OAuth helpers. Compiles to both native and `wasm32-unknown-unknown`.

pub mod auth;
pub mod github;
pub mod mapping;
pub mod models;
