//! GitHub API client traits and types.
//!
//! Defines a `GitHubClient` trait abstracting over platform-specific HTTP
//! clients (`reqwest` on native, `gloo-net` on Wasm). Concrete implementations
//! will be added in Phase 1 when HTTP dependencies are introduced.

// Phase 1: implement GitHubClient trait and REST API client.
