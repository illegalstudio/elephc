//! Purpose:
//! Groups the `__rt_curl_*` runtime helpers backing PHP's `ext/curl` easy handle. Each
//! one calls the `elephc_curl` C ABI through a fail-closed function-pointer slot, so the
//! SHARED, CACHED runtime object never names an `elephc_curl_*` symbol and a curl-free
//! program neither references the bridge nor needs `libcurl.a`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - `__rt_curl_easy_free` additionally from `__rt_mixed_free_deep`'s resource-kind
//!   ladder (kind 6), which is what makes a `CurlHandle` object's teardown release the
//!   native handle.
//!
//! Key details:
//! - WHY INDIRECT SLOTS AND NOT DIRECT CALLS. These helpers live in the runtime object,
//!   which is built once per (target, heap size, feature set) and reused across programs.
//!   A direct `bl elephc_curl_easy_init` there would make EVERY compiled program carry an
//!   undefined libcurl symbol. The slots (`_elephc_curl_*_fn`, `.comm`-declared in
//!   `runtime::data::fixed`, zero by default) are published by the curl lowerings, which
//!   only ever appear in a program that actually uses curl. Same pattern, and same
//!   reasoning, as `_elephc_crypto_*_fn` (`runtime::strings::hash_context`).
//! - EVERY HELPER FAILS CLOSED on a null slot: it returns curl's own "nothing happened"
//!   answer (`false` / `0` / `""`) rather than branching into an unlinked symbol. A null
//!   slot means the bridge is not linked, which cannot happen on a path that reached a
//!   curl builtin, so these arms are defensive only.
//! - HANDLE IDENTITY: the ABI addresses easy handles by a small positive `i64` id from
//!   the bridge's own table, never by pointer. Id `0` is the bridge's "allocation failed"
//!   answer, so it is also the one id `__rt_curl_easy_init` turns into PHP `false`.

mod easy_body;
mod easy_error;
mod easy_free;
mod easy_init;
mod easy_scalar;
pub(crate) mod slots;
mod version;

pub(crate) use easy_body::emit_curl_easy_body;
pub(crate) use easy_error::emit_curl_easy_error;
pub(crate) use easy_free::emit_curl_easy_free;
pub(crate) use easy_init::emit_curl_easy_init;
pub(crate) use easy_scalar::emit_curl_easy_scalar_helpers;
pub(crate) use version::emit_curl_version;

/// Emits every `__rt_curl_*` helper for the target.
pub(crate) fn emit_curl(emitter: &mut crate::codegen_support::emit::Emitter) {
    emit_curl_easy_init(emitter);
    emit_curl_easy_scalar_helpers(emitter);
    emit_curl_easy_body(emitter);
    emit_curl_easy_error(emitter);
    emit_curl_version(emitter);
    emit_curl_easy_free(emitter);
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the curl runtime's two structural contracts on ALL THREE supported targets:
    //! every helper and slot exists, and the shared runtime object references no
    //! `elephc_curl_*` symbol.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These are assembly-text assertions rather than execution tests deliberately: the
    //!   two Linux targets cannot be RUN from a macOS developer machine, and "was it
    //!   emitted for linux-x86_64 at all" is exactly the failure an aarch64-only test
    //!   would miss (locked decision 11 requires all three targets in the same change).
    //! - The no-`elephc_curl_*`-symbol assertion is the pay-for-use invariant in its most
    //!   direct form: the runtime object is built once and linked into EVERY program, so a
    //!   single direct reference there would make every compiled binary demand libcurl.

    use crate::codegen_support::platform::Target;
    use crate::codegen_support::RuntimeFeatures;

    /// Every `__rt_curl_*` entry point the lowerings and `__rt_mixed_free_deep` call.
    const CURL_RUNTIME_LABELS: &[&str] = &[
        "__rt_curl_easy_init",
        "__rt_curl_easy_setopt_long",
        "__rt_curl_easy_setopt_str",
        "__rt_curl_easy_perform",
        "__rt_curl_easy_errno",
        "__rt_curl_easy_error",
        "__rt_curl_easy_body",
        "__rt_curl_version",
        "__rt_curl_easy_free",
    ];

    /// Renders the shared runtime object's assembly for one supported target.
    fn runtime_for(target_name: &str) -> String {
        let target = Target::parse(target_name).expect("supported target");
        crate::codegen_support::generate_runtime_with_features(
            8_388_608,
            target,
            RuntimeFeatures::none(),
        )
    }

    /// Every helper and every bridge slot is emitted on every supported target.
    #[test]
    fn curl_runtime_is_emitted_for_all_supported_targets() {
        for target_name in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
            let asm = runtime_for(target_name);
            for label in CURL_RUNTIME_LABELS {
                assert!(
                    asm.contains(&format!("{label}:\n")),
                    "{label} must be emitted for {target_name}"
                );
            }
            for (_, slot) in super::slots::CURL_ABI_SLOTS {
                assert!(
                    asm.contains(slot),
                    "{slot} must be declared for {target_name}"
                );
            }
        }
    }

    /// The runtime object never names an `elephc_curl_*` symbol: the bridge is reached
    /// only through the null-by-default slots, which a curl-free program never publishes.
    #[test]
    fn the_shared_runtime_never_references_the_curl_bridge() {
        for target_name in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
            let asm = runtime_for(target_name);
            for (c_name, _) in super::slots::CURL_ABI_SLOTS {
                // The slot symbols are the C names with an `_fn` suffix and a leading
                // underscore, so a bare-name search would match them; the check looks for
                // the C symbol NOT followed by the slot suffix.
                for occurrence in asm.match_indices(c_name) {
                    let rest = &asm[occurrence.0 + c_name.len()..];
                    assert!(
                        rest.starts_with("_fn"),
                        "{target_name} runtime references {c_name} outside its slot name"
                    );
                }
            }
        }
    }

    /// `__rt_mixed_free_deep` routes resource kind 6 to the curl destructor, which is what
    /// makes a `CurlHandle` object's teardown release its libcurl handle.
    ///
    /// The branch target is matched by NAME rather than as a whole line, because local
    /// labels carry a platform-specific prefix (`L...` on macOS) that is not part of the
    /// contract being pinned.
    #[test]
    fn mixed_free_deep_routes_kind_six_to_the_curl_destructor() {
        for (target_name, compare, mnemonic) in [
            ("macos-aarch64", "cmp x9, #6", "b.eq"),
            ("linux-aarch64", "cmp x9, #6", "b.eq"),
            ("linux-x86_64", "cmp r9, 6", "je"),
        ] {
            let asm = runtime_for(target_name);
            assert!(
                asm.contains(&format!("    {compare}\n")),
                "{target_name} must test the resource kind against 6"
            );
            assert!(
                asm.lines().any(|line| {
                    let line = line.trim();
                    line.starts_with(&format!("{mnemonic} "))
                        && line.ends_with("__rt_mixed_free_deep_resource_curl")
                }),
                "{target_name} must branch to the curl destructor arm"
            );
            assert!(
                asm.contains("__rt_mixed_free_deep_resource_curl:\n"),
                "{target_name} must define the curl destructor arm"
            );
            assert!(
                asm.lines().any(|line| line.trim().ends_with("__rt_curl_easy_free")),
                "{target_name} curl destructor arm must call __rt_curl_easy_free"
            );
        }
    }
}
