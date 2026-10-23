//! Purpose:
//! The curl prelude's PHP migration oracle: the source text the shipped AST was
//! transcribed from, kept so the node-by-node comparison can prove the two agree.
//!
//! Called from:
//! - `crate::curl_prelude::prelude_source_for_version` (re-exported) and the oracle tests
//!   in `crate::synthetic_class::transcribe`.
//!
//! Key details:
//! - TEST-ONLY. The shipped declarations are built in Rust by
//!   `crate::curl_prelude::build::curl_declarations`; this module is never compiled into
//!   the compiler binary.
//! - `CURL_PRELUDE_SRC` is the full 8.5 text; `prelude_source_for_version` strips the
//!   later-than-`php_version` fences for the older profiles.

#[cfg(test)]
pub(crate) const CURL_PRELUDE_SRC: &str = r#"<?php

final class CurlHandle {
    // CURLMOPT_PUSHFUNCTION's user callable, copied from the multi when this handle is
    // attached. The bridge trampoline reads slot 7, which points at the prelude dispatcher
    // below, and the dispatcher reads THIS property off the parent handle it receives.
    public mixed $__elephc_push_user = null;
    // Server-pushed handles minted inside the push callback, keyed by bridge id. They live
    // here until `curl_multi_info_read()` hands them out, because the callback's own local
    // is the only other reference and php-src promises the SAME object comes back from
    // info_read. A static map rather than a back-pointer on the handle: a handle -> multi
    // link would close a reference cycle the refcount-only runtime can never break.
    public static array $__elephc_pushed = [];
    public mixed $__elephc_handle = null;
    public bool $__elephc_return_transfer = false;
    public mixed $__elephc_private = false;
    // GC ROOT for the callables installed through curl_setopt()'s callback options,
    // keyed by the bridge's slot index (0 write, 1 header, 2 read, 3 progress, 4 debug,
    // 5 xferinfo). The bridge stores only the raw descriptor POINTER — it cannot touch a
    // refcount it has no way to see — so the normalized callable has to stay reachable
    // from PHP for as long as it is installed, exactly as `Pdo\Sqlite` roots its SQLite
    // user functions. Assigning over a slot (or `null`ing it) is also what releases the
    // previous callable, so re-setopt and `curl_reset()` need no explicit free.
    public array $__elephc_callbacks = [];
    // Whether the ACTIVE write mode is PHP_CURL_USER. `__elephc_return_transfer` alone
    // cannot answer that: php-src's third mode, PHP_CURL_STDOUT, has both flags false
    // while the callable stays rooted (e.g. CURLOPT_RETURNTRANSFER set to *false* after a
    // CURLOPT_WRITEFUNCTION). `curl_copy_handle()` needs the real mode to decide whether
    // to re-register slot 0 on the duplicate.
    public bool $__elephc_write_user = false;
    // THE FOUR STREAM OPTIONS' SINKS/SOURCES, and the GC ROOTS that keep them open for as
    // long as they are registered. libcurl never sees any of them: each is serviced by an
    // internal closure installed in the matching callback slot above, which reads the
    // stream back off `$ch` (the handle every curl callback receives as its first
    // argument) instead of capturing it — so `curl_copy_handle()` re-registering that
    // closure on the DUPLICATE makes it follow the duplicate's own streams, and no
    // closure ever captures the handle it lives on (which would be the refcount cycle
    // `curl_setopt()`'s callback branch documents).
    public mixed $__elephc_file = null;         // CURLOPT_FILE (10001)
    public mixed $__elephc_writeheader = null;  // CURLOPT_WRITEHEADER (10029)
    public mixed $__elephc_infile = null;       // CURLOPT_INFILE/CURLOPT_READDATA (10009)
    public mixed $__elephc_stderr = null;       // CURLOPT_STDERR (10037)
    // The user's CURLOPT_READFUNCTION, kept OFF the slot table. The read slot always holds
    // this prelude's dispatcher, because php's read path is the one place where the
    // callback does NOT simply win by being set last: a user READFUNCTION outranks
    // CURLOPT_INFILE whichever order the two arrive in, and clearing it falls BACK to the
    // stream. See `curl_setopt()`'s `$kind === 9` branch for the measurements.
    public mixed $__elephc_read_user = null;
    // The user's CURLOPT_PREREQFUNCTION, parked the same way `$__elephc_read_user` is:
    // the installed wrapper reads it off `$ch` because a captured local would carry the
    // flow type of the debug wrapper's 3-parameter closure and the checker would reject
    // the five-argument call_user_func. `curl_copy_handle()` copies it onto the
    // duplicate, whose re-registered wrapper reads the COPY's property.
    public mixed $__elephc_prereq_user = null;
    // Whether CURLOPT_DEBUGFUNCTION has EVER been set on this handle (even to null).
    // php-src installs its own C debug trampoline the first time and never removes it, so
    // libcurl's `data->set.fdebug` stays non-NULL and CURLOPT_STDERR is shadowed for the
    // rest of the handle's life. Measured on PHP 8.4.20: after
    // `curl_setopt($ch, CURLOPT_DEBUGFUNCTION, null)`, a CURLOPT_STDERR stream that
    // worked before receives NOTHING — even though no PHP callable is installed any more.
    public bool $__elephc_debug_user = false;
    // Whether the DEBUG slot currently holds a USER callable (as opposed to the inert
    // no-op the null case installs). PHP's CURLINFO_HEADER_OUT ValueError fires only when a
    // real debug callback is in effect, not merely because the option was touched.
    public bool $__elephc_debug_installed = false;
    // CURLINFO_HEADER_OUT (2): the outgoing request header captured through the debug
    // callback while the option is enabled. `null` means it was never captured.
    public mixed $__elephc_header_out = null;
    public bool $__elephc_header_out_enabled = false;
    // Set only while `__elephc_wrap()` mints an instance, so the public
    // constructor can throw PHP's own "Cannot directly construct" Error for a
    // user's `new CurlHandle` (php-src does exactly this at runtime) while the
    // prelude can still build one internally.
    private static bool $__elephc_minting = false;

    public function __construct() {
        if (!self::$__elephc_minting) {
            throw new \Error("Cannot directly construct CurlHandle, use curl_init() instead");
        }
    }

