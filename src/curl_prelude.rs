//! Purpose:
//! PHP's `ext/curl` surface in elephc-PHP: the `CurlHandle` class plus the easy-handle
//! functions (`curl_init` / `curl_setopt` / `curl_setopt_array` / `curl_exec` /
//! `curl_getinfo` / `curl_close` / `curl_errno` / `curl_error` / `curl_version` / …), and
//! the `CurlMultiHandle` class plus the whole `curl_multi_*` family, implemented on top of
//! the internal `__elephc_curl_*` builtins. PHP 8 models a session as an OBJECT
//! (`CurlHandle`, `CurlMultiHandle`), not a resource, and this prelude is what makes
//! `curl_init()`/`curl_multi_init()` return real objects that `is_object`, `get_class`,
//! `instanceof`, and `var_dump` all agree about.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`,
//!   after the hash prelude and before name resolution.
//!
//! Key details:
//! - WHY A PRELUDE AND NOT A NATIVE CLASS. Every curl `RuntimeFnId` declares
//!   `BuiltinRequirement::Bridge("elephc_curl")` (`crate::ir::runtime_fn`), and the
//!   bridge in turn requires the managed native `curl` package
//!   (`crate::pipeline::backend`), so the surface is bridge-gated: a program that never
//!   uses curl must neither declare `CurlHandle` nor link `-lelephc_curl` nor need
//!   `libcurl.a`. Injecting only on demand preserves that (locked decision 4), and the
//!   whole feature then compiles through the ordinary class/function pipeline.
//! - WHY THE BUILTINS ARE INTERNAL. A prelude function cannot shadow a builtin of the
//!   same name (`Cannot redeclare built-in function`), so the raw work lives in
//!   `internal: true` `__elephc_curl_*` builtins (`crate::builtins::curl`) and the PHP
//!   names are these wrappers. `function_exists('curl_init')` is therefore true exactly
//!   when the prelude is injected.
//! - OWNERSHIP: the object holds the Mixed handle cell in `$__elephc_handle` and adds NO
//!   retain/release of its own. That cell owns the native libcurl easy handle: the
//!   standard object-free path (`__rt_heap_free` → `object_free_deep`) releases property
//!   storage, which reaches `__rt_mixed_free_deep` and thence `__rt_curl_easy_free`
//!   (resource kind 6). There is deliberately no `__destruct`: adding one would
//!   introduce a second free path.
//! - `curl_close()` IS A NO-OP, exactly as in PHP 8 — the handle stays usable until the
//!   object is destroyed. It is declared so `function_exists('curl_close')` is true and
//!   so existing PHP code that calls it keeps working.
//! - EVERY wrapper BINDS `$handle->__elephc_handle` TO A LOCAL (`$raw`) BEFORE CALLING.
//!   This is not style. Passing a `mixed` object property *inline* as a call argument
//!   trips a pre-existing codegen leak (one heap block per call), documented in full in
//!   `crate::hash_prelude`; binding to a local sidesteps it. The same rule applies to
//!   every wrapper added by later curl tasks.
//! - `CURLOPT_RETURNTRANSFER` (19913) is mirrored into `$__elephc_return_transfer`
//!   because `curl_exec()`'s RETURN SHAPE depends on it (`string` when capturing,
//!   `true` otherwise) and the Task-3 C ABI exposes no way to ask a handle whether it is
//!   capturing. The option is still forwarded to the bridge, which owns the actual
//!   capture behaviour (`crates/elephc-curl/src/php_layer.rs`); the property only
//!   selects between `curl_exec()`'s two success shapes.
//! - `__serialize()` THROWS rather than returning a reduced array, matching php-src:
//!   `CurlHandle` is not serializable there either
//!   (`Exception: Serialization of 'CurlHandle' is not allowed`), and elephc could not
//!   reproduce a live libcurl handle from a serialized blob in any case.
//! - `curl_version()` decodes the bridge's JSON blob through the ordinary `json_decode`
//!   builtin instead of a bespoke array builder, so the array reflects the libcurl that
//!   is actually linked AT RUN TIME rather than anything baked in at compile time.
//! - `curl_setopt()` CLASSIFIES THE OPTION NUMBER BEFORE FORWARDING, because
//!   `curl_easy_setopt` reads its variadic argument according to the option's numeric
//!   RANGE — forwarding a PHP value into a pointer-typed range is a wild pointer, not
//!   merely a wrong answer. The classification comes from the bridge's frozen option
//!   table (`crates/elephc-curl/src/options.rs`) through
//!   `__elephc_curl_option_kind()`; see the comment above the function for the kind
//!   codes and the libcurl source that fixes the ranges. An option php-src does not
//!   recognize at all raises php-src's own `ValueError`; one it recognizes that this
//!   build cannot carry answers `false` with a PHP warning (locked decision 7), never an
//!   inert `true`.
//! - `curl_getinfo()` DISPATCHES ON THE `CURLINFO_*` TYPE MASK, the read-side mirror of
//!   `curl_setopt()`'s option table and php-src's own structure: string/long/double/slist/
//!   off_t each read through their own typed entry point, three options are special-cased
//!   before the mask (`CURLINFO_PRIVATE`, `CURLINFO_CERTINFO`, `CURLINFO_HEADER_OUT`), and
//!   anything else answers `false` — never a fabricated value. The no-`$option` form
//!   returns PHP's documented associative array, built as JSON by the bridge and decoded
//!   with the ordinary `json_decode` builtin, the same route `curl_version()` takes.
//! - FIVE SIGNATURES DIVERGE FROM php-src, all forced by the same checker limitation
//!   (a `T|false` union is accepted nowhere a `T` is expected, and is not narrowed by a
//!   `=== false` guard) and all documented at their declaration below: `curl_init()` and
//!   `curl_copy_handle()` return a plain `CurlHandle` and THROW instead of returning
//!   `CurlHandle|false`; `curl_escape()`/`curl_unescape()` return a plain `string` and
//!   throw instead of `string|false`; `curl_version()` leaves its return type undeclared
//!   instead of `array|false`. `curl_strerror()` returns `string` rather than `?string`,
//!   which is not a limitation but a value that is never null. Runtime behaviour is
//!   otherwise PHP's. `docs/php/curl.md` (Task 14) is where these reach users.
//! - THE MULTI INTERFACE ADDS FOUR MORE DIVERGENCES OF THE SAME FAMILY, all in Task 9's
//!   surface and all documented at their declarations: `curl_multi_info_read()` leaves its
//!   return type undeclared instead of `array|false` (declaring the union makes a
//!   `$info === false` guard answer TRUE for a real array once the same variable is later
//!   indexed — measured, the exact shape `curl_version()` documents); and
//!   `curl_multi_add_handle()`/`curl_multi_remove_handle()`/`curl_multi_getcontent()` take
//!   `mixed $handle` instead of `CurlHandle $handle`, with a runtime `instanceof` guard
//!   that raises php-src's own `TypeError`. That last one is a BACKEND limitation rather
//!   than a checker one: an object that reaches the caller as a `mixed` — which every
//!   handle read out of `curl_multi_info_read()`'s array or `curl_multi_get_handles()`'s
//!   list necessarily is — arrives at a TYPED object parameter as the boxed Mixed cell, so
//!   the callee reads the cell's header where the object's slots should be and
//!   `$handle->__elephc_handle` comes back `null`. A `mixed` parameter receives the object
//!   itself. Without this, `curl_multi_getcontent($info['handle'])` — the canonical PHP
//!   multi loop — would be a compile error, or, written with the `instanceof` narrowing
//!   the checker accepts, a SILENTLY WRONG answer.

