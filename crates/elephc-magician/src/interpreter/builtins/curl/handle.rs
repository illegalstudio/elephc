//! Purpose:
//! Shared curl easy-handle resolution and `curl_setopt()`'s option-KIND dispatch, reused
//! by every home file in this family (mirrors `crate::curl_prelude::curl_setopt`'s body
//! one-for-one, minus the callback/share arms this module's doc defers).
//!
//! Called from:
//! - Every `curl_*` home file in this directory.

use crate::curl_ffi as ffi;

use super::*;

/// Returns whether a cell is a PHP ARRAY — and nothing else.
///
/// NOT `RuntimeValueOps::is_array_like`, which is a deliberately WIDER predicate: it answers
/// "can eval index this like an array for a WRITE", and therefore reports `true` for an
/// OBJECT too (runtime tag 6), so that `ArrayAccess`-capable objects reach the runtime's own
/// set helpers. Every `curl_setopt()` guard in this family is asking php-src's `is_array()`
/// question instead — `CURLOPT_HTTPHEADER`'s "must be of type array" `TypeError`, and
/// `CURLOPT_POSTFIELDS`'s array-versus-scalar fork — and using the wider predicate for them
/// is a real misclassification, not a stylistic difference: it sent a `CURLFile` object down
/// the multipart walk's NESTED-ARRAY branch, where it silently produced ZERO parts (measured:
/// a four-field `CURLOPT_POSTFIELDS` array containing a `CURLFile` and a `CURLStringFile`
/// posted three parts instead of five, with both file parts missing and no error anywhere).
pub(in crate::interpreter) fn eval_curl_is_php_array(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(matches!(
        values.type_tag(value)?,
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC
    ))
}

/// Computes the "given" type name AOT's curl prelude uses in its own `TypeError`/
/// `ValueError` messages: `get_class($value)` for an object, the literal `"null"` for
/// `null` (NOT `gettype()`'s `"NULL"`), and `gettype($value)` for everything else —
/// `crate::curl_prelude::curl_setopt`'s own `$given` ternary, reproduced here so an eval
/// curl error message reads identically to the AOT one for the same misuse.
///
/// THE EVAL DYNAMIC-OBJECT REGISTRY IS CONSULTED FIRST, and it has to be. A class DECLARED
/// INSIDE the `eval()` fragment is allocated as a backing `stdClass` cell tagged by object
/// identity in `ElephcEvalContext` (`crate::interpreter::statements::class_resolution`'s
/// `eval_dynamic_class_allocate_object`), so `RuntimeValueOps::object_class_name` — which
/// reads the RUNTIME cell's class — answers `"stdClass"` for it. Reaching only for that is
/// how `curl_setopt($ch, CURLOPT_POSTFIELDS, ['f' => new MyThing()])` reported
/// "... stdClass given" instead of naming the real class (measured). `get_class()` itself
/// resolves this the same way, through `dynamic_object_class_name`.
pub(in crate::interpreter) fn eval_curl_given_type_name(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_NULL {
        return Ok("null".to_string());
    }
    if tag == EVAL_TAG_OBJECT {
        let identity = values.object_identity(value)?;
        if let Some(name) = context.dynamic_object_class_name(identity) {
            return Ok(name);
        }
        let class_name = values.object_class_name(value)?;
        let bytes = values.string_bytes(class_name)?;
        return Ok(String::from_utf8_lossy(&bytes).into_owned());
    }
    Ok(eval_gettype_name(tag).to_string())
}

