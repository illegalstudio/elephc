//! Purpose:
//! The one place the in-crate WASM unit tests ask whether the `wasmer` host exists.
//!
//! Called from:
//! - the `wasmer_available` helpers of the `#[cfg(test)]` modules under `crate::codegen_wasm`
//!   (mod, gc, arrays, float, hashes, heap, mixed, closures, refcount, runtime, strict,
//!   transfer_tests).
//!
//! Key details:
//! - Same contract as `wasmer_missing_skips_test` in `tests/codegen/support/projects.rs`: a
//!   missing host prints a `SKIPPED <test>: ...` line, and with `ELEPHC_TEST_REQUIRE_WASM_HOSTS`
//!   set (CI installs the hosts and sets it) it fails the test instead. These tests used to
//!   return early without a word and ignored the variable, so a CI image without wasmer went
//!   green while running nothing.

/// Answers whether `wasmer` can be started; a missing host skips loudly or fails under
/// `ELEPHC_TEST_REQUIRE_WASM_HOSTS`.
pub(super) fn wasmer_available() -> bool {
    let present = std::process::Command::new("wasmer")
        .arg("--version")
        .output()
        .is_ok();
    !host_missing_skips(present, std::env::var_os("ELEPHC_TEST_REQUIRE_WASM_HOSTS").is_some())
}

/// The decision behind `wasmer_available`, split out so the contract can be unit tested.
///
/// Returns true when the test must be skipped (host missing, not required). Panics when the host
/// is missing but required. The `SKIPPED` line names the running test.
fn host_missing_skips(present: bool, required: bool) -> bool {
    if present {
        return false;
    }
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("<unnamed test>");
    assert!(
        !required,
        "{test}: `wasmer` cannot be started although ELEPHC_TEST_REQUIRE_WASM_HOSTS is set"
    );
    eprintln!(
        "SKIPPED {test}: the `wasmer` runtime is not installed or not on PATH, so the generated \
         wasm module was validated but not executed"
    );
    true
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the skip-or-fail contract of the wasmer host probe.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.

    use super::host_missing_skips;

    /// A present host never skips, required or not.
    #[test]
    fn a_present_host_runs() {
        assert!(!host_missing_skips(true, false));
        assert!(!host_missing_skips(true, true));
    }

    /// A missing optional host skips, with a SKIPPED line on stderr.
    #[test]
    fn a_missing_optional_host_skips() {
        assert!(host_missing_skips(false, false));
    }

    /// A missing REQUIRED host fails the test instead of skipping it.
    #[test]
    #[should_panic(expected = "ELEPHC_TEST_REQUIRE_WASM_HOSTS is set")]
    fn a_missing_required_host_fails() {
        host_missing_skips(false, true);
    }
}
