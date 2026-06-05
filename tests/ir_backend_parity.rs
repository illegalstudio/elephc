//! Purpose:
//! Integration test root for EIR backend parity checks against the legacy backend.
//!
//! Called from:
//! - `cargo test --test ir_backend_parity` through Rust's test harness.
//!
//! Key details:
//! - Test bodies live under `tests/ir_backend_parity/` so parity regressions can
//!   grow as a focused corpus separate from smoke tests.

#[path = "ir_backend_parity/cases.rs"]
mod cases;
