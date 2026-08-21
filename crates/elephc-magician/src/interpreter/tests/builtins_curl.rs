//! Purpose:
//! Regression tests for the `CURLOPT_PRIVATE` retain/release fix in
//! `crate::stream_resources::curl` — releasing a stored `CURLOPT_PRIVATE` value when its
//! easy handle is torn down inside `eval()`.
//!
//! Called from:
//! - `cargo test -p elephc-magician --features curl` through Rust's test harness.
//!
//! Key details:
//! - Feature-gated: this whole file exists only when `crate::stream_resources::curl`
//!   does (see `crate::interpreter::tests::mod`'s `#[cfg(feature = "curl")] mod
//!   builtins_curl;`). Building the whole test binary with `--features curl` still needs
//!   `crate::curl_ffi`'s `#[cfg(test)] mod test_stubs` to link — see that module's doc —
//!   even though none of these tests call into `curl_ffi` at all (see the next bullet).
//! - CALLS `EvalStreamResources`'s curl methods DIRECTLY, bypassing `eval()` PHP source
//!   and the `curl_setopt()`/`curl_getinfo()` builtin dispatch entirely. This is
//!   deliberate: running the repro through full PHP source (`curl_init(); $k = "...";
//!   curl_setopt(...); unset($k); ...`) makes `values.releases` also collect the
//!   INTERPRETER's own end-of-program scope-cleanup releases for every local variable
//!   ($ch/$k/$copy/...), which drowns out the specific release calls this fix is
//!   responsible for and makes an exact-count assertion flaky/meaningless. Calling the
//!   storage methods directly gives full control over exactly which handles are retained
//!   and released, with no scope-cleanup noise.
//! - `FakeOps::retain` (`interpreter::tests::support::lifecycle_ops`) is an IDENTITY
//!   no-op (returns the same handle, does not simulate a real refcount) and
//!   `FakeOps::release` only LOGS into `releases` without reclaiming anything — so these
//!   tests cannot observe a literal use-after-free crash the way `--heap-debug` against
//!   the real runtime would. What they DO precisely observe is the RELEASE CALL
//!   SEQUENCE: before this fix, `set_curl_easy_private`/`reset_curl_easy_mirror` never
//!   called `values.release()` at all (the private value was simply overwritten/dropped
//!   in Rust, silently leaking the PHP-level reference this fix now retains), and
//!   `adopt_curl_easy_handle` never called `values.retain()` for the copy (the exact bug
//!   the review flagged: two table entries sharing one PHP-level reference with only one
//!   owner ever recorded).

use crate::context::ElephcEvalContext;
use crate::errors::EvalStatus;
use crate::interpreter::builtins::{
    eval_curl_close_values_result, eval_curl_escape_values_result,
    eval_curl_unescape_values_result,
};
use crate::interpreter::RuntimeValueOps;

use super::support::{FakeOps, FakeValue};

/// `set_curl_easy_private` must retain its OWN independent reference: releasing the
/// caller's original cell (simulating `unset($k)` after `curl_setopt($ch, CURLOPT_PRIVATE,
/// $k)`) must not be the only reference storage was relying on. This is the review's exact
/// repro shape, expressed at the storage layer instead of through PHP source.
#[test]
fn set_curl_easy_private_retains_independently_of_the_caller_cell() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let id = context.stream_resources_mut().open_curl_easy_handle(42);

    let k = values.string("secret-token").expect("fake string cell");
    let stored = context
        .stream_resources_mut()
        .set_curl_easy_private(id, k, &mut values)
        .expect("set_curl_easy_private");
    assert!(stored);
    assert!(
        values.releases.is_empty(),
        "the first store must not release anything"
    );

    // Simulates `unset($k)`: the CALLER releases its own scope-owned reference. Under the
    // pre-fix code, storage held this exact same (unretained) handle, so this release was
    // effectively releasing storage's only reference too.
    values.release(k).expect("release the caller's own cell");
    assert_eq!(values.releases, vec![k]);

    // Storage's OWN retained copy must still answer correctly and be independently
    // retainable for a caller to read (`curl_getinfo`'s own `values.retain(stored)` call).
    let read_back = context
        .stream_resources()
        .curl_easy_private(id)
        .expect("private value still present after the caller's own release");
    let for_caller = values.retain(read_back).expect("retain the read-back value");
    assert_eq!(values.get(for_caller), FakeValue::String("secret-token".to_string()));
}

