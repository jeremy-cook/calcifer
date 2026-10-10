//! Request-level security policies, applied as middleware around the routers.
//!
//! - [`local`] only answers requests from this machine (ADR 5: there is no auth).

pub mod local;
