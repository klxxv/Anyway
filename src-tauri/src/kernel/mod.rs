//! Privileged Anyway kernel vocabulary and phase-one state models.
//!
//! This module is intentionally wired into the crate before legacy Tauri
//! commands are routed through it. That keeps the new contracts continuously
//! compiled while the migration proceeds without a flag-day rewrite.

pub mod audit;
pub mod blob;
pub mod bus;
pub mod identity;
pub mod lifecycle;
pub mod package_gate;
pub mod policy;
pub mod rpc;
pub mod scheduler;
pub mod service_registry;
pub mod state;
pub mod supervisor;