/// Overwriting `CURLOPT_PRIVATE` on the same handle must release exactly the value it
/// replaces, and only that value — not the pre-fix behavior, where the old
/// `Option<RuntimeCellHandle>` was silently replaced with no `values.release()` call at
/// all, leaking one retained reference per repeated `curl_setopt(..., CURLOPT_PRIVATE,
/// ...)` on a loop-reused handle.
#[test]
fn set_curl_easy_private_releases_the_previous_value_on_overwrite() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let id = context.stream_resources_mut().open_curl_easy_handle(42);

    let first = values.string("first").expect("fake string cell");
    context
        .stream_resources_mut()
        .set_curl_easy_private(id, first, &mut values)
        .expect("first store");
    assert!(values.releases.is_empty());

    let second = values.string("second").expect("fake string cell");
    context
        .stream_resources_mut()
        .set_curl_easy_private(id, second, &mut values)
        .expect("overwrite");
    assert_eq!(values.releases, vec![first], "only the overwritten value is released");

    let current = context
        .stream_resources()
        .curl_easy_private(id)
        .expect("private value present");
    assert_eq!(values.get(current), FakeValue::String("second".to_string()));
}

/// `reset_curl_easy_mirror` (`curl_reset()`'s backing call) must release the stored
/// `CURLOPT_PRIVATE` value and clear it — mirroring `crate::curl_prelude::curl_reset`'s
/// `$handle->__elephc_private = false;`, which drops the PHP-level reference the same way.
#[test]
fn reset_curl_easy_mirror_releases_the_stored_value_and_clears_it() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let id = context.stream_resources_mut().open_curl_easy_handle(42);

    let k = values.string("reset-me").expect("fake string cell");
    context
        .stream_resources_mut()
        .set_curl_easy_private(id, k, &mut values)
        .expect("store");

    context
        .stream_resources_mut()
        .reset_curl_easy_mirror(id, &mut values)
        .expect("reset");

    assert_eq!(values.releases, vec![k]);
    assert!(context.stream_resources().curl_easy_private(id).is_none());
}

/// `adopt_curl_easy_handle` (`curl_copy_handle()`'s backing call) must retain an
/// INDEPENDENT reference to the copied `CURLOPT_PRIVATE` value: resetting the ORIGINAL
/// releases only the original's own retained reference, and the copy's own reference —
/// and therefore its `curl_getinfo(..., CURLINFO_PRIVATE)` answer — is unaffected. The
/// review's exact concern: without retaining on the copy, both entries would share one
/// reference with only one owner ever recorded, so resetting either would silently corrupt
/// the other's storage.
#[test]
fn adopt_curl_easy_handle_retains_an_independent_private_reference() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let original_id = context.stream_resources_mut().open_curl_easy_handle(42);

    let k = values.string("shared-secret").expect("fake string cell");
    context
        .stream_resources_mut()
        .set_curl_easy_private(original_id, k, &mut values)
        .expect("store on original");

    let (return_transfer, write_user, private_value) = context
        .stream_resources()
        .curl_easy_mirror(original_id)
        .expect("mirror snapshot");
    let copy_id = context
        .stream_resources_mut()
        .adopt_curl_easy_handle(43, return_transfer, write_user, private_value, &mut values)
        .expect("adopt");

    // The copy's own reference is retained at adoption time — nothing released yet.
    assert!(values.releases.is_empty());

    // Resetting the ORIGINAL must release only the original's own reference; the copy's
    // storage must still answer correctly afterward.
    context
        .stream_resources_mut()
        .reset_curl_easy_mirror(original_id, &mut values)
        .expect("reset original");
    assert_eq!(values.releases.len(), 1);
    assert!(context.stream_resources().curl_easy_private(original_id).is_none());

    let copy_value = context
        .stream_resources()
        .curl_easy_private(copy_id)
        .expect("copy's own private value survives the original's reset");
    assert_eq!(values.get(copy_value), FakeValue::String("shared-secret".to_string()));

    // Resetting the COPY releases its own (second, independent) reference.
    context
        .stream_resources_mut()
        .reset_curl_easy_mirror(copy_id, &mut values)
        .expect("reset copy");
    assert_eq!(values.releases.len(), 2);
}

