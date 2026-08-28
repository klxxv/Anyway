//! Native plugin boundaries.
//!
//! Provider-specific behavior belongs in this module tree. The generic LLM
//! kernel remains provider-neutral and is only used for providers that do not
//! need a native request policy.

pub mod pdf_canvas_agent;
