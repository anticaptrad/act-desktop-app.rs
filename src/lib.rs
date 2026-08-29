//! Native media, networking, and Qt control plane for the `AntiCapTrad` studio.

#[allow(clippy::all, clippy::pedantic)]
#[path = "../generated/rust/env.rs"]
#[rustfmt::skip]
mod env;
#[allow(clippy::all, clippy::pedantic)]
#[path = "../generated/rust/runtime.rs"]
#[rustfmt::skip]
mod env_runtime;

pub mod bridge;
pub mod core;
pub mod media;
pub mod runtime;

/// Keeps the native library linked into the Qt application and exposes a
/// stable diagnostic for future packaging checks.
#[must_use]
pub const fn native_stack_summary() -> &'static str {
    runtime::RuntimeSupervisor::stack_summary()
}