/// `release_curl_easy_private_values` must release
/// a retained `CURLOPT_PRIVATE` value that was NEVER explicitly reset/overwritten — the
/// exact case `EvalStreamResources::drop` could never cover, because `Drop::drop` has no
/// `RuntimeValueOps` to call `release` through. This is the storage-layer half of the fix;
/// `crate::ffi::context::__elephc_eval_context_free` is the real call site, but this test
/// exercises the storage method directly, matching this file's own established pattern
/// (see the module doc).
#[test]
fn release_curl_easy_private_values_releases_every_still_retained_entry() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let first_id = context.stream_resources_mut().open_curl_easy_handle(42);
    let second_id = context.stream_resources_mut().open_curl_easy_handle(43);

    let first_key = values.string("first-secret").expect("fake string cell");
    context
        .stream_resources_mut()
        .set_curl_easy_private(first_id, first_key, &mut values)
        .expect("store on first handle");
    let second_key = values.string("second-secret").expect("fake string cell");
    context
        .stream_resources_mut()
        .set_curl_easy_private(second_id, second_key, &mut values)
        .expect("store on second handle");

    // Neither handle was reset or closed — the exact "never explicitly reset" case that
    // used to leak both retained references at context teardown.
    assert!(values.releases.is_empty());

    context
        .stream_resources_mut()
        .release_curl_easy_private_values(&mut values);

    assert_eq!(values.releases.len(), 2, "both retained cells must be released");
    assert!(
        context.stream_resources().curl_easy_private(first_id).is_none(),
        "the first handle's stored private value must be cleared, not just released"
    );
    assert!(
        context.stream_resources().curl_easy_private(second_id).is_none(),
        "the second handle's stored private value must be cleared, not just released"
    );

    // Calling it again (mirroring a second teardown attempt) must be a safe no-op: nothing
    // left to release, so no further `values.release()` calls and no double-release.
    context
        .stream_resources_mut()
        .release_curl_easy_private_values(&mut values);
    assert_eq!(values.releases.len(), 2, "a second call must not double-release");
}

/// A handle that never had `CURLOPT_PRIVATE` set at all must be a complete no-op —
/// `release_curl_easy_private_values` must not touch `values` for it.
#[test]
fn release_curl_easy_private_values_skips_handles_with_no_stored_private_value() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    context.stream_resources_mut().open_curl_easy_handle(42);

    context
        .stream_resources_mut()
        .release_curl_easy_private_values(&mut values);

    assert!(values.releases.is_empty());
}

/// Returns the fake `message` property string stored on a fake `Throwable`-like object,
/// exactly as `crate::interpreter::tests::support`'s `runtime_construct_object` stores it
/// for `new Error($message, $code)`-shaped calls (see that function's doc).
fn fake_exception_message(values: &mut FakeOps, exception: crate::value::RuntimeCellHandle) -> String {
    let FakeValue::Object(properties) = values.get(exception) else {
        panic!("expected a fake exception object");
    };
    let message_cell = properties
        .iter()
        .find(|(name, _)| name == "message")
        .map(|(_, value)| *value)
        .expect("a message property must be set");
    match values.get(message_cell) {
        FakeValue::String(message) => message,
        other => panic!("expected a fake string message, got {other:?}"),
    }
}

/// Item 8 (WP-B, curl punch list): a libcurl escape failure must throw a catchable
/// `\RuntimeException` with AOT's exact message
/// (`crate::curl_prelude::curl_escape`'s own `throw new \RuntimeException("curl_escape():
/// libcurl could not URL-encode the string")`), not silently answer `false` the way this
/// used to. `crate::curl_ffi`'s `cfg(test)` stub always reports `elephc_curl_easy_str_op`
/// failure (produced = 0), so calling the values-dispatch entry point directly against a
/// real (fake-table) handle deterministically exercises this path.
#[test]
fn curl_escape_failure_throws_a_catchable_runtime_exception() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let table_id = context.stream_resources_mut().open_curl_easy_handle(42);
    let handle = values.curl_handle(table_id).expect("box the fake handle");
    let string = values.string("hello world").expect("fake string cell");

    let result = eval_curl_escape_values_result(&[handle, string], &mut context, &mut values);
    assert_eq!(result, Err(EvalStatus::UncaughtThrowable));

    let exception = context
        .take_pending_throw()
        .expect("a throwable must be scheduled");
    assert_eq!(
        values.object_class_name(exception).ok().map(|cell| values.get(cell)),
        Some(FakeValue::String("RuntimeException".to_string()))
    );
    assert_eq!(
        fake_exception_message(&mut values, exception),
        "curl_escape(): libcurl could not URL-encode the string"
    );
}