mod detect;

/// The elephc-PHP curl prelude: the `CurlHandle` class and the easy-handle functions
/// that produce and consume it.
///
/// Numeric option literals are used directly (`19913` = `CURLOPT_RETURNTRANSFER`,
/// `10002` = `CURLOPT_URL`) because the `CURLOPT_*` constant table lands in a later task;
/// both values are frozen in `scripts/docs/curl_surface.json` and cross-checked by
/// `crates/elephc-curl/src/php_layer.rs`.
pub(crate) const CURL_PRELUDE_SRC: &str = r#"<?php

final class CurlHandle {
    public mixed $__elephc_handle = null;
    public bool $__elephc_return_transfer = false;
    public mixed $__elephc_private = false;

    private function __construct() {}

    public static function __elephc_wrap(mixed $raw): CurlHandle {
        $h = new self();
        $h->__elephc_handle = $raw;
        return $h;
    }

    public function __debugInfo(): array {
        return [];
    }

    public function __serialize(): array {
        throw new \Exception("Serialization of 'CurlHandle' is not allowed");
    }
}

// DIVERGENCE FROM PHP: php-src declares `curl_init(): CurlHandle|false` and answers
// `false` when libcurl cannot allocate a handle. elephc returns a plain `CurlHandle` and
// THROWS instead, for the reason `src/image_prelude.rs` documents for `imagecreatefrom*`:
// the checker neither accepts a `CurlHandle|false` argument where a `CurlHandle` is
// expected nor narrows it after a `=== false` guard, so the union return would make the
// standard `$ch = curl_init(); curl_setopt($ch, …);` flow a COMPILE ERROR. Failure here
// means libcurl is out of memory, which no real program recovers from anyway. Error
// handling uses try/catch. (docs/php/curl.md, Task 14, records this for users.)
function curl_init(?string $url = null): CurlHandle {
    $raw = __elephc_curl_easy_init();
    if ($raw === false) {
        throw new \RuntimeException("curl_init(): libcurl could not allocate an easy handle");
    }
    if ($url !== null) {
        __elephc_curl_easy_setopt_str($raw, 10002, $url);
    }
    return CurlHandle::__elephc_wrap($raw);
}

