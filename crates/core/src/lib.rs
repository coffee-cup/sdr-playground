//! Shared types and traits: sample types, `Event`, configuration, and the central
//! traits (`Source`, `Demodulator`, `Decoder`, the tap/stage interface).
//!
//! This is the root of the dependency graph and stays IO-free with no dependencies;
//! every other crate depends on it. See `docs/ARCHITECTURE.md`.
//!
//! Intentionally empty until the pipeline lands — no radio yet.