/// Same as `curl_escape_failure_throws_a_catchable_runtime_exception`, for
/// `curl_unescape()`'s matching AOT message.
#[test]
fn curl_unescape_failure_throws_a_catchable_runtime_exception() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let table_id = context.stream_resources_mut().open_curl_easy_handle(42);
    let handle = values.curl_handle(table_id).expect("box the fake handle");
    let string = values.string("hello%20world").expect("fake string cell");

    let result = eval_curl_unescape_values_result(&[handle, string], &mut context, &mut values);
    assert_eq!(result, Err(EvalStatus::UncaughtThrowable));

    let exception = context
        .take_pending_throw()
        .expect("a throwable must be scheduled");
    assert_eq!(
        fake_exception_message(&mut values, exception),
        "curl_unescape(): libcurl could not URL-decode the string"
    );
}

/// Item 9 (WP-B, curl punch list): `curl_close()` on a non-`CurlHandle` value must throw a
/// catchable `\TypeError`, matching real PHP 8.4.20's own wording for the same misuse
/// (verified: `curl_close("x")` -> `TypeError: curl_close(): Argument #1 ($handle) must be
/// of type CurlHandle, string given`) — it used to accept literally anything.
#[test]
fn curl_close_on_a_non_handle_value_throws_a_catchable_type_error() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let not_a_handle = values.string("not a handle").expect("fake string cell");

    let result = eval_curl_close_values_result(&[not_a_handle], &mut context, &mut values);
    assert_eq!(result, Err(EvalStatus::UncaughtThrowable));

    let exception = context
        .take_pending_throw()
        .expect("a throwable must be scheduled");
    assert_eq!(
        values.object_class_name(exception).ok().map(|cell| values.get(cell)),
        Some(FakeValue::String("TypeError".to_string()))
    );
    assert_eq!(
        fake_exception_message(&mut values, exception),
        "curl_close(): Argument #1 ($handle) must be of type CurlHandle, string given"
    );
}

/// FIX ROUND 1, MINOR 5: the two PHP 8.5-only curl names must be INVISIBLE to eval's
/// function-existence probe on an older compatibility profile, so `function_exists()` agrees
/// with AOT — where the curl prelude's `-- elephc PHP >= 8.5 ... --` fences strip the
/// declaration outright and the name simply does not exist.
///
/// A UNIT TEST RATHER THAN A CODEGEN FIXTURE, for a harness reason documented at the foot of
/// `tests/codegen/curl/eval.rs`: `compile_and_run_with_php_version` threads its version into
/// prelude injection but never reaches `codegen::set_compile_profile`, so the profile the
/// compiled binary publishes to this interpreter stays at the default and no eval-side
/// version gate is observable through it. `scoped_profile` drives the same input directly.
#[test]
fn php_85_only_curl_names_are_hidden_from_introspection_below_85() {
    for profile in [80_200, 80_300, 80_400] {
        let _guard = crate::eval_php_profile::scoped_profile(profile);
        for name in ["curl_multi_get_handles", "curl_share_init_persistent"] {
            assert!(
                crate::interpreter::builtins::eval_builtin_hidden_by_php_version(name),
                "{name} must be hidden from introspection on profile {profile}"
            );
        }
        // The rest of the curl surface is version-independent and must stay visible.
        for name in ["curl_multi_init", "curl_share_init", "curl_init", "curl_setopt"] {
            assert!(
                !crate::interpreter::builtins::eval_builtin_hidden_by_php_version(name),
                "{name} exists on every supported profile and must stay visible ({profile})"
            );
        }
    }
}

