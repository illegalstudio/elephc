//! Purpose:
//! Eval homes for PHP's `ext/curl`. The easy interface — `curl_init`,
//! `curl_setopt[_array]`, `curl_exec`, `curl_getinfo`, `curl_close`, `curl_copy_handle`,
//! `curl_errno`, `curl_error`, `curl_escape`/`curl_unescape`, `curl_pause`, `curl_reset`,
//! `curl_upkeep`, `curl_version`, `curl_strerror` — plus the MULTI interface
//! (`curl_multi_*`) and the SHARE interface (`curl_share_*`). Every one of them calls the
//! SAME `elephc_curl_*` C ABI the AOT `curl_setopt()`/etc. prelude wrappers call
//! (`crate::curl_ffi`) — nothing here reimplements HTTP, TLS, or any `CURLOPT_*` semantic.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::{EvalDirectHook, EvalValuesHook}::Curl`.
//!
//! Key details — READ THIS BEFORE TOUCHING ANYTHING CURL-RELATED IN THIS CRATE:
//!
//! ## Why this whole tree sits behind the `curl` Cargo feature
//!
//! `crates/elephc-magician`'s Cargo.toml has never had a path dependency on `elephc-curl`
//! (or on the root `elephc` crate at all — `crate::interpreter::curl_constants` hits the
//! identical constraint for the constant tables). `crate::curl_ffi` therefore reaches the bridge the SAME way
//! `elephc-curl` itself reaches raw libcurl: bare `extern "C"` declarations that stay
//! unresolved in `libelephc_magician.a`'s own archive and are resolved only when the
//! FINAL PHP-program link ALSO supplies `libelephc_curl.a`.
//!
//! The difference from a plain "just declare the externs" story is `elephc_curl_*`'s
//! REGISTRATION: `eval_builtin!` submits every home below through `inventory::submit!`,
//! and `__elephc_eval_execute` (this crate's one entry point every eval()-using program
//! calls) reaches that whole registry through the interpreter's normal dispatch — there is
//! no per-builtin object-file boundary a linker's archive-member selection could rely on to
//! drop an UNUSED family's code from a program that never calls it (this crate is not built
//! with `-ffunction-sections`/`--gc-sections`-style fine-grained stripping, and even if it
//! were, `inventory`'s registration relies on every submitting object file being linked in
//! the first place). Concretely: if these `elephc_curl_*` references were compiled into
//! `libelephc_magician.a` UNCONDITIONALLY, EVERY program that calls `eval()` at all —
//! including one that never mentions curl anywhere — would need `elephc_curl` (and
//! therefore the pinned native libcurl/OpenSSL/zlib) linked into its final binary just to
//! resolve those symbols, even though nothing in the program ever calls
//! `elephc_curl_easy_init`. That is exactly the pay-for-use guarantee this crate holds for
//! every optional native surface, and it is
//! why this module — unlike `elephc-crypto`'s hash/openssl homes, which need no native
//! library at all and are therefore unconditional — cannot be unconditional too.
//!
//! The fix is the SAME shape `src/linker/bridges.rs` already uses for
//! `elephc_pdo` (rebuilding that bridge with driver-specific `--features` depending on
//! what the compiled PROGRAM needs, via `super::pdo::cargo_features()`): `elephc-magician`
//! gains its own `curl` Cargo feature (off by default), this whole module (plus
//! `crate::curl_ffi` and the curl fields on `EvalStreamResources`) compiles ONLY when it
//! is enabled, and `src/linker/bridges.rs`'s bridge-archive resolver builds
//! `libelephc_magician.a` WITH `--features curl` exactly when a compiled program needs
//! BOTH `elephc_magician` (it calls `eval()`) AND `elephc_curl` (it — outside eval, or via
//! `--with-curl` — uses the curl surface) at once. A program that only calls `eval()`
//! keeps linking the exact curl-free `libelephc_magician.a` this crate has always
//! produced; a program that only uses curl outside `eval()` is unaffected because it never
//! links `elephc_magician` in the first place. See `src/linker/bridges.rs`'s
//! `curl_feature_needed_for_magician` (or its equivalent — check that file for the
//! current name) for the production wiring, and `tests/codegen/support/runner.rs` for the
//! codegen-test-harness equivalent.
//!
//! ## `extension_loaded('curl')`
//!
//! Answers `cfg!(feature = "curl")` (`crate::interpreter::builtins::network_env::
//! extension_loaded`) — true exactly when this module was compiled in, which (by the
//! above) only happens for a program that already links `elephc_curl` outside eval. This
//! is the brief's "stays false unless the eval host linked the bridge", made concrete: AOT
//! and eval can never disagree about whether curl is available in the SAME compiled
//! program.
//!
//! ## Handle representation — analogous to `HashContext`, not a real object
//!
//! PHP 8 models `curl_init()`'s result as a `CurlHandle` OBJECT
//! (`crate::curl_prelude`'s AOT `final class CurlHandle { public mixed $__elephc_handle;
//! ... }`). The eval interpreter has no such class here: exactly like
//! `hash_init()`'s `HashContext` (`crate::interpreter::builtins::string::hash_init`'s own
//! doc), a curl easy handle is boxed as a runtime tag-9 cell with RESOURCE KIND 5 — "eval-
//! owned inert handle: no PHP id, no destructor" — through the new
//! `RuntimeValueOps::curl_handle` default method, which literally reuses `hash_context()`'s
//! mechanism (see that method's doc for why one generated wrapper serves both). The real
//! bridge id and the small PHP-layer mirror state (`RETURNTRANSFER`/`CURLOPT_PRIVATE`/
//! write-mode — `crate::curl_prelude`'s equivalent object properties) live in
//! `EvalStreamResources`'s curl tables (`crate::stream_resources::curl`), keyed by the
//! SAME eval resource-id counter every other eval-owned handle uses, so a curl handle id
//! can never collide with (or shift) a real PHP resource id — see
//! `crate::stream_resources::storage`'s `take_next_id` doc, which this module does NOT
//! bypass.
//!
//! CONSEQUENCE — NOT A REAL OBJECT: `get_class($ch)`, `instanceof CurlHandle`, and
//! `var_dump($ch)` inside `eval()` do NOT report `CurlHandle` the way real PHP does
//! (`gettype()` reports `"resource"`, matching `hash_init()`'s own, already-accepted,
//! already-documented divergence in this crate). `curl_setopt($ch, ...)` and friends still
//! work correctly by id. This is a genuine, intentional, documented gap, not an oversight.
//!
//! CONSEQUENCE — TWO DISTINCT OBJECT SPACES: a `CurlHandle` created by AOT-compiled code
//! and a curl handle created inside `eval()` are NOT interchangeable. AOT's `CurlHandle` is
//! a real Zend-style object with a `$__elephc_handle` property; eval's is a raw resource-
//! kind-5 cell keyed into `EvalStreamResources`, a table that lives ONLY inside the
//! `ElephcEvalContext` the eval() call is running against. Passing an eval-created handle
//! OUT of `eval()` into surrounding compiled code (as an `eval()` return value, or written
//! into a captured-by-reference variable) hands the caller an opaque cell whose payload is
//! an `EvalStreamResources` table key, not a real object — the surrounding program has no
//! `CurlHandle` class instance to call `curl_setopt()` on even if the raw id happened to be
//! readable. **A curl handle cannot escape `eval()`.** Conversely, an AOT `CurlHandle`
//! object passed INTO an `eval()` string (as a variable capture) is a real object as far as
//! eval's generic object machinery is concerned (property reads work), but none of the
//! functions in this module accept it: they all expect a resource-kind-5 cell from THIS
//! module's own `curl_init()`, so handing one a real AOT `CurlHandle` fails the same way
//! passing a `HashContext` object would. This exactly mirrors `HashContext`'s own,
//! pre-existing interop story, which this module deliberately does not attempt to improve
//! on.
//!
//! ## Scope shipped vs. deferred
//!
//! SHIPPED: the complete easy interface above, the full 689-entry `CURLOPT_*`/
//! `CURLINFO_*`/`CURLE_*`/`CURL_*` constant table (`crate::interpreter::curl_constants`,
//! unconditional — see that module's own doc for why constants need no feature gate), and
//! `curl_setopt()`'s generic option-KIND dispatch (the same table-driven mechanism
//! `crate::curl_prelude::curl_setopt` uses, not a per-option hardcode), so every LONG/
//! STRING/SLIST/OFF_T/PHP-LAYER option `curl_setopt()` recognizes works, not just a
//! hand-picked subset.
//!
//! ALSO SHIPPED — THE MULTI AND SHARE INTERFACES (R3-C): `curl_multi_init`/`_add_handle`/
//! `_remove_handle`/`_exec`/`_select`/`_info_read`/`_getcontent`/`_setopt`/`_errno`/
//! `_strerror`/`_close` plus PHP 8.5's `curl_multi_get_handles`, and `curl_share_init`/
//! `_setopt`/`_errno`/`_strerror`/`_close` plus PHP 8.5's `curl_share_init_persistent` —
//! together with `curl_setopt()`'s `CURLOPT_SHARE` (option KIND 7). Every one of them
//! calls the SAME `elephc_curl_multi_*`/`elephc_curl_share_*` ABI the AOT prelude calls.
//!
//! WHAT MADE THAT POSSIBLE, since an earlier revision of this doc argued it was blocked by
//! an OBJECT-MODEL MISMATCH: the argument was that multi and share are OBJECT-IDENTITY
//! APIs (`curl_multi_info_read()` must hand back the SAME `CurlHandle` instance that was
//! added) and that a resource-cell world has no home for that without a parallel identity
//! map. The premise was right and the conclusion was wrong. AOT needs the identity map for
//! an OWNERSHIP reason, spelled out at `crate::curl_prelude::CurlMultiHandle`: its
//! `CurlHandle` object OWNS the native handle (resource kind 6 -> `curl_easy_cleanup`), so
//! minting a second object around one reported id would DOUBLE-FREE it. An eval curl handle
//! owns nothing — it is an inert resource-kind-5 cell whose payload is an
//! `EvalStreamResources` table key, freed only by that table's own `Drop` — so two cells
//! carrying the same key are simply interchangeable, and `curl_multi_info_read()` can
//! re-box the key it resolves. What eval needs instead is only an ADD-ORDER LIST of table
//! keys per multi handle (`EvalCurlMultiHandle::attached`), which is what
//! `curl_multi_get_handles()` reports. The share side needs even less: the BRIDGE owns the
//! attachment bookkeeping and its deferred-free protocol
//! (`crates/elephc-curl/src/share.rs`), so eval stores only the raw id.
//!
//! TYPING IS BY TABLE MEMBERSHIP. Every eval-owned curl handle — easy, multi, share —
//! draws its key from ONE shared `take_next_id()` counter, so a key that resolves in the
//! multi table cannot also be an easy or share key. That is what lets
//! `curl_multi_add_handle($mh, $ch)` tell its two arguments apart, and it is the eval
//! counterpart of the `CurlMultiHandle`/`CurlHandle` parameter types the AOT prelude gets
//! checked at COMPILE time. See `handle.rs`'s resolver family.
//!
//! ALSO SHIPPED — `CURLFile`/`CURLStringFile` AND `CURLOPT_POSTFIELDS`'s ARRAY FORM (R3-C):
//! `curl_setopt($ch, CURLOPT_POSTFIELDS, [...])` posts real `multipart/form-data` through
//! the SAME `elephc_curl_mime_*` ABI the AOT prelude drives — see `multipart.rs`, which
//! mirrors `crate::curl_prelude::__elephc_curl_build_multipart` part for part.
//!
//! THE TWO CLASSES ARE THE AOT ONES, NOT EVAL RE-DECLARATIONS, and that is the one place
//! this module deliberately DOES cross the object-space boundary it warns about above. The
//! warning is about handles: an eval curl handle is a resource cell and an AOT `CurlHandle`
//! is an object, so neither works where the other is expected. `CURLFile`/`CURLStringFile`
//! are not handles — they are PURE PHP DATA CLASSES wrapping no native resource at all
//! (`crate::curl_prelude`'s own comment above their declarations: no `$__elephc_handle`, no
//! private constructor, no factory). The real AOT class therefore works correctly inside
//! `eval()`: `new CURLFile(...)` constructs it through the ordinary native-class fallback,
//! `get_class()` reports `CURLFile`, `instanceof` matches, the three getters and two setters
//! dispatch, and `multipart.rs` reads its properties. `curl_file_create()` likewise resolves
//! to the AOT prelude function, which is a plain alias of the constructor.
//!
//! THE CLASSES ARE ALWAYS THERE WHEN THIS MODULE IS. A program that reaches the eval curl
//! surface at all has, by construction, been linked with `elephc_curl`
//! (`src/linker/bridges.rs`'s `needs_curl`), and that same condition is what injects the
//! curl prelude — so `CURLFile`/`CURLStringFile` are declared in every build where this code
//! exists. There is no configuration in which `new CURLFile(...)` compiles into eval's path
//! and finds no class.
//!
//! STILL DEFERRED:
//! - Callback options (`CURLOPT_WRITEFUNCTION`/`_HEADERFUNCTION`/`_READFUNCTION`/
//!   `_PROGRESSFUNCTION`/`_DEBUGFUNCTION`/`_XFERINFOFUNCTION`, option KIND 8): rejected
//!   through the SAME honest "option ... is not supported by this build" warning +
//!   `false` path KIND 6 (a real option this build cannot carry) already uses.
//! - `CURLOPT_FILE`/`CURLOPT_INFILE`/`CURLOPT_WRITEHEADER`/`CURLOPT_STDERR` (PHP-stream-
//!   valued options): these are KIND 9 (`KIND_STREAM`) options that need a live PHP stream
//!   resource on the far end.
//!
//! NO CURL NAME IS INTERCEPTED ANY MORE. `crate::interpreter::builtins::registry::names`
//! used to carry `eval_curl_deferred_function_name`/`eval_curl_deferred_class_name`, checked
//! ahead of the native-function/native-class fallbacks in six places, so that an
//! unimplemented curl name could not silently resolve to the REAL compiled implementation
//! and hand back a genuine, apparently-working AOT object — which then failed confusingly
//! the moment it met an eval-owned easy handle (the "TWO DISTINCT OBJECT SPACES"
//! consequence documented above). Both predicates and all six checks are gone: every
//! `curl_multi_*`/`curl_share_*` name now has a real eval home answered by the builtin
//! registry before any fallback runs, and `CURLFile`/`CURLStringFile`/`curl_file_create()`
//! are deliberately served BY that fallback for the data-class reason spelled out above.

