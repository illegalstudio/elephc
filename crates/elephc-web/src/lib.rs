//! Purpose:
//! C-ABI surface for the elephc `--web` prefork HTTP server bridge. Exposes the
//! server entry point and (in later phases) request/response marshaling under
//! `#[no_mangle] extern "C"` symbols the compiled PHP program calls/links.
//!
//! Called from:
//! - Compiled `--web` binaries: the emitted process entry tail-calls
//!   `elephc_web_run`; the staticlib is linked via the `BRIDGES` table in
//!   `crate::linker`.
//! - Tests: directly through the `rlib` crate type.
//!
//! Key details:
//! - One process per prefork worker means no shared-thread state: per-worker
//!   request/response data lives in plain process statics, not behind a mutex.

mod request_state;
mod server;
mod worker;

// Re-exported so the compiled `--web` runtime's `__rt_stdout_write` capture
// branch links against the real per-request output sink (defined in
// `request_state`), which replaced the Phase-1 no-op stub.
pub use request_state::elephc_web_write;

/// Returns the elephc-web C ABI version. Bumped when the exported symbol set or
/// any symbol's signature changes shape.
#[no_mangle]
pub extern "C" fn elephc_web_version() -> i32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the crate links and the ABI version constant is the v1 value.
    #[test]
    fn version_is_one() {
        assert_eq!(elephc_web_version(), 1);
    }
}
