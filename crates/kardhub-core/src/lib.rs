//! Platform-agnostic core logic for KardHub.
//!
//! Provides domain models, card-to-column mapping, GitHub API client traits,
//! and OAuth helpers. Compiles to both native and `wasm32-unknown-unknown`.

pub mod auth;
pub mod filtering;
pub mod github;
pub mod linking;
pub mod mapping;
pub mod markdown;
pub mod models;