mod handle;
mod multipart;

mod curl_close;
mod curl_copy_handle;
mod curl_errno;
mod curl_error;
mod curl_escape;
mod curl_exec;
mod curl_getinfo;
mod curl_init;
mod curl_multi_add_handle;
mod curl_multi_close;
mod curl_multi_errno;
mod curl_multi_exec;
mod curl_multi_get_handles;
mod curl_multi_getcontent;
mod curl_multi_info_read;
mod curl_multi_init;
mod curl_multi_remove_handle;
mod curl_multi_select;
mod curl_multi_setopt;
mod curl_multi_strerror;
mod curl_pause;
mod curl_reset;
mod curl_setopt;
mod curl_setopt_array;
mod curl_share_close;
mod curl_share_errno;
mod curl_share_init;
mod curl_share_init_persistent;
mod curl_share_setopt;
mod curl_share_strerror;
mod curl_strerror;
mod curl_unescape;
mod curl_upkeep;
mod curl_version;

pub(in crate::interpreter) use curl_close::*;
pub(in crate::interpreter) use curl_copy_handle::*;
pub(in crate::interpreter) use curl_errno::*;
pub(in crate::interpreter) use curl_error::*;
pub(in crate::interpreter) use curl_escape::*;
pub(in crate::interpreter) use curl_exec::*;
pub(in crate::interpreter) use curl_getinfo::*;
pub(in crate::interpreter) use curl_init::*;
pub(in crate::interpreter) use curl_multi_add_handle::*;
pub(in crate::interpreter) use curl_multi_close::*;
pub(in crate::interpreter) use curl_multi_errno::*;
pub(in crate::interpreter) use curl_multi_exec::*;
pub(in crate::interpreter) use curl_multi_get_handles::*;
pub(in crate::interpreter) use curl_multi_getcontent::*;
pub(in crate::interpreter) use curl_multi_info_read::*;
pub(in crate::interpreter) use curl_multi_init::*;
pub(in crate::interpreter) use curl_multi_remove_handle::*;
pub(in crate::interpreter) use curl_multi_select::*;
pub(in crate::interpreter) use curl_multi_setopt::*;
pub(in crate::interpreter) use curl_multi_strerror::*;
pub(in crate::interpreter) use curl_pause::*;
pub(in crate::interpreter) use curl_reset::*;
pub(in crate::interpreter) use curl_setopt::*;
pub(in crate::interpreter) use curl_setopt_array::*;
pub(in crate::interpreter) use curl_share_close::*;
pub(in crate::interpreter) use curl_share_errno::*;
pub(in crate::interpreter) use curl_share_init::*;
pub(in crate::interpreter) use curl_share_init_persistent::*;
pub(in crate::interpreter) use curl_share_setopt::*;
pub(in crate::interpreter) use curl_share_strerror::*;
pub(in crate::interpreter) use curl_strerror::*;
pub(in crate::interpreter) use curl_unescape::*;
pub(in crate::interpreter) use curl_upkeep::*;
pub(in crate::interpreter) use curl_version::*;

