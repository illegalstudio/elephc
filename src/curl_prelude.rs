//! Purpose:
//! PHP's `ext/curl` easy-handle surface — the `CurlHandle` class plus the
//! `curl_init` / `curl_setopt` / `curl_setopt_array` / `curl_exec` / `curl_getinfo` /
//! `curl_close` / `curl_errno` / `curl_error` / `curl_version` functions — implemented in
//! elephc-PHP on top of the internal `__elephc_curl_*` builtins. PHP 8 models a session as
//! an OBJECT (`CurlHandle`), not a resource, and this prelude is what makes `curl_init()`
//! return a real object that `is_object`, `get_class`, `instanceof`, and `var_dump` all
//! agree about.
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
//! - TWO SIGNATURES DIVERGE FROM php-src, both forced by checker limitations and both
//!   documented at their declaration below: `curl_init()` returns a plain `CurlHandle`
//!   and throws instead of returning `CurlHandle|false`, and `curl_version()` leaves its
//!   return type undeclared instead of `array|false`. Runtime behaviour is otherwise
//!   PHP's. `docs/php/curl.md` (Task 14) is where these reach users.

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
    // a string: PHP encodes an array of scalars as `application/x-www-form-urlencoded`.
    // (An array containing a CURLFile would be multipart/form-data — Task 11.)
    if ($option === 10015 && is_array($value)) {
        return __elephc_curl_easy_setopt_str($raw, $option, __elephc_curl_form_encode($value));
    }
    if (!is_int($value) && !is_bool($value) && !is_float($value) && !is_string($value)) {
        $given = is_array($value) ? "array" : (is_object($value) ? get_class($value) : (is_null($value) ? "null" : gettype($value)));
        throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be of type string|int|float|bool, " . $given . " given");
    }
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
    if ($kind === 2) {
        return __elephc_curl_easy_setopt_str($raw, $option, (string) $value);
    }
    if ($kind === 1 || $kind === 4) {
        return __elephc_curl_easy_setopt_long($raw, $option, (int) $value);
    }
    __elephc_curl_setopt_unsupported_warning($option);
    return false;
}

// `CURLOPT_POSTFIELDS`'s ARRAY form, encoded exactly as PHP does when no `CURLFile` is
// present: `application/x-www-form-urlencoded`, keys and values through `urlencode()`
// (spaces as `+`, everything else percent-encoded), joined with `&`.
//
// AN OBJECT IN THE ARRAY IS REFUSED, LOUDLY. In php-src an object here is a `CURLFile`
// (or `CURLStringFile`), which switches the whole body to `multipart/form-data` — a
// completely different request. `CURLFile` lands in Task 11; until then a half-done
// encoding that silently posted `"Object"` as a field value would be worse than an error,
// so this throws and says exactly what is missing.
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
        return __elephc_curl_easy_body($raw);
    }
    return true;
}

function curl_errno(CurlHandle $handle): int {
    $raw = $handle->__elephc_handle;
    return __elephc_curl_easy_errno($raw);
}

function curl_error(CurlHandle $handle): string {
    $raw = $handle->__elephc_handle;
    return __elephc_curl_easy_error($raw);
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

// TASK 7 SCOPE: only `CURLINFO_HTTP_CODE` (2097154) is implemented. Every other option,
// AND the no-`$option` associative-array form PHP documents, answer `false` — an honest
// "not implemented yet" signal, never a fabricated array or a wrong number. Task 8 Wave C
// owns the rest of the info surface (`.superpowers/sdd/php-curl-family/global-constraints.md`'s
// file map).
//
// OPTION VALIDATION MIRRORS `curl_setopt()`'s (see the comment above that function):
// `curl_easy_getinfo()` also dispatches its OUTPUT pointer's shape purely from the
// option's numeric TYPE MASK (libcurl 8.21.0, lib/getinfo.c, `Curl_getinfo`) — a
// STRING-typed option's `char **` and a LONG-typed option's `long *` are both 8 bytes on
// every target this compiler ships for, so mismatching them cannot corrupt memory the way
// `curl_setopt()`'s pointer-range mixups could, but it can still hand back a raw pointer
// value dressed up as a PHP int. Rather than reasoning through every `CURLINFO_*` shape
// now, only the one option this build actually understands is forwarded by exact number;
// everything else answers `false` before reaching the bridge at all.
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
"#;

/// Injects the curl prelude when the program references the `ext/curl` surface, leaving
/// every other program untouched.
///
/// `force` comes from an explicit opt-in (`--with-curl`, and the codegen harness);
/// otherwise the decision is `detect::program_uses_curl`. The prelude carries only
/// declarations, so prepending it is order-independent — PHP hoists them.
pub fn inject_if_used(
    program: crate::parser::ast::Program,
    force: bool,
) -> crate::parser::ast::Program {
    if !force && !detect::program_uses_curl(&program) {
        return program;
    }
    let tokens = crate::lexer::tokenize(CURL_PRELUDE_SRC).expect("curl prelude must tokenize");
    let mut combined = crate::parser::parse_internal(&tokens).expect("curl prelude must parse");
    combined.extend(program);
    combined
}