// OPTION NUMBERS ARE CLASSIFIED BEFORE ANYTHING REACHES libcurl, and that check is a
// MEMORY-SAFETY boundary, not a politeness. `curl_easy_setopt` is variadic and picks how
// to read its third argument PURELY FROM THE OPTION'S NUMERIC RANGE (libcurl 8.21.0,
// lib/setopt.c, `Curl_vsetopt`): below 10000 it reads a `long`; 10000-19999 a `char *` or
// a `struct curl_slist *`; 20000-29999 a function pointer; 30000-39999 a `curl_off_t`;
// 40000+ a `struct curl_blob *`. Forwarding an integer into any of the pointer ranges
// hands libcurl a wild pointer it will dereference — `curl_setopt($ch, 20011, 1)`
// (CURLOPT_WRITEFUNCTION) would overwrite the bridge's own write callback with the
// address 1. libcurl accepts it and only crashes later, inside curl_exec.
//
// The RANGE alone is not enough to pick a setter, though: 10000-19999 holds both `char *`
// and `struct curl_slist *` options, and only a table can tell them apart. That table is
// `crates/elephc-curl/src/options.rs`, frozen from `scripts/docs/curl_surface.json`, and
// `__elephc_curl_option_kind()` is how this function reads it. Its answer is one of:
//
//   0  not a cURL option at all      -> ValueError, exactly as php-src does
//   1  long / bool / enum            -> __elephc_curl_easy_setopt_long
//   2  string                        -> __elephc_curl_easy_setopt_str
//   3  string list                   -> __elephc_curl_easy_setopt_slist (an array value)
//   4  curl_off_t                    -> __elephc_curl_easy_setopt_long (the bridge widens)
//   5  PHP-layer pseudo-option       -> handled in this prelude, libcurl never sees it
//   6  real option, not carryable    -> false + PHP's warning (locked decision 7)
//
// THE 0-vs-6 SPLIT IS php-src's OWN. `_php_curl_setopt` (ext/curl/interface.c) ends its
// switch with `zend_argument_value_error(2, "is not a valid cURL option")`, so an option
// number php-src does not recognize THROWS; an option it recognizes but that fails at the
// libcurl level merely returns `false`. Kind 6 is elephc's honest version of the second
// case: the option is real PHP API surface, this build just cannot carry it (a blob, a
// callback, a PHP stream, a share handle), so it answers `false` and says why.
function curl_setopt(CurlHandle $handle, int $option, mixed $value): bool {
    $raw = $handle->__elephc_handle;
    $kind = __elephc_curl_option_kind($option);
    if ($kind === 0) {
        throw new \ValueError("curl_setopt(): Argument #2 (\$option) is not a valid cURL option");
    }
    // A string-list option takes an ARRAY and nothing else, matching php-src's own
    // `zend_type_error(... must be of type array ...)` for the same options. The items
    // travel to the bridge as one NUL-FRAMED blob (`item . "\0"` per element): PHP
    // strings are binary-safe, so `[]` is `""` and `[""]` is a single NUL byte, which a
    // separator-joined encoding could not tell apart.
    if ($kind === 3) {
        if (!is_array($value)) {
            $given = is_object($value) ? get_class($value) : (is_null($value) ? "null" : gettype($value));
            throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be of type array, " . $given . " given");
        }
        $blob = "";
        foreach ($value as $item) {
            if (is_array($item) || is_object($item) || is_null($item)) {
                throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be an array of strings for this option");
            }
            $blob .= (string) $item . "\0";
        }
        return __elephc_curl_easy_setopt_slist($raw, $option, $blob);
    }
    // CURLOPT_POSTFIELDS (10015) is the one option whose value may be an array as well as
    // a string.
    //
    // DIVERGENCE FROM PHP: php-src routes ANY non-empty array through
    // `build_mime_structure_from_hash` and posts `multipart/form-data`, unconditionally —
    // `application/x-www-form-urlencoded` is what PHP produces for a STRING body, never
    // for an array. elephc has no mime builder yet (Task 11 adds `CURLFile` and the mime
    // structure with it), so an array of scalars is urlencoded here instead. That is the
    // stopgap this feature's brief authorizes, not parity: a receiver that distinguishes
    // the two content types WILL see a different request than PHP would have sent.
    // Task 11 replaces this branch with a real mime multipart body.
    if ($option === 10015 && is_array($value)) {
        return __elephc_curl_easy_setopt_str($raw, $option, __elephc_curl_form_encode($value));
    }
    // THE PHP-LAYER OPTIONS ARE DISPATCHED BEFORE THE SCALAR TYPE GUARD BELOW, because
    // they do not reach libcurl and therefore have no C type to be wrong for. php-src
    // `ZVAL_COPY`s whatever it is handed for CURLOPT_PRIVATE and runs `zend_is_true` on
    // the others, so `curl_setopt($ch, CURLOPT_PRIVATE, ['a'])` is legal PHP; running the
    // guard first made it a TypeError.
    if ($kind === 5) {
        // CURLOPT_RETURNTRANSFER (19913) is mirrored onto the object because `curl_exec()`'s
        // RETURN SHAPE depends on it, and forwarded to the bridge because the write
        // callback's capture-or-stdout decision lives there.
        if ($option === 19913) {
            $handle->__elephc_return_transfer = (bool) $value;
            return __elephc_curl_easy_setopt_long($raw, $option, $value ? 1 : 0);
        }
        // CURLOPT_PRIVATE (10103) stores an arbitrary PHP value that
        // `curl_getinfo(..., CURLINFO_PRIVATE)` reads back verbatim. libcurl has its own
        // `CURLOPT_PRIVATE`, but php-src never uses it: the value is a zval, so it lives
        // on the PHP object here for the same reason.
        if ($option === 10103) {
            $handle->__elephc_private = $value;
            return true;
        }
        // CURLOPT_SAFE_UPLOAD (-1) is always on and cannot be turned off, matching
        // php-src's own `zend_value_error` for a falsy value.
        if ($option === -1) {
            if (!$value) {
                throw new \ValueError("curl_setopt(): Disabling safe uploads is no longer supported");
            }
            return true;
        }
        // CURLOPT_BINARYTRANSFER (19914): a documented no-op in modern PHP.
        return true;
    }
    if (!is_int($value) && !is_bool($value) && !is_float($value) && !is_string($value)) {
        $given = is_array($value) ? "array" : (is_object($value) ? get_class($value) : (is_null($value) ? "null" : gettype($value)));
        throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be of type string|int|float|bool, " . $given . " given");
    }
    if ($kind === 2) {
        return __elephc_curl_easy_setopt_str($raw, $option, (string) $value);
    }
    if ($kind === 1 || $kind === 4) {
        return __elephc_curl_easy_setopt_long($raw, $option, (int) $value);
    }
    __elephc_curl_setopt_unsupported_warning($option);
    return false;
}

// `CURLOPT_POSTFIELDS`'s ARRAY form as `application/x-www-form-urlencoded`: keys and
// values percent-encoded (spaces as `+`), joined with `&`.
//
// DIVERGENCE FROM PHP, stated in full at the call site above: php-src posts an array as
// `multipart/form-data`, always. This is Task 11's placeholder, not PHP's encoding.
//
// AN OBJECT IN THE ARRAY IS REFUSED, LOUDLY. In php-src an object here is a `CURLFile`
// (or `CURLStringFile`) and its file contents become a mime part. Silently posting the
// object's string cast as a field value would be worse than an error, so this throws and
// says exactly what is missing.
//
// `$fields` IS DECLARED `mixed`, NOT `array`, ONLY BECAUSE OF THE CHECKER: the caller
// reaches this from inside `curl_setopt()`'s `mixed $value`, and elephc's checker does not
// narrow a `mixed` to `Array(Mixed)` after an `is_array()` guard, so an `array` parameter
// makes the call a COMPILE ERROR. The guard is still there at the only call site.
function __elephc_curl_form_encode(mixed $fields): string {
    $encoded = "";
    foreach ($fields as $name => $value) {
        if (is_object($value)) {
            throw new \RuntimeException("curl_setopt(): CURLOPT_POSTFIELDS with an object value (multipart/form-data upload) is not supported by this build");
        }
        if (is_array($value)) {
            throw new \RuntimeException("curl_setopt(): CURLOPT_POSTFIELDS with a nested array value is not supported by this build");
        }
        if ($encoded !== "") {
            $encoded .= "&";
        }
        // `(string) $x . ""`, NOT `(string) $x`, AND BOUND TO A LOCAL BEFORE THE CALL.
        // A bare `(string) $mixed` cast handed straight to a function argument leaks its
        // temporary (measured with `--gc-stats`: one block per cast, unbounded across a
        // loop); routing it through a concatenation first produces an ordinary owned
        // string local that the caller releases normally. Same family of pre-existing
        // codegen leaks as the "bind the property to a local" rule `crate::hash_prelude`
        // documents, and the reason every wrapper in this prelude binds before it calls.
        $nameRaw = (string) $name . "";
        $valueRaw = (string) $value . "";
        $key = __elephc_curl_urlencode($nameRaw);
        $val = __elephc_curl_urlencode($valueRaw);
        $encoded .= $key . "=" . $val;
    }
    return $encoded;
}