/// Resolves a `curl_*()` call's `$handle` argument to its EVAL TABLE KEY and bridge raw id,
/// throwing PHP's own `\TypeError` when the cell is not a live curl easy handle this same
/// `ElephcEvalContext` created (not a foreign resource cell, and not a resource at all).
///
/// `function` names the calling builtin for the message; every `curl_*()` function that
/// takes a handle takes it as argument #1, so the position is not parameterized. The
/// message wording — "Argument #1 ($handle) must be of type CurlHandle, X given" — matches
/// `crate::curl_prelude`'s own runtime `instanceof` guards for the handful of curl
/// functions that cannot enforce `CurlHandle` statically (`curl_multi_add_handle`,
/// `curl_multi_getcontent`, `curl_setopt()`'s `CURLOPT_SHARE` case) and real PHP 8.4.20
/// (verified: `curl_close("x")` -> `TypeError: curl_close(): Argument #1 ($handle) must be
/// of type CurlHandle, string given`).
///
/// Every OTHER `curl_*()` function (`curl_escape`, `curl_close`, `curl_setopt`, …)
/// declares `CurlHandle $handle` STRICTLY in the AOT prelude, so a mismatched call site is
/// rejected at COMPILE TIME there — never a runtime throw (verified:
/// `curl_escape($mixedTypedValue, "s")` is a checker error, not a runtime one). eval() has
/// no static checker at all, so this is the runtime-only counterpart of that same compiled
/// guarantee, not a literal reproduction of an AOT runtime throw that does not exist.
pub(in crate::interpreter) fn eval_curl_easy_handle(
    function: &str,
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(i64, i64), EvalStatus> {
    if values.type_tag(handle)? == EVAL_TAG_RESOURCE {
        if let Ok(table_id) = i64::try_from(values.raw_value_word(handle)?) {
            if let Some(raw) = context.stream_resources().curl_easy_raw(table_id) {
                return Ok((table_id, raw));
            }
        }
    }
    let given = eval_curl_given_type_name(handle, context, values)?;
    eval_throw_type_error(
        &format!("{function}(): Argument #1 ($handle) must be of type CurlHandle, {given} given"),
        context,
        values,
    )
}

/// `eval_curl_easy_handle`, keeping only the bridge raw id — for callers that never need
/// the eval table key.
pub(in crate::interpreter) fn eval_curl_easy_raw(
    function: &str,
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    Ok(eval_curl_easy_handle(function, handle, context, values)?.1)
}

/// `eval_curl_easy_handle` for a handle that is NOT argument #1 — `curl_multi_add_handle()`
/// and `curl_multi_remove_handle()` take the easy handle as argument #2, and their AOT
/// counterparts spell that position out in their own runtime `instanceof` guards
/// (`crate::curl_prelude::curl_multi_add_handle`), so the eval message has to as well.
pub(in crate::interpreter) fn eval_curl_easy_handle_at(
    function: &str,
    position: usize,
    parameter: &str,
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(i64, i64), EvalStatus> {
    if values.type_tag(handle)? == EVAL_TAG_RESOURCE {
        if let Ok(table_id) = i64::try_from(values.raw_value_word(handle)?) {
            if let Some(raw) = context.stream_resources().curl_easy_raw(table_id) {
                return Ok((table_id, raw));
            }
        }
    }
    let given = eval_curl_given_type_name(handle, context, values)?;
    eval_throw_type_error(
        &format!(
            "{function}(): Argument #{position} (${parameter}) must be of type CurlHandle, \
             {given} given"
        ),
        context,
        values,
    )
}

/// Resolves a `curl_multi_*()` call's `$multi_handle` argument (always #1) to its EVAL
/// TABLE KEY and bridge raw id, throwing PHP's own catchable `\TypeError` for anything
/// else.
///
/// TYPING BY TABLE MEMBERSHIP is what makes this exact: every eval-owned curl handle draws
/// its key from ONE shared `take_next_id()` counter (`crate::stream_resources`), so a key
/// that resolves in the multi table cannot also be an easy or share key. Passing
/// `curl_multi_exec()` an easy handle therefore lands here, not in a confusing partial
/// success — the eval counterpart of the `CurlMultiHandle $multi_handle` parameter type the
/// AOT prelude gets checked at compile time (`eval_curl_easy_handle`'s own doc explains why
/// eval needs a runtime check where AOT needs none).
pub(in crate::interpreter) fn eval_curl_multi_handle(
    function: &str,
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(i64, i64), EvalStatus> {
    if values.type_tag(handle)? == EVAL_TAG_RESOURCE {
        if let Ok(table_id) = i64::try_from(values.raw_value_word(handle)?) {
            if let Some(raw) = context.stream_resources().curl_multi_raw(table_id) {
                return Ok((table_id, raw));
            }
        }
    }
    let given = eval_curl_given_type_name(handle, context, values)?;
    eval_throw_type_error(
        &format!(
            "{function}(): Argument #1 ($multi_handle) must be of type CurlMultiHandle, \
             {given} given"
        ),
        context,
        values,
    )
}

/// `eval_curl_multi_handle`, keeping only the bridge raw id.
pub(in crate::interpreter) fn eval_curl_multi_raw(
    function: &str,
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    Ok(eval_curl_multi_handle(function, handle, context, values)?.1)
}

/// Resolves a `curl_share_*()` call's `$share_handle` argument (always #1) to its bridge raw
/// id, throwing PHP's own catchable `\TypeError` for anything else.
///
/// A PERSISTENT share (PHP 8.5's `CurlSharePersistentHandle`) IS REFUSED HERE, because
/// php-src does not make that class a subclass of `CurlShareHandle`: `curl_share_setopt()`/
/// `curl_share_errno()`/`curl_share_close()` are all declared `CurlShareHandle` and never
/// accept it (`crate::curl_prelude`'s own comment on the class). The AOT build gets that
/// from the parameter type; eval has to check it.
pub(in crate::interpreter) fn eval_curl_share_raw(
    function: &str,
    handle: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(handle)? == EVAL_TAG_RESOURCE {
        if let Ok(table_id) = i64::try_from(values.raw_value_word(handle)?) {
            if let Some(raw) = context.stream_resources().curl_share_raw(table_id) {
                if !context.stream_resources().curl_share_is_persistent(table_id) {
                    return Ok(raw);
                }
                return eval_throw_type_error(
                    &format!(
                        "{function}(): Argument #1 ($share_handle) must be of type \
                         CurlShareHandle, CurlSharePersistentHandle given"
                    ),
                    context,
                    values,
                );
            }
        }
    }
    let given = eval_curl_given_type_name(handle, context, values)?;
    eval_throw_type_error(
        &format!(
            "{function}(): Argument #1 ($share_handle) must be of type CurlShareHandle, \
             {given} given"
        ),
        context,
        values,
    )
}

/// Rejects a PHP-8.5-only curl function on an older compatibility profile with PHP's own
/// catchable `Call to undefined function` `\Error`.
///
/// THE AOT SIDE GETS THIS FROM THE PRELUDE'S VERSION FENCE:
/// `curl_multi_get_handles()`/`curl_share_init_persistent()` sit inside
/// `-- elephc PHP >= 8.5 ... --` blocks that `prelude_source_for_version` STRIPS below 8.5,
/// so a program compiled with `--php-version 8.4` never declares them and the call fails as
/// undefined, exactly as that runtime would. eval carries one registry for every profile,
/// so the same gate has to be a runtime check against the profile generated code published
/// through `__elephc_eval_set_php_version_id` (`crate::eval_php_profile`).
pub(in crate::interpreter) fn eval_curl_require_php_85(
    function: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if crate::eval_php_profile::eval_php_version_id() >= 80_500 {
        return Ok(());
    }
    // `eval_throw_error` only ever returns `Err(EvalStatus::UncaughtThrowable)` — the
    // `Ok` arm of its `Result<RuntimeCellHandle, _>` is uninhabited in practice — so the
    // cell type it would nominally produce is irrelevant here and named only to satisfy
    // inference. Every other `eval_throw_*` caller in this family returns the value
    // directly and gets the type from its own return position.
    let _: RuntimeCellHandle = eval_throw_error(
        &format!("Call to undefined function {function}()"),
        context,
        values,
    )?;
    Ok(())
}

/// `curl_setopt()`'s message for KIND 6 (a real option this build cannot carry), KIND 8
/// (callbacks) and KIND 9 (the PHP-stream options) — accepted PHP API this eval interpreter
/// specifically does not wire, per this family's module doc — formatted exactly like the
/// AOT prelude's own
/// `__elephc_curl_setopt_unsupported_warning` (`src/codegen_support/runtime/curl/
/// warn_option.rs`'s `CURL_SETOPT_UNSUPPORTED_PREFIX`/`_SUFFIX`), so a script observing
/// the warning text sees the identical wording whether it runs compiled or through
/// `eval()`.
fn warn_unsupported_option(
    option: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    values.warning(&format!(
        "curl_setopt(): Option {option} is not supported by this build"
    ))
}

/// `warn_unsupported_option`'s `curl_multi_setopt()` twin, for `CURLMOPT_PUSHFUNCTION` —
/// the one real php-src multi option this build cannot carry. A SEPARATE STRING because
/// PHP names the function that refused the option, exactly as the AOT side keeps
/// `CURL_MULTI_SETOPT_UNSUPPORTED_PREFIX` separate from `CURL_SETOPT_UNSUPPORTED_PREFIX`
/// (`src/codegen_support/runtime/data`).
pub(in crate::interpreter) fn eval_curl_warn_unsupported_multi_option(
    option: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    values.warning(&format!(
        "curl_multi_setopt(): Option {option} is not supported by this build"
    ))
}

/// Applies one `curl_setopt($handle, $option, $value)` call, given the handle's bridge raw
/// id, EVAL TABLE KEY (for the PHP-layer mirror fields), and already-evaluated option/value
/// cells. Returns the same `bool` `curl_setopt()` itself returns.
///
/// Mirrors `crate::curl_prelude::curl_setopt`'s body kind-for-kind (see that function's own
/// extensive comments for the libcurl-side rationale of each branch), MINUS the
/// `CURLOPT_POSTFIELDS` array/`multipart` special case and KIND 8 (callbacks) — both fall
/// into the honest "not supported by this build" warning path instead, per this family's
/// module doc.
pub(in crate::interpreter) fn eval_curl_setopt_apply(
    raw: i64,
    table_id: i64,
    option: i64,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // CLASSIFY BEFORE NARROWING. `option_kind` takes the option number as the SAME `i64`
    // `curl_setopt()`'s own `int $option` parameter carries (mirroring
    // `crate::curl_prelude::curl_setopt`'s `__elephc_curl_option_kind($option)`, whose PHP
    // `$option` is a 64-bit int too) — narrowing to `i32` FIRST, before classification,
    // used to answer an uncatchable `RuntimeFatal` for any option number outside `i32`
    // range (e.g. `2**40`), even though AOT throws the same catchable `ValueError` an
    // ordinary unrecognized option gets: `option_kind` on a value that large still
    // correctly answers `KIND_INVALID` (it is a lookup against a frozen table of real,
    // small `CURLOPT_*` numbers), so classifying first catches this shape as an ordinary
    // "not a valid cURL option" rejection instead of a special one.
    let kind = ffi::option_kind(option);
    if kind == ffi::KIND_INVALID {
        // php-src's own `ValueError` for an option number it does not recognize at all
        // (`crate::curl_prelude::curl_setopt`'s header). This interpreter DOES have a
        // catchable-exception path from internals — `eval_throw_builtin_value_error`,
        // the same mechanism `range()`'s step `ValueError` or `callable_validation.rs`'s
        // `TypeError`s use — so this mirrors AOT's message verbatim instead of hard-
        // faulting.
        return eval_throw_builtin_value_error(
            "curl_setopt(): Argument #2 ($option) is not a valid cURL option",
            context,
            values,
        );
    }
    // Every option number `option_kind` classifies as anything OTHER than `KIND_INVALID`
    // is one of the frozen table's real `CURLOPT_*` numbers — comfortably inside `i32`
    // (below 10000 for LONG/bool/enum options, up to the low 40000s for blob options) — so
    // this narrowing cannot fail for a value that reached this point. Kept as an explicit
    // guard rather than an unchecked cast, for the same "should not realistically happen"
    // tradeoff every other internal-invariant check in this crate already makes.
    let Ok(opt) = i32::try_from(option) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if kind == ffi::KIND_SLIST {
        if !eval_curl_is_php_array(value, values)? {
            let given = eval_curl_given_type_name(value, context, values)?;
            return eval_throw_type_error(
                &format!(
                    "curl_setopt(): Argument #3 ($value) must be of type array, {given} given"
                ),
                context,
                values,
            );
        }
        let mut blob = Vec::new();
        let len = values.array_len(value)?;
        for position in 0..len {
            let key = values.array_iter_key(value, position)?;
            let item = values.array_get(value, key)?;
            // AOT throws a catchable `\TypeError` for an array/object/null item instead
            // of silently casting it (`crate::curl_prelude::curl_setopt`'s own `is_array
            // ($item) || is_object($item) || is_null($item)` guard) — an eval-side
            // `(string)` cast of an array is a warning plus `"Array"`, not the same
            // failure, so this checks the item's tag before ever reaching `cast_string`.
            let item_tag = values.type_tag(item)?;
            if matches!(
                item_tag,
                EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT | EVAL_TAG_NULL
            ) {
                return eval_throw_type_error(
                    "curl_setopt(): Argument #3 ($value) must be an array of strings for this option",
                    context,
                    values,
                );
            }
            let item = values.cast_string(item)?;
            blob.extend_from_slice(&values.string_bytes(item)?);
            blob.push(0);
        }
        return values.bool_value(ffi::easy_setopt_slist(raw, opt, &blob));
    }
    if kind == ffi::KIND_PHP_LAYER {
        // CURLOPT_RETURNTRANSFER (19913): mirrored onto the eval-side mirror fields
        // because `curl_exec()`'s return shape depends on it, and forwarded to the
        // bridge because the write callback's capture-or-stdout decision lives there.
        if option == 19913 {
            let truthy = values.truthy(value)?;
            context
                .stream_resources_mut()
                .set_curl_easy_write_mode(table_id, truthy, false);
            return values.bool_value(ffi::easy_setopt_long(raw, opt, i64::from(truthy)));
        }
        // CURLOPT_PRIVATE (10103): retained and stored, read back by
        // `curl_getinfo(..., CURLINFO_PRIVATE)`. `set_curl_easy_private` retains its own
        // independent reference — see that method's doc for why storing the caller's bare
        // (unretained) `value` cell here would be a use-after-free as soon as the caller's
        // own variable is unset or reassigned.
        if option == 10103 {
            let stored = context
                .stream_resources_mut()
                .set_curl_easy_private(table_id, value, values)?;
            return values.bool_value(stored);
        }
        // CURLOPT_SAFE_UPLOAD (-1): always on, matching php-src's own rejection of a
        // falsy value with a catchable `\ValueError` (`crate::curl_prelude::curl_setopt`'s
        // own guard, mirrored verbatim).
        if option == -1 {
            if !values.truthy(value)? {
                return eval_throw_builtin_value_error(
                    "curl_setopt(): Disabling safe uploads is no longer supported",
                    context,
                    values,
                );
            }
            return values.bool_value(true);
        }
        // CURLOPT_BINARYTRANSFER (19914): documented no-op in modern PHP.
        return values.bool_value(true);
    }
    // CURLOPT_SHARE (10100) IS THE ONE OBJECT-VALUED `curl_setopt()` OPTION, handled here
    // before the scalar-type guard below (an eval share handle is a resource cell, never
    // int/string/float/bool). BOTH share kinds are accepted — `CurlShareHandle` and PHP
    // 8.5's `CurlSharePersistentHandle` — matching php-src's own `CURLOPT_SHARE` contract
    // and `crate::curl_prelude::curl_setopt`'s `$kind === 7` branch, which likewise accepts
    // the persistent class even though no other share function does.
    //
    // NO PHP-LEVEL REFERENCE IS TAKEN from the easy handle to the share, deliberately: the
    // BRIDGE is what keeps a share alive as long as any easy handle still points at it
    // (`crates/elephc-curl/src/share.rs`'s deferred-free protocol), exactly as on the AOT
    // side. eval could not usefully take one anyway — its curl cells own nothing.
    if kind == ffi::KIND_SHARE {
        if values.type_tag(value)? == EVAL_TAG_RESOURCE {
            if let Ok(share_table_id) = i64::try_from(values.raw_value_word(value)?) {
                if let Some(share_raw) =
                    context.stream_resources().curl_share_raw(share_table_id)
                {
                    return values.bool_value(ffi::easy_set_share(raw, share_raw));
                }
            }
        }
        let given = eval_curl_given_type_name(value, context, values)?;
        return eval_throw_type_error(
            &format!(
                "curl_setopt(): Argument #3 ($value) must be of type CurlShareHandle, \
                 {given} given"
            ),
            context,
            values,
        );
    }
    // CALLBACK OPTIONS (option KIND 8). The callable is rooted in this eval context's own
    // curl table and the bridge slot gets two opaque INTEGERS plus this crate's adapter
    // address — see `crate::interpreter::builtins::curl::callbacks`, whose module doc also
    // carries the reentrancy argument that makes calling back into the interpreter from
    // libcurl's thread sound.
    if kind == ffi::KIND_CALLBACK {
        // A KIND 8 option this mapping does not know would fall through to the scalar guard
        // below and reject a perfectly good closure with a confusing type error, so an
        // unrecognized one takes the honest unsupported-option path instead. Not reachable
        // today: the bridge's own KIND 8 set is exactly these six.
        let Some((slot, option_name)) = eval_curl_callback_slot(option) else {
            warn_unsupported_option(option, values)?;
            return values.bool_value(false);
        };
        return eval_curl_setopt_callback(
            raw, table_id, slot, option_name, value, context, values,
        );
    }
    // KIND_STREAM IS IN THIS LIST FOR A REASON THE OTHER IS NOT: without it, the four
    // PHP-stream options would fall PAST the warning and into the scalar-type guard below,
    // where a stream resource is none of int/string/float/bool and therefore a HARD FATAL.
    // They used to be `KIND_UNSUPPORTED`, so leaving them out here would have turned a
    // `false` + warning into an uncatchable fault the moment the AOT side implemented them.
    if kind == ffi::KIND_STREAM || kind == ffi::KIND_UNSUPPORTED {
        warn_unsupported_option(option, values)?;
        return values.bool_value(false);
    }
    // CURLOPT_POSTFIELDS (10015) is the one option whose value may be an ARRAY as well as a
    // string, and an array posts real `multipart/form-data` through the SAME
    // `elephc_curl_mime_*` ABI the AOT walker uses — see
    // `crate::interpreter::builtins::curl::multipart`, which mirrors
    // `crate::curl_prelude::__elephc_curl_build_multipart` part for part. The plain STRING
    // form (the common urlencoded-body case) still goes through the ordinary KIND_STRING
    // path below.
    //
    // AN EMPTY ARRAY IS AN EMPTY STRING BODY, NOT AN EMPTY MULTIPART, and the special case
    // lives HERE rather than in the walker because that is where the AOT prelude puts it:
    // php-src returns `curl_easy_setopt(cp, CURLOPT_POSTFIELDS, "")` before building any
    // mime structure, and the difference is observable on the wire (an urlencoded content
    // type and an empty body, versus a multipart content type and a boundary-only body).
    if option == 10015 && eval_curl_is_php_array(value, values)? {
        if values.array_len(value)? == 0 {
            return values.bool_value(ffi::easy_setopt_str(raw, opt, b""));
        }
        return eval_curl_build_multipart(raw, value, context, values);
    }
    // php-src rejects a non-scalar `$value` with a catchable `\TypeError` here
    // (`crate::curl_prelude::curl_setopt`'s own guard, mirrored verbatim) — see
    // `KIND_INVALID`'s branch above for the same catchable-exception mechanism.
    let tag = values.type_tag(value)?;
    if !matches!(
        tag,
        EVAL_TAG_INT | EVAL_TAG_STRING | EVAL_TAG_FLOAT | EVAL_TAG_BOOL
    ) {
        let given = eval_curl_given_type_name(value, context, values)?;
        return eval_throw_type_error(
            &format!(
                "curl_setopt(): Argument #3 ($value) must be of type string|int|float|bool, {given} given"
            ),
            context,
            values,
        );
    }
    if kind == ffi::KIND_STRING {
        let value = values.cast_string(value)?;
        let bytes = values.string_bytes(value)?;
        return values.bool_value(ffi::easy_setopt_str(raw, opt, &bytes));
    }
    if kind == ffi::KIND_LONG || kind == ffi::KIND_OFF_T {
        let value = eval_int_value(value, values)?;
        return values.bool_value(ffi::easy_setopt_long(raw, opt, value));
    }
    warn_unsupported_option(option, values)?;
    values.bool_value(false)
}