    public static function __elephc_wrap(mixed $raw): CurlHandle {
        self::$__elephc_minting = true;
        $h = new self();
        self::$__elephc_minting = false;
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
// handling uses try/catch. (docs/php/curl.md records this for users.)
function curl_init(?string $url = null): CurlHandle {
    $raw = __elephc_curl_easy_init();
    if ($raw === false) {
        throw new \RuntimeException("curl_init(): libcurl could not allocate an easy handle");
    }
    if ($url !== null) {
        // THE URL GOES THROUGH curl_setopt, NOT the direct internal setopt: the open_basedir
        // FILE-protocol strip lives in curl_setopt's URL branch (php-src's
        // `php_curl_option_url()`), and the direct internal call here skipped it — measured:
        // `curl_init("file:///x")` still transferred where the setopt path refused.
        $handle = CurlHandle::__elephc_wrap($raw);
        curl_setopt($handle, CURLOPT_URL, $url);
        return $handle;
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
//   6  real option, not carryable    -> false + PHP's warning
//   7  CURLOPT_SHARE                 -> __elephc_curl_easy_set_share (a CurlShareHandle
//                                        object, not a scalar; the ONLY option this whole
//                                        function reads an OBJECT out of $value for)
//   8  callback option               -> __elephc_curl_easy_set_callback (a PHP callable,
//                                        or null to restore the default)
//   9  PHP stream option             -> handled in this prelude, by installing an internal
//                                        closure in the matching callback slot; libcurl
//                                        never sees the stream
//
// THE 0-vs-6 SPLIT IS php-src's OWN. `_php_curl_setopt` (ext/curl/interface.c) ends its
// switch with `zend_argument_value_error(2, "is not a valid cURL option")`, so an option
// number php-src does not recognize THROWS; an option it recognizes but that fails at the
// libcurl level merely returns `false`. Kind 6 is elephc's honest version of the second
// case: the option is real PHP API surface, this build just cannot carry it (a blob, or a
// callback outside the six implemented ones), so it answers `false` and says why.
// The canonical PHP name of a `CURLOPT_*` string-list option, used only for php-src's
// `The <NAME> option must have an array value` TypeError wording. Falls back to the
// decimal number for any option not in the list.
function __elephc_curl_slist_option_name(int $option): string {
    if ($option === 10023) { return "CURLOPT_HTTPHEADER"; }
    if ($option === 10104) { return "CURLOPT_HTTP200ALIASES"; }
    if ($option === 10243) { return "CURLOPT_CONNECT_TO"; }
    if ($option === 10187) { return "CURLOPT_MAIL_RCPT"; }
    if ($option === 10039) { return "CURLOPT_POSTQUOTE"; }
    if ($option === 10093) { return "CURLOPT_PREQUOTE"; }
    if ($option === 10228) { return "CURLOPT_PROXYHEADER"; }
    if ($option === 10028) { return "CURLOPT_QUOTE"; }
    if ($option === 10203) { return "CURLOPT_RESOLVE"; }
    if ($option === 10070) { return "CURLOPT_TELNETOPTIONS"; }
    return "CURLOPT_" . $option;
}

// The `CURLOPT_*` string options php-src treats as NULLABLE: handed `null`, it forwards a
// NULL pointer to libcurl (a reset), not an empty string. Every other string option
// converts `null` to `""`. The numbers are the frozen `scripts/docs/curl_surface.json`
// values.
function __elephc_curl_nullable_string_option(int $option): bool {
    return $option === 10036   // CURLOPT_CUSTOMREQUEST
        || $option === 10017   // CURLOPT_FTPPORT
        || $option === 10007   // CURLOPT_RANGE
        || $option === 10134   // CURLOPT_FTP_ACCOUNT
        || $option === 10190   // CURLOPT_RTSP_SESSION_ID
        || $option === 10102   // CURLOPT_ACCEPT_ENCODING
        || $option === 10221   // CURLOPT_DNS_INTERFACE
        || $option === 10222   // CURLOPT_DNS_LOCAL_IP4
        || $option === 10223   // CURLOPT_DNS_LOCAL_IP6
        || $option === 10220   // CURLOPT_XOAUTH2_BEARER
        || $option === 10231   // CURLOPT_UNIX_SOCKET_PATH
        || $option === 10279   // CURLOPT_DOH_URL
        || $option === 10300   // CURLOPT_HSTS
        || $option === 10063   // CURLOPT_KRBLEVEL
        || $option === 10005   // CURLOPT_USERPWD
        || $option === 10173   // CURLOPT_USERNAME
        || $option === 10174   // CURLOPT_PASSWORD
        || $option === 10328;  // CURLOPT_SSL_SIGNATURE_ALGORITHMS
}

/// Returns the PHP type name php-src's own `zend_argument_type_error` would spell for a value,
/// used to reproduce `curl_setopt()`'s run-time TypeError text.
function __elephc_curl_given_type(mixed $value): string {
    if (is_string($value)) {
        return "string";
    }
    if (is_int($value)) {
        return "int";
    }
    if (is_float($value)) {
        return "float";
    }
    if (is_bool($value)) {
        return "bool";
    }
    if (is_array($value)) {
        return "array";
    }
    if (is_object($value)) {
        return get_class($value);
    }
    if (is_null($value)) {
        return "null";
    }
    return gettype($value);
}

// OPEN_BASEDIR ENFORCEMENT (php-src `ext/curl/interface.c`). When the directive is in
// effect, php refuses to activate the FILE protocol for a transfer (protocol options and
// the URL setter), and refuses file-path options that point outside the allowed paths.
// The three answer shapes php-src produces, measured against the PHP 8.5 fixtures:
//   - CURLOPT_PROTOCOLS/CURLOPT_REDIR_PROTOCOLS with CURLPROTO_FILE:
//     "CURLPROTO_FILE cannot be activated when an open_basedir is set" + false;
//   - the _STR protocol options with a case-insensitive "file" or "all" substring:
//     "The FILE protocol cannot be activated when an open_basedir is set" + false
//     (php-src checks SUBSTRINGS, so "all" and "files" match alike);
//   - the file-path options with a non-empty out-of-tree value: the core engine's
//     "open_basedir restriction in effect. File(…) is not within the allowed path(s): (…)"
//     warning + false.
function __elephc_curl_open_basedir(): string {
    $value = ini_get("open_basedir");
    return is_string($value) ? $value : "";
}

function __elephc_curl_open_basedir_is_set(): bool {
    return __elephc_curl_open_basedir() !== "";
}

function __elephc_curl_open_basedir_warn(string $message): void {
    trigger_error($message, 512);
}

// php_check_open_basedir's containment rule, reduced to the fixture shapes: the path is
// allowed when it equals the allowed entry or sits inside its directory (the entry plus a
// trailing slash prefixes the path). php splits a list on ":" — the single-entry forms the
// fixtures use skip that loop.
function __elephc_curl_path_within_open_basedir(string $path): bool {
    $allowed = __elephc_curl_open_basedir();
    if ($allowed === "") {
        return true;
    }
    // Judge the FILE THE PATH NAMES, not its text: php-src's `php_check_open_basedir` runs
    // the path through the realpath expansion, so `/srv/app/../private/x` and a symlink
    // inside the allowed tree are judged by what they resolve to.
    $resolved = realpath($path);
    if ($resolved === false) {
        // A file that does not exist yet (a cookie jar) is judged by its parent directory.
        $parent = realpath(dirname($path));
        if ($parent === false) {
            return false;
        }
        $resolved = $parent . "/" . basename($path);
    }
    // Both supported platforms use ':' as PATH_SEPARATOR, and the directive lists its roots
    // separated by it (`PATH_SEPARATOR` is not in this compiler's constant catalog).
    foreach (explode(":", $allowed) as $root) {
        $real = realpath($root);
        if ($real === false) {
            $real = $root;
        }
        if ($real === "/") {
            return true;
        }
        if (strlen($real) > 1 && substr($real, strlen($real) - 1) === "/") {
            $real = substr($real, 0, strlen($real) - 1);
        }
        if ($resolved === $real || str_starts_with($resolved, $real . "/")) {
            return true;
        }
    }
    return false;
}

// php-src's `php_curl_option_url()` strip: every URL set while open_basedir is in effect
// answers libcurl `CURLOPT_PROTOCOLS = CURLPROTO_ALL & ~CURLPROTO_FILE` before anything
// else, so a `file://` URL can never transfer (gh16802). `$raw` is the boxed Mixed handle
// cell every `__elephc_curl_easy_*` entry point takes — the runtime unboxes the resource
// tag itself, so an `int` parameter here would route the wrong representation through.
function __elephc_curl_strip_file_protocol(mixed $raw): void {
    if (!__elephc_curl_open_basedir_is_set()) {
        return;
    }
    __elephc_curl_easy_setopt_long($raw, 181, CURLPROTO_ALL - CURLPROTO_FILE);
}

function curl_setopt(mixed $handle, mixed $option, mixed $value): bool {
    // THE HANDLE IS UNTYPED ON PURPOSE: a callback invoked by the bridge receives `$ch`
    // as an untyped parameter (php-src's own fixtures do this), and inside such a
    // callback the checker sees `mixed` — a typed `CurlHandle` parameter would reject
    // the call at compile time. The negated instanceof guard is what narrows the rest
    // of the body (the same shape curl_errno uses), and its TypeError answers exactly
    // like PHP's own typed-parameter failure.
    if (!$handle instanceof CurlHandle) {
        throw new \TypeError("curl_setopt(): Argument #1 (\$handle) must be of type CurlHandle, " . __elephc_curl_given_type($handle) . " given");
    }
    // PHP declares `int $option` but performs the coercion at run time, so a non-numeric
    // string raises a TypeError when the call runs rather than when the program is compiled.
    // `$option` is `mixed` here so that call reaches this guard. A numeric string (or float)
    // coerces like php's own `zval_get_long`; only the values it cannot convert are refused.
    if (!is_int($option)) {
        if (is_numeric($option)) {
            $option = (int) $option;
        } else {
            throw new \TypeError("curl_setopt(): Argument #2 (\$option) must be of type int, " . __elephc_curl_given_type($option) . " given");
        }
    }
    // Unbox the mixed parameter to a plain integer so every `$option === N` comparison below
    // sees an int, not a boxed cell.
    $option = (int) $option;
    // OPEN_BASEDIR GUARDS, ahead of everything that forwards to libcurl: php-src raises
    // its own warning and answers FAILURE for a protocol option asking for FILE, for the
    // protocol-string forms whose text mentions "file" or "all", and for a file-path
    // option pointing outside the allowed paths (bug61948, bug65646, curl_basic_028,
    // gh16802). An empty path value is not checked — php-src skips it too, which is what
    // makes `CURLOPT_COOKIEFILE ""` the documented "cookie engine off" no-op.
    if (__elephc_curl_open_basedir_is_set()) {
        if (($option === 181 || $option === 182) && ((int) $value & CURLPROTO_FILE) !== 0) {
            __elephc_curl_open_basedir_warn("curl_setopt(): CURLPROTO_FILE cannot be activated when an open_basedir is set");
            return false;
        }
        if (($option === 10318 || $option === 10319) && (is_string($value) || is_int($value) || is_float($value))) {
            $protocols = (string) $value;
            if (stripos($protocols, "file") !== false || stripos($protocols, "all") !== false) {
                __elephc_curl_open_basedir_warn("curl_setopt(): The FILE protocol cannot be activated when an open_basedir is set");
                return false;
            }
        }
        $fileBasedirOptions = [10031, 10082, 10076, 10025, 10118, 10152, 10153, 10183, 10169, 10170];
        if (($option === 10031 || $option === 10082 || $option === 10076 || $option === 10025 || $option === 10118 || $option === 10152 || $option === 10153 || $option === 10183 || $option === 10169 || $option === 10170)
            && is_string($value) && $value !== ""
            && !__elephc_curl_path_within_open_basedir($value)) {
            __elephc_curl_open_basedir_warn("curl_setopt(): open_basedir restriction in effect. File(" . $value . ") is not within the allowed path(s): (" . __elephc_curl_open_basedir() . ")");
            return false;
        }
    }
    $raw = $handle->__elephc_handle;
    $kind = __elephc_curl_option_kind($option);
    if ($kind === 0) {
        throw new \ValueError("curl_setopt(): Argument #2 (\$option) is not a valid cURL option");
    }
    // CURLINFO_HEADER_OUT (2) is a curl_setopt() option despite its name: php-src
    // installs a debug callback to capture the outgoing request header and reads it
    // back with curl_getinfo(). Handled before the kind table, which classifies it as
    // unsupported.
    if ($option === 2) {
        // php-src raises this only when a REAL debug callback is in effect: the fixture
        // `curl_setopt($ch, CURLOPT_DEBUGFUNCTION, null)` — which installs an inert no-op —
        // still allows CURLINFO_HEADER_OUT afterwards, while `CURLOPT_DEBUGFUNCTION,
        // 'strlen'` does not.
        if ($handle->__elephc_debug_installed) {
            throw new \ValueError("CURLINFO_HEADER_OUT option must not be set when the CURLOPT_DEBUGFUNCTION option is set");
        }
        if ((bool) $value) {
            $handle->__elephc_header_out_enabled = true;
            // NOT YET CAPTURED: php-src answers `false` for CURLINFO_HEADER_OUT until a
            // transfer has actually delivered the outgoing header, so the enabling must not
            // materialize an empty string here.
            $handle->__elephc_header_out = null;
            // php-src sets CURLOPT_VERBOSE (41) so libcurl delivers the outgoing header
            // to the debug callback; the callback swallows the rest of the trace.
            __elephc_curl_easy_setopt_long($raw, 41, 1);
            return __elephc_curl_install_internal_callback($handle, 4, function (CurlHandle $ch, int $type, string $data): int {
                if ($type === 2) {
                    $ch->__elephc_header_out = $data;
                }
                return 0;
            });
        }
        $handle->__elephc_header_out_enabled = false;
        $handle->__elephc_header_out = null;
        __elephc_curl_easy_setopt_long($raw, 41, 0);
        return true;
    }
    // A string-list option takes an ARRAY and nothing else, matching php-src's own
    // `zend_type_error(... must be of type array ...)` for the same options. The items
    // travel to the bridge as one NUL-FRAMED blob (`item . "\0"` per element): PHP
    // strings are binary-safe, so `[]` is `""` and `[""]` is a single NUL byte, which a
    // separator-joined encoding could not tell apart.
    if ($kind === 3) {
        if (!is_array($value)) {
            throw new \TypeError("curl_setopt(): The " . __elephc_curl_slist_option_name($option) . " option must have an array value");
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
    // a string. An array posts real `multipart/form-data`, exactly matching php-src's own
    // `build_mime_structure_from_hash` (`ext/curl/interface.c`) — see
    // `__elephc_curl_build_multipart` below for the field-by-field mapping and its one
    // documented divergence.
    if ($option === 10015 && is_array($value)) {
        // AN EMPTY ARRAY IS AN EMPTY STRING BODY, NOT AN EMPTY MULTIPART. php-src
        // special-cases it before building any mime structure ("no need to build the mime
        // structure for an empty array" -> `curl_easy_setopt(cp, CURLOPT_POSTFIELDS, "")`),
        // and it is observable on the wire: measured against a local echo server on PHP
        // 8.4.20, `CURLOPT_POSTFIELDS => []` sends `Content-Type:
        // application/x-www-form-urlencoded` with an empty body — byte for byte what
        // `CURLOPT_POSTFIELDS => ""` sends — while a built-but-empty `curl_mime` would send
        // a `multipart/form-data` content type and a boundary-only body.
        if (count($value) === 0) {
            return __elephc_curl_easy_setopt_str($raw, $option, "");
        }
        return __elephc_curl_build_multipart($raw, $value);
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
            // php-src keeps ONE write mode, so this ALWAYS deselects a previously
            // installed CURLOPT_WRITEFUNCTION — to PHP_CURL_RETURN when true, and to
            // PHP_CURL_STDOUT when false (the callable stays rooted either way). The
            // bridge's own `write_user` flag is cleared by the same forwarded call.
            $handle->__elephc_write_user = false;
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
    // CURLOPT_SHARE (10100) IS THE ONE OBJECT-VALUED curl_setopt() OPTION, handled here
    // before the scalar-type guard below (an object is never int/bool/float/string). Both
    // `CurlShareHandle` and PHP 8.5's `CurlSharePersistentHandle` are accepted, matching
    // php-src's own CURLOPT_SHARE contract. The LATTER is matched by class NAME
    // (`get_class()`) rather than `instanceof`, because `CurlSharePersistentHandle` is
    // declared only for PHP >= 8.5 and THIS function's body is never version-gated, so a literal
    // `instanceof CurlSharePersistentHandle` would fail to resolve the class name at all
    // when compiling for 8.4 — a plain runtime string comparison needs no such
    // declaration.
    //
    // THE LIFETIME ARGUMENT for why attaching a share here never takes a PHP-level
    // reference from `$handle` to `$value` lives in `crates/elephc-curl/src/share.rs`'s
    // module doc and `__elephc_curl_easy_set_share`'s: the BRIDGE, not this prelude, is
    // what keeps a share alive as long as any easy handle still points at it, by
    // detaching every attached easy handle before the share is ever actually freed.
    if ($kind === 7) {
        $isShare = ($value instanceof CurlShareHandle)
            || (is_object($value) && get_class($value) === "CurlSharePersistentHandle");
        if (!$isShare) {
            $given = is_object($value) ? get_class($value) : (is_null($value) ? "null" : gettype($value));
            throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be of type CurlShareHandle, " . $given . " given");
        }
        $shareRaw = $value->__elephc_handle;
        return __elephc_curl_easy_set_share($raw, $shareRaw);
    }
    // ORDER IS LOAD-BEARING: this block must stay AFTER the `$kind === 7` block above.
    // The type checker's flow narrowing of `$value` leaks out of an `if` body, and the
    // `is_callable()`/`is_string()`/`is_null()` tests below re-narrow it hard enough that
    // `CURLOPT_SHARE`'s `$value->__elephc_handle` stops type-checking ("Property access
    // requires an object or typed pointer") when this block is placed first. That is a
    // checker wart rather than a curl one — the two branches are mutually exclusive and
    // the runtime behavior is identical either way — but the ordering is the cheap fix.
    if ($kind === 8) {
        // CALLBACK OPTIONS. The callable is decomposed here, at the PHP layer, into a
        // descriptor pointer plus the shared codegen adapter's address, so no bridge
        // extern ever declares a `callable` parameter — the same split `Pdo\Sqlite`
        // uses for SQLite's user functions. `$handle` itself travels with them because
        // libcurl callbacks receive `$ch` as their first PHP argument and the identity is
        // observable (`$ch === $captured`); the bridge borrows the object as a
        // non-owning back-pointer, exactly like php-src's `ch->self`.
        $slot = 5;
        $optionName = "CURLOPT_XFERINFOFUNCTION";
        if ($option === 20011) {
            $slot = 0;
            $optionName = "CURLOPT_WRITEFUNCTION";
        } elseif ($option === 20079) {
            $slot = 1;
            $optionName = "CURLOPT_HEADERFUNCTION";
        } elseif ($option === 20012) {
            $slot = 2;
            $optionName = "CURLOPT_READFUNCTION";
        } elseif ($option === 20056) {
            $slot = 3;
            $optionName = "CURLOPT_PROGRESSFUNCTION";
        } elseif ($option === 20094) {
            $slot = 4;
            $optionName = "CURLOPT_DEBUGFUNCTION";
        } elseif ($option === 20312) {
            $slot = 6;
            $optionName = "CURLOPT_PREREQFUNCTION";
        }
        // TOUCHING CURLOPT_DEBUGFUNCTION AT ALL — even with `null` — PERMANENTLY SHADOWS
        // CURLOPT_STDERR, because php-src installs its C trampoline here and never takes
        // it back out, leaving libcurl's `data->set.fdebug` non-NULL forever. Set BEFORE
        // the null branch below so both paths record it.
        if ($slot === 4) {
            $handle->__elephc_debug_user = true;
        }
        if (is_null($value)) {
            // php-src restores the option's DEFAULT, which for CURLOPT_WRITEFUNCTION is
            // stdout — NOT whatever CURLOPT_RETURNTRANSFER was set to earlier. Measured
            // on PHP 8.4.20: after `curl_setopt($ch, CURLOPT_WRITEFUNCTION, null)` on a
            // RETURNTRANSFER handle, `curl_exec()` prints the body and returns `true`.
            //
            // THE READ SLOT IS THE EXCEPTION: clearing CURLOPT_READFUNCTION falls back to
            // the CURLOPT_INFILE stream when one is set, rather than to "no source"
            // (measured: READFUNCTION, then INFILE, then READFUNCTION=null uploads the
            // FILE's bytes). `__elephc_curl_sync_read_slot()` picks whichever is now in
            // charge.
            if ($slot === 2) {
                $handle->__elephc_read_user = null;
                return __elephc_curl_sync_read_slot($handle);
            }
            // THE DEBUG SLOT IS NEVER DEREGISTERED ONCE TOUCHED, for the same class of
            // reason the READ slot is never deregistered at all (see `apply_registration`
            // in `crates/elephc-curl/src/callbacks.rs`): clearing the libcurl registration
            // does not restore "nothing", it restores libcurl's OWN default, and here that
            // default is LOUDER than what php does.
            //
            // With `CURLOPT_VERBOSE` on and no `CURLOPT_DEBUGFUNCTION` installed, libcurl's
            // `trc_write` falls back to `data->set.err`, which defaults to the process's
            // stderr (`lib/url.c`). elephc never hands libcurl a `CURLOPT_STDERR` — the
            // option is serviced on this side — so clearing the registration dumps the
            // whole verbose trace onto FD 2. php emits NOTHING there: its C trampoline is
            // still installed and simply has no PHP callable to call. Measured on PHP
            // 8.4.20 (VERBOSE + STDERR + DEBUGFUNCTION=null, both orders): php prints
            // nothing to stdout, to the stream, or to fd 2, while clearing the slot here
            // printed the entire trace to fd 2.
            //
            // A NO-OP closure reproduces php exactly: `set.fdebug` stays non-NULL, so
            // `trc_write` calls it and never reaches `set.err`. Only when the user has
            // TOUCHED the option, though — with `CURLOPT_VERBOSE` alone and
            // `CURLOPT_DEBUGFUNCTION` never set, php leaks to fd 2 too (measured, both
            // agree), and that parity is what `__elephc_debug_user` gates.
            if ($slot === 4) {
                $handle->__elephc_debug_installed = false;
                return __elephc_curl_install_internal_callback($handle, 4, function (CurlHandle $_ch, int $_type, string $_data): int {
                    return 0;
                });
            }
            // PROGRESS RESET IS LIKE THE DEBUG SLOT: php-src's C progress trampoline
            // stays installed after `CURLOPT_PROGRESSFUNCTION`/`CURLOPT_XFERINFOFUNCTION`
            // is set to null, so libcurl keeps calling it (it just has no user callable)
            // and never falls back to its default progress METER. Clearing the
            // registration here would let libcurl write the meter to stderr, which php
            // never does — gh21023 sets both to null with NOPROGRESS=0 and expects no
            // meter.
            if ($slot === 3 || $slot === 5) {
                return __elephc_curl_install_internal_callback($handle, $slot, function (CurlHandle $_ch, int $_dlTotal, int $_dlNow, int $_ulTotal, int $_ulNow): int {
                    return 0;
                });
            }
        // THE ROOT GOES THROUGH A LOCAL. The `$handle` here is the untyped parameter
        // narrowed by the guard above; the checker accepts the code, but the lowering
        // still sees a `mixed` receiver for the INDEXED property write, and the
        // Mixed-receiver path of `$o->prop[$i] = …` currently drops writes to declared
        // slots of non-stdClass classes. Read the array, mutate the local, and store the
        // named property back — the plain named store takes the class-dispatched path.
        $callbacks = $handle->__elephc_callbacks;
        $callbacks[$slot] = null;
        $handle->__elephc_callbacks = $callbacks;
        if ($slot === 0) {
            $handle->__elephc_return_transfer = false;
            $handle->__elephc_write_user = false;
        }
        // The prereq user callable goes with the slot, like the read dispatcher's.
        if ($slot === 6) {
            $handle->__elephc_prereq_user = null;
        }
        return __elephc_curl_easy_set_callback($raw, $slot, 0, $handle, 0);
        }
        // PHP resolves a `[$object, 'method']` callable through `__call` when the class
        // declares it and the named method is absent. The compiled callable-array
        // descriptor only selects DECLARED public methods, so bind the magic dispatch
        // into a closure the descriptor can carry. `is_callable([$receiver, '__call'])`
        // is the runtime "class declares public __call" probe (the method-name table
        // already contains `__call`), and `get_class_methods()` decides whether the named
        // method is really absent — a present method must NOT be routed to `__call`.
        if (is_array($value) && count($value) === 2 && is_object($value[0]) && is_string($value[1])) {
            $receiver = $value[0];
            $method = $value[1];
            if (!in_array($method, get_class_methods($receiver)) && is_callable([$receiver, '__call'])) {
                $value = function () use ($receiver, $method) {
                    return $receiver->__call($method, func_get_args());
                };
            }
        }
        if (!is_callable($value)) {
            if (is_string($value)) {
                throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be a valid callback for option " . $optionName . ", function \"" . $value . "\" not found or invalid function name");
            }
            throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be a valid callback for option " . $optionName . ", no array or string given");
        }
        // php-src's C debug trampoline captures the outgoing request header AND dispatches to
        // the user's callable, so installing a CURLOPT_DEBUGFUNCTION must not displace the
        // CURLINFO_HEADER_OUT capture. Wrapping the callable here is what keeps both working.
        if ($slot === 4) {
            $user = $value;
            $value = function (CurlHandle $ch, int $type, string $data) use ($user) {
                // php-src's debug trampoline records the outgoing request header whenever
                // libcurl delivers it (VERBOSE on), whether or not CURLINFO_HEADER_OUT was
                // the thing that enabled it — the shared-handles fixture reads it back
                // without ever setting that option.
                if ($type === 2) {
                    $ch->__elephc_header_out = $data;
                }
                return call_user_func($user, $ch, $type, $data);
            };
        }
        // php-src's C prereq trampoline validates the RETURN value before libcurl sees it:
        // an int outside {CURL_PREREQFUNC_OK, CURL_PREREQFUNC_ABORT} is a ValueError, any
        // non-int (including null from a callback that returns nothing) is a TypeError,
        // and both carry the same message. Raising inside the wrapper routes through the
        // adapter's firewall: the transfer aborts with CURLE_ABORTED_BY_CALLBACK and the
        // exception re-raises once curl_exec() returns, which is exactly what the fixture
        // observes.
        if ($slot === 6) {
            // The user callable is parked on the handle the same way the read
            // dispatcher's is: the installed wrapper reads it off `$ch`, because a
            // captured local would carry the flow type of the debug wrapper's
            // 3-parameter closure assigned to `$value` above, and the checker would
            // reject the five-argument call_user_func against it.
            $handle->__elephc_prereq_user = $value;
            $value = function (CurlHandle $ch, string $primaryIp, string $localIp, int $primaryPort, int $localPort) {
                $ret = call_user_func($ch->__elephc_prereq_user, $ch, $primaryIp, $localIp, $primaryPort, $localPort);
                if (is_int($ret)) {
                    if ($ret === 0 || $ret === 1) {
                        return $ret;
                    }
                    throw new \ValueError("The CURLOPT_PREREQFUNCTION callback must return either CURL_PREREQFUNC_OK or CURL_PREREQFUNC_ABORT");
                }
                throw new \TypeError("The CURLOPT_PREREQFUNCTION callback must return either CURL_PREREQFUNC_OK or CURL_PREREQFUNC_ABORT");
            };
        }
        $normalized = __elephc_normalize_callable($value);
        // THE READ SLOT NEVER HOLDS THE USER'S CALLABLE DIRECTLY. It is parked on the
        // handle and the slot gets this prelude's dispatcher, so that (a) a later
        // CURLOPT_INFILE cannot displace it — php gives the callback priority in BOTH
        // orders — and (b) the callback still receives the INFILE stream as its `$fd`
        // argument, which is what php-src passes and what the bridge, having no way to
        // marshal a PHP resource, cannot pass on its own.
        if ($slot === 2) {
            $handle->__elephc_read_user = $normalized;
            return __elephc_curl_sync_read_slot($handle);
        }
        $descriptor = __elephc_callable_ptr($normalized);
        $adapter = __elephc_curl_adapter_addr();
        if (!__elephc_curl_easy_set_callback($raw, $slot, $descriptor, $handle, $adapter)) {
            return false;
        }
        // Rooted only AFTER the bridge accepted it, and rooted before this function
        // returns: $normalized is a live local until then, so the descriptor cannot be
        // released between the two statements.
        //
        // LIMITATION — A CALLBACK THAT CAPTURES ITS OWN HANDLE LEAKS THE SESSION.
        // This root closes a reference cycle whenever the installed closure captures the
        // same CurlHandle it is installed on:
        //     CurlHandle -> __elephc_callbacks -> Closure -> captured $ch -> CurlHandle
        // elephc is refcount-only with no cycle collector, so that handle's refcount never
        // reaches zero: the object is never freed, `curl_easy_cleanup()` never runs, and
        // the libcurl session (with its socket and TLS state) lives until the process
        // exits. php-src has the identical cycle and survives it only because Zend has a
        // cycle collector. There is no fix at this layer — a weak root would let the
        // descriptor die while libcurl still holds the pointer. WORKAROUND for programs
        // that create many handles in a loop: do not `use ($ch)` in the callback (the
        // callback already receives the handle as its first argument, which is what it is
        // there for), or capture only what you need by value. Pinned by
        // `curl_callback_capturing_its_own_handle_leaks_the_session` in
        // tests/codegen/curl/callbacks.rs; see docs/php/curl.md.
        // THE ROOT GOES THROUGH A LOCAL, for the same mixed-receiver reason the
        // null-clear branch documents: the indexed property write on the narrowed
        // untyped `$handle` would land on the stdClass-only path and be dropped.
        $callbacks = $handle->__elephc_callbacks;
        $callbacks[$slot] = $normalized;
        $handle->__elephc_callbacks = $callbacks;
        if ($slot === 4) {
            $handle->__elephc_debug_installed = true;
        }
        if ($slot === 0) {
            // php-src keeps ONE write mode: installing CURLOPT_WRITEFUNCTION selects
            // PHP_CURL_USER, which deselects PHP_CURL_RETURN. Measured: with a write
            // callback installed last, `curl_exec()` returns `true` even when
            // CURLOPT_RETURNTRANSFER was set, and the body reaches only the callback.
            $handle->__elephc_return_transfer = false;
            $handle->__elephc_write_user = true;
        }
        return true;
    }
    // THE FOUR PHP STREAM OPTIONS: CURLOPT_FILE (10001), CURLOPT_INFILE/CURLOPT_READDATA
    // (10009), CURLOPT_WRITEHEADER (10029), CURLOPT_STDERR (10037). php-src hands libcurl
    // a `FILE *`; an elephc stream is not one, so all four are implemented HERE by
    // composing the callback slots — an internal closure that `fwrite()`s to (or
    // `fread()`s from) the stream. libcurl never receives a stream pointer.
    //
    // EVERY PRECEDENCE RULE BELOW WAS MEASURED against PHP 8.4.20 + libcurl 8.19.0
    // (transcripts in `.superpowers/sdd/curl-punchlist/wpC-report.md`). They are NOT one
    // rule: the write and header sinks are a single LAST-SET-WINS mode, the read source is
    // a FIXED PRECEDENCE, and the debug sink is a ONE-WAY SHADOW.
    //
    //   WRITE (CURLOPT_FILE) — php-src keeps ONE `handlers.write->method`, so whichever of
    //   FILE / RETURNTRANSFER / WRITEFUNCTION is set LAST wins, and `null` on any of them
    //   falls back to PHP_CURL_STDOUT (never to a previously-selected sibling):
    //     FILE                     -> body to the stream, curl_exec() answers `true`
    //     FILE then RETURNTRANSFER -> capture (FILE deselected)
    //     RETURNTRANSFER then FILE -> stream  (RETURNTRANSFER deselected)
    //     FILE then WRITEFUNCTION  -> callback;  WRITEFUNCTION then FILE -> stream
    //     FILE then RETURNTRANSFER=false, or WRITEFUNCTION=null, or FILE=null -> stdout
    //
    //   HEADER (CURLOPT_WRITEHEADER) — the same last-set-wins pair with
    //   CURLOPT_HEADERFUNCTION, except the DEFAULT is "discard" rather than stdout, so
    //   clearing either one silences headers instead of printing them.
    //
    //   READ (CURLOPT_INFILE) — NOT last-set-wins. A user CURLOPT_READFUNCTION outranks
    //   the stream in BOTH orders (measured: setting INFILE after READFUNCTION does not
    //   displace the callback), and clearing the callback falls BACK to the stream. That
    //   is why the read slot always holds this prelude's dispatcher and the user's
    //   callable is parked in `__elephc_read_user`. The dispatcher is also what lets the
    //   user callback receive the INFILE stream as its `$fd` argument, which is what
    //   php-src passes.
    //
    //   STDERR (CURLOPT_STDERR) — a FALLBACK sink, not a mode. libcurl's own
    //   `trc_write` (pinned 8.21.0, lib/curl_trc.c) writes to `data->set.err` ONLY when
    //   `data->set.fdebug` is NULL, so a debug callback always wins no matter the order.
    //   php-src installs its C trampoline on the first CURLOPT_DEBUGFUNCTION and never
    //   removes it, so once that option is touched — even with `null` — CURLOPT_STDERR
    //   stays shadowed for the handle's life. `__elephc_debug_user` models exactly that.
    if ($kind === 9) {
        // php ACCEPTS `null` for all four, answers `true`, and clears the sink; it is the
        // ONLY non-resource value that is not a TypeError.
        if ($option === 10001) {
            // CURLOPT_FILE -> the write slot.
            if ($value === null) {
                $handle->__elephc_file = null;
                $handle->__elephc_return_transfer = false;
                $handle->__elephc_write_user = false;
                return __elephc_curl_install_internal_callback($handle, 0, null);
            }
            __elephc_curl_check_stream_option($value, true);
            $handle->__elephc_file = $value;
            // Installing slot 0 makes the BRIDGE select PHP_CURL_USER and clear its own
            // return_transfer (see `apply_callback`); these mirror it on the object, which
            // is what decides `curl_exec()`'s return shape and what `curl_copy_handle()`
            // reads to decide whether slot 0 is ACTIVE on the duplicate.
            $handle->__elephc_return_transfer = false;
            $handle->__elephc_write_user = true;
            return __elephc_curl_install_internal_callback($handle, 0, function (CurlHandle $ch, string $data): int {
                $sink = $ch->__elephc_file;
                if (!is_resource($sink)) {
                    // The stream was closed or cleared MID-TRANSFER (php-src detects the
                    // identical state per chunk, and attributes the warning to the active
                    // built-in — `curl_exec()` for an easy transfer). php resets to
                    // PHP_CURL_STDOUT and keeps delivering, so mirror that: warn once
                    // (the `write_user` flag is the FILE-mode marker and goes false with
                    // the reset), then write the rest of the body to stdout.
                    if ($ch->__elephc_write_user) {
                        trigger_error("curl_exec(): CURLOPT_FILE resource has gone away, resetting to default", E_USER_WARNING);
                        $ch->__elephc_file = null;
                        $ch->__elephc_return_transfer = false;
                        $ch->__elephc_write_user = false;
                    }
                    echo $data;
                    return strlen($data);
                }
                $written = fwrite($sink, $data);
                if (!is_int($written)) {
                    return 0;
                }
                return $written;
            });
        }
        if ($option === 10029) {
            // CURLOPT_WRITEHEADER -> the header slot.
            if ($value === null) {
                $handle->__elephc_writeheader = null;
                return __elephc_curl_install_internal_callback($handle, 1, null);
            }
            __elephc_curl_check_stream_option($value, true);
            $handle->__elephc_writeheader = $value;
            return __elephc_curl_install_internal_callback($handle, 1, function (CurlHandle $ch, string $data): int {
                $sink = $ch->__elephc_writeheader;
                if (!is_resource($sink)) {
                    // The stream was closed or cleared mid-transfer: php warns once and
                    // resets the header sink to its default (discard), which is what
                    // clearing the slot does — so the reset UNINSTALLS this closure and
                    // the following chunks never re-enter it.
                    trigger_error("curl_exec(): CURLOPT_WRITEHEADER resource has gone away, resetting to default", E_USER_WARNING);
                    $ch->__elephc_writeheader = null;
                    return __elephc_curl_install_internal_callback($ch, 1, null);
                }
                $written = fwrite($sink, $data);
                if (!is_int($written)) {
                    return 0;
                }
                return $written;
            });
        }
        if ($option === 10009) {
            // CURLOPT_INFILE / CURLOPT_READDATA -> the read slot's dispatcher. php does
            // NOT check this stream for readability (measured: a write-only handle is
            // accepted), so neither does this.
            if ($value !== null) {
                __elephc_curl_check_stream_option($value, false);
            }
            $handle->__elephc_infile = $value;
            return __elephc_curl_sync_read_slot($handle);
        }
        // CURLOPT_STDERR -> the debug slot, but only while no CURLOPT_DEBUGFUNCTION has
        // ever been set on this handle.
        if ($value === null) {
            $handle->__elephc_stderr = null;
            if ($handle->__elephc_debug_user) {
                return true;
            }
            return __elephc_curl_install_internal_callback($handle, 4, null);
        }
        __elephc_curl_check_stream_option($value, true);
        $handle->__elephc_stderr = $value;
        if ($handle->__elephc_debug_user) {
            // Accepted and remembered, but inert — a debug callback owns the sink. php
            // answers `true` here too; it just never routes anything to the stream.
            return true;
        }
        return __elephc_curl_install_internal_callback($handle, 4, function (CurlHandle $ch, int $type, string $data): int {
            $sink = $ch->__elephc_stderr;
            if (!is_resource($sink)) {
                return 0;
            }
            // libcurl's OWN default trace format, reproduced byte for byte from the pinned
            // 8.21.0 `lib/curl_trc.c`:
            //
            //     static const char s_infotype[CURLINFO_END][3] =
            //       { "* ", "< ", "> ", "{ ", "} ", "{ ", "} " };
            //     switch(type) {
            //     case CURLINFO_TEXT: case CURLINFO_HEADER_OUT: case CURLINFO_HEADER_IN:
            //       fwrite(s_infotype[type], 2, 1, data->set.err);
            //       fwrite(ptr, size, 1, data->set.err);
            //       break;
            //     default: /* nada */
            //     }
            //
            // Two details that a per-line prefixer would get wrong: the prefix is written
            // ONCE PER CALLBACK INVOCATION, not once per line — so a multi-line
            // HEADER_OUT block gets "> " only on its first line — and the DATA_IN/
            // DATA_OUT/SSL_DATA_* types are DROPPED ENTIRELY rather than written raw.
            // Verified byte-identical against a real CURLOPT_STDERR transfer on PHP
            // 8.4.20 (only the ephemeral port/date differ).
            $prefix = "";
            if ($type === 0) {
                $prefix = "* ";
            } elseif ($type === 1) {
                $prefix = "< ";
            } elseif ($type === 2) {
                $prefix = "> ";
            } else {
                return 0;
            }
            fwrite($sink, $prefix);
            fwrite($sink, $data);
            // php-src's debug trampoline always answers 0, and libcurl ignores the value.
            return 0;
        });
    }
    // CURLOPT_SSH_HOSTKEYFUNCTION (20316): php installs the callback and answers `true`; it
    // fires only for an SSH transfer. This build cannot invoke it, but accepting the call
    // matches php for every non-SSH transfer (and the fixtures never run SSH), so it answers
    // `true` rather than the unsupported warning a genuinely-unimplemented option gets.
    if ($option === 20316) {
        if (is_null($value)) {
            return true;
        }
        if (!is_callable($value)) {
            if (is_string($value)) {
                throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be a valid callback for option CURLOPT_SSH_HOSTKEYFUNCTION, function \"" . $value . "\" not found or invalid function name");
            }
            throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be a valid callback for option CURLOPT_SSH_HOSTKEYFUNCTION, no array or string given");
        }
        return true;
    }
    // Callback options this build ACCEPTS-AND-VALIDATES but cannot invoke, because the
    // libcurl feature behind them is not carried. php-src raises its callback TypeError for
    // a bad value, so validate the callable here; a valid one still gets the unsupported
    // warning and `false`, since silently accepting it would be an inert `true`.
    if ($option === 20200) {
        $optionName = "CURLOPT_FNMATCH_FUNCTION";
        if (is_null($value)) {
            // php-src resets a callback option to its default on null and reports
            // success. This build never invokes these callbacks, so a reset is an
            // inert no-op that still has to answer `true` like php does.
            return true;
        }
        if (!is_callable($value)) {
            if (is_string($value)) {
                throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be a valid callback for option " . $optionName . ", function \"" . $value . "\" not found or invalid function name");
            }
            throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be a valid callback for option " . $optionName . ", no array or string given");
        }
        __elephc_curl_setopt_unsupported_warning($option);
        return false;
    }
    if ($kind === 6) {
        // REJECTED BEFORE THE VALUE IS TYPE-CHECKED. An option this build cannot carry
        // gets PHP's unsupported-option warning and `false` whatever
        // the value's type is; running the scalar guard first would answer a TypeError
        // for e.g. a closure passed to a still-unimplemented callback option, which is
        // both a worse diagnostic and further from php-src (which accepts those).
        __elephc_curl_setopt_unsupported_warning($option);
        return false;
    }
    // NULL is legal PHP for any option value: a nullable string option resets to its
    // default (libcurl NULL), every other string option becomes `""`, and a long option
    // becomes `0` — all exactly php-src's `zval_get_*` conversions.
    if (is_null($value)) {
        if ($kind === 2) {
            if (__elephc_curl_nullable_string_option($option)) {
                return __elephc_curl_easy_setopt_null($raw, $option);
            }
            return __elephc_curl_easy_setopt_str($raw, $option, "");
        }
        if ($kind === 1 || $kind === 4) {
            return __elephc_curl_easy_setopt_long($raw, $option, 0);
        }
    }
    // An OBJECT handed to a STRING option is php's own conversion failure, not a
    // parameter-type error: php raises the catchable
    // `Error: Object of class X could not be converted to string` while it coerces the
    // value for libcurl (measured: CURLOPT_URL and CURLOPT_POSTFIELDS both answer that
    // Error). The trace then names this call site, which is what bug79741 compares.
    if ($kind === 2 && is_object($value)) {
        throw new \Error("Object of class " . get_class($value) . " could not be converted to string");
    }
    if (!is_int($value) && !is_bool($value) && !is_float($value) && !is_string($value)) {
        $given = is_array($value) ? "array" : (is_object($value) ? get_class($value) : (is_null($value) ? "null" : gettype($value)));
        throw new \TypeError("curl_setopt(): Argument #3 (\$value) must be of type string|int|float|bool, " . $given . " given");
    }
    if ($kind === 2) {
        // php-src's `php_curl_option_str()` raises a ValueError for an embedded NUL
        // before libcurl ever sees the value; the bridge cannot build a C string from
        // it, so refuse here with PHP's own wording rather than truncating. POSTFIELDS
        // is the exception: PHP hands its bytes to `CURLOPT_COPYPOSTFIELDS` with an
        // explicit length, so a binary body may contain NUL.
        if ($option !== 10015 && is_string($value) && str_contains($value, "\0")) {
            throw new \ValueError("curl_setopt(): cURL option must not contain any null bytes");
        }
        $set = __elephc_curl_easy_setopt_str($raw, $option, (string) $value);
        if ($option === 10002 && $set) {
            // php-src's `php_curl_option_url()`: every URL set while open_basedir is in
            // effect silently re-strips FILE from libcurl's transfer protocols.
            __elephc_curl_strip_file_protocol($raw);
        }
        return $set;
    }
    if ($kind === 1 || $kind === 4) {
        // php-src never forwards 1 to libcurl for CURLOPT_SSL_VERIFYHOST: it warns and
        // substitutes 2 (`interface.c`, `case CURLOPT_SSL_VERIFYHOST`).
        if ($option === 81 && (int) $value === 1) {
            trigger_error("curl_setopt(): CURLOPT_SSL_VERIFYHOST no longer accepts the value 1, value 2 will be used instead", E_USER_NOTICE);
            return __elephc_curl_easy_setopt_long($raw, $option, 2);
        }
        return __elephc_curl_easy_setopt_long($raw, $option, (int) $value);
    }
    __elephc_curl_setopt_unsupported_warning($option);
    return false;
}

// Validates the value of one of the four PHP-stream `curl_setopt()` options and hands it
// back, or throws the error php-src throws.
//
// DIVERGENCE FROM PHP, and it is elephc's stream layer rather than curl's: php answers a
// DIFFERENT TypeError for a CLOSED stream ("curl_setopt(): supplied resource is not a
// valid File-Handle resource") than for a value that was never a resource ("supplied
// argument …"). elephc's `is_resource()` still answers `true` for a stream that has been
// `fclose()`d — closed-ness is not tracked on the value — so this build cannot tell the
// two apart and gives the "supplied argument" message for both. The one that matters,
// rejecting a value that is not a resource at all, is exact. Recorded in
// `docs/php/curl.md`.
//
// The guard is also `is_resource()` rather than "is a STREAM resource", which php checks
// (`php_stream_from_zval_no_verify`). The gap is narrow rather than theoretical-only:
// PHP 8 has promoted almost every remaining resource type to an object (curl, sockets,
// GD, FTP), and the ones left — including `opendir()` handles — report
// `get_resource_type() === "stream"` and so pass php's check too (measured: php answers
// the WRITABILITY `ValueError` for an `opendir()` handle, not a type error). elephc has
// no narrower predicate to use here, so a hypothetical non-stream resource would be
// accepted where php would reject it.
//
// The WRITABILITY check is php's own, and only the three WRITE sinks get it:
// `curl_setopt($ch, CURLOPT_FILE, fopen($f, "rb"))` is a `ValueError` in php, while
// `CURLOPT_INFILE` accepts even a write-only handle (both measured on PHP 8.4.20).
// IT VALIDATES AND RETURNS NOTHING, and the caller then assigns `$value` ITSELF. Handing
// the stream back out of here — `$handle->__elephc_file = __elephc_curl_check_stream(...)`
// — miscompiles: a `mixed`-declared function that returns a value `is_resource()` has
// narrowed to a resource produces a BORROWED return, and storing that into a property
// releases a reference the caller still owns. The second
// `curl_setopt($ch, CURLOPT_FILE, $s)` on the same stream then leaves the CALLER's `$s`
// dangling (observed as `gettype($s) === "integer"`/`"NULL"`). Reduced to a curl-free
// repro and recorded in `.superpowers/sdd/curl-punchlist/wpC-report.md`; it is a codegen
// ownership bug, not a curl one, so this prelude routes around it rather than papering
// over it. Assigning the parameter DIRECTLY at the call site is the shape that is correct.
function __elephc_curl_check_stream_option(mixed $value, bool $mustBeWritable): void {
    if (!is_resource($value)) {
        throw new \TypeError("curl_setopt(): supplied argument is not a valid File-Handle resource");
    }
    if ($mustBeWritable) {
        // elephc NORMALIZES the reported mode to one of "r", "r+", "w" (measured: "w+b"
        // reports "r+", "ab" reports "w"), where php echoes the mode string verbatim.
        // Testing for a "+" or a non-"r" first character is therefore correct on both:
        // php's full set (w/a/x/c and every "+" form) and elephc's normalized three.
        $mode = "";
        $meta = stream_get_meta_data($value);
        if (array_key_exists("mode", $meta)) {
            $mode = (string) $meta["mode"];
        }
        if ($mode !== "" && !str_contains($mode, "+") && substr($mode, 0, 1) === "r") {
            throw new \ValueError("curl_setopt(): The provided file handle must be writable");
        }
    }
}

// Resets any of the four PHP-stream options whose resource was CLOSED after it was set,
// warning exactly as php-src does. php detects this when the transfer is set up — at
// `curl_exec()` for an easy handle and at `curl_multi_add_handle()` for a multi one — so
// `$caller` names the PHP-visible function the warning is attributed to, matching php's
// message. Only a stream that has gone away is reset; an open stream is left alone.
function __elephc_curl_reset_gone_away(CurlHandle $handle, string $caller): void {
    if ($handle->__elephc_stderr !== null && !is_resource($handle->__elephc_stderr)) {
        trigger_error($caller . "(): CURLOPT_STDERR resource has gone away, resetting to stderr", E_USER_WARNING);
        $handle->__elephc_stderr = null;
        if (!$handle->__elephc_debug_user) {
            __elephc_curl_install_internal_callback($handle, 4, null);
        }
    }
    if ($handle->__elephc_file !== null && !is_resource($handle->__elephc_file)) {
        trigger_error($caller . "(): CURLOPT_FILE resource has gone away, resetting to default", E_USER_WARNING);
        $handle->__elephc_file = null;
        $handle->__elephc_return_transfer = false;
        $handle->__elephc_write_user = false;
        __elephc_curl_install_internal_callback($handle, 0, null);
    }
    if ($handle->__elephc_writeheader !== null && !is_resource($handle->__elephc_writeheader)) {
        trigger_error($caller . "(): CURLOPT_WRITEHEADER resource has gone away, resetting to default", E_USER_WARNING);
        $handle->__elephc_writeheader = null;
        __elephc_curl_install_internal_callback($handle, 1, null);
    }
    if ($handle->__elephc_infile !== null && !is_resource($handle->__elephc_infile)) {
        trigger_error($caller . "(): CURLOPT_INFILE resource has gone away, resetting to default", E_USER_WARNING);
        $handle->__elephc_infile = null;
        __elephc_curl_sync_read_slot($handle);
    }
}

// Installs (or, with a `null` closure, clears) one of THIS PRELUDE'S OWN closures in a
// callback slot. Identical plumbing to `curl_setopt()`'s `$kind === 8` branch — normalize,
// take the descriptor, register, then root — but for a closure the user never wrote and
// cannot see.
//
// The internal closures take NO `use` captures: each reads the stream it needs off `$ch`,
// the handle every curl callback receives as its first argument. That is what makes them
// safe to re-register verbatim on a `curl_copy_handle()` duplicate (the bridge re-points
// the object back-pointer, so the closure follows the COPY's streams, not the original's)
// and what keeps them out of the refcount cycle a `use ($ch)` capture would create.
function __elephc_curl_install_internal_callback(CurlHandle $handle, int $slot, mixed $closure): bool {
    $raw = $handle->__elephc_handle;
    if ($closure === null) {
        $handle->__elephc_callbacks[$slot] = null;
        return __elephc_curl_easy_set_callback($raw, $slot, 0, $handle, 0);
    }
    $normalized = __elephc_normalize_callable($closure);
    $descriptor = __elephc_callable_ptr($normalized);
    $adapter = __elephc_curl_adapter_addr();
    if (!__elephc_curl_easy_set_callback($raw, $slot, $descriptor, $handle, $adapter)) {
        return false;
    }
    $handle->__elephc_callbacks[$slot] = $normalized;
    return true;
}

// Re-derives the read slot from the two things that can drive it, after either has
// changed: the user's `CURLOPT_READFUNCTION` (`__elephc_read_user`) and the
// `CURLOPT_INFILE` stream (`__elephc_infile`).
//
// The slot holds a DISPATCHER, never the user's callable, because php's read path is a
// FIXED PRECEDENCE rather than the last-set-wins the write path uses: a callback outranks
// the stream in both orders, and clearing the callback falls back to the stream. Resolving
// that inside the dispatcher — at call time, off `$ch` — means neither `curl_setopt()`
// order nor `curl_copy_handle()` has to re-derive anything.
//
// With NEITHER set the slot is cleared, which returns the handle to the bridge's default
// end-of-data read behaviour.
function __elephc_curl_sync_read_slot(CurlHandle $handle): bool {
    if ($handle->__elephc_read_user === null && $handle->__elephc_infile === null) {
        return __elephc_curl_install_internal_callback($handle, 2, null);
    }
    // The trampoline's own `$fd` argument is IGNORED (and named with a leading underscore
    // so it does not read as an oversight): the bridge has no way to marshal a PHP
    // resource, so it always passes null there. The stream a user callback should see is
    // the CURLOPT_INFILE one, which this dispatcher reads off `$ch` and substitutes.
    return __elephc_curl_install_internal_callback($handle, 2, function (CurlHandle $ch, mixed $_fd, int $length): string|int {
        $user = $ch->__elephc_read_user;
        $source = $ch->__elephc_infile;
        if ($user !== null) {
            // php-src passes the CURLOPT_INFILE stream (or null) as the second argument,
            // NOT the `$fd` libcurl handed the trampoline.
            $produced = call_user_func($user, $ch, $source, $length);
            if (is_int($produced)) {
                // A PHP read callback may return an int — a byte count, CURL_READFUNC_PAUSE,
                // or CURL_READFUNC_ABORT — which the bridge forwards to libcurl unchanged
                // (a paused upload resumes on the next curl_pause()).
                return $produced;
            }
            if (!is_string($produced)) {
                // php-src treats any other non-string return as end-of-data.
                return "";
            }
            // A longer-than-requested string is TRUNCATED by the bridge's `out_cap`, which
            // is php-src's own `MIN(size * nmemb, len)` behaviour.
            return $produced;
        }
        if (!is_resource($source)) {
            return "";
        }
        $chunk = fread($source, $length);
        if (!is_string($chunk)) {
            return "";
        }
        return $chunk;
    });
}

// `CURLFile` / `CURLStringFile`: pure PHP data classes — neither wraps a native
// handle, so neither needs `$__elephc_handle`, a private constructor, or a factory the way
// every OTHER curl class in this file does. Property names, defaults and constructor
// signatures are php-src's own (`ext/curl/curl_file.stub.php`): `CURLFile`'s constructor
// order is `(filename, mimeType, postFilename)`; `CURLStringFile`'s is
// `(data, postname, mime)` — DIFFERENT ARGUMENT ORDER, deliberately not harmonized, because
// that is what php-src itself declares.
//
// NEITHER CLASS IS `final`, AND `CURLStringFile` DOES NOT EXTEND `CURLFile` — both verified
// against a real PHP 8.4.20/libcurl `ext/curl` (`is_subclass_of('CURLStringFile',
// 'CURLFile')` is `false`; subclassing `CURLFile` from userland succeeds) rather than
// assumed from the property-name similarity.
class CURLFile {
    public string $name = "";
    public string $mime = "";
    public string $postname = "";

    // `?string $mimeType`/`?string $postFilename` COLLAPSE `null` TO `""` HERE, not left
    // nullable on the properties: php-src declares both properties plain `string` (never
    // `?string`) and reads back `""` from `getMimeType()`/`getPostFilename()` when the
    // constructor argument was omitted or explicitly `null` — verified directly against a
    // real `ext/curl`.
    public function __construct(string $filename, ?string $mimeType = null, ?string $postFilename = null) {
        $this->name = $filename;
        $this->mime = $mimeType ?? "";
        $this->postname = $postFilename ?? "";
    }

    public function getFilename(): string {
        return $this->name;
    }

    public function getMimeType(): string {
        return $this->mime;
    }

    public function getPostFilename(): string {
        return $this->postname;
    }

    public function setMimeType(string $mime): void {
        $this->mime = $mime;
    }

    public function setPostFilename(string $postname): void {
        $this->postname = $postname;
    }
}

// `curl_file_create()` is a plain alias of `CURLFile::__construct()`, matching php-src's
// own `ext/curl/curl_file.stub.php` (both take the same three arguments in the same order).
function curl_file_create(string $filename, ?string $mime_type = null, ?string $posted_filename = null): CURLFile {
    return new CURLFile($filename, $mime_type, $posted_filename);
}

// `CURLStringFile`'s `$mime` DEFAULTS TO `"application/octet-stream"`, NOT `""` — the one
// property default that differs from `CURLFile`'s all-empty defaults, and it is why
// `__elephc_curl_build_multipart()` below always sets a `Content-Type` for a
// `CURLStringFile` part but only conditionally for a `CURLFile` one. `$postname` has NO
// default (php-src's constructor requires it) and no getter/setter pair — php-src gives
// `CURLStringFile` only a constructor and its three public properties, none of
// `CURLFile`'s six methods.
class CURLStringFile {
    public string $data = "";
    public string $postname = "";
    public string $mime = "application/octet-stream";

    public function __construct(string $data, string $postname, string $mime = "application/octet-stream") {
        $this->data = $data;
        $this->postname = $postname;
        $this->mime = $mime;
    }
}

// `CURLOPT_POSTFIELDS`'s ARRAY form as REAL `multipart/form-data`, field for field matching
// php-src's `build_mime_structure_from_hash` (`ext/curl/interface.c`):
//
//   scalar          -> a plain form field: NAME + binary-safe DATA (`curl_mime_data`,
//                       never NUL-terminated, so an embedded NUL in the value survives)
//   CURLFile        -> a FILE part read from disk at transfer time: NAME = the array key,
//                       the part's data source = `$file->name` (`curl_mime_filedata`),
//                       TYPE = `$file->mime`, OR THE LITERAL `"application/octet-stream"`
//                       WHEN IT IS EMPTY — ALWAYS SET, NEVER SKIPPED. An earlier version of
//                       this comment (and the code) claimed libcurl's own default for an
//                       unset file-part type was `application/octet-stream` and skipped the
//                       call when `$file->mime` was empty; that was WRONG and has been
//                       corrected after review against the pinned libcurl 8.21.0 source:
//                       `Curl_mime_prepare_headers` (`lib/mime.c`) SNIFFS an unset type from
//                       the part's POSTED filename through a small extension table (`.gif`
//                       `.jpg` `.jpeg` `.png` `.svg` `.txt` `.htm` `.html` `.pdf` `.xml`) and
//                       only falls back to `application/octet-stream` when the extension is
//                       not in that table — so `curl_file_create('/tmp/avatar.png')` with no
//                       mime would have sent `image/png`, not `application/octet-stream`,
//                       and the sniff keys off the POSTED name (`FIELD_FILENAME`), so even
//                       changing only the postname would have changed the header. php-src
//                       itself always calls `curl_mime_type()` with an explicit value —
//                       `$file->mime` when set, the literal `"application/octet-stream"`
//                       otherwise — which is exactly what this walker now does too, closing
//                       the sniffing hole entirely rather than relying on libcurl's guess.
//                       FILENAME =
//                       `$file->postname` UNLESS IT IS EMPTY, in which case FILENAME IS
//                       `$file->name` VERBATIM — the full path as given, NOT its
//                       `basename()`. This looks like a mistake, and PHP users have
//                       reported it as one (the local filesystem path leaks into the
//                       posted filename), but it is what a real `ext/curl` sends on the
//                       wire, measured directly rather than assumed from the docs.
//   CURLStringFile  -> an IN-MEMORY file part: NAME = the array key, DATA = `$file->data`
//                       (binary-safe), FILENAME = `$file->postname` (always — the
//                       constructor requires it), TYPE = `$file->mime` (always — the
//                       constructor defaults it to `"application/octet-stream"`).
//   scalar (again)  -> everything that is not a `CURLFile`/`CURLStringFile`/array/object
//                       goes through `(string) $value`, matching php-src's own fallback
//                       (`zval_get_tmp_string`) for a plain scalar.
//
// DIVERGENCE FROM PHP: ANY OTHER OBJECT IS REFUSED, LOUDLY, rather than string-cast.
// php-src's own fallback for a non-`CURLFile` object is `zval_get_tmp_string`, which posts
// a `Stringable` object's string value as an ordinary field and raises a catchable
// `\Error: Object of class … could not be converted to string` for one with no
// `__toString()` — measured directly against a real `ext/curl`. elephc's OWN `(string)`
// cast for an object with no matching `__toString()` is NOT a catchable error, though: it
// is a hard, UNCATCHABLE process exit (`src/codegen/lower_inst/conversions.rs`'s
// `emit_missing_tostring_fatal`/`emit_mixed_missing_tostring_fatal`), a pre-existing
// limitation of that general cast, not something curl introduces. Relying on it here would
// mean a bad `CURLOPT_POSTFIELDS` array value KILLS THE PROCESS instead of raising an
// exception the caller can catch — clearly worse than php-src's own answer for the same
// input. This function therefore refuses ANY object that is not a `CURLFile`/
// `CURLStringFile` explicitly, with a catchable `\TypeError`, before ever reaching a
// `(string)` cast. The one thing this gives up versus real PHP is a `Stringable` object
// being accepted as a plain field; every other object shape (the vast majority, since
// `Stringable` `CURLOPT_POSTFIELDS` values are not a documented, common pattern) ends up
// with the SAME kind of loud, honest rejection php-src gives it, just a different
// exception class and message.
//
// A NESTED ARRAY VALUE FLATTENS ONE LEVEL, matching php-src's own
// `build_mime_structure_from_hash` — this is the STANDARD repeated-field idiom, deliberate
// on php-src's part, not an accident this build should refuse. Measured directly against a
// real `ext/curl` (not assumed from the brief, which guessed "REJECTS" — it does not):
// `['a' => ['x' => '1', 'y' => '2']]` sends TWO separate parts, both named `"a"`, one per
// INNER array element's value (`1`, then `2`) — the outer key repeated as every inner
// element's field name, the inner KEYS discarded entirely (`x`/`y` never appear on the
// wire). `__elephc_curl_build_multipart()` reproduces exactly this.
//
// DIVERGENCE FROM PHP, the one part of this shape still refused: going one level deeper
// (`['a' => [['q' => 'deep']]]`) stops php-src's own recursion and instead raises its
// ordinary `Warning: Array to string conversion`, posting the literal string `"Array"` for
// that inner element. Reproducing that exact warn-and-mangle shape — a surprising corner
// of php-src's own implementation, not documented PHP behavior anyone is likely to rely on
// — was judged not worth the complexity; an inner element that is itself an array or
// object gets a clear, loud `\TypeError` instead of a silently mangled request.
//
// `$raw` AND `$fields` ARE DECLARED `mixed`, ONLY BECAUSE OF THE CHECKER: the caller reaches
// this from inside `curl_setopt()`'s `mixed $value`/local `$raw`, and elephc's checker does
// not narrow a `mixed` to a more specific type after an `is_array()`/property-read guard, so
// tighter parameter types would make the call a COMPILE ERROR. The `is_array()` guard is
// still enforced at the only call site, in `curl_setopt()` above.
//
// READING `$value->name`/`->mime`/`->postname`/`->data` ON A `mixed` LOCAL NARROWED BY
// `instanceof`, NEVER PASSING `$value` ITSELF TO A TYPED PARAMETER: this is the same idiom
// `curl_multi_info_read()`'s `CurlMultiHandle::__elephc_lookup()` result requires (see this
// file's header and `docs/php/compatibility.md`) — property reads on a Mixed-sourced object
// work; it is passing such an object to a TYPED parameter that miscompiles, which this
// walker never does.
//
// EVERY FAILURE PATH CALLS `__elephc_curl_mime_abort()` BEFORE RETURNING/THROWING, so a walk
// that dies partway (an unsupported shape, an embedded NUL in a name/path/type/filename, a
// file libcurl itself refuses) never leaves the half-built structure this crate owns
// dangling, and never disturbs whatever mime is already ATTACHED from an earlier,
// successful `curl_setopt()` call on the same handle — see
// `crates/elephc-curl/src/mime.rs`'s module doc for the pending/attached split this mirrors.
//
// `__elephc_curl_mime_part_field()`'s SECOND ARGUMENT is the field-kind code
// `crates/elephc-curl/src/mime.rs`'s `FIELD_*` constants define: `0` NAME, `1` DATA
// (binary-safe), `2` FILEDATA (a local file path), `3` TYPE (MIME type), `4` FILENAME
// (posted/remote filename) — literal numbers below, matching every other option/kind code
// this whole prelude already spells out as a literal rather than importing a constant.
// Reads the content a `CURLFile` filename names. A `data://` URI is decoded HERE: elephc's
// dynamic `file_get_contents()` does not route `data://` to a decoder (only a LITERAL
// `file_get_contents("data://…")` is decoded at compile time), and a `CURLFile`'s filename is
// a runtime string. Every other URL or path goes through the ordinary stream reader.
function __elephc_curl_read_file_url(string $path): string|false {
    if (!str_starts_with($path, "data://")) {
        return file_get_contents($path);
    }
    $rest = substr($path, 7);
    $comma = strpos($rest, ",");
    if ($comma === false) {
        return false;
    }
    $meta = substr($rest, 0, $comma);
    $payload = substr($rest, $comma + 1);
    if (str_ends_with(strtolower($meta), ";base64")) {
        return base64_decode($payload, false);
    }
    return urldecode($payload);
}

function __elephc_curl_build_multipart(mixed $raw, mixed $fields): bool {
    if (!__elephc_curl_mime_new($raw)) {
        return false;
    }
    foreach ($fields as $name => $value) {
        // `(string) $x . ""`, NOT `(string) $x`, AND BOUND TO A LOCAL BEFORE EVERY CALL
        // BELOW. A bare `(string) $mixed` cast handed straight to a function argument
        // leaks its temporary (measured with `--gc-stats`: one block per cast, unbounded
        // across a loop); routing it through a concatenation first produces an ordinary
        // owned string local the caller releases normally. Same family of pre-existing
        // codegen leaks as the "bind the property to a local" rule this module's header
        // documents.
        $nameRaw = (string) $name . "";
        // A NESTED ARRAY VALUE FLATTENS ONE LEVEL, matching php-src's own
        // `build_mime_structure_from_hash` exactly (measured, documented in full above this
        // function): one part PER INNER ELEMENT, every one of them named with the SAME
        // outer key — the standard repeated-field-name idiom (`<input name="tags[]">`'s
        // wire shape, without the `[]` suffix). This runs BEFORE `__elephc_curl_mime_
        // add_part()` for the outer item, deliberately: an array value needs zero, one, or
        // many parts, never the single unconditional one every other value shape gets.
        //
        // DIVERGENCE FROM PHP, the one this function's header still documents: an INNER
        // element that is itself an array or object is refused with a `\TypeError` rather
        // than reproducing php-src's second-level behavior (an ordinary `Warning: Array to
        // string conversion` and the literal string `"Array"` for a nested array; an
        // object goes through the SAME refusal this walker already gives one at the outer
        // level, for the identical uncatchable-cast safety reason).
        if (is_array($value)) {
            foreach ($value as $inner) {
                if (is_array($inner) || is_object($inner)) {
                    __elephc_curl_mime_abort($raw);
                    throw new \TypeError("curl_setopt(): CURLOPT_POSTFIELDS nested array value must contain only scalars");
                }
                if (!__elephc_curl_mime_add_part($raw)) {
                    __elephc_curl_mime_abort($raw);
                    return false;
                }
                if (!__elephc_curl_mime_part_field($raw, 0, $nameRaw)) {
                    __elephc_curl_mime_abort($raw);
                    return false;
                }
                $innerRaw = (string) $inner . "";
                if (!__elephc_curl_mime_part_field($raw, 1, $innerRaw)) {
                    __elephc_curl_mime_abort($raw);
                    return false;
                }
            }
            continue;
        }
        if (!__elephc_curl_mime_add_part($raw)) {
            __elephc_curl_mime_abort($raw);
            return false;
        }
        if (!__elephc_curl_mime_part_field($raw, 0, $nameRaw)) {
            __elephc_curl_mime_abort($raw);
            return false;
        }
        if ($value instanceof CURLFile) {
            $path = (string) $value->name . "";
            // A STREAM URL (`data://`, `php://`, …) is read through the PHP stream layer,
            // not libcurl's own file opener: php's curl_file path does the same, so the
            // part carries the stream's CONTENT rather than a filename libcurl opens.
            if (str_contains($path, "://")) {
                $data = __elephc_curl_read_file_url($path);
                if (!is_string($data)) {
                    __elephc_curl_mime_abort($raw);
                    return false;
                }
                if (!__elephc_curl_mime_part_field($raw, 1, $data)) {
                    __elephc_curl_mime_abort($raw);
                    return false;
                }
            } elseif (!__elephc_curl_mime_part_field($raw, 2, $path)) {
                __elephc_curl_mime_abort($raw);
                return false;
            }
            // ALWAYS SET, NEVER SKIPPED — see this function's header for why: libcurl
            // sniffs an unset file-part type from the POSTED filename's extension, so
            // skipping this call is not "no type", it is "whatever libcurl guesses from
            // the name". `"application/octet-stream"` is php-src's own literal default.
            $mime = (string) $value->mime . "";
            $mimeType = $mime !== "" ? $mime : "application/octet-stream";
            if (!__elephc_curl_mime_part_field($raw, 3, $mimeType)) {
                __elephc_curl_mime_abort($raw);
                return false;
            }
            // NO `basename()` HERE — see this function's header for why the full path is
            // the measured, correct fallback.
            $postname = (string) $value->postname . "";
            $filename = $postname !== "" ? $postname : $path;
            if (!__elephc_curl_mime_part_field($raw, 4, $filename)) {
                __elephc_curl_mime_abort($raw);
                return false;
            }
            continue;
        }
        if ($value instanceof CURLStringFile) {
            $data = (string) $value->data . "";
            if (!__elephc_curl_mime_part_field($raw, 1, $data)) {
                __elephc_curl_mime_abort($raw);
                return false;
            }
            $postname = (string) $value->postname . "";
            if (!__elephc_curl_mime_part_field($raw, 4, $postname)) {
                __elephc_curl_mime_abort($raw);
                return false;
            }
            $mime = (string) $value->mime . "";
            if (!__elephc_curl_mime_part_field($raw, 3, $mime)) {
                __elephc_curl_mime_abort($raw);
                return false;
            }
            continue;
        }
        // `is_array($value)` NEEDS NO CHECK HERE: every array shape was already handled
        // (and `continue`d past) above, before `__elephc_curl_mime_add_part()` ran for
        // this iteration — this point is only ever reached for a scalar or an object.
        //
        // DIVERGENCE FROM PHP, documented in full above this function: ANY object here is
        // refused BEFORE the `(string)` cast below ever runs, precisely so that cast never
        // has a chance to reach elephc's own uncatchable object-to-string fatal.
        if (is_object($value)) {
            __elephc_curl_mime_abort($raw);
            throw new \TypeError("curl_setopt(): CURLOPT_POSTFIELDS array value must be of type string|int|float|bool|CURLFile|CURLStringFile, " . get_class($value) . " given");
        }
        $valueRaw = (string) $value . "";
        if (!__elephc_curl_mime_part_field($raw, 1, $valueRaw)) {
            __elephc_curl_mime_abort($raw);
            return false;
        }
    }
    return __elephc_curl_mime_post($raw);
}

function curl_setopt_array(CurlHandle $handle, array $options): bool {
    foreach ($options as $option => $value) {
        try {
            if (!curl_setopt($handle, (int) $option, $value)) {
                return false;
            }
        } catch (\TypeError $e) {
            // php-src runs the same option validation under the curl_setopt_array()
            // frame, so its message names that function and argument #2.
            throw new \TypeError(str_replace("curl_setopt(): Argument #3 (\$value)", "curl_setopt_array(): Argument #2 (\$options)", $e->getMessage()));
        } catch (\ValueError $e) {
            throw new \ValueError(str_replace("curl_setopt(): Argument #3 (\$value)", "curl_setopt_array(): Argument #2 (\$options)", $e->getMessage()));
        }
    }
    return true;
}

function curl_exec(CurlHandle $handle): string|bool {
    __elephc_curl_reset_gone_away($handle, "curl_exec");
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

// php-src declares `CurlHandle $handle`; the parameter is widened to `mixed` with a
// runtime guard because a handle read out of `curl_multi_info_read()`'s result array is
// typed `mixed` by this checker, exactly like `curl_multi_add_handle()`.
function curl_errno(mixed $handle): int {
    if (!$handle instanceof CurlHandle) {
        $given = is_object($handle) ? get_class($handle) : (is_null($handle) ? "null" : gettype($handle));
        throw new \TypeError("curl_errno(): Argument #1 (\$handle) must be of type CurlHandle, " . $given . " given");
    }
    $raw = $handle->__elephc_handle;
    return __elephc_curl_easy_errno($raw);
}

// The message is BOUND TO A LOCAL BEFORE IT IS RETURNED. Returning a builtin's freshly
// allocated string straight out of a prelude function leaks it — measured with
// `--gc-stats`: `$m = curl_error($ch)` in a loop grew the heap by one block per call while
// the same builtin called directly stayed balanced. Same family as the "bind the property
// to a local" rule this module's header documents.
// See `curl_errno()` above for why the parameter is `mixed` with a runtime guard.
function curl_error(mixed $handle): string {
    if (!$handle instanceof CurlHandle) {
        $given = is_object($handle) ? get_class($handle) : (is_null($handle) ? "null" : gettype($handle));
        throw new \TypeError("curl_error(): Argument #1 (\$handle) must be of type CurlHandle, " . $given . " given");
    }
    $raw = $handle->__elephc_handle;
    $message = __elephc_curl_easy_error($raw);
    return $message;
}

// `$handle` is unused because the function is a no-op, and it CANNOT be renamed to the
// `_handle` form the checker ignores: the name is PHP-VISIBLE, since `curl_close(handle:
// $ch)` is legal PHP 8 named-argument syntax and `src/types/signatures.rs` requires
// parameter names to stay coherent with php-src. This used to print a bogus
// `Unused variable: $handle` on every compile of every program that called it, pointing
// at an invisible prelude line. The fix is in the checker, not here:
// `src/types/warnings/scope_usage.rs` now exempts EMPTY-BODIED functions entirely, since
// a body with no statements cannot read anything and the parameter list of a deliberate
// no-op is public contract. `curl_multi_close()` below is the same shape and covered by
// the same exemption; `curl_close()` and `curl_share_close()` add only the PHP 8.5
// deprecation read, which keeps their parameters used. Pinned in
// `tests/error_tests/warnings.rs::test_warning_empty_body_params_not_flagged_as_unused`.
function curl_close(CurlHandle $handle): void {
    // -- elephc PHP >= 8.5 curl_close deprecation begin --
    // PHP 8.5 marks curl_close() deprecated via `#[\Deprecated(since: '8.5',
    // message: "as it has no effect since PHP 8.0")]` (curl.stub.php). The read of
    // $handle keeps the parameter "used" so the unused-variable warning exemption
    // for empty bodies is not needed; the branch itself is never taken.
    if ($handle->__elephc_handle !== null) {
        trigger_error("Function curl_close() is deprecated since 8.5, as it has no effect since PHP 8.0", E_USER_DEPRECATED);
    }
    // -- elephc PHP >= 8.5 curl_close deprecation end --
}

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
        // php-src adds `request_header` only once it has captured text.
        if ($handle->__elephc_header_out !== null) {
            $decoded["request_header"] = $handle->__elephc_header_out;
        }
        return $decoded;
    }
    // CURLINFO_PRIVATE (1048597): the arbitrary PHP value CURLOPT_PRIVATE stored on the
    // object. libcurl never held it, so it is answered without a bridge call. `false` on
    // a handle that never set it, matching php-src.
    if ($option === 1048597) {
        return $handle->__elephc_private;
    }
    // CURLINFO_HEADER_OUT (2): the outgoing request header the debug TRAMPOLINE captured
    // in C (`crates/elephc-curl/src/callbacks.rs`'s `debug_callback`), read through the
    // bridge's string-op route. php-src captures the same bytes in its own C trampoline
    // (`interface.c`:917) — independent of the PHP callback, so a throw inside the
    // callback still leaves the header captured. `false` when no transfer delivered one.
    if ($option === 2) {
        $header = __elephc_curl_easy_str_op($raw, 7, "", 0);
        if (!is_string($header)) {
            return false;
        }
        return $header;
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
    $handle->__elephc_write_user = false;
    $handle->__elephc_private = false;
    // php-src's curl_reset() releases the handler callables too (measured against PHP
    // 8.4.20: a write callback installed before curl_reset() never fires afterwards).
    // The bridge dropped its own slots and libcurl registrations inside
    // __elephc_curl_easy_reset; this drops the GC roots that kept the descriptors alive.
    $handle->__elephc_callbacks = [];
    // The four stream options go with them (measured: after curl_reset(), a CURLOPT_FILE
    // stream receives nothing and the body prints to stdout instead). Dropping the roots
    // here is also what lets the streams be closed/collected — nothing else holds them.
    $handle->__elephc_file = null;
    $handle->__elephc_writeheader = null;
    $handle->__elephc_infile = null;
    $handle->__elephc_stderr = null;
    $handle->__elephc_read_user = null;
    $handle->__elephc_prereq_user = null;
    // A reset handle has never had CURLOPT_DEBUGFUNCTION set, so CURLOPT_STDERR works
    // again on it — libcurl's own `curl_easy_reset` clears `set.fdebug` too.
    $handle->__elephc_debug_user = false;
    $handle->__elephc_header_out = null;
    $handle->__elephc_header_out_enabled = false;
}

// DIVERGENCE FROM PHP, for the same reason `curl_init()` diverges (see its comment):
// php-src declares `curl_copy_handle(CurlHandle $handle): CurlHandle|false`, but a union
// return would make `$copy = curl_copy_handle($ch); curl_exec($copy);` a compile error, so
// this returns a plain `CurlHandle` and THROWS on the allocation failure php-src answers
// `false` for.
//
// THE COPY DUPLICATES BOTH LAYERS. `curl_easy_duphandle` copies libcurl's own options; the
// bridge carries over the capture flag (while the buffered body, errno, and error text all
// start clean on the copy, matching PHP); and this wrapper copies the two object-side
// mirrors. Missing any one of the three would produce a handle that looks identical and
// behaves differently — the RETURNTRANSFER mirror in particular decides `curl_exec()`'s
// RETURN TYPE, so a copy without it would answer `true` where the original answers a string.
function curl_copy_handle(CurlHandle $handle): CurlHandle {
    $raw = $handle->__elephc_handle;
    $copy = __elephc_curl_easy_copy($raw);
    if ($copy === false) {
        throw new \RuntimeException("curl_copy_handle(): libcurl could not duplicate the easy handle");
    }
    $new = CurlHandle::__elephc_wrap($copy);
    $new->__elephc_return_transfer = $handle->__elephc_return_transfer;
    $new->__elephc_write_user = $handle->__elephc_write_user;
    $new->__elephc_private = $handle->__elephc_private;
    // THE STREAM OPTIONS ARE CARRIED ONTO THE COPY, which is php's behaviour — measured
    // on PHP 8.4.20 for all four: a copy of a CURLOPT_FILE/WRITEHEADER/STDERR/INFILE
    // handle writes to (or reads from) the SAME stream the original was pointed at.
    // php-src copies its handler structs; these are the same facts on this object.
    //
    // Copying the property is ALSO what makes the internal closures work on the copy:
    // they read the stream off `$ch`, and the callback loop below re-registers them with
    // the COPY as that `$ch`. Copy the streams after the flags and BEFORE the loop.
    $new->__elephc_file = $handle->__elephc_file;
    $new->__elephc_writeheader = $handle->__elephc_writeheader;
    $new->__elephc_infile = $handle->__elephc_infile;
    $new->__elephc_stderr = $handle->__elephc_stderr;
    $new->__elephc_read_user = $handle->__elephc_read_user;
    $new->__elephc_prereq_user = $handle->__elephc_prereq_user;
    $new->__elephc_debug_user = $handle->__elephc_debug_user;
    $new->__elephc_debug_installed = $handle->__elephc_debug_installed;
    // CURLINFO_HEADER_OUT travels with the handle: the copy must know the option is enabled
    // so its OWN debug wrapper keeps capturing, and it starts not-yet-captured like the
    // original did before its transfer.
    $new->__elephc_header_out_enabled = $handle->__elephc_header_out_enabled;
    $new->__elephc_header_out = $handle->__elephc_header_out;
    // CALLBACKS ARE RE-REGISTERED, NEVER INHERITED. libcurl's `dupset` copies the
    // callback function pointers AND their CURLOPT_*DATA values, and every one of those
    // data values is the ORIGINAL handle's bridge id — a copy left as libcurl made it
    // would call the original's PHP callables with the ORIGINAL's $ch. The bridge clears
    // every registration on the duplicate for exactly that reason; this loop puts them
    // back pointing at the COPY, which is what php-src's curl_copy_handle does when it
    // re-points its own handler struct at the new php_curl (measured: the copy's callback
    // receives the copy's CurlHandle, not the original's).
    $newRaw = $new->__elephc_handle;
    $adapter = __elephc_curl_adapter_addr();
    foreach ($handle->__elephc_callbacks as $slot => $callback) {
        if ($callback === null) {
            continue;
        }
        $slotIndex = (int) $slot;
        // THE WRITE SLOT IS ROOTED BUT NOT NECESSARILY ACTIVE. php-src keeps ONE write
        // mode, so anything that selects PHP_CURL_RETURN or PHP_CURL_STDOUT after a
        // `CURLOPT_WRITEFUNCTION` leaves the callable rooted but INACTIVE — both
        // `CURLOPT_RETURNTRANSFER => true` and `=> false` do it. Registering slot 0 on the
        // copy regardless would re-select PHP_CURL_USER and desync the two sides: the
        // bridge would call the callback AND, because installing a write callback clears
        // `return_transfer`, `curl_exec()` on the copy would answer an empty capture (or
        // stop printing). The decision is therefore the ACTIVE MODE mirror, not
        // `__elephc_return_transfer`, which cannot tell RETURN from STDOUT. Copy the ROOT
        // so a later `curl_setopt($copy, WRITEFUNCTION, …)` is not needed, registration off.
        if ($slotIndex === 0 && !$new->__elephc_write_user) {
            $new->__elephc_callbacks[0] = $callback;
            continue;
        }
        $descriptor = __elephc_callable_ptr($callback);
        __elephc_curl_easy_set_callback($newRaw, $slotIndex, $descriptor, $new, $adapter);
        $new->__elephc_callbacks[$slotIndex] = $callback;
    }
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
// running, i.e. from inside a callback; on an idle handle libcurl accepts the
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
// (docs/php/curl.md records this for users.)
function curl_version(): array|false {
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

// `phpinfo()`: elephc does not render PHP's full information page, but a curl fixture reads
// `curl_version()`'s feature list back out of it, so this prelude declares the function and
// renders the curl section. Every other section PHP would print is out of scope here.
function phpinfo(int $flags = 0): bool {
    $info = curl_version();
    echo "phpinfo()\n\nphpinfo\n\n";
    if (is_array($info)) {
        $features = $info["feature_list"];
        if (is_array($features)) {
            foreach ($features as $key => $value) {
                echo $key . " => " . ($value ? "Yes" : "No") . "\n";
            }
        }
    }
    return true;
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
    // CURLMOPT_PUSHFUNCTION's normalized wrapper, rooted here so the descriptor it owns
    // outlives the curl_multi_setopt() call. Installed as slot 7 on every attached easy
    // handle; the bridge's multi trampoline reads it from the PARENT handle.
    public mixed $__elephc_push_user = null;

    private static bool $__elephc_minting = false;

    public function __construct() {
        if (!self::$__elephc_minting) {
            throw new \Error("Cannot directly construct CurlMultiHandle, use curl_multi_init() instead");
        }
    }

    public static function __elephc_wrap(mixed $raw): CurlMultiHandle {
        self::$__elephc_minting = true;
        $h = new self();
        self::$__elephc_minting = false;
        $h->__elephc_handle = $raw;
        return $h;
    }

    public function __elephc_attach(int $id, CurlHandle $handle): void {
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
    __elephc_curl_reset_gone_away($handle, "curl_multi_add_handle");
    $multiRaw = $multi_handle->__elephc_handle;
    $easyRaw = $handle->__elephc_handle;
    $code = __elephc_curl_multi_add($multiRaw, $easyRaw);
    if ($code === 0) {
        $id = __elephc_curl_easy_id($easyRaw);
        $multi_handle->__elephc_attach($id, $handle);
        // A push callback set before this add applies to the new parent handle too.
        if ($multi_handle->__elephc_push_user !== null) {
            $handle->__elephc_push_user = $multi_handle->__elephc_push_user;
            $normalized = __elephc_normalize_callable("__elephc_curl_push_invoke");
            $descriptor = __elephc_callable_ptr($normalized);
            $adapter = __elephc_curl_adapter_addr();
            __elephc_curl_easy_set_callback($easyRaw, 7, $descriptor, $handle, $adapter);
        }
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
function curl_multi_exec(CurlMultiHandle $multi_handle, &$still_running): int {
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
    // php-src rejects a timeout outside [0, INT_MAX/1000] before forwarding it
    // (`ext/curl/multi.c`, GH-15547); the bound is INT_MAX / 1000.0.
    if ($timeout < 0.0 || $timeout > 2147483.647) {
        throw new \ValueError("curl_multi_select(): Argument #2 (\$timeout) must be between 0 and 2147483.647000");
    }
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
function curl_multi_info_read(CurlMultiHandle $multi_handle, &$queued_messages = null) {
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
        // A server-pushed handle is not in the multi's added-handle map (the program never
        // added it); the push callback parked the object it was handed.
        $pushed = CurlHandle::$__elephc_pushed;
        if (isset($pushed[$easyId])) {
            $handle = $pushed[$easyId];
            $rest = [];
            foreach ($pushed as $pendingId => $pendingHandle) {
                if ($pendingId !== $easyId) {
                    $rest[$pendingId] = $pendingHandle;
                }
            }
            CurlHandle::$__elephc_pushed = $rest;
        }
    }
    if ($handle === null) {
        return ["msg" => $msg, "result" => $result];
    }
    return ["msg" => $msg, "result" => $result, "handle" => $handle];
}

// `$multi_handle` is unused for the same reason `curl_close()`'s `$handle` is (see its
// comment): the function is a no-op in PHP 8 and the parameter name is PHP-visible
// named-argument surface that cannot be renamed to `_multi_handle`. The checker's
// empty-body exemption keeps it from warning.
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
// PHP's warning), `-1` not a cURL multi option at all (php-src's own
// `ValueError`).
//
// THE OPTION IS CLASSIFIED BEFORE THE VALUE IS TYPE-CHECKED, which is the order php-src
// uses and the order `curl_setopt()` above already uses (its `$kind === 6` block runs
// before the scalar guard). php-src's `curl_multi_setopt` is one `switch (option)`: an
// option it does not recognize is `ValueError` and `CURLMOPT_PUSHFUNCTION` is a CALLBACK
// question, and neither ever looks at the value's scalar type. Measured on PHP 8.4.20:
//   curl_multi_setopt($mh, 999999, function () {})  -> ValueError (not TypeError)
//   curl_multi_setopt($mh, 999999, [1])             -> ValueError
//   curl_multi_setopt($mh, 20014,  null)            -> TypeError about a CALLBACK
// `CURLMOPT_PUSHFUNCTION` (20014) is a CALLBACK option, and callbacks never reach the
// bridge's option table by number: the branch below installs the callable on the multi and
// on every attached easy handle, then answers `true`. Checking `is_int()` first (as this
// function used to) made a closure on it a `TypeError` about scalar types, which is the
// one shape php-src never produces for it.
//
// THE THREE NUMBERS BELOW MIRROR `multi_option_kind` (`crates/elephc-curl/src/multi.rs`),
// which mirrors the frozen surface (`scripts/docs/curl_surface.json`, `option_kinds`);
// the bridge stays the AUTHORITY and re-classifies every call that gets past here, so
// this table only decides WHICH of the three answers is reached first. Both sides move
// together when the frozen surface grows a `CURLMOPT_*`.
// The bridge's push trampoline calls this named function through the parent handle's
// push slot. `$parent` is a real CurlHandle object (the bridge boxes it), and the pushed
// handle arrives as a bridge id because only PHP can mint the `CurlHandle` object for it.
function __elephc_curl_push_invoke(mixed $parent, mixed $pushed_id, mixed $headers_json): int {
    $user = $parent->__elephc_push_user;
    $pushed = CurlHandle::__elephc_wrap(__elephc_curl_easy_from_id((int) $pushed_id));
    // php-src's pushed handle is libcurl's duplicate of the parent, so the PHP-layer write
    // mode comes along: a RETURNTRANSFER parent captures the pushed body too.
    $pushed->__elephc_return_transfer = $parent->__elephc_return_transfer;
    $pushed->__elephc_write_user = false;
    // libcurl duplicates the parent's CALLBACKS as well, and php-src duplicates the PHP
    // handler structs with them: the pushed transfer runs the SAME write/header callables,
    // re-bound to the pushed handle. Re-installing them here is what makes `$ch` inside a
    // callback the pushed `CurlHandle` (and keeps a discarding callback discarding).
    $callbacks = $parent->__elephc_callbacks;
    if ($parent->__elephc_write_user && isset($callbacks[0]) && $callbacks[0] !== null) {
        curl_setopt($pushed, CURLOPT_WRITEFUNCTION, $callbacks[0]);
    }
    if (isset($callbacks[1]) && $callbacks[1] !== null) {
        curl_setopt($pushed, CURLOPT_HEADERFUNCTION, $callbacks[1]);
    }
    $headers = $headers_json === "" ? [] : json_decode((string) $headers_json, true);
    $answer = (int) call_user_func($user, $parent, $pushed, $headers);
    // php-src hands the SAME CurlHandle object back from curl_multi_info_read() when the
    // pushed transfer completes. Park it only when the push is ACCEPTED: a denied push has
    // its native handle destroyed by libcurl, and the bridge drops the entry with it.
    if ($answer === CURL_PUSH_OK) {
        CurlHandle::$__elephc_pushed[(int) $pushed_id] = $pushed;
    }
    return $answer;
}

function curl_multi_setopt(CurlMultiHandle $multi_handle, int $option, mixed $value): bool {
    $raw = $multi_handle->__elephc_handle;
    // 3 PIPELINING, 6 MAXCONNECTS, 7 MAX_HOST_CONNECTIONS, 8 MAX_PIPELINE_LENGTH,
    // 13 MAX_TOTAL_CONNECTIONS, 16 MAX_CONCURRENT_STREAMS -> long;
    // 30009 CONTENT_LENGTH_PENALTY_SIZE, 30010 CHUNK_LENGTH_PENALTY_SIZE -> off_t.
    $carryable = $option === 3 || $option === 6 || $option === 7 || $option === 8
        || $option === 13 || $option === 16 || $option === 30009 || $option === 30010;
    if (!$carryable) {
        // 20014 CURLMOPT_PUSHFUNCTION: HTTP/2 server push. php-src validates the callable
        // first, so a bad value is the callback TypeError.
        if ($option === 20014) {
            // PHP resolves a `[$object, 'method']` callable through `__call` when the class
            // declares it and the named method is absent (php-src's push trampoline does the
            // same). Bind the magic dispatch into a closure the descriptor can carry.
            if (is_array($value) && count($value) === 2 && is_object($value[0]) && is_string($value[1])) {
                $receiver = $value[0];
                $method = $value[1];
                if (!in_array($method, get_class_methods($receiver)) && is_callable([$receiver, "__call"])) {
                    $value = function () use ($receiver, $method) {
                        return $receiver->__call($method, func_get_args());
                    };
                }
            }
            if (!is_callable($value)) {
                if (is_string($value)) {
                    throw new \TypeError("curl_multi_setopt(): Argument #2 (\$option) must be a valid callback for option CURLMOPT_PUSHFUNCTION, function \"" . $value . "\" not found or invalid function name");
                }
                throw new \TypeError("curl_multi_setopt(): Argument #2 (\$option) must be a valid callback for option CURLMOPT_PUSHFUNCTION, no array or string given");
            }
            // HTTP/2 server push. The bridge installs one trampoline per multi and reads
            // the callable from the PARENT easy handle's slot 7. The slot always holds the
            // prelude dispatcher — a plain named function, so no closure capture has to
            // cross the ABI — and the user callable travels on the handle property.
            $multi_handle->__elephc_push_user = $value;
            $normalized = __elephc_normalize_callable("__elephc_curl_push_invoke");
            $descriptor = __elephc_callable_ptr($normalized);
            $adapter = __elephc_curl_adapter_addr();
            foreach ($multi_handle->__elephc_handles as $attached) {
                $attached->__elephc_push_user = $value;
                __elephc_curl_easy_set_callback($attached->__elephc_handle, 7, $descriptor, $attached, $adapter);
            }
            return true;
        }
        // Record php-src's CURLM_UNKNOWN_OPTION on the handle before throwing: the bridge
        // classifies the option as invalid and sets the errno, then this raises.
        __elephc_curl_multi_setopt($raw, $option, 0);
        throw new \ValueError("curl_multi_setopt(): Argument #2 (\$option) is not a valid cURL multi option");
    }
    if (!is_int($value) && !is_bool($value) && !is_float($value) && !is_string($value)) {
        $given = is_array($value) ? "array" : (is_object($value) ? get_class($value) : (is_null($value) ? "null" : gettype($value)));
        throw new \TypeError("curl_multi_setopt(): Argument #3 (\$value) must be of type string|int|float|bool, " . $given . " given");
    }
    $applied = __elephc_curl_multi_setopt($raw, $option, (int) $value);
    // Both remaining answers stay honored rather than assumed unreachable: the bridge is
    // the authority, and `0` is also how it reports an option libcurl itself refused.
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

// THE SHARE INTERFACE. `CurlShareHandle` needs no identity map the way `CurlMultiHandle`
// does — nothing ever reads a share handle back OUT of the bridge the way
// `curl_multi_info_read()`/`curl_multi_get_handles()` read easy handles back, so there is
// no "which PHP object does this native id belong to" question to answer. Its object
// shape is otherwise identical to `CurlHandle`'s: a private constructor, a static wrap
// factory, an empty `__debugInfo()`, and a `__serialize()` that throws.
//
// THE LIFETIME QUESTION: libcurl 8.21.0 REFCOUNTS a share (`CURLOPT_SHARE` increments it;
// an easy handle's own close decrements it), so `curl_share_cleanup()` while an easy
// handle still references it does NOT corrupt anything — it FAILS with `CURLSHE_IN_USE`
// and frees nothing, a silent PERMANENT LEAK of the share (its DNS cache, cookie jar, TLS
// session cache, connection pool) if that failure is ever ignored, not a use-after-free.
// `curl_setopt($ch, CURLOPT_SHARE, $sh)` (this file's `curl_setopt()`, `$kind === 7`
// branch) therefore does NOT take a PHP-level reference from `$ch` to `$sh` the way
// `CurlMultiHandle::__elephc_attach()` does for an added easy handle — the BRIDGE is the
// source of truth instead. `crates/elephc-curl/src/share.rs`'s module doc carries the full
// argument; in short, every share entry tracks which easy ids are attached to it (its own
// mirror of libcurl's refcount), and freeing a share while that list is non-empty
// (`__elephc_curl_share_free`, reached from this class's Mixed cell teardown — there is
// deliberately no `__destruct`) does NOT call `curl_share_cleanup()` yet: it DEFERS,
// marking the entry pending, and the real cleanup finally runs once the LAST attached easy
// handle detaches or is freed — mirroring, at the bridge level, the real Zend GC reference
// php-src itself relies on. No attached easy handle's `CURLOPT_SHARE` is ever forcibly
// cleared by this path. A PHP program is therefore free to `unset()` the share before the
// easy handles attached to it: the transfer already run is unaffected, any FUTURE transfer
// on those handles still succeeds (still genuinely sharing, until it too is freed), and the
// native share is destroyed exactly once, only after every attachment has genuinely ended.
final class CurlShareHandle {
    public mixed $__elephc_handle = null;

    private static bool $__elephc_minting = false;

    public function __construct() {
        if (!self::$__elephc_minting) {
            throw new \Error("Cannot directly construct CurlShareHandle, use curl_share_init() instead");
        }
    }

    public static function __elephc_wrap(mixed $raw): CurlShareHandle {
        self::$__elephc_minting = true;
        $h = new self();
        self::$__elephc_minting = false;
        $h->__elephc_handle = $raw;
        return $h;
    }

    public function __debugInfo(): array {
        return [];
    }

    public function __serialize(): array {
        throw new \Exception("Serialization of 'CurlShareHandle' is not allowed");
    }
}

// php-src declares `curl_share_init(): CurlShareHandle` with NO `false` arm — PHP's own
// docs describe it as never failing — the same shape `curl_multi_init()` has. libcurl's
// own `curl_share_init()` CAN still return null on allocation failure, so the bridge's
// `false` answer becomes a thrown `RuntimeException` here, the same divergence
// `curl_init()`/`curl_multi_init()` already document (see this file's header).
function curl_share_init(): CurlShareHandle {
    $raw = __elephc_curl_share_init();
    if ($raw === false) {
        throw new \RuntimeException("curl_share_init(): libcurl could not allocate a share handle");
    }
    return CurlShareHandle::__elephc_wrap($raw);
}

// ONLY TWO `CURLSHOPT_*` VALUES ARE REAL PHP SURFACE: `CURLSHOPT_SHARE` (1) and
// `CURLSHOPT_UNSHARE` (2). Confirmed against the frozen PHP 8.2-8.5 extraction
// (scripts/docs/curl_surface.json): `CURLSHOPT_LOCKFUNC`/`UNLOCKFUNC`/`USERDATA` are
// C-API-only locking hooks PHP never exposes as constants at all, so php-src's own
// `curl_share_setopt()` switch has exactly two cases and a `default:
// zend_argument_value_error(...)` — there is no third "real option this build cannot
// carry" bucket the way `curl_setopt()`/`curl_multi_setopt()` need one.
//
// `$value` IS THE `CURL_LOCK_DATA_*` CONSTANT naming which cache to (un)share. libcurl
// itself validates it and answers `CURLSHE_BAD_OPTION` for a value it does not recognize
// (or `CURLSHE_NOT_BUILT_IN` for one this libcurl build lacks); this function turns EITHER
// into a plain `false` — no fabricated warning, the same answer `curl_setopt()` gives for
// a real, carryable option that libcurl itself refuses. The true code stays retrievable
// through `curl_share_errno()`/`curl_share_strerror()`.
function curl_share_setopt(mixed $share_handle, int $option, mixed $value): bool {
    // The declared parameter is `CurlShareHandle`, but a `CurlSharePersistentHandle`
    // must be a RUNTIME TypeError (php-src's own message), not a compile-time one.
    if (!$share_handle instanceof CurlShareHandle) {
        $given = is_object($share_handle) ? get_class($share_handle) : (is_null($share_handle) ? "null" : gettype($share_handle));
        throw new \TypeError("curl_share_setopt(): Argument #1 (\$share_handle) must be of type CurlShareHandle, " . $given . " given");
    }
    $raw = $share_handle->__elephc_handle;
    if (!is_int($value) && !is_bool($value) && !is_float($value) && !is_string($value)) {
        $given = is_array($value) ? "array" : (is_object($value) ? get_class($value) : (is_null($value) ? "null" : gettype($value)));
        throw new \TypeError("curl_share_setopt(): Argument #3 (\$value) must be of type string|int|float|bool, " . $given . " given");
    }
    $applied = __elephc_curl_share_setopt($raw, $option, (int) $value);
    if ($applied === -1) {
        throw new \ValueError("curl_share_setopt(): Argument #2 (\$option) is not a valid cURL share option");
    }
    return $applied === 1;
}

// `$share_handle` is unused for the same reason `curl_close()`'s `$handle` is (see its
// comment): the function is a no-op in PHP 8 and the parameter name is PHP-visible
// named-argument surface that cannot be renamed. The checker's empty-body exemption
// keeps it from warning.
function curl_share_close(CurlShareHandle $share_handle): void {
    // -- elephc PHP >= 8.5 curl_share_close deprecation begin --
    // See curl_close() above: PHP 8.5 deprecates this one too. curl_multi_close() is
    // NOT deprecated and keeps its empty body.
    if ($share_handle->__elephc_handle !== null) {
        trigger_error("Function curl_share_close() is deprecated since 8.5, as it has no effect since PHP 8.0", E_USER_DEPRECATED);
    }
    // -- elephc PHP >= 8.5 curl_share_close deprecation end --
}

function curl_share_errno(CurlShareHandle $share_handle): int {
    $raw = $share_handle->__elephc_handle;
    return __elephc_curl_share_errno($raw);
}

// DIVERGENCE FROM PHP, the same one `curl_strerror()`/`curl_multi_strerror()` document:
// php-src declares `?string`; libcurl's `curl_share_strerror()` never answers a null
// pointer (it falls back to "Unknown error" for a code it does not recognize).
function curl_share_strerror(int $error_code): string {
    $message = __elephc_curl_share_strerror($error_code);
    return $message;
}
// -- elephc PHP >= 8.5 curl_multi_get_handles begin --
// PHP 8.5 ONLY: 8.2-8.4 have no `curl_multi_get_handles`, and a
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
// -- elephc PHP >= 8.5 curl_share_init_persistent begin --
// PHP 8.5 ONLY: `curl_share_init_persistent()`/`CurlSharePersistentHandle`
// do not exist before 8.5, exactly like `curl_multi_get_handles()` above. The block
// markers are what `prelude_source_for_version` strips.
//
// `CurlSharePersistentHandle` IS A SIBLING OF `CurlShareHandle`, NOT A SUBCLASS —
// php-src does not extend it, so `curl_share_setopt()`/`curl_share_errno()`/
// `curl_share_close()` all stay typed `CurlShareHandle` and never accept this class.
// `curl_setopt()`'s `CURLOPT_SHARE` branch (this file's `$kind === 7` code, ABOVE this
// fenced block and therefore compiled for EVERY PHP version) matches it by `get_class()`
// STRING rather than `instanceof`, precisely so that always-compiled code never has to
// resolve this class name on an 8.4 profile — see that branch's own comment.
//
// PROCESS-LIFETIME, NEVER FREED. `curl_share_init_persistent()` is php-src's PHP-FPM
// answer to "build the share once, reuse it across every worker request forever" — elephc
// has no FPM-style worker-restart boundary to key that lifetime off, so the underlying
// libcurl share is kept alive until the PROCESS itself exits:
// `__elephc_curl_share_free()`/`crates/elephc-curl/src/share.rs`'s `elephc_curl_share_free`
// is a documented no-op for a share id created through this path. The PHP OBJECT wrapping
// it can still be garbage-collected normally like any other object — only the native
// share and its bridge-table entry are immortal.
//
// SAME OPTIONS -> SAME UNDERLYING SHARE. Repeated calls with an equivalent (order- and
// duplicate-insensitive) option set return a handle onto the identical native share, keyed
// by the sorted+deduplicated `CURL_LOCK_DATA_*` list — php-src's own semantics.
final class CurlSharePersistentHandle {
    public mixed $__elephc_handle = null;
    // PHP declares this `public readonly array $options`; elephc keeps it writable
    // (readonly is not enforced) but only `curl_share_init_persistent()` ever assigns it.
    public array $options = [];

    private static bool $__elephc_minting = false;

    public function __construct() {
        if (!self::$__elephc_minting) {
            throw new \Error("Cannot directly construct CurlSharePersistentHandle, use curl_share_init_persistent() instead");
        }
    }

    public static function __elephc_wrap(mixed $raw): CurlSharePersistentHandle {
        self::$__elephc_minting = true;
        $h = new self();
        self::$__elephc_minting = false;
        $h->__elephc_handle = $raw;
        return $h;
    }

    public function __debugInfo(): array {
        return [];
    }

    public function __serialize(): array {
        throw new \Exception("Serialization of 'CurlSharePersistentHandle' is not allowed");
    }
}

// ONLY THE FIVE `CURL_LOCK_DATA_*` VALUES PHP ACTUALLY EXPOSES ARE ACCEPTED — COOKIE,
// DNS, SSL_SESSION, CONNECT, PSL (frozen in scripts/docs/curl_surface.json; PHP does not
// define a userland `CURL_LOCK_DATA_HSTS` even though libcurl itself has one since 7.74) —
// everything else is php-src's own `ValueError`. Literal numbers, matching every other
// option check in this file: 2 COOKIE, 3 DNS, 4 SSL_SESSION, 5 CONNECT, 6 PSL.
//
// THE ARRAY CROSSES THE ABI AS A COMMA-SEPARATED STRING of the validated decimal ints
// (`__elephc_curl_share_init_persistent()`'s own doc comment explains why: this crate's C
// ABI has no native array shape, and encoding a variable-length PHP value as one byte
// blob for a fixed-arity C entry point is the same pattern `curl_setopt()`'s string-list
// options already establish). The BRIDGE does the sorting/deduplication that makes an
// equivalent option set — any order, with duplicates — resolve to the same share.
function curl_share_init_persistent(array $share_options): CurlSharePersistentHandle {
    // php-src validates the option list in this order (ext/curl/share.c): empty,
    // then each element's type, then CURL_LOCK_DATA_COOKIE, then membership in the
    // five exposed CURL_LOCK_DATA_* values. The messages are PHP's own.
    if (count($share_options) === 0) {
        throw new \ValueError("curl_share_init_persistent(): Argument #1 (\$share_options) must not be empty");
    }
    foreach ($share_options as $opt) {
        if (!is_int($opt)) {
            $given = is_array($opt) ? "array" : (is_object($opt) ? get_class($opt) : gettype($opt));
            throw new \TypeError("curl_share_init_persistent(): Argument #1 (\$share_options) must contain only int values, " . $given . " given");
        }
        if ($opt === 2) {
            throw new \ValueError("curl_share_init_persistent(): Argument #1 (\$share_options) must not contain CURL_LOCK_DATA_COOKIE because sharing cookies across PHP requests is unsafe");
        }
        if ($opt !== 3 && $opt !== 4 && $opt !== 5 && $opt !== 6) {
            throw new \ValueError("curl_share_init_persistent(): Argument #1 (\$share_options) must contain only CURL_LOCK_DATA_* constants");
        }
    }
    // PHP exposes `CurlSharePersistentHandle::$options` as the SORTED, de-duplicated
    // option list, and uses that normalized set as the share's identity. Build it by
    // ascending membership so no `sort()` is needed.
    //
    // NOTE: the append must stay OUTSIDE the inner `foreach` — appending an outer
    // loop variable from inside a nested `foreach` currently stores an empty value
    // (a general elephc array-append miscompilation, not a curl one).
    $unique = [];
    $csv = "";
    foreach ([3, 4, 5, 6] as $candidate) {
        $found = false;
        foreach ($share_options as $opt) {
            if ($opt === $candidate) {
                $found = true;
                break;
            }
        }
        if ($found) {
            $unique[] = $candidate;
            if ($csv !== "") {
                $csv .= ",";
            }
            $csv .= (string) $candidate;
        }
    }
    $raw = __elephc_curl_share_init_persistent($csv);
    if ($raw === false) {
        throw new \RuntimeException("curl_share_init_persistent(): libcurl could not allocate a share handle");
    }
    $handle = CurlSharePersistentHandle::__elephc_wrap($raw);
    $handle->options = $unique;
    return $handle;
}
// -- elephc PHP >= 8.5 curl_share_init_persistent end --
"#;

/// Returns the curl prelude source for one PHP compatibility version: the full text for
/// 8.5 and later, with each later-than-`php_version` block removed otherwise.
///
/// `pub(crate)` for the migration tooling: `synthetic_class::transcribe` reads it to
/// rebuild the same AST through the Rust builders, and `CURL_PRELUDE_SRC` stays the
/// oracle until the parsed form is retired.
#[cfg(test)]
pub(crate) fn prelude_source_for_version(
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
    remove_version_block(
        &mut source,
        "// -- elephc PHP >= 8.5 curl_share_init_persistent begin --",
        "// -- elephc PHP >= 8.5 curl_share_init_persistent end --",
    );
    remove_version_block(
        &mut source,
        "// -- elephc PHP >= 8.5 curl_close deprecation begin --",
        "// -- elephc PHP >= 8.5 curl_close deprecation end --",
    );
    remove_version_block(
        &mut source,
        "// -- elephc PHP >= 8.5 curl_share_close deprecation begin --",
        "// -- elephc PHP >= 8.5 curl_share_close deprecation end --",
    );
    std::borrow::Cow::Owned(source)
}

/// Removes one `begin`/`end`-fenced block (markers included) from the prelude source.
///
/// A MISSING MARKER PANICS rather than silently leaving the block in: the whole point of
/// the fence is that an 8.4 build cannot declare an 8.5 function, and a marker renamed by
/// an unrelated edit would break that quietly. Mirrors `pdo_prelude::remove_version_block`.
#[cfg(test)]
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