// `application/x-www-form-urlencoded` percent-encoding: `[A-Za-z0-9_.-]` pass through, a
// space becomes `+`, every other byte becomes uppercase `%XX`. Exactly PHP's `urlencode()`.
//
// IT DOES NOT CALL THE `urlencode()` BUILTIN, DELIBERATELY. elephc's `__rt_urlencode` /
// `__rt_rawurlencode` helpers percent-encode ASCII DIGITS: their `A-Z` range test branches
// straight to the punctuation check when the byte is below `'A'`, so the `0-9` test that
// follows is unreachable (`urlencode("42")` answers `"%34%32"`, PHP answers `"42"`). That
// is a pre-existing bug in a shared string builtin, outside this feature's blast radius to
// fix here — but a form body that mangles every number in it would be worse than useless,
// so this encoder is self-contained. When the builtin is fixed, this can collapse back to
// a `urlencode()` call and the behaviour will not change.
function __elephc_curl_urlencode(string $raw): string {
    $out = "";
    $len = strlen($raw);
    for ($i = 0; $i < $len; $i++) {
        $byte = ord($raw[$i]);
        if (($byte >= 48 && $byte <= 57)
            || ($byte >= 65 && $byte <= 90)
            || ($byte >= 97 && $byte <= 122)
            || $byte === 45 || $byte === 46 || $byte === 95) {
            $out .= $raw[$i];
        } elseif ($byte === 32) {
            $out .= "+";
        } else {
            $out .= "%" . "0123456789ABCDEF"[($byte >> 4) & 15] . "0123456789ABCDEF"[$byte & 15];
        }
    }
    return $out;
}

function curl_setopt_array(CurlHandle $handle, array $options): bool {
    foreach ($options as $option => $value) {
        if (!curl_setopt($handle, (int) $option, $value)) {
            return false;
        }
    }
    return true;
}

function curl_exec(CurlHandle $handle): string|bool {
    $raw = $handle->__elephc_handle;
    if (!__elephc_curl_easy_perform($raw)) {
        return false;
    }
    if ($handle->__elephc_return_transfer) {
        // BOUND, NOT RETURNED INLINE: `return <builtin returning a fresh string>;` leaks
        // that string (measured with `--gc-stats`: one block per call), the same
        // pre-existing codegen leak `curl_error()`/`curl_strerror()` below route around.
        $body = __elephc_curl_easy_body($raw);
        return $body;
    }
    return true;
}

function curl_errno(CurlHandle $handle): int {
    $raw = $handle->__elephc_handle;
    return __elephc_curl_easy_errno($raw);
}

// The message is BOUND TO A LOCAL BEFORE IT IS RETURNED. Returning a builtin's freshly
// allocated string straight out of a prelude function leaks it — measured with
// `--gc-stats`: `$m = curl_error($ch)` in a loop grew the heap by one block per call while
// the same builtin called directly stayed balanced. Same family as the "bind the property
// to a local" rule this module's header documents.
function curl_error(CurlHandle $handle): string {
    $raw = $handle->__elephc_handle;
    $message = __elephc_curl_easy_error($raw);
    return $message;
}

// KNOWN NOISE, NOT FIXED: `$handle` is unused (the function is a no-op) and the
// checker (`src/types/warnings/scope_usage.rs`) has no per-parameter or per-prelude
// exemption for that — only a leading `_` in the name suppresses it, which is not
// available here because `$handle` is PHP-VISIBLE: `curl_close(handle: $ch)` is legal
// PHP 8 named-argument syntax, and renaming the parameter would silently break it
// (`src/types/signatures.rs` requires parameter names to stay coherent with php-src).
// Compiling any program that calls `curl_close()` therefore prints a harmless
// `Unused variable: $handle` warning from the injected prelude (verified: it appears
// unconditionally when compiling through the real CLI, e.g. `elephc file.php
// --emit-asm`, not gated behind any flag) — no codegen test caught this because none
// of them assert on the FULL warning set, only on specific expected diagnostics.
function curl_close(CurlHandle $handle): void {}

// `curl_getinfo()` DISPATCHES ON THE OPTION'S TYPE MASK, exactly as php-src does
// (`ext/curl/interface.c`) and for the same reason `curl_setopt()` consults the option
// table: `curl_easy_getinfo` writes through its out-parameter according to
// `$option & CURLINFO_TYPEMASK` (0xf00000) and nothing else, so the shape of the read has
// to be chosen from those bits before libcurl is called.
//
//   0x100000 string    0x200000 long    0x300000 double    0x400000 slist    0x600000 off_t
//
// Three options are answered BEFORE the mask, because php-src special-cases them too:
// CURLINFO_PRIVATE (a PHP value this prelude stored), CURLINFO_CERTINFO (SLIST-tagged but
// really a `struct curl_certinfo *`), and CURLINFO_HEADER_OUT (not a getinfo type at all).
// Anything else — including the socket and pointer type masks PHP has never exposed —
// answers `false`, which is php-src's `default:` too.
function curl_getinfo(CurlHandle $handle, ?int $option = null): mixed {
    $raw = $handle->__elephc_handle;
    if ($option === null) {
        // The no-`$option` associative array. The bridge builds it as JSON (php-src's own
        // key names and value types) and the ordinary `json_decode` builtin turns it into
        // a PHP array, the same route `curl_version()` takes.
        $json = __elephc_curl_easy_str_op($raw, 5, "", 0);
        if (!is_string($json)) {
            return false;
        }
        $decoded = json_decode($json, true);
        if (!is_array($decoded)) {
            return false;
        }
        return $decoded;
    }
    // CURLINFO_PRIVATE (1048597): the arbitrary PHP value CURLOPT_PRIVATE stored on the
    // object. libcurl never held it, so it is answered without a bridge call. `false` on
    // a handle that never set it, matching php-src.
    if ($option === 1048597) {
        return $handle->__elephc_private;
    }
    // CURLINFO_HEADER_OUT (2): php-src returns the captured request header, but capturing
    // it needs the CURLOPT_DEBUGFUNCTION plumbing this build does not have yet (Task 12),
    // and `curl_setopt($ch, CURLINFO_HEADER_OUT, true)` is refused for that same reason.
    // php-src answers `false` for a handle where the option was never enabled, which is
    // therefore also the honest answer here — not a fabricated empty string.
    if ($option === 2) {
        return false;
    }
    // CURLINFO_CERTINFO (4194338): shares the SLIST type mask but hands back a
    // `struct curl_certinfo *`. The bridge encodes it as php-src's array-of-arrays shape.
    if ($option === 4194338) {
        $json = __elephc_curl_easy_str_op($raw, 6, "", $option);
        if (!is_string($json)) {
            return false;
        }
        $decoded = json_decode($json, true);
        if (!is_array($decoded)) {
            return false;
        }
        return $decoded;
    }
    $mask = $option & 15728640;
    if ($mask === 2097152 || $mask === 6291456) {
        // CURLINFO_LONG and CURLINFO_OFF_T. Both are 64-bit integers on every target
        // elephc supports and both surface as PHP `int`, so one entry point reads both —
        // see `crates/elephc-curl/src/easy.rs`'s `getinfo_long` for the LP64 argument.
        return __elephc_curl_easy_getinfo_long($raw, $option);
    }
    if ($mask === 3145728) {
        return __elephc_curl_easy_getinfo_double($raw, $option);
    }
    if ($mask === 1048576) {
        return __elephc_curl_easy_str_op($raw, 3, "", $option);
    }
    if ($mask === 4194304) {
        // A `CURLINFO_SLIST` field arrives as one NUL-FRAMED blob (`item . "\0"` per
        // entry), the same framing `curl_setopt()` sends string lists in. The trailing
        // terminator is trimmed BEFORE the split rather than the trailing empty fragment
        // being popped afterwards: `array_pop()` leaks the value it discards (measured
        // with `--gc-stats`: one block per call), a pre-existing codegen leak this prelude
        // routes around the same way it routes around the others documented above.
        $blob = __elephc_curl_easy_str_op($raw, 4, "", $option);
        if (!is_string($blob)) {
            return false;
        }
        $text = (string) $blob . "";
        if ($text === "") {
            return [];
        }
        return explode("\0", substr($text, 0, strlen($text) - 1));
    }
    return false;
}

