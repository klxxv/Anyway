//! Privileged Anyway kernel vocabulary and phase-one state models.
//!
//! This module is intentionally wired into the crate before legacy Tauri
//! commands are routed through it. That keeps the new contracts continuously
//! compiled while the migration proceeds without a flag-day rewrite.

pub mod blob;
pub mod identity;
pub mod lifecycle;
pub mod rpc;
pub mod supervisor;