use super::super::*;
use handle::*;
use multipart::*;

/// Dispatches one curl builtin from unevaluated positional expressions. Every arm
/// evaluates its arguments left to right and forwards to the same `*_result` helper the
/// values-based dispatcher below also calls — the same shape `hash_init`/`hash_update` use.
///
/// TWO ARMS ARE NOT PURE FORWARDERS: `curl_multi_exec()` and `curl_multi_info_read()` take a
/// BY-REFERENCE out-parameter, so their arms resolve the lvalue out of the raw `EvalExpr`
/// (through `eval_call_arg_value`) and write it back. A literal call reaches them through
/// `crate::interpreter::expressions::calls::eval_call`'s own `&[EvalCallArg]` interception
/// instead, which additionally carries named-argument metadata; see each home file's header
/// for the full path list and for why the remaining by-value paths warn.
pub(in crate::interpreter) fn eval_builtin_curl_declared_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "curl_close" => eval_builtin_curl_close(args, context, scope, values),
        "curl_copy_handle" => eval_builtin_curl_copy_handle(args, context, scope, values),
        "curl_errno" => eval_builtin_curl_errno(args, context, scope, values),
        "curl_error" => eval_builtin_curl_error(args, context, scope, values),
        "curl_escape" => eval_builtin_curl_escape(args, context, scope, values),
        "curl_exec" => eval_builtin_curl_exec(args, context, scope, values),
        "curl_getinfo" => eval_builtin_curl_getinfo(args, context, scope, values),
        "curl_init" => eval_builtin_curl_init(args, context, scope, values),
        "curl_multi_add_handle" => {
            eval_builtin_curl_multi_add_handle(args, context, scope, values)
        }
        "curl_multi_close" => eval_builtin_curl_multi_close(args, context, scope, values),
        "curl_multi_errno" => eval_builtin_curl_multi_errno(args, context, scope, values),
        "curl_multi_exec" => eval_builtin_curl_multi_exec(args, context, scope, values),
        "curl_multi_get_handles" => {
            eval_builtin_curl_multi_get_handles(args, context, scope, values)
        }
        "curl_multi_getcontent" => {
            eval_builtin_curl_multi_getcontent(args, context, scope, values)
        }
        "curl_multi_info_read" => eval_builtin_curl_multi_info_read(args, context, scope, values),
        "curl_multi_init" => eval_builtin_curl_multi_init(args, context, values),
        "curl_multi_remove_handle" => {
            eval_builtin_curl_multi_remove_handle(args, context, scope, values)
        }
        "curl_multi_select" => eval_builtin_curl_multi_select(args, context, scope, values),
        "curl_multi_setopt" => eval_builtin_curl_multi_setopt(args, context, scope, values),
        "curl_multi_strerror" => eval_builtin_curl_multi_strerror(args, context, scope, values),
        "curl_pause" => eval_builtin_curl_pause(args, context, scope, values),
        "curl_reset" => eval_builtin_curl_reset(args, context, scope, values),
        "curl_setopt" => eval_builtin_curl_setopt(args, context, scope, values),
        "curl_setopt_array" => eval_builtin_curl_setopt_array(args, context, scope, values),
        "curl_share_close" => eval_builtin_curl_share_close(args, context, scope, values),
        "curl_share_errno" => eval_builtin_curl_share_errno(args, context, scope, values),
        "curl_share_init" => eval_builtin_curl_share_init(args, context, values),
        "curl_share_init_persistent" => {
            eval_builtin_curl_share_init_persistent(args, context, scope, values)
        }
        "curl_share_setopt" => eval_builtin_curl_share_setopt(args, context, scope, values),
        "curl_share_strerror" => eval_builtin_curl_share_strerror(args, context, scope, values),
        "curl_strerror" => eval_builtin_curl_strerror(args, context, scope, values),
        "curl_unescape" => eval_builtin_curl_unescape(args, context, scope, values),
        "curl_upkeep" => eval_builtin_curl_upkeep(args, context, scope, values),
        "curl_version" => eval_builtin_curl_version(args, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches one curl builtin from already evaluated argument cells.
pub(in crate::interpreter) fn eval_curl_declared_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "curl_close" => eval_curl_close_values_result(evaluated_args, context, values),
        "curl_copy_handle" => eval_curl_copy_handle_values_result(evaluated_args, context, values),
        "curl_errno" => eval_curl_errno_values_result(evaluated_args, context, values),
        "curl_error" => eval_curl_error_values_result(evaluated_args, context, values),
        "curl_escape" => eval_curl_escape_values_result(evaluated_args, context, values),
        "curl_exec" => eval_curl_exec_values_result(evaluated_args, context, values),
        "curl_getinfo" => eval_curl_getinfo_values_result(evaluated_args, context, values),
        "curl_init" => eval_curl_init_values_result(evaluated_args, context, values),
        "curl_multi_add_handle" => {
            eval_curl_multi_add_handle_values_result(evaluated_args, context, values)
        }
        "curl_multi_close" => eval_curl_multi_close_values_result(evaluated_args, context, values),
        "curl_multi_errno" => eval_curl_multi_errno_values_result(evaluated_args, context, values),
        "curl_multi_exec" => eval_curl_multi_exec_values_result(evaluated_args, context, values),
        "curl_multi_get_handles" => {
            eval_curl_multi_get_handles_values_result(evaluated_args, context, values)
        }
        "curl_multi_getcontent" => {
            eval_curl_multi_getcontent_values_result(evaluated_args, context, values)
        }
        "curl_multi_info_read" => {
            eval_curl_multi_info_read_values_result(evaluated_args, context, values)
        }
        "curl_multi_init" => eval_curl_multi_init_values_result(evaluated_args, context, values),
        "curl_multi_remove_handle" => {
            eval_curl_multi_remove_handle_values_result(evaluated_args, context, values)
        }
        "curl_multi_select" => eval_curl_multi_select_values_result(evaluated_args, context, values),
        "curl_multi_setopt" => eval_curl_multi_setopt_values_result(evaluated_args, context, values),
        "curl_multi_strerror" => eval_curl_multi_strerror_values_result(evaluated_args, values),
        "curl_pause" => eval_curl_pause_values_result(evaluated_args, context, values),
        "curl_reset" => eval_curl_reset_values_result(evaluated_args, context, values),
        "curl_setopt" => eval_curl_setopt_values_result(evaluated_args, context, values),
        "curl_setopt_array" => eval_curl_setopt_array_values_result(evaluated_args, context, values),
        "curl_share_close" => eval_curl_share_close_values_result(evaluated_args, context, values),
        "curl_share_errno" => eval_curl_share_errno_values_result(evaluated_args, context, values),
        "curl_share_init" => eval_curl_share_init_values_result(evaluated_args, context, values),
        "curl_share_init_persistent" => {
            eval_curl_share_init_persistent_values_result(evaluated_args, context, values)
        }
        "curl_share_setopt" => eval_curl_share_setopt_values_result(evaluated_args, context, values),
        "curl_share_strerror" => eval_curl_share_strerror_values_result(evaluated_args, values),
        "curl_strerror" => eval_curl_strerror_values_result(evaluated_args, values),
        "curl_unescape" => eval_curl_unescape_values_result(evaluated_args, context, values),
        "curl_upkeep" => eval_curl_upkeep_values_result(evaluated_args, context, values),
        "curl_version" => eval_curl_version_values_result(evaluated_args, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