// `curl_reset()` puts the handle back to a fresh handle's OPTIONS while keeping its
// identity, its live connections and its cookie/DNS caches — libcurl's own
// `curl_easy_reset` contract, which is what php-src forwards to. The three PHP-layer
// mirrors this prelude keeps on the object have to be cleared alongside libcurl's own
// options, or a reset handle would still report the old `CURLOPT_PRIVATE` value and still
// take `curl_exec()`'s capturing return shape.
function curl_reset(CurlHandle $handle): void {
    $raw = $handle->__elephc_handle;
    __elephc_curl_easy_reset($raw);
    $handle->__elephc_return_transfer = false;
    $handle->__elephc_private = false;
}

// DIVERGENCE FROM PHP, for the same reason `curl_init()` diverges (see its comment):
// php-src declares `curl_copy_handle(CurlHandle $handle): CurlHandle|false`, but a union
// return would make `$copy = curl_copy_handle($ch); curl_exec($copy);` a compile error, so
// this returns a plain `CurlHandle` and THROWS on the allocation failure php-src answers
// `false` for.
//
// THE COPY DUPLICATES BOTH LAYERS. `curl_easy_duphandle` copies libcurl's own options; the
// bridge copies the capture flag and the captured body it keeps beside them; and this
// wrapper copies the two object-side mirrors. Missing any one of the three would produce a
// handle that looks identical and behaves differently — the RETURNTRANSFER mirror in
// particular decides `curl_exec()`'s RETURN TYPE, so a copy without it would answer `true`
// where the original answers a string.
function curl_copy_handle(CurlHandle $handle): CurlHandle {
    $raw = $handle->__elephc_handle;
    $copy = __elephc_curl_easy_copy($raw);
    if ($copy === false) {
        throw new \RuntimeException("curl_copy_handle(): libcurl could not duplicate the easy handle");
    }
    $new = CurlHandle::__elephc_wrap($copy);
    $new->__elephc_return_transfer = $handle->__elephc_return_transfer;
    $new->__elephc_private = $handle->__elephc_private;
    return $new;
}

// `curl_escape()` / `curl_unescape()` go through `curl_easy_escape`/`curl_easy_unescape`
// rather than PHP's own `urlencode()`/`rawurldecode()`, because they are not the same
// function: libcurl percent-encodes every byte outside the unreserved set INCLUDING the
// space (as `%20`, never `+`), and its decoder is binary-safe. Both take a handle in PHP
// even though the encoding does not depend on one.
//
// DIVERGENCE FROM PHP, the same one `curl_init()` documents: php-src declares
// `string|false` and answers `false` when libcurl cannot allocate. Declared that way here,
// the result would be UNUSABLE — elephc's checker rejects a `Union([Str, False])` wherever
// a `string` is expected and does not narrow it after a `=== false` guard, so even
// `curl_unescape($ch, curl_escape($ch, $s))` would be a compile error. These return a
// plain `string` and THROW on the allocation failure instead.
function curl_escape(CurlHandle $handle, string $string): string {
    $raw = $handle->__elephc_handle;
    $escaped = __elephc_curl_easy_str_op($raw, 1, $string, 0);
    if (!is_string($escaped)) {
        throw new \RuntimeException("curl_escape(): libcurl could not URL-encode the string");
    }
    return $escaped;
}

function curl_unescape(CurlHandle $handle, string $string): string {
    $raw = $handle->__elephc_handle;
    $decoded = __elephc_curl_easy_str_op($raw, 2, $string, 0);
    if (!is_string($decoded)) {
        throw new \RuntimeException("curl_unescape(): libcurl could not URL-decode the string");
    }
    return $decoded;
}

// `curl_pause()` returns libcurl's raw `CURLcode`, not a boolean — `0` (`CURLE_OK`) means
// the pause state was applied. It only does anything to a transfer that is actually
// running, i.e. from inside a callback (Task 12); on an idle handle libcurl accepts the
// bitmask and answers `CURLE_OK`, which is also what php-src reports.
function curl_pause(CurlHandle $handle, int $flags): int {
    $raw = $handle->__elephc_handle;
    return __elephc_curl_easy_pause($raw, $flags);
}

function curl_upkeep(CurlHandle $handle): bool {
    $raw = $handle->__elephc_handle;
    return __elephc_curl_easy_upkeep($raw);
}

// DIVERGENCE FROM PHP: php-src declares `curl_strerror(int $error_code): ?string` and
// answers `null` only if libcurl hands back a null pointer, which `curl_easy_strerror`
// never does — it answers `"Unknown error"` for a code it does not recognize. The return
// type is declared `string` here because the `?string` union buys nothing over a value
// that is always a string, and elephc's checker treats a nullable return as a value every
// caller must narrow before using.
function curl_strerror(int $error_code): string {
    $message = __elephc_curl_strerror($error_code);
    return $message;
}

// DIVERGENCE FROM PHP: php-src declares `curl_version(): array|false`. The RETURN VALUE
// here is exactly that (an associative array, or `false`), but the return type is left
// UNDECLARED, because declaring it changes the value. `array|false` checks as
// `Union([Array(Mixed), False])`, whose array arm carries the INDEXED-list
// representation; returning the decoded hash through it reinterprets the payload and
// every string-keyed read comes back `NULL` (measured: `$v['version']` was `NULL` with
// the declaration, `"8.21.0"` without). Undeclared keeps the runtime shape honest.
// (docs/php/curl.md, Task 14, records this for users.)
function curl_version() {
    $json = __elephc_curl_version();
    if ($json === "") {
        return false;
    }
    $decoded = json_decode($json, true);
    if (!is_array($decoded)) {
        return false;
    }
    return $decoded;
}