/// The other half of the same gate: on 8.5 (and later) both names are ordinary, visible
/// builtins. Without this, a predicate that simply always answered `true` would pass the
/// test above.
#[test]
fn php_85_only_curl_names_are_visible_on_85() {
    for profile in [80_500, 80_600] {
        let _guard = crate::eval_php_profile::scoped_profile(profile);
        for name in ["curl_multi_get_handles", "curl_share_init_persistent"] {
            assert!(
                !crate::interpreter::builtins::eval_builtin_hidden_by_php_version(name),
                "{name} must be visible on profile {profile}"
            );
        }
    }
}

/// FIX ROUND 1, MINOR 8: an installed callback is a RETAINED root, and every path that
/// replaces or drops one must release exactly the reference it retained — the same
/// discipline `set_curl_easy_private` already has, and the same failure mode if it is
/// missing (a PHP-level reference leaked for the lifetime of the eval context, invisible to
/// `--gc-stats` because the cell is still reachable from this table).
///
/// Covers all four transitions in one sequence: install, overwrite, `curl_reset()`, and the
/// context-teardown sweep.
#[test]
fn callback_slots_retain_on_install_and_release_on_every_drop() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let id = context.stream_resources_mut().open_curl_easy_handle(42);

    let first = values.string("first-callback").expect("fake string cell");
    let stored = context
        .stream_resources_mut()
        .set_curl_easy_callback(
            id,
            0,
            crate::stream_resources::EvalCurlCallbackSlot::Callable(first),
            &mut values,
        )
        .expect("install");
    assert!(stored);
    // `FakeOps::retain` is an identity no-op that records nothing (see this file's module
    // doc), so what these tests observe is the RELEASE SEQUENCE — which is exactly where a
    // missing-release leak shows up.
    assert!(values.releases.is_empty(), "installing must release nothing");

    // Overwriting the SAME slot releases the previous root and retains the new one — without
    // this, `curl_setopt($ch, CURLOPT_WRITEFUNCTION, $a); curl_setopt($ch, ..., $b);` would
    // leak `$a`'s reference, unbounded across a loop.
    let second = values.string("second-callback").expect("fake string cell");
    context
        .stream_resources_mut()
        .set_curl_easy_callback(
            id,
            0,
            crate::stream_resources::EvalCurlCallbackSlot::Callable(second),
            &mut values,
        )
        .expect("overwrite");
    assert_eq!(
        values.releases,
        vec![first],
        "overwriting must release exactly the previous root"
    );

    // `curl_reset()` drops every slot.
    context
        .stream_resources_mut()
        .clear_curl_easy_callbacks(id, &mut values)
        .expect("clear");
    assert_eq!(
        values.releases,
        vec![first, second],
        "curl_reset() must release every still-installed root"
    );

    // And the teardown sweep releases whatever survived to the end of the context.
    let third = values.string("third-callback").expect("fake string cell");
    context
        .stream_resources_mut()
        .set_curl_easy_callback(
            id,
            1,
            crate::stream_resources::EvalCurlCallbackSlot::Callable(third),
            &mut values,
        )
        .expect("install header slot");
    context
        .stream_resources_mut()
        .release_curl_easy_private_values(&mut values);
    assert_eq!(
        values.releases,
        vec![first, second, third],
        "context teardown must release the callback roots too, not only CURLOPT_PRIVATE"
    );
}

/// `EvalCurlCallbackSlot::Silent` (the `CURLOPT_DEBUGFUNCTION => null` state) holds no PHP
/// value, so it must neither retain nor release — and installing it over a real callable
/// must still release that callable. Without the second half, clearing a debug callback
/// would leak its root.
#[test]
fn silent_callback_slot_retains_nothing_but_still_releases_what_it_replaces() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let id = context.stream_resources_mut().open_curl_easy_handle(42);

    let debug = values.string("debug-callback").expect("fake string cell");
    context
        .stream_resources_mut()
        .set_curl_easy_callback(
            id,
            4,
            crate::stream_resources::EvalCurlCallbackSlot::Callable(debug),
            &mut values,
        )
        .expect("install debug");
    assert!(values.releases.is_empty());

    context
        .stream_resources_mut()
        .set_curl_easy_callback(
            id,
            4,
            crate::stream_resources::EvalCurlCallbackSlot::Silent,
            &mut values,
        )
        .expect("silence debug");
    assert_eq!(
        values.releases,
        vec![debug],
        "silencing must release the callable it replaced"
    );
}