// THE MULTI HANDLE CARRIES AN IDENTITY MAP, AND THAT IS THE WHOLE DESIGN OF THIS CLASS.
//
// `curl_multi_info_read()` must hand back the SAME `CurlHandle` OBJECT that was added
// (php-src's `_php_curl_multi_find_easy_handle`, so `$info['handle'] === $ch` is true),
// and `curl_multi_get_handles()` must list those objects in add order. libcurl, and
// therefore the bridge, only ever speaks in native handle ids — so SOMETHING has to
// remember which PHP object each id belongs to, and that something has to be on the PHP
// side, because only PHP can hold a PHP object.
//
// WRAPPING A FRESH `CurlHandle` AROUND A REPORTED ID WOULD BE A DOUBLE FREE. The
// `CurlHandle` object's Mixed handle cell OWNS the native handle (resource kind 6 ->
// `curl_easy_cleanup`), so two objects built around one id would each release it: the
// second release would run `curl_easy_cleanup` on freed memory. The map exists so no
// second object is ever minted.
//
// THE MAP IS ALSO WHAT KEEPS AN ATTACHED HANDLE ALIVE. `$__elephc_handles` holds ordinary
// PHP references, so `unset($ch)` while the handle is attached does NOT tear the native
// handle down — the multi handle still references the object. That mirrors php-src, which
// takes a real reference (`Z_ADDREF_P`) on every added handle for exactly this reason: a
// multi handle driving a transfer through a freed easy handle would be a use-after-free.
//
// IT IS TWO PARALLEL LISTS, NOT ONE `[$id => $object]` HASH, because removing a handle
// would then need `unset($this->__elephc_ids[$id])`, and unsetting an element of an array
// held in a declared property is not a shape this compiler lowers. Two positionally
// aligned lists rebuild cleanly and keep add order for free, which is what
// `curl_multi_get_handles()` needs anyway.
final class CurlMultiHandle {
    public mixed $__elephc_handle = null;
    public array $__elephc_ids = [];
    public array $__elephc_handles = [];

    private function __construct() {}

    public static function __elephc_wrap(mixed $raw): CurlMultiHandle {
        $h = new self();
        $h->__elephc_handle = $raw;
        return $h;
    }

    // `mixed $handle`, NOT `CurlHandle $handle`, and that is FORCED BY THE BACKEND, not a
    // looseness: a value that reached the caller as a `mixed` (an array element, e.g.
    // `$info['handle']`) and is then passed to a TYPED object parameter arrives as the
    // boxed Mixed cell rather than the object, so every property read inside the callee
    // reads the cell's header instead of the object's slots. Measured: a `CurlHandle`
    // whose `$__elephc_handle` is a live resource reads back as `null` through a typed
    // parameter and as the resource through a `mixed` one. The runtime guard at each
    // public entry point below is what keeps the type honest.
    public function __elephc_attach(int $id, mixed $handle): void {
        $this->__elephc_ids[] = $id;
        $this->__elephc_handles[] = $handle;
    }

    public function __elephc_detach(int $id): void {
        $ids = [];
        $handles = [];
        $count = count($this->__elephc_ids);
        for ($i = 0; $i < $count; $i++) {
            if ($this->__elephc_ids[$i] === $id) {
                continue;
            }
            $ids[] = $this->__elephc_ids[$i];
            $handles[] = $this->__elephc_handles[$i];
        }
        $this->__elephc_ids = $ids;
        $this->__elephc_handles = $handles;
    }

    // `mixed`, not `?CurlHandle`: the caller (`curl_multi_info_read()`) puts the answer
    // straight into a PHP array, and a nullable class return would have to be narrowed
    // before it could be used at all.
    public function __elephc_lookup(int $id): mixed {
        $count = count($this->__elephc_ids);
        for ($i = 0; $i < $count; $i++) {
            if ($this->__elephc_ids[$i] === $id) {
                return $this->__elephc_handles[$i];
            }
        }
        return null;
    }

    public function __debugInfo(): array {
        return [];
    }

    public function __serialize(): array {
        throw new \Exception("Serialization of 'CurlMultiHandle' is not allowed");
    }
}

// php-src declares `curl_multi_init(): CurlMultiHandle` with NO `false` arm — libcurl
// failing to allocate is not something it models — so the allocation failure the bridge
// can still report becomes a thrown `RuntimeException` here rather than a return value
// PHP's own signature does not have.
function curl_multi_init(): CurlMultiHandle {
    $raw = __elephc_curl_multi_init();
    if ($raw === false) {
        throw new \RuntimeException("curl_multi_init(): libcurl could not allocate a multi handle");
    }
    return CurlMultiHandle::__elephc_wrap($raw);
}

// The bookkeeping happens ONLY on `CURLM_OK`, so a refused attach (`CURLM_ADDED_ALREADY`
// when the handle is already on this or another multi handle) leaves the map exactly as
// it was — otherwise `curl_multi_get_handles()` would list a handle libcurl never took.
function curl_multi_add_handle(CurlMultiHandle $multi_handle, mixed $handle): int {
    if (!($handle instanceof CurlHandle)) {
        $given = is_object($handle) ? get_class($handle) : (is_null($handle) ? "null" : gettype($handle));
        throw new \TypeError("curl_multi_add_handle(): Argument #2 (\$handle) must be of type CurlHandle, " . $given . " given");
    }
    $multiRaw = $multi_handle->__elephc_handle;
    $easyRaw = $handle->__elephc_handle;
    $code = __elephc_curl_multi_add($multiRaw, $easyRaw);
    if ($code === 0) {
        $id = __elephc_curl_easy_id($easyRaw);
        $multi_handle->__elephc_attach($id, $handle);
    }
    return $code;
}

function curl_multi_remove_handle(CurlMultiHandle $multi_handle, mixed $handle): int {
    if (!($handle instanceof CurlHandle)) {
        $given = is_object($handle) ? get_class($handle) : (is_null($handle) ? "null" : gettype($handle));
        throw new \TypeError("curl_multi_remove_handle(): Argument #2 (\$handle) must be of type CurlHandle, " . $given . " given");
    }
    $multiRaw = $multi_handle->__elephc_handle;
    $easyRaw = $handle->__elephc_handle;
    $code = __elephc_curl_multi_remove($multiRaw, $easyRaw);
    if ($code === 0) {
        $id = __elephc_curl_easy_id($easyRaw);
        $multi_handle->__elephc_detach($id);
    }
    return $code;
}

// `$still_running` IS A REQUIRED BY-REFERENCE PARAMETER, exactly as in php-src, and the
// bridge returns BOTH of this function's outputs packed into one integer so it can be:
// the still-running count occupies the high 32 bits and the `CURLMcode` the low 32
// (`crates/elephc-curl/src/multi.rs`). Unpacking here rather than in C keeps the
// by-reference write an ordinary PHP assignment.
//
// THE LOW HALF IS SIGN-EXTENDED BY HAND because a `CURLMcode` can be negative
// (`CURLM_CALL_MULTI_PERFORM` is `-1`) and the packing carries it as an unsigned 32-bit
// field; without this, `-1` would surface as `4294967295`.
function curl_multi_exec(CurlMultiHandle $multi_handle, int &$still_running): int {
    $raw = $multi_handle->__elephc_handle;
    $packed = __elephc_curl_multi_exec($raw);
    $still_running = ($packed >> 32) & 4294967295;
    $code = $packed & 4294967295;
    if ($code >= 2147483648) {
        $code -= 4294967296;
    }
    return $code;
}

// php-src converts the `float $timeout` (seconds) to libcurl's millisecond timeout with a
// plain `(int)(timeout * 1000.0)` cast, and so does this — including for a negative or
// zero timeout, which libcurl reads as "return immediately".
function curl_multi_select(CurlMultiHandle $multi_handle, float $timeout = 1.0): int {
    $raw = $multi_handle->__elephc_handle;
    $milliseconds = (int) ($timeout * 1000.0);
    return __elephc_curl_multi_select($raw, $milliseconds);
}

// DIVERGENCE FROM PHP: php-src declares `curl_multi_info_read(...): array|false`. The
// RETURN VALUE here is exactly that, but the return TYPE is left undeclared, for the same
// measured reason `curl_version()` leaves its own undeclared: with `array|false` declared,
// a caller that writes the documented loop —
//
//     while (true) { $info = curl_multi_info_read($mh, $q); if ($info === false) break; …$info['msg']… }
//
// takes the `=== false` branch on the FIRST iteration even though a real array was
// returned, because indexing `$info` later in the loop types its slot as an array and the
// union value is reinterpreted through that representation. Undeclared keeps the runtime
// value honest and the loop working.
//
// `$queued_messages` IS AN OPTIONAL BY-REFERENCE PARAMETER, matching php-src, and it is
// LEFT UNTOUCHED when the queue is empty — php-src returns `false` before it reaches its
// own `ZEND_TRY_ASSIGN_REF_LONG`, so a caller's variable keeps whatever it held.
//
// THE `handle` KEY IS OMITTED WHEN THE OBJECT CANNOT BE FOUND, which is also php-src's
// behaviour (`_php_curl_multi_find_easy_handle` returning NULL simply adds no key): the
// alternative would be minting a second `CurlHandle` around the same native handle, which
// would double-free it. See `CurlMultiHandle`'s comment for the whole ownership argument.
function curl_multi_info_read(CurlMultiHandle $multi_handle, int &$queued_messages = null) {
    $raw = $multi_handle->__elephc_handle;
    if (__elephc_curl_multi_info_read($raw, 0) !== 1) {
        return false;
    }
    $queued_messages = __elephc_curl_multi_info_read($raw, 4);
    $msg = __elephc_curl_multi_info_read($raw, 1);
    $result = __elephc_curl_multi_info_read($raw, 2);
    $easyId = __elephc_curl_multi_info_read($raw, 3);
    $handle = $multi_handle->__elephc_lookup($easyId);
    if ($handle === null) {
        return ["msg" => $msg, "result" => $result];
    }
    return ["msg" => $msg, "result" => $result, "handle" => $handle];
}

// KNOWN NOISE, NOT FIXED: `$multi_handle` is unused, for the same reason `curl_close()`'s
// `$handle` is (see its comment) — the function is a no-op in PHP 8 and the parameter name
// is PHP-visible named-argument surface that cannot be renamed to `_multi_handle`.
function curl_multi_close(CurlMultiHandle $multi_handle): void {}

// `curl_multi_getcontent()` READS THE CAPTURE BUFFER WITHOUT CONSUMING IT, because
// php-src's `RETURN_STR_COPY(ch->handlers.write->buf.s)` copies and leaves the buffer in
// place: calling it twice answers the same body twice. The buffer is reset where php-src
// resets it — at the start of the next transfer, and on `curl_multi_add_handle()`.
//
// `null` (NOT `""`) FOR A HANDLE WITHOUT `CURLOPT_RETURNTRANSFER`, matching php-src's own
// `RETURN_NULL()` for a handle whose write method is not `PHP_CURL_RETURN`. The two are
// genuinely different answers: `""` means "captured nothing", `null` means "was never
// capturing".
function curl_multi_getcontent(mixed $handle): ?string {
    if (!($handle instanceof CurlHandle)) {
        $given = is_object($handle) ? get_class($handle) : (is_null($handle) ? "null" : gettype($handle));
        throw new \TypeError("curl_multi_getcontent(): Argument #1 (\$handle) must be of type CurlHandle, " . $given . " given");
    }
    if (!$handle->__elephc_return_transfer) {
        return null;
    }
    $raw = $handle->__elephc_handle;
    $body = __elephc_curl_easy_body($raw);
    return $body;
}

// The `CURLMOPT_*` numbers are classified INSIDE THE BRIDGE
// (`crates/elephc-curl/src/multi.rs`), not here, for the same memory-safety reason
// `curl_setopt()` consults the easy option table: `curl_multi_setopt` is variadic and
// reads its third argument from the option's numeric range, so handing a PHP integer to
// `CURLMOPT_PUSHFUNCTION` (20014) would install it as a function pointer. The answer is
// three-way: `1` applied, `0` recognized but not carryable by this build (`false` plus
// PHP's warning, locked decision 7), `-1` not a cURL multi option at all (php-src's own
// `ValueError`).
function curl_multi_setopt(CurlMultiHandle $multi_handle, int $option, mixed $value): bool {
    $raw = $multi_handle->__elephc_handle;
    if (!is_int($value) && !is_bool($value) && !is_float($value) && !is_string($value)) {
        $given = is_array($value) ? "array" : (is_object($value) ? get_class($value) : (is_null($value) ? "null" : gettype($value)));
        throw new \TypeError("curl_multi_setopt(): Argument #3 (\$value) must be of type string|int|float|bool, " . $given . " given");
    }
    $applied = __elephc_curl_multi_setopt($raw, $option, (int) $value);
    if ($applied === -1) {
        throw new \ValueError("curl_multi_setopt(): Argument #2 (\$option) is not a valid cURL multi option");
    }
    if ($applied === 0) {
        __elephc_curl_multi_setopt_unsupported_warning($option);
        return false;
    }
    return true;
}

function curl_multi_errno(CurlMultiHandle $multi_handle): int {
    $raw = $multi_handle->__elephc_handle;
    return __elephc_curl_multi_errno($raw);
}

// DIVERGENCE FROM PHP, the same one `curl_strerror()` documents: php-src declares
// `?string` and answers `null` only for a null pointer libcurl never returns. The message
// is bound to a local before it is returned for the leak reason this module's header
// explains.
function curl_multi_strerror(int $error_code): string {
    $message = __elephc_curl_multi_strerror($error_code);
    return $message;
}
// -- elephc PHP >= 8.5 curl_multi_get_handles begin --
// PHP 8.5 ONLY (locked decision 8): 8.2-8.4 have no `curl_multi_get_handles`, and a
// program compiled with `--php-version 8.4` must see an "undefined function" error for it,
// exactly as those runtimes do. The block markers are what
// `prelude_source_for_version` strips; `src/pdo_prelude.rs` uses the identical mechanism.
//
// The handles come back IN ADD ORDER and are the same objects that were added — the map
// this file's `CurlMultiHandle` comment describes is the whole reason that is possible.
function curl_multi_get_handles(CurlMultiHandle $multi_handle): array {
    $handles = $multi_handle->__elephc_handles;
    return $handles;
}
// -- elephc PHP >= 8.5 curl_multi_get_handles end --
"#;

/// Injects the curl prelude when the program references the `ext/curl` surface, leaving
/// every other program untouched, for the DEFAULT PHP compatibility version.
///
/// `force` comes from an explicit opt-in (`--with-curl`, and the codegen harness);
/// otherwise the decision is `detect::program_uses_curl`. The prelude carries only
/// declarations, so prepending it is order-independent — PHP hoists them.
pub fn inject_if_used(
    program: crate::parser::ast::Program,
    force: bool,
) -> crate::parser::ast::Program {
    inject_if_used_for_version(program, force, crate::php_version::PhpVersion::default())
}

/// Injects the curl prelude generated for an explicit PHP compatibility version.
///
/// The curl surface is not the same in every supported PHP: `curl_multi_get_handles()`
/// exists only from 8.5 (locked decision 8), so compiling with `--php-version 8.4` must
/// leave it undeclared and let the call fail as "undefined function", the way that runtime
/// would. The version-specific parts of the source are fenced with
/// `// -- elephc PHP >= <version> … begin/end --` markers and removed by
/// [`prelude_source_for_version`], the same mechanism `crate::pdo_prelude` uses.
pub fn inject_if_used_for_version(
    program: crate::parser::ast::Program,
    force: bool,
    php_version: crate::php_version::PhpVersion,
) -> crate::parser::ast::Program {
    if !force && !detect::program_uses_curl(&program) {
        return program;
    }
    let source = prelude_source_for_version(php_version);
    let tokens = crate::lexer::tokenize(source.as_ref()).expect("curl prelude must tokenize");
    let mut combined = crate::parser::parse_internal(&tokens).expect("curl prelude must parse");
    combined.extend(program);
    combined
}

/// Returns the curl prelude source for one PHP compatibility version: the full text for
/// 8.5 and later, with each later-than-`php_version` block removed otherwise.
fn prelude_source_for_version(
    php_version: crate::php_version::PhpVersion,
) -> std::borrow::Cow<'static, str> {
    if php_version >= crate::php_version::PhpVersion::Php85 {
        return std::borrow::Cow::Borrowed(CURL_PRELUDE_SRC);
    }
    let mut source = CURL_PRELUDE_SRC.to_owned();
    remove_version_block(
        &mut source,
        "// -- elephc PHP >= 8.5 curl_multi_get_handles begin --",
        "// -- elephc PHP >= 8.5 curl_multi_get_handles end --",
    );
    std::borrow::Cow::Owned(source)
}

/// Removes one `begin`/`end`-fenced block (markers included) from the prelude source.
///
/// A MISSING MARKER PANICS rather than silently leaving the block in: the whole point of
/// the fence is that an 8.4 build cannot declare an 8.5 function, and a marker renamed by
/// an unrelated edit would break that quietly. Mirrors `pdo_prelude::remove_version_block`.
fn remove_version_block(source: &mut String, begin: &str, end: &str) {
    let start = source
        .find(begin)
        .unwrap_or_else(|| panic!("missing curl prelude version-gate marker: {begin}"));
    let relative_end = source[start..]
        .find(end)
        .unwrap_or_else(|| panic!("missing curl prelude version-gate marker: {end}"));
    let mut finish = start + relative_end + end.len();
    if source.as_bytes().get(finish) == Some(&b'\n') {
        finish += 1;
    }
    source.replace_range(start..finish, "");
}

#[cfg(test)]
mod version_tests {
    use super::*;
    use crate::php_version::PhpVersion;

    /// 8.5 (elephc's default) declares `curl_multi_get_handles()`; every earlier profile
    /// must not, because those runtimes do not have the function at all.
    #[test]
    fn curl_multi_get_handles_is_php_85_only() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            let declared = source.contains("function curl_multi_get_handles(");
            assert_eq!(
                declared,
                version >= PhpVersion::Php85,
                "curl_multi_get_handles must be declared exactly for PHP >= 8.5, not {version}"
            );
        }
    }

    /// Stripping the 8.5 block must not take any of the version-independent multi surface
    /// with it — the failure mode of a mis-placed end marker.
    #[test]
    fn every_version_keeps_the_version_independent_multi_surface() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            for declaration in [
                "final class CurlMultiHandle {",
                "function curl_multi_init(): CurlMultiHandle {",
                "function curl_multi_add_handle(",
                "function curl_multi_exec(",
                "function curl_multi_info_read(",
                "function curl_multi_getcontent(",
                "function curl_multi_strerror(",
            ] {
                assert!(
                    source.contains(declaration),
                    "{version} must keep {declaration}"
                );
            }
        }
    }

    /// The gated source still tokenizes and parses on every profile: a stripped block that
    /// left an unbalanced brace would otherwise only fail when a user compiled for 8.4.
    #[test]
    fn every_version_parses() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            let tokens = crate::lexer::tokenize(source.as_ref())
                .unwrap_or_else(|e| panic!("{version} curl prelude must tokenize: {e:?}"));
            crate::parser::parse_internal(&tokens)
                .unwrap_or_else(|e| panic!("{version} curl prelude must parse: {e:?}"));
        }
    }
}
