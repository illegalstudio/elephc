//! Purpose:
//! The `elephc_curl_*` C ABI: every `#[no_mangle]` entry point this crate exposes —
//! easy-handle lifecycle, URL/option setup in every typed form (`long`/`off_t`/`char *`/
//! slist/blob/stream/share), performing a transfer, `curl_getinfo()`, the multi and share
//! interfaces, MIME/multipart uploads, and callback registration. Every function here is
//! thin: it validates arguments, takes the table lock, and delegates to `crate::easy` (raw
//! libcurl), `crate::php_layer` (PHP-only semantics), or one of the interface-specific
//! modules (`crate::multi`, `crate::share`, `crate::mime`, `crate::info`).
//!
//! Called from:
//! - The PHP-program linker, once a compiled program uses curl: the curl prelude
//!   (`src/curl_prelude.rs`) emits `extern "elephc_curl"` calls into these entry points.
//! - This crate's own gated tests (`crates/elephc-curl/src/tests.rs`).
//!
//! Key details:
//! - Every entry point routes its whole body through
//!   `handles::ffi_guard` so a Rust panic never unwinds across the `extern
//!   "C"` boundary (undefined behavior) — it becomes the documented fallback
//!   value instead.
//! - Return-code convention: every `int32_t` "did it work" return here is a
//!   plain boolean (`1` success, `0` failure), with exactly one deliberate
//!   exception — `elephc_curl_easy_errno`, which returns the raw `CURLcode`
//!   by design (it exists specifically to back PHP's `curl_errno()`).
//! - `elephc_curl_easy_take_body`/`elephc_curl_easy_error`/
//!   `elephc_curl_global_info` write into caller-owned buffers and always
//!   report the required/actual length through their `*_len` out-parameter,
//!   even on a `0` (too-small-buffer) return — mirroring
//!   `elephc-crypto`'s cipher ABI (`crates/elephc-crypto/src/cipher/abi.rs`).
//! - **Caller contract — no concurrent calls on the same id.** The table
//!   mutex (`crate::handles::handles`) only serializes access to the table
//!   itself; `elephc_curl_easy_perform`, `elephc_curl_easy_pause` and
//!   `elephc_curl_easy_free` all deliberately drop that lock before calling
//!   into libcurl (see their own
//!   doc comments below for why), so it does NOT stop a `free(id)` on one
//!   thread from running `curl_easy_cleanup` on the same `*mut CURL` while
//!   another thread is still blocked inside `perform(id)` — a real
//!   use-after-free at the libcurl level. This is sound only because
//!   elephc-compiled PHP programs are effectively single-threaded today, so
//!   no two `elephc_curl_*` calls for the same id are ever concurrent. Any
//!   future concurrent caller (a multi-threaded driver, or `crate::multi` if
//!   it ever fans work across OS threads — today it does not) MUST itself guarantee
//!   that no two threads call into this ABI for the same id at once — see
//!   `crate::handles::EasyEntry`'s `Send` impl for the full reasoning.

use std::ffi::{c_char, c_int, c_void, CString};

use crate::ca;
use crate::easy;
use crate::handles::{self, EasyEntry};
use crate::info;
use crate::options;
use crate::php_layer;

/// Returns the `elephc_curl` ABI version. v1 = the surface described in this
/// module; bumped when the C ABI shape changes.
#[no_mangle]
pub extern "C" fn elephc_curl_version_abi() -> i32 {
    1
}

/// Classifies a `curl_setopt()` option number: which setter (if any) can carry
/// its value. See `crate::options` for the kind codes and for why this
/// classification is a memory-safety boundary rather than a convenience.
///
/// This is the ONE curl entry point that touches no handle and no libcurl
/// state: it is a pure lookup in the frozen option table, so the curl prelude
/// can ask it before it has decided which setter to call.
///
/// `opt` is an `int64_t`, not an `int32_t`, ON PURPOSE: PHP integers are
/// 64-bit, and a 32-bit parameter would let `curl_setopt($ch, 4294967298, …)`
/// truncate onto option `2` and be classified as a real option. Anything
/// outside `i32`'s range is simply not a cURL option, so it answers
/// [`options::KIND_INVALID`] and the prelude raises php-src's `ValueError`.
#[no_mangle]
pub extern "C" fn elephc_curl_option_kind(opt: i64) -> i32 {
    handles::ffi_guard(options::KIND_INVALID, || {
        i32::try_from(opt).map_or(options::KIND_INVALID, options::option_kind)
    })
}

/// Allocates a new libcurl easy handle, installs the write callback (see
/// `crate::php_layer::install_write_callback`) and error buffer, and
/// registers it in the handle table under a fresh id. Returns `0` on libcurl
/// allocation failure.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_init() -> i64 {
    handles::ffi_guard(0, || {
        let curl = easy::init();
        if curl.is_null() {
            return 0;
        }
        let id = handles::next_id();
        let mut entry = EasyEntry::new(curl, id);
        unsafe {
            php_layer::install_write_callback(curl, id);
            // Installed from creation, not lazily on the first `curl_setopt()`: without it
            // a fresh handle with `CURLOPT_UPLOAD` and no PHP read callback would upload
            // the PROCESS'S OWN STDIN through libcurl's default `fread`/`CURLOPT_READDATA`
            // pair. See `crate::callbacks::install_read_callback`.
            crate::callbacks::install_read_callback(curl, id);
            // `error_buf`'s heap allocation address is stable across the
            // upcoming move into the handle table (moving a `Vec` only moves
            // its {ptr,len,cap} header, never reallocates), so handing
            // libcurl this pointer now and inserting `entry` afterward is
            // sound as long as `error_buf` is never resized (see
            // `EasyEntry::error_buf`'s docs).
            easy::setopt_ptr(
                curl,
                easy::CURLOPT_ERRORBUFFER,
                entry.error_buf.as_mut_ptr() as *mut c_void,
            );
        }
        apply_discovered_cainfo(curl);
        handles::lock_recover(handles::handles()).insert(id, entry);
        id
    })
}

/// Installs this process's RUNTIME-DISCOVERED CA bundle (`crate::ca`) on a handle that is
/// known to carry no CA configuration of its own — a fresh `curl_init()` handle, or one
/// `curl_easy_reset` has just cleared.
///
/// A NO-OP ON THE OVERWHELMINGLY COMMON PATH. `ca::discovered_cainfo()` answers `None`
/// whenever libcurl advertises Apple SecTrust or the CA bundle it was built against still
/// exists on this machine. Only a file-based build whose CA layout changed — or a
/// file-based cross build with no baked-in default — reaches the `setopt` below. In
/// particular, iOS must stay on this no-op path: setting CAINFO would mark the handle as
/// custom and disable curl's automatic Security.framework verification.
///
/// CALLED WHILE THE HANDLE IS STILL UNSHARED (before `elephc_curl_easy_init` inserts its
/// entry in the table) or under the table lock (`elephc_curl_easy_reset`); it touches
/// nothing but this one handle, so there is no ordering hazard with any other entry point.
///
/// THE VALUE GOES THROUGH PLAIN `curl_easy_setopt(CURLOPT_CAINFO, …)`, deliberately, and
/// not through any private door: that is what makes a later `curl_setopt($ch,
/// CURLOPT_CAINFO, …)` from PHP overwrite it by ordinary option assignment, and it is also
/// what sets libcurl's own `ssl.custom_cafile` bit so `Curl_ssl_easy_config_complete`
/// stops applying the baked-in default (pinned libcurl 8.21.0,
/// `lib/vtls/vtls_config.c:280`).
///
/// BOTH HOPS GET THE SAME BUNDLE. libcurl verifies the TLS connection to an HTTPS PROXY
/// against an entirely separate CA configuration (`data->set.proxy_ssl`,
/// `STRING_SSL_CAFILE_PROXY`) with its own injection of the same compile-time
/// `CURL_CA_BUNDLE` (`lib/vtls/vtls_config.c:337-355`), so a binary on a foreign machine
/// failed proxy verification for exactly the reason it failed origin verification, and
/// setting only `CURLOPT_CAINFO` would have left `curl_setopt($ch, CURLOPT_PROXY,
/// "https://…")` broken. MEASURED before this was added: with `$CURL_CA_BUNDLE` pointing
/// at a missing file, a transfer through a local HTTPS proxy still answered `60`
/// (the readable BAKED bundle was in force on the proxy hop) rather than the `77` the
/// origin hop already gave. `CURLOPT_PROXY_CAINFO` is set from the SAME resolution —
/// there is no second probe and no second decision, because the reason the baked path is
/// unusable is a property of the MACHINE, not of the hop.
///
/// THE PROXY SIDE IS EXACTLY SYMMETRIC, verified rather than assumed. `lib/setopt.c:1770`
/// is `s->proxy_ssl.custom_cafile = !!s->str[STRING_SSL_CAFILE_PROXY];` — the same shape as
/// its non-proxy twin at 1906, including the NULL-resets-the-flag behavior — and the
/// `vtls_config.c` proxy block re-injects `CURL_CA_BUNDLE` under the identical
/// `!custom_cafile && !str[…]` condition. So every rule below transfers: a user
/// `CURLOPT_PROXY_CAINFO` replaces this value, a user `CURLOPT_PROXY_CAPATH` composes with
/// it, and clearing it would re-inject the baked path just as harmfully.
/// `CURLOPT_PROXY_CAINFO_BLOB` would take precedence in libcurl, but it is
/// `KIND_UNSUPPORTED` in `crate::options` and unreachable from PHP, exactly like its
/// non-proxy twin.
///
/// NOTHING EVER TAKES THE BUNDLE BACK OUT, and an earlier revision of this file was wrong
/// to try. It retired the discovered bundle when a program set `CURLOPT_CAPATH`, on the
/// theory that a directory store should not be silently widened by a bundle the program
/// never asked for. MEASURED against this exact archive, that theory does not survive
/// contact with libcurl: `curl_easy_setopt(CURLOPT_CAINFO, NULL)` sets `custom_cafile`
/// back to FALSE (`lib/setopt.c:1906`), which is precisely the condition
/// `Curl_ssl_easy_config_complete` re-injects the compile-time `CURL_CA_BUNDLE` under
/// (`lib/vtls/vtls_config.c:279-284`). libcurl's own verbose trace for the retire sequence,
/// with `$CURL_CA_BUNDLE` pointed at a missing file and a user capath set:
///
/// ```text
/// *   CAfile: /etc/ssl/cert.pem      <- the BAKED default, re-injected
/// *   CApath: /tmp/…/empty-capath
/// ```
///
/// So the retire never produced "the program's capath alone" — it produced "capath UNION
/// the BAKED bundle", the very union it was meant to prevent, just against a store chosen
/// by the build machine instead of by discovery. Worse, on the machine that actually
/// matters — one where the baked path does NOT exist — that re-injected `CAfile` is
/// unreadable and `ossl_populate_x509_store` aborts with `CURLE_SSL_CACERT_BADFILE`
/// BEFORE it ever loads the `CApath`, so a program that set a perfectly good capath would
/// fail outright. Leaving the discovered bundle in place gives "discovered UNION capath",
/// which is the same shape stock libcurl gives ("baked UNION capath") and the only variant
/// that stays portable. `tests/codegen/curl/easy_ca.rs`'s
/// `setting_capath_keeps_the_discovered_bundle` pins it.
fn apply_discovered_cainfo(curl: *mut easy::CURL) {
    let Some(path) = ca::discovered_cainfo() else {
        return;
    };
    // A refusal leaves the handle exactly as libcurl configured it — the same "never turn
    // a working configuration into a broken one" rule the whole feature follows — so
    // neither return value is propagated: there is no caller who could do anything better
    // than carry on with libcurl's own default. That also makes the proxy line safe on a
    // hypothetical `CURL_DISABLE_PROXY` build, where it would simply answer
    // `CURLE_UNKNOWN_OPTION` and change nothing. (Not this build: the recipe passes no
    // `--disable-proxy`, and `curl_setopt($ch, CURLOPT_PROXY_CAINFO, …)` is measured to
    // return true here.)
    unsafe {
        easy::setopt_str(curl, ca::CURLOPT_CAINFO, path.as_ptr());
        easy::setopt_str(curl, ca::CURLOPT_PROXY_CAINFO, path.as_ptr());
    }
}

/// Sets `CURLOPT_URL` on handle `id` from a raw byte string (not required to
/// be UTF-8: a URL is just a NUL-free byte string as far as libcurl cares).
/// Returns `0` for an unknown id, embedded NUL bytes, or a libcurl setopt
/// failure.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_set_url(id: i64, ptr: *const u8, len: usize) -> i32 {
    handles::ffi_guard(0, || {
        let Some(url) = bytes_to_cstring(ptr, len) else {
            return 0;
        };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        let code = unsafe { easy::setopt_str(entry.curl, easy::CURLOPT_URL, url.as_ptr()) };
        (code == easy::CURLE_OK) as i32
    })
}

/// Sets a `long`- or `off_t`-valued `curl_setopt` option (including the
/// PHP-only `CURLOPT_RETURNTRANSFER` pseudo-option — see
/// `crate::php_layer::apply_long_option` for the full table). Returns `0` for
/// an unknown id, an option this setter does not
/// carry (see [`long_option_is_carryable`]), or a libcurl setopt failure.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_setopt_long(id: i64, opt: i32, value: i64) -> i32 {
    handles::ffi_guard(0, || {
        if !long_option_is_carryable(opt) {
            return 0;
        }
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        php_layer::apply_long_option(entry, opt, value) as i32
    })
}

/// THE ABI'S OWN KIND CHECK, THE WRITE-SIDE MIRROR OF `easy::getinfo_long`'s
/// `CURLINFO_TYPEMASK` GUARD. `curl_easy_setopt` is variadic and reads its third
/// argument purely from the option's numeric range (libcurl 8.21.0,
/// `lib/setopt.c`): a `long` handed to a `10000..20000` option is read as a
/// `char *` and dereferenced, and an integer handed to a `20000..30000` option is
/// read as a FUNCTION POINTER and called. Measured against this pinned libcurl:
/// `curl_easy_setopt(curl, CURLOPT_URL, 42L)` kills the process with SIGSEGV,
/// while `curl_easy_setopt(curl, CURLOPT_TIMEOUT, "file:///dev/null")` and the
/// same pointer on `CURLOPT_HTTPHEADER` are ACCEPTED with `CURLE_OK` — a wrong
/// call is as likely to be silently wrong as to crash. The curl prelude classifies correctly
/// before it picks a setter (`crate::options::option_kind`), and so does the eval
/// interpreter — but "the only two callers get it right" is not the same
/// guarantee as "the boundary refuses to get it wrong", and this is an
/// `extern "C"` boundary any future caller can reach. Each setter therefore
/// re-checks the frozen table itself and answers its ordinary failure return (`0`,
/// PHP `false`) WITHOUT calling libcurl when the kinds disagree. The cost is one
/// binary search over a ~300-row static table per `curl_setopt()` call.
///
/// [`elephc_curl_easy_setopt_long`] accepts the two kinds whose value really is an
/// integer — `KIND_LONG` and `KIND_OFF_T`, which `apply_long_option` routes to
/// `setopt_long`/`setopt_off_t` by range — plus `CURLOPT_RETURNTRANSFER`, the one
/// PHP-layer pseudo-option the prelude forwards here (it never reaches libcurl;
/// `apply_long_option` handles it and returns). Every other `KIND_PHP_LAYER` row
/// is refused: `CURLOPT_BINARYTRANSFER` (19914) sits in libcurl's `char *` range
/// and would be exactly the wild-pointer write this check exists to stop.
fn long_option_is_carryable(opt: i32) -> bool {
    if opt == php_layer::CURLOPT_RETURNTRANSFER {
        return true;
    }
    matches!(
        options::option_kind(opt),
        options::KIND_LONG | options::KIND_OFF_T
    )
}

/// Sets a string-valued `curl_setopt` option, forwarded to real
/// `curl_easy_setopt` unchanged — except `CURLOPT_POSTFIELDS`, which goes
/// through [`set_postfields`] because a request body is not a C string.
/// Returns `0` for an unknown id, an option that is not `KIND_STRING` in the
/// frozen table (the kind check [`long_option_is_carryable`] documents),
/// embedded NUL bytes, or a libcurl setopt failure.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_setopt_str(
    id: i64,
    opt: i32,
    ptr: *const u8,
    len: usize,
) -> i32 {
    handles::ffi_guard(0, || {
        // Refused BEFORE the bytes are even read: a `char *` written into a `long`
        // option is the mirror image of the hazard the long setter guards, and a
        // slist option would make libcurl walk the string as a linked list.
        if options::option_kind(opt) != options::KIND_STRING {
            return 0;
        }
        // The body must be read as BYTES, before any `CString` conversion: a
        // POST body may legitimately contain NUL (a PHP string is binary-safe,
        // and so is `application/octet-stream`), and `bytes_to_cstring` would
        // reject exactly that.
        if opt == easy::CURLOPT_POSTFIELDS {
            let body: &[u8] = if len == 0 {
                &[]
            } else if ptr.is_null() {
                return 0;
            } else {
                unsafe { std::slice::from_raw_parts(ptr, len) }
            };
            let mut guard = handles::lock_recover(handles::handles());
            let Some(entry) = guard.get_mut(&id) else {
                return 0;
            };
            return unsafe { set_postfields(entry, body) } as i32;
        }
        let Some(value) = bytes_to_cstring(ptr, len) else {
            return 0;
        };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        // NO CA-SPECIFIC HANDLING HERE, AND THAT IS THE DESIGN, not an omission. On a
        // file-based build, CAINFO replaces and CAPATH composes with the discovered file
        // through libcurl's own option slots (see [`apply_discovered_cainfo`]). On an
        // AppleSecTrust build, either custom option disables automatic native trust unless
        // the matching SSL_OPTIONS also requests CURLSSLOPT_NATIVE_CA. Forwarding all four
        // untouched is what preserves those upstream semantics.
        let code = unsafe { easy::setopt_str(entry.curl, opt as c_int, value.as_ptr()) };
        (code == easy::CURLE_OK) as i32
    })
}

/// Sets a `struct curl_slist *` option (`CURLOPT_HTTPHEADER`, `CURLOPT_QUOTE`,
/// `CURLOPT_RESOLVE`, ...) from a blob of NUL-TERMINATED items: each item's
/// bytes followed by one `0` byte, so an empty blob is an empty list and a
/// single empty item is one `0` byte. That framing is unambiguous where a
/// separator-joined encoding is not, and it is exactly what
/// `curl_slist_append` wants anyway.
///
/// The new list is stored on the handle (`EasyEntry::slists`) because libcurl
/// does NOT copy it: it walks the list during the transfer, so it must outlive
/// every `curl_easy_perform`. The PREVIOUS list for this option is freed only
/// after libcurl has accepted the replacement, so a failed `setopt` leaves the
/// handle exactly as it was rather than pointing at freed memory.
///
/// Returns `0` for an unknown id, an option that is not `KIND_SLIST` in the
/// frozen table (the kind check [`long_option_is_carryable`] documents), a null
/// pointer with a nonzero length, an item libcurl could not allocate, or a
/// libcurl setopt failure.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_setopt_slist(
    id: i64,
    opt: i32,
    ptr: *const u8,
    len: usize,
) -> i32 {
    handles::ffi_guard(0, || {
        // Checked before anything is allocated, so a refusal also builds no list to
        // leak: handing a `struct curl_slist *` to a `char *`/`long` option would have
        // libcurl read the list header as a string or an integer.
        if options::option_kind(opt) != options::KIND_SLIST {
            return 0;
        }
        let blob: &[u8] = if len == 0 {
            &[]
        } else if ptr.is_null() {
            return 0;
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };

        let items = split_nul_terminated(blob);
        let Some(list) = (unsafe { build_slist(&items) }) else {
            return 0;
        };

        let code = unsafe { easy::setopt_slist(entry.curl, opt as c_int, list) };
        if code != easy::CURLE_OK {
            unsafe { easy::slist_free_all(list) };
            return 0;
        }
        // libcurl now points at `list`; only here is the old list unreachable.
        if let Some(previous) = entry.slists.insert(opt, list) {
            unsafe { easy::slist_free_all(previous) };
        }
        if opt == easy::CURLOPT_HTTPHEADER {
            entry.user_http_headers = Some(items.iter().map(|item| item.to_vec()).collect());
            entry.applied_traceparent = None;
        }
        1
    })
}

/// `multipart/form-data` uploads. Starts a fresh `curl_mime` builder for handle
/// `id`, discarding any earlier PENDING (never-posted) one. Does not touch whatever mime is
/// already ATTACHED via `CURLOPT_MIMEPOST` from an earlier successful call — see
/// `crate::mime`'s module doc for the whole pending/attached split. `0` for an unknown id
/// or a libcurl allocation failure.
#[no_mangle]
pub extern "C" fn elephc_curl_mime_new(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        (unsafe { crate::mime::new_pending(entry) }) as i32
    })
}

/// Appends a fresh, empty part to the pending builder, which becomes the target of every
/// following `elephc_curl_mime_part_field` call. `0` for an unknown id, no pending builder
/// (`elephc_curl_mime_new` was never called), or a libcurl allocation failure.
#[no_mangle]
pub extern "C" fn elephc_curl_mime_add_part(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        (unsafe { crate::mime::add_part(entry) }) as i32
    })
}

/// Sets one field on the current pending part. `kind` is one of `crate::mime`'s `FIELD_*`
/// codes (`NAME`/`DATA`/`FILEDATA`/`TYPE`/`FILENAME`); `FIELD_DATA` is read as `len`
/// binary-safe bytes, every other kind as a NUL-free byte string. `0` for an unknown id, no
/// current part (`elephc_curl_mime_add_part` was never called), an unrecognized `kind`, an
/// embedded NUL in a non-`FIELD_DATA` value, or a libcurl-level rejection.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_mime_part_field(
    id: i64,
    kind: i32,
    ptr: *const u8,
    len: usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let bytes: &[u8] = if len == 0 {
            &[]
        } else if ptr.is_null() {
            return 0;
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        (unsafe { crate::mime::set_field(entry, kind, bytes) }) as i32
    })
}

/// Attaches the pending builder to handle `id` via `CURLOPT_MIMEPOST`, completing the
/// `CURLOPT_POSTFIELDS` array walk this whole family exists for. `0` for an unknown id, no
/// pending builder, or a libcurl setopt failure — in which case the pending builder is
/// freed here and whatever was previously attached (if anything) is left in place; see
/// `crate::mime::post`.
#[no_mangle]
pub extern "C" fn elephc_curl_mime_post(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        (unsafe { crate::mime::post(entry) }) as i32
    })
}

/// Discards the pending builder without attaching it, for a PHP-level array walk that
/// failed partway through. Always `1`, including for an unknown id or when there is no
/// pending builder — this is a cleanup call, not a status query, and every other shape of
/// "nothing to do here" in this ABI is likewise not a failure.
#[no_mangle]
pub extern "C" fn elephc_curl_mime_abort(id: i64) -> i32 {
    handles::ffi_guard(1, || {
        let mut guard = handles::lock_recover(handles::handles());
        if let Some(entry) = guard.get_mut(&id) {
            crate::mime::abort(entry);
        }
        1
    })
}

/// Runs the configured transfer to completion. Resets the RETURNTRANSFER
/// capture buffer and error buffer first, so each call starts a fresh
/// capture independent of whether a previous body was taken. Returns `0` for
/// an unknown id or any non-`CURLE_OK` result; call
/// `elephc_curl_easy_errno`/`elephc_curl_easy_error` for the specific reason.
///
/// Deliberately drops the table lock before calling `curl_easy_perform`
/// (below) — the write callback needs to re-lock the table per chunk from
/// this same thread, which would deadlock otherwise. Consequence: while a
/// `perform(id)` is blocked inside libcurl, the table lock does NOT protect
/// `curl`'s underlying `*mut CURL` from a concurrent `elephc_curl_easy_free`
/// on the same `id` from another thread (see `crate::handles::EasyEntry`'s
/// `Send` impl). Sound only under this crate's caller contract: no two
/// `elephc_curl_*` calls for the same id run concurrently.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_perform(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let curl = {
            let mut guard = handles::lock_recover(handles::handles());
            let Some(entry) = guard.get_mut(&id) else {
                return 0;
            };
            unsafe { refresh_monitor_traceparent(entry) };
            entry.body.clear();
            entry.callback_threw = false;
            // Opens a fresh callback-throw scope: the process-wide gate that suppresses
            // further callbacks after a throw must not survive into this transfer.
            // Zero the error buffer first: libcurl is not guaranteed to
            // write it on success (only documented to write it on error).
            entry.error_buf.iter_mut().for_each(|byte| *byte = 0);
            entry.curl
        };
        let hooks = crate::monitoring::hooks();
        hooks.note_operation();
        let measure_wait = hooks.is_active();
        crate::callbacks::begin_transfer(measure_wait);
        // The table lock is NOT held across the blocking transfer: the write
        // callback (crate::php_layer::write_callback) re-locks the table
        // per chunk from the same thread, which would deadlock on a
        // non-reentrant `Mutex` otherwise. See this function's doc comment
        // for the resulting caller contract (no concurrent calls on `id`).
        let started = measure_wait.then(std::time::Instant::now);
        let code = unsafe { easy::perform(curl) };
        let callback_ns = crate::callbacks::finish_transfer(measure_wait);
        if let Some(started) = started {
            let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            hooks.note_wait_excluding(elapsed, callback_ns);
        }
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        if entry.callback_threw {
            // A PHP callback threw: the transfer was aborted by the exception, not by
            // libcurl, and the exception itself is what the program will see. php-src
            // reports `curl_errno() === 0` in exactly this case (measured on 8.4.20),
            // so the abort's own `CURLcode` is deliberately NOT recorded. The runtime
            // re-raises the pending throwable as soon as this returns.
            //
            // The error state is CLEARED rather than merely left alone: on a REUSED
            // handle, "left alone" means the PREVIOUS transfer's `CURLcode` and message
            // stay visible, so `curl_errno()` would answer e.g. 7 (couldn't connect) for
            // a transfer that actually ended in a PHP exception. php-src reports 0 here,
            // and 0 is what a caller inspecting the handle inside `catch` must see.
            entry.last_errno = easy::CURLE_OK;
            entry.last_error.clear();
            return 0;
        }
        entry.last_errno = code;
        entry.last_error = extract_error_message(&entry.error_buf);
        (code == easy::CURLE_OK) as i32
    })
}

/// Returns the `CURLcode` from the most recent `elephc_curl_easy_perform`
/// call on `id` (`0` = `CURLE_OK`), or `0` for an unknown id or a handle that
/// has never performed a transfer — matching PHP's `curl_errno()`.
///
/// Unlike every other status return in this ABI, this is the raw `CURLcode`,
/// not a boolean: it exists specifically to back `curl_errno()`.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_errno(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        guard.get(&id).map_or(0, |entry| entry.last_errno)
    })
}

/// Copies the most recent transfer's error message into `out`. Always
/// reports the message's byte length through `out_len`, even when `out_cap`
/// is too small (`0` return) or `id` is unknown (`out_len` set to `0`).
///
/// # Safety
/// `out` must be valid for `out_cap` bytes when non-null. `out_len` must be
/// valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_error(
    id: i64,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get(&id) else {
            write_out_len(out_len, 0);
            return 0;
        };
        unsafe { publish_bytes(&entry.last_error, out, out_cap, out_len) }
    })
}

/// Reads a `long`-typed `curl_easy_getinfo` field on handle `id` into `out`
/// (e.g. `CURLINFO_RESPONSE_CODE`, PHP's `CURLINFO_HTTP_CODE` = 2097154).
/// Returns `0` — leaving `out` untouched — for an unknown id, an `info` value
/// outside libcurl's `CURLINFO_LONG` type range (see `crate::easy::
/// CURLINFO_LONG`'s doc comment for why that check exists: `curl_easy_getinfo`
/// dispatches its output type purely from `info`'s numeric bits, so this
/// wrapper never asks it to write a `long` through a pointer typed for a
/// different shape), or a libcurl `getinfo` failure.
///
/// # Safety
/// `out` must be valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_getinfo_long(id: i64, info: i32, out: *mut i64) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get(&id) else {
            return 0;
        };
        match unsafe { easy::getinfo_long(entry.curl, info as c_int) } {
            Some(value) => {
                if !out.is_null() {
                    unsafe {
                        *out = value;
                    }
                }
                1
            }
            None => 0,
        }
    })
}

/// Reads a `double`-typed `curl_easy_getinfo` field on handle `id` into `out`
/// (`CURLINFO_TOTAL_TIME`, `CURLINFO_SPEED_DOWNLOAD`, ...). Returns `0` —
/// leaving `out` untouched — for an unknown id, an `info` outside libcurl's
/// `CURLINFO_DOUBLE` type range, or a libcurl `getinfo` failure. The type-range
/// check is the same read-side guard `elephc_curl_easy_getinfo_long` documents.
///
/// # Safety
/// `out` must be valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_getinfo_double(
    id: i64,
    info: i32,
    out: *mut f64,
) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get(&id) else {
            return 0;
        };
        match unsafe { easy::getinfo_double(entry.curl, info as c_int) } {
            Some(value) => {
                if !out.is_null() {
                    unsafe {
                        *out = value;
                    }
                }
                1
            }
            None => 0,
        }
    })
}

/// `elephc_curl_easy_str_op`'s `op`: URL-encode the string argument
/// (`curl_easy_escape`).
pub const ELEPHC_CURL_STR_OP_ESCAPE: i32 = 1;
/// `elephc_curl_easy_str_op`'s `op`: URL-decode the string argument
/// (`curl_easy_unescape`).
pub const ELEPHC_CURL_STR_OP_UNESCAPE: i32 = 2;
/// `elephc_curl_easy_str_op`'s `op`: read a `CURLINFO_STRING` field
/// (`number` = the `CURLINFO_*` value).
pub const ELEPHC_CURL_STR_OP_INFO_STRING: i32 = 3;
/// `elephc_curl_easy_str_op`'s `op`: read a `CURLINFO_SLIST` field into a
/// NUL-framed item blob (`number` = the `CURLINFO_*` value).
pub const ELEPHC_CURL_STR_OP_INFO_SLIST: i32 = 4;
/// `elephc_curl_easy_str_op`'s `op`: build the whole no-`$option`
/// `curl_getinfo()` associative array as a JSON blob.
pub const ELEPHC_CURL_STR_OP_INFO_ALL: i32 = 5;
/// `elephc_curl_easy_str_op`'s `op`: build `CURLINFO_CERTINFO` as a JSON blob.
pub const ELEPHC_CURL_STR_OP_INFO_CERTINFO: i32 = 6;

/// Runs one STRING-PRODUCING operation on handle `id` and parks its result in
/// the handle's scratch buffer, for [`elephc_curl_easy_take_scratch`] to hand
/// back. Returns `0` for an unknown id, an unknown `op`, or an operation
/// libcurl could not answer (a wrong info type, a field this transfer has no
/// value for).
///
/// WHY ONE ENTRY POINT AND NOT SIX. Every string-shaped `curl_getinfo()` answer
/// — and, from Wave D, `curl_escape`/`curl_unescape` — needs the identical
/// two-step dance: produce bytes the bridge owns, then copy them into a PHP
/// string before the next call can overwrite them. Each separate entry point
/// would need its own hand-written `__rt_curl_*` helper on three targets to do
/// exactly that. Folding them into one `op` keeps that assembly written once,
/// and the `op` codes above are the whole of the added indirection.
///
/// `ptr`/`len` carry the operation's STRING argument (unused, and expected
/// empty, by the info ops above); `number` carries its INTEGER argument (the
/// `CURLINFO_*` value for the info ops).
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_str_op(
    id: i64,
    op: i32,
    ptr: *const u8,
    len: usize,
    number: i64,
) -> i32 {
    handles::ffi_guard(0, || {
        let argument: &[u8] = if len == 0 {
            &[]
        } else if ptr.is_null() {
            return 0;
        } else {
            unsafe { std::slice::from_raw_parts(ptr, len) }
        };
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        let Ok(number) = i32::try_from(number) else {
            return 0;
        };
        let produced = match op {
            ELEPHC_CURL_STR_OP_ESCAPE => unsafe { easy::escape(entry.curl, argument) },
            ELEPHC_CURL_STR_OP_UNESCAPE => unsafe { easy::unescape(entry.curl, argument) },
            // `.flatten()`: a NULL `char *` (libcurl succeeded, this transfer has no
            // value for the field) collapses into the failure arm on purpose, so the
            // typed `curl_getinfo($ch, CURLINFO_*)` form answers `false` the way php-src
            // does rather than a fabricated empty string.
            ELEPHC_CURL_STR_OP_INFO_STRING => {
                unsafe { easy::getinfo_str(entry.curl, number) }.flatten()
            }
            ELEPHC_CURL_STR_OP_INFO_SLIST => unsafe { easy::getinfo_slist(entry.curl, number) }
                .map(|items| info::frame_items(&items)),
            ELEPHC_CURL_STR_OP_INFO_ALL => Some(unsafe { info::getinfo_all_json(entry.curl) }),
            ELEPHC_CURL_STR_OP_INFO_CERTINFO => Some(unsafe { info::certinfo_json(entry.curl) }),
            _ => None,
        };
        match produced {
            Some(bytes) => {
                entry.scratch = bytes;
                1
            }
            None => 0,
        }
    })
}

/// Hands back the bytes the last [`elephc_curl_easy_str_op`] on this `id`
/// produced, as a pointer/length pair valid until the next `elephc-curl` call
/// that touches this `id` — the same "borrowed until overwritten" convention
/// [`elephc_curl_easy_take_body`] uses, and the reason the caller copies
/// immediately. Returns `0` for an unknown id.
///
/// # Safety
/// `ptr` and `len` must be valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_take_scratch(
    id: i64,
    ptr: *mut *mut u8,
    len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            write_out_len(len, 0);
            return 0;
        };
        write_out_len(len, entry.scratch.len());
        if !ptr.is_null() {
            unsafe {
                *ptr = entry.scratch.as_mut_ptr();
            }
        }
        1
    })
}

/// Publishes the RETURNTRANSFER-captured body as a pointer/length pair through
/// `ptr`/`len`. The returned pointer stays valid until the next `elephc-curl`
/// call that touches this `id` (`elephc_curl_easy_take_body` again,
/// `elephc_curl_easy_perform`, or `elephc_curl_easy_free`) — the same
/// "borrowed until overwritten" convention `elephc-pdo`'s
/// `store_bytes`/`elephc-image`'s `out_cell` use; the caller is expected to copy
/// the bytes out immediately. Returns `0` for an unknown id.
///
/// READING THE BODY DOES NOT CONSUME IT, and that is php-src's semantics rather
/// than an accident: both `curl_exec()` (with `RETURNTRANSFER`) and
/// `curl_multi_getcontent()` end in `RETURN_STR_COPY(ch->handlers.write->buf.s)`,
/// which copies the buffer and leaves it in place, so calling
/// `curl_multi_getcontent()` twice answers the same body twice. The capture
/// buffer is reset where php-src resets it instead: at the start of every
/// `elephc_curl_easy_perform`, and in `elephc_curl_multi_add` (php-src's
/// `_php_curl_cleanup_handle` on `curl_multi_add_handle`).
///
/// # Safety
/// `ptr` and `len` must be valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_take_body(
    id: i64,
    ptr: *mut *mut u8,
    len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            write_out_len(len, 0);
            return 0;
        };
        // A COPY, not a move: see this function's doc comment — php-src hands
        // the body back without clearing the capture buffer, so a second
        // `curl_multi_getcontent()` must see the same bytes.
        entry.taken_body = entry.body.clone();
        write_out_len(len, entry.taken_body.len());
        if !ptr.is_null() {
            unsafe {
                *ptr = entry.taken_body.as_mut_ptr();
            }
        }
        1
    })
}

/// Installs, replaces, or clears one of `curl_setopt()`'s callback options on handle
/// `id`. Returns `1` on success and `0` for an unknown id or an out-of-range `slot`.
///
/// `slot` is a `crate::callbacks::SLOT_*` index, NOT a `CURLOPT_*` number: the mapping
/// from option number to slot lives in the curl prelude, next to the rest of the
/// option dispatch. A null `descriptor` clears the slot (PHP `null` restores the
/// option's default behavior); a non-null one replaces whatever was there.
///
/// The three pointers are the "decompose the callable at the PHP layer" triple this
/// bridge shares with `elephc-pdo`: `descriptor` is the normalized callable's
/// descriptor record, `self_obj` is the `CurlHandle` object to pass as `$ch`, and
/// `adapter` is the address of the codegen-emitted `__rt_curl_invoke_callback`. None of
/// them is owned here — see `crate::callbacks`'s module doc for the ownership argument
/// (in particular why `self_obj` MUST stay a non-owning back-pointer).
///
/// # Safety
/// `descriptor` must be null or a live callable descriptor rooted on the PHP side for
/// at least as long as it stays installed; `self_obj` must be the `CurlHandle` object
/// that owns `id`; `adapter` must be null or the address `__elephc_curl_adapter_addr()`
/// produced.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_easy_set_callback(
    id: i64,
    slot: i32,
    descriptor: *mut c_void,
    self_obj: *mut c_void,
    adapter: *const c_void,
) -> i32 {
    handles::ffi_guard(0, || {
        if slot < 0 || slot as usize >= crate::callbacks::SLOT_COUNT {
            return 0;
        }
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        let value = if descriptor.is_null() {
            crate::callbacks::CallbackSlot::empty()
        } else {
            crate::callbacks::CallbackSlot {
                descriptor,
                self_obj,
                adapter,
            }
        };
        crate::callbacks::apply_callback(entry, slot as usize, value);
        1
    })
}

/// Resets every libcurl option on handle `id` to its default and clears the
/// PHP-layer state that goes with them, matching PHP's `curl_reset()`. Returns
/// `0` for an unknown id.
///
/// THREE THINGS MUST BE PUT BACK AFTERWARDS, because `curl_easy_reset` resets
/// OPTIONS and elephc's own plumbing is made of options: the write callback and
/// its `userdata` (without them a later `curl_exec()` would write the body to
/// libcurl's default stdout and the RETURNTRANSFER capture would silently stop
/// working), and `CURLOPT_ERRORBUFFER` (without it `curl_error()` would report
/// nothing forever after). The handle's slists are freed here too — libcurl
/// dropped its pointers to them in the reset, so this is the first moment they
/// are unreachable.
///
/// **`CURLOPT_SHARE`/`entry.share_id` ARE DELIBERATELY LEFT UNTOUCHED.** `curl_easy_reset`
/// resets `data->set` (libcurl's easy-handle OPTIONS struct); `data->share` lives OUTSIDE
/// it (pinned libcurl 8.21.0, `lib/easy.c:1089`), so `curl_easy_reset()` does NOT clear the
/// share attachment — confirmed against real PHP too: `curl_share_setopt()` on a share
/// still reports it in use after `curl_reset($ch)` on an attached handle. An EARLIER
/// version of this function called `crate::share::detach_easy` here, which desynced this
/// crate's `attached` bookkeeping from libcurl's real internal state: the share believed
/// itself unattached while libcurl still held a live reference, so a later
/// `elephc_curl_share_free` would either call `curl_share_cleanup()` too early (getting
/// `CURLSHE_IN_USE` back — a discarded return value made this a SILENT, PERMANENT leak of
/// the share, not a crash — see `crate::share`'s module doc) or, under the since-removed
/// forced-unlink design, sever a link libcurl itself still needed. Leaving the bookkeeping
/// alone here is what keeps it correct.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_reset(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get_mut(&id) else {
            return 0;
        };
        unsafe { easy::reset(entry.curl) };
        entry.free_slists();
        entry.free_mime();
        // php-src's `curl_reset()` releases the handler callables too (measured: a
        // write callback installed before `curl_reset()` never fires afterwards).
        // `curl_easy_reset` has already dropped libcurl's own registrations, so this
        // only has to forget the slots and re-apply the (now empty) registration set.
        crate::callbacks::clear_all(entry);
        entry.return_transfer = false;
        entry.callback_threw = false;
        entry.body.clear();
        entry.taken_body.clear();
        entry.scratch.clear();
        entry.last_errno = easy::CURLE_OK;
        entry.last_error.clear();
        entry.error_buf.iter_mut().for_each(|byte| *byte = 0);
        unsafe {
            php_layer::install_write_callback(entry.curl, id);
            crate::callbacks::install_read_callback(entry.curl, id);
            easy::setopt_ptr(
                entry.curl,
                easy::CURLOPT_ERRORBUFFER,
                entry.error_buf.as_mut_ptr() as *mut c_void,
            );
        }
        // RE-RUN, because `curl_easy_reset` restored libcurl's own defaults — including
        // the dead baked-in `CURL_CA_BUNDLE` path a handle running on a foreign machine
        // must not be left with. php-src documents `curl_reset()` as giving back a handle
        // "as if freshly created", and a freshly created handle here carries the
        // discovered bundle.
        apply_discovered_cainfo(entry.curl);
        1
    })
}

/// Applies a `CURLPAUSE_*` bitmask to handle `id`, returning libcurl's raw
/// `CURLcode` — which is what PHP's `curl_pause()` returns. An unknown id
/// answers `CURLE_BAD_FUNCTION_ARGUMENT` (43) rather than `CURLE_OK`, so a
/// caller cannot read "nothing happened" as success.
///
/// LIKE `elephc_curl_easy_perform`, THIS DROPS THE TABLE LOCK BEFORE CALLING LIBCURL.
/// `CURLPAUSE_CONT` does not merely flip a flag: libcurl flushes whatever it buffered
/// while paused, which runs the write trampoline, which re-locks this same table from this
/// same thread. Holding the guard across `curl_easy_pause` was therefore a self-deadlock
/// on a non-reentrant `Mutex` — and, once a PHP write callback is installed, it also runs
/// compiled PHP, which may re-enter this crate on other handles. The same caller contract
/// `elephc_curl_easy_perform` documents applies (no two `elephc_curl_*` calls for the same
/// id concurrently).
#[no_mangle]
pub extern "C" fn elephc_curl_easy_pause(id: i64, bitmask: i32) -> i32 {
    handles::ffi_guard(CURLE_BAD_FUNCTION_ARGUMENT, || {
        let curl = {
            let guard = handles::lock_recover(handles::handles());
            let Some(entry) = guard.get(&id) else {
                return CURLE_BAD_FUNCTION_ARGUMENT;
            };
            entry.curl
        };
        unsafe { easy::pause(curl, bitmask as c_int) }
    })
}

/// `CURLE_BAD_FUNCTION_ARGUMENT` (43): the `CURLcode`
/// `elephc_curl_easy_pause` reports for an unknown handle id.
const CURLE_BAD_FUNCTION_ARGUMENT: i32 = 43;

/// Runs `curl_easy_upkeep` on handle `id`. Returns `1` only when libcurl
/// reported `CURLE_OK`, matching PHP's `curl_upkeep(): bool`.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_upkeep(id: i64) -> i32 {
    handles::ffi_guard(0, || {
        let guard = handles::lock_recover(handles::handles());
        let Some(entry) = guard.get(&id) else {
            return 0;
        };
        let hooks = crate::monitoring::hooks();
        (hooks.timed(|| unsafe { easy::upkeep(entry.curl) }) == easy::CURLE_OK) as i32
    })
}

/// Duplicates handle `id` — libcurl options AND the bridge's own PHP-layer
/// state — and registers the copy under a fresh id, which it returns. `0` for
/// an unknown id or a libcurl allocation failure.
///
/// **THE COPY STARTS WITH A CLEAN TRANSFER RECORD: NO CAPTURED BODY, NO
/// `curl_errno()`, NO `curl_error()`.** Only the OPTIONS travel — including
/// `CURLOPT_RETURNTRANSFER`, which is an option (php-src's `curl_copy_handle`
/// copies its write METHOD, so the copy keeps `curl_exec()`'s return shape) —
/// never the RESULT of a transfer the copy never ran. Measured on PHP 8.4.20 /
/// libcurl 8.19.0:
///
/// ```text
/// $ch = curl_init(); curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
/// curl_setopt($ch, CURLOPT_URL, "http://127.0.0.1:1/nope"); curl_exec($ch);
/// // orig: curl_errno 7, curl_error "Failed to connect …"
/// $copy = curl_copy_handle($ch);
/// // copy: curl_errno 0, curl_error "", curl_multi_getcontent "" — and the same
/// // three answers after a SUCCESSFUL 549-byte transfer on the original.
/// ```
///
/// An earlier version of this function copied `body`/`last_errno`/`last_error`
/// onto the duplicate (the original plan mandated carrying the captured body
/// across), which made `curl_multi_getcontent($copy)`/`curl_errno($copy)`/
/// `curl_error($copy)` report the ORIGINAL's last transfer before the copy had
/// performed one — the state php-src leaves untouched in its freshly `ecalloc`'d
/// `php_curl` (`_php_setup_easy_copy_handlers` copies handlers and options, not
/// `err.no`/`err.str`/the write buffer).
///
/// THE COPY'S CALLBACK PLUMBING IS REINSTALLED, not inherited.
/// `curl_easy_duphandle` copies every option VALUE, which means the copy would
/// otherwise point `CURLOPT_WRITEDATA` at the ORIGINAL handle's id (so the
/// copy's response body would land in the original's capture buffer) and
/// `CURLOPT_ERRORBUFFER` at the original entry's buffer (a dangling write the
/// moment the original is freed). Both are re-pointed at the new entry here.
///
/// EVERY LIST OPTION IS REBUILT FROM SCRATCH FOR THE COPY, and that is a
/// USE-AFTER-FREE FIX, not tidiness. `curl_easy_duphandle` does NOT duplicate
/// `struct curl_slist *` options: `dupset` (libcurl 8.21.0, `lib/easy.c`) starts
/// with a shallow `dst->set = src->set` and then re-duplicates only the strings,
/// the blobs, `CURLOPT_COPYPOSTFIELDS` and the mime part — every slist pointer
/// rides the struct copy verbatim, because libcurl treats slists as
/// APPLICATION-owned. So a copy that inherited the pointer would be reading a
/// list this bridge owns and frees on the SOURCE's behalf:
///
/// ```text
/// $a = curl_init(...); curl_setopt($a, CURLOPT_HTTPHEADER, [...]);
/// $b = curl_copy_handle($a);
/// unset($a);            // EasyEntry::free_slists frees the list
/// curl_exec($b);        // libcurl walks freed memory
/// ```
///
/// `curl_reset($a)` and simply setting `CURLOPT_HTTPHEADER` on `$a` again reach
/// the same dangling read. Copying the bridge's own map would be worse still —
/// two entries owning one list, hence a double free. Rebuilding is the only
/// shape where each handle owns exactly what it points at.
///
/// THE MIME PART NEEDS NO REBUILD, UNLIKE SLISTS — it is in the OTHER bucket
/// `dupset` re-duplicates (alongside the strings, the blobs and `CURLOPT_COPYPOSTFIELDS`),
/// so the copy already has its own independent `curl_mime` structure the moment
/// `easy::duphandle` returns, before any Rust code here runs. The new `EasyEntry` below
/// deliberately leaves `mime`/`pending_mime` at their `EasyEntry::new` default of `None`
/// rather than tracking a pointer to that duplicate: `curl_easy_duphandle` hands back no
/// way to READ the option it just copied (there is no `curl_easy_getinfo` for
/// `CURLOPT_MIMEPOST`), so this bridge never legitimately holds a pointer to the
/// duplicate and therefore never calls `curl_mime_free` on it either — freeing a pointer
/// obtained by any other means here would not be sound. Whatever `curl_easy_cleanup(copy)`
/// eventually does with libcurl's own internally-duplicated structure is between the
/// duplicate handle and libcurl; this bridge's `mime` bookkeeping continues to track only
/// what IT itself built via `elephc_curl_mime_new`/`_post` on this (or any other) id.
#[no_mangle]
pub extern "C" fn elephc_curl_easy_duphandle(id: i64) -> i64 {
    handles::ffi_guard(0, || {
        let mut guard = handles::lock_recover(handles::handles());

        // Snapshot everything the copy needs while the source is borrowed, list
        // options included: `easy::read_slist` walks each list into owned bytes
        // so the rebuild below never reads the source's memory again. The
        // source's `body`/`last_errno`/`last_error` are deliberately NOT among
        // them — see this function's doc comment for the measurement.
        let (copied, return_transfer, slist_items) = {
            let Some(source) = guard.get(&id) else {
                return 0;
            };
            let slist_items: Vec<(i32, Vec<Vec<u8>>)> = source
                .slists
                .iter()
                .map(|(&opt, &list)| {
                    let items = if opt == easy::CURLOPT_HTTPHEADER {
                        source.user_http_headers.clone().unwrap_or_default()
                    } else {
                        unsafe { easy::read_slist(list) }
                    };
                    (opt, items)
                })
                .collect();
            let copied = unsafe { easy::duphandle(source.curl) };
            if copied.is_null() {
                return 0;
            }
            (copied, source.return_transfer, slist_items)
        };

        let new_id = handles::next_id();
        let mut entry = EasyEntry::new(copied, new_id);
        entry.return_transfer = return_transfer;
        // NO CA DISCOVERY RUNS FOR THE COPY, and none is needed: `curl_easy_duphandle`
        // copies every string option, so whatever CA file the source carried — user-set or
        // discovered — is already on the copy as its own independent allocation
        // (`dupset`'s `Curl_setstropt` loop, pinned libcurl 8.21.0 `lib/easy.c:891`).
        // Re-running discovery here would be actively WRONG: a copy of a handle whose PHP
        // program had set `CURLOPT_CAINFO` itself would have that value overwritten by
        // this bridge's guess.
        //
        // THE PROXY BUNDLE TRAVELS BY THE SAME MECHANISM. `CURLOPT_PROXY_CAINFO` lives at
        // `STRING_SSL_CAFILE_PROXY`, which is an ordinary entry in the SAME
        // `0..STRING_LASTZEROTERMINATED` range `dupset`'s loop walks, so the copy gets its
        // own `strdup` of it exactly like the non-proxy one. Nothing hop-specific has to be
        // rebuilt here; the shallow `dst->set = src->set` even carries libcurl's own
        // `proxy_ssl.custom_cafile` bit across, so the copy answers a later
        // `CURLOPT_PROXY_CAPATH` the same way the source would.
        unsafe {
            php_layer::install_write_callback(copied, new_id);
            crate::callbacks::install_read_callback(copied, new_id);
            easy::setopt_ptr(
                copied,
                easy::CURLOPT_ERRORBUFFER,
                entry.error_buf.as_mut_ptr() as *mut c_void,
            );
            // CURLOPT_SHARE IS EXPLICITLY CLEARED ON THE COPY, not inherited — AND THAT IS
            // PHP'S OWN ANSWER, not merely this bridge's convenience. The question was
            // reopened by a punch-list item claiming php-src re-applies `ch->share` to the
            // duplicate; MEASURED on PHP 8.4.20 / libcurl 8.19.0, it does not, at either
            // level:
            //
            // ```text
            // $sh = curl_share_init(); curl_share_setopt($sh, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
            // $ch = curl_init(); curl_setopt($ch, CURLOPT_SHARE, $sh);
            // $copy = curl_copy_handle($ch); unset($ch);
            // curl_share_setopt($sh, CURLSHOPT_UNSHARE, CURL_LOCK_DATA_DNS);  // true
            // ```
            //
            // libcurl answers `CURLSHE_IN_USE` (PHP `false`) from `curl_share_setopt` for as
            // long as ANY easy handle is attached (`share->dirty`), so a `true` there means
            // the surviving copy holds no attachment. The probe is not a false negative: with
            // two REAL attached handles, freeing one still answers `false`, and PHP's
            // `WeakReference` shows the copy does not even hold a PHP-level reference to the
            // share object (it is destroyed by `unset($ch)` while `$copy` is alive), so
            // php-src's `_php_setup_easy_copy_handlers` carries no `share` across either.
            // A copy therefore starts genuinely unattached in PHP, exactly as it does here.
            //
            // Clearing the option is also what keeps this bridge's own bookkeeping honest —
            // the same "re-point rather than trust the copied value" rule this function
            // already applies to WRITEDATA/ERRORBUFFER: whether `curl_easy_duphandle` itself
            // carries the raw `CURLSH *` pointer (and its refcount) across is an internal
            // libcurl detail this bridge does not rely on either way. The new entry's
            // `share_id` field defaults to `None`
            // (`EasyEntry::new`), so setting the option to null here keeps the bridge's
            // `attached`-list bookkeeping and libcurl's real refcount on the share in
            // agreement by construction: the copy starts genuinely unattached. Without
            // this, a copy that silently kept a live libcurl-level reference the bridge
            // never counted would make `crate::share`'s `attached` list UNDERCOUNT real
            // attachments — the deferred-free design (`crate::share`'s module doc) could
            // then believe a share is safe to actually clean up (the ORIGINAL's own detach
            // emptied the list) while the copy still references it, and
            // `curl_share_cleanup()` would answer `CURLSHE_IN_USE` instead of `CURLSHE_OK`
            // — caught loudly by `crate::share::finish_share_cleanup`'s `debug_assert_eq!`,
            // but a bug this avoids having at all.
            easy::setopt_ptr(copied, easy::CURLOPT_SHARE, std::ptr::null_mut());
        }

        // EVERY CALLBACK REGISTRATION IS CLEARED ON THE COPY, for the reason that makes
        // this the most dangerous thing `curl_easy_duphandle` does: libcurl's `dupset`
        // carries the callback FUNCTION POINTERS *and* their `CURLOPT_*DATA` values
        // across verbatim — and every one of those data values is the ORIGINAL handle's
        // bridge id. A copy left as libcurl made it would fire the original's PHP
        // callables, with the original's `CurlHandle` as `$ch`, for the COPY's transfers.
        // (`install_write_callback` above is the same fix for `CURLOPT_WRITEDATA`.)
        // The curl prelude re-registers the copy's callbacks against the COPY's object
        // immediately afterwards, which is also what php-src's `curl_copy_handle` does
        // when it re-points its own handler struct at the new `php_curl`.
        crate::callbacks::clear_all(&mut entry);

        for (opt, items) in slist_items {
            // A rebuild this build cannot complete must CLEAR the option on the
            // copy, never leave it pointing at the source's list: the whole point
            // of this loop is that the inherited pointer is not safe to keep.
            let rebuilt = unsafe { build_slist(&items) }.unwrap_or(std::ptr::null_mut());
            let code = unsafe { easy::setopt_slist(copied, opt as c_int, rebuilt) };
            if code == easy::CURLE_OK {
                entry.slists.insert(opt, rebuilt);
                if opt == easy::CURLOPT_HTTPHEADER {
                    entry.user_http_headers = Some(items);
                }
            } else {
                unsafe {
                    easy::slist_free_all(rebuilt);
                    easy::setopt_slist(copied, opt as c_int, std::ptr::null_mut());
                }
            }
        }

        guard.insert(new_id, entry);
        new_id
    })
}

/// Copies libcurl's own message for a `CURLcode` into `out`, for PHP's
/// `curl_strerror()`. Handle-free: a `CURLcode`'s text does not depend on any
/// transfer. Always reports the message length through `out_len`, even when
/// `out_cap` is too small (`0` return).
///
/// # Safety
/// `out` must be valid for `out_cap` bytes when non-null. `out_len` must be
/// valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_strerror(
    code: i32,
    out: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let message = easy::strerror(code as c_int);
        unsafe { publish_bytes(&message, out, out_cap, out_len) }
    })
}

/// Removes handle `id` from the table and runs `curl_easy_cleanup` on it.
/// A no-op for an unknown/already-freed id (ids are never reused, so a
/// double-free is harmless here, unlike a raw libcurl double-cleanup).
///
/// Deliberately drops the table lock before calling `curl_easy_cleanup`
/// (below), matching `elephc_curl_easy_perform`'s reason for the same
/// pattern. Consequence: this removes `id` from the table (safe under the
/// mutex — nothing else can look `id` up again after this returns), but if
/// another thread is mid-`elephc_curl_easy_perform(id)` when this runs, that
/// thread's already-copied `*mut CURL` is freed out from under it — a real
/// use-after-free at the libcurl level. Sound only under this crate's caller
/// contract: no two `elephc_curl_*` calls for the same id run concurrently
/// (see `crate::handles::EasyEntry`'s `Send` impl for the full reasoning).
#[no_mangle]
pub extern "C" fn elephc_curl_easy_free(id: i64) {
    handles::ffi_guard((), || {
        let mut guard = handles::lock_recover(handles::handles());
        if let Some(mut entry) = guard.remove(&id) {
            // Do not hold the table lock across `curl_easy_cleanup`.
            drop(guard);
            unsafe { easy::cleanup(entry.curl) };
            // AFTER cleanup, never before: libcurl holds raw pointers to the
            // handle's slist options until the handle itself is gone.
            entry.free_slists();
            // Same ordering, same reason: freed only after `curl_easy_cleanup` has run.
            entry.free_mime();
            // Remove this id from its share's `attached` bookkeeping (if any). Called
            // AFTER `curl_easy_cleanup()` above, which is what actually releases libcurl's
            // own reference on the share (`crate::share`'s module doc — `data->share` is
            // untouched by `curl_easy_reset`, so this and a real `curl_setopt()`
            // re-attachment are the ONLY two places an attachment ever legitimately ends).
            // THIS IS HOW A DEFERRED SHARE FREE COMPLETES: if `elephc_curl_share_free` ran
            // earlier while this id was still attached, the share is sitting in the table
            // marked pending-free, and THIS call — should it empty the share's `attached`
            // list — is what finally runs the real `curl_share_cleanup()`
            // (`crate::share::detach_easy`'s own doc comment).
            if let Some(share_id) = entry.share_id {
                crate::share::detach_easy(share_id, id);
            }
        }
    })
}

/// Writes the `curl_version()` JSON blob (libcurl's own
/// `curl_version_info(CURLVERSION_NOW)`, not tied to any easy handle) into
/// `out_json`. Always reports the required byte length through `len`, even
/// when `cap` is too small (`0` return).
///
/// # Safety
/// `out_json` must be valid for `cap` bytes when non-null. `len` must be
/// valid for a write when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_curl_global_info(
    out_json: *mut u8,
    cap: usize,
    len: *mut usize,
) -> i32 {
    handles::ffi_guard(0, || {
        let json = build_global_info_json();
        unsafe { publish_bytes(json.as_bytes(), out_json, cap, len) }
    })
}

/// Splits a blob of NUL-TERMINATED items into its items, dropping a trailing
/// empty tail. `b""` is zero items; `b"\0"` is one empty item; `b"a\0b\0"` is
/// two. See [`elephc_curl_easy_setopt_slist`] for why this framing rather than
/// a separator.
fn split_nul_terminated(blob: &[u8]) -> Vec<&[u8]> {
    let mut items = Vec::new();
    let mut rest = blob;
    while let Some(end) = rest.iter().position(|&byte| byte == 0) {
        items.push(&rest[..end]);
        rest = &rest[end + 1..];
    }
    // Anything after the last NUL is an unterminated fragment the caller did
    // not frame; the prelude always terminates, so this only guards a caller
    // that does not.
    if !rest.is_empty() {
        items.push(rest);
    }
    items
}

/// Builds a fresh `struct curl_slist` from owned byte items, or `None` when an item
/// carries an embedded NUL (which would silently truncate it) or libcurl could not
/// allocate — freeing whatever had been built in that case, so a failure leaks nothing
/// and hands back no half-list.
///
/// An EMPTY item list yields a null list, which is not a failure: `curl_easy_setopt` with
/// a null `struct curl_slist *` is how an option is CLEARED, and that is exactly what
/// `curl_setopt($ch, CURLOPT_HTTPHEADER, [])` means.
///
/// # Safety
/// The returned list, when non-null, is the caller's to free with
/// `easy::slist_free_all` once no live easy handle still points at it.
unsafe fn build_slist(items: &[impl AsRef<[u8]>]) -> Option<*mut easy::CurlSlist> {
    let mut list: *mut easy::CurlSlist = std::ptr::null_mut();
    for item in items {
        let Ok(item) = CString::new(item.as_ref()) else {
            unsafe { easy::slist_free_all(list) };
            return None;
        };
        match unsafe { easy::slist_append(list, item.as_ptr()) } {
            Some(next) => list = next,
            None => {
                unsafe { easy::slist_free_all(list) };
                return None;
            }
        }
    }
    Some(list)
}

/// Returns whether a user header explicitly owns the traceparent field.
fn is_traceparent_header(header: &[u8]) -> bool {
    let Some(delimiter) = header
        .iter()
        .position(|byte| matches!(*byte, b':' | b';'))
    else {
        return false;
    };
    let name = &header[..delimiter];
    let start = name
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(name.len());
    let end = name
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index + 1);
    name[start..end].eq_ignore_ascii_case(b"traceparent")
}

/// Builds the effective HTTP header list and reports the automatic trace value
/// represented by it. An explicit user traceparent always wins.
#[cfg(test)]
fn effective_http_headers(
    user_headers: &[Vec<u8>],
    traceparent: Option<String>,
) -> (Vec<Vec<u8>>, Option<String>) {
    let injected = automatic_traceparent(user_headers, traceparent);
    let mut effective = user_headers.to_vec();
    if let Some(value) = injected.as_deref() {
        let mut header = b"traceparent: ".to_vec();
        header.extend_from_slice(value.as_bytes());
        effective.push(header);
    }
    (effective, injected)
}

/// Resolves the automatic trace value after applying user-header precedence.
fn automatic_traceparent(
    user_headers: &[Vec<u8>],
    traceparent: Option<String>,
) -> Option<String> {
    if user_headers
        .iter()
        .any(|header| is_traceparent_header(header))
    {
        None
    } else {
        traceparent
    }
}

/// Refreshes the automatic traceparent on an easy handle before a transfer.
///
/// The applied slist is rebuilt only when the propagated value changes. User
/// headers remain authoritative, and a failed allocation or setopt leaves the
/// previous live list untouched.
///
/// # Safety
/// `entry.curl` must be a live easy handle, and no transfer may be using its
/// current HTTP header list concurrently.
pub(crate) unsafe fn refresh_monitor_traceparent(entry: &mut EasyEntry) {
    let user_headers = entry.user_http_headers.as_deref().unwrap_or(&[]);
    let injected = automatic_traceparent(user_headers, crate::monitoring::traceparent());
    if entry.applied_traceparent == injected {
        return;
    }
    let mut headers = user_headers.to_vec();
    if let Some(value) = injected.as_deref() {
        let mut header = b"traceparent: ".to_vec();
        header.extend_from_slice(value.as_bytes());
        headers.push(header);
    }
    let Some(list) = (unsafe { build_slist(&headers) }) else {
        return;
    };
    // An easy handle may already be attached to a multi handle here. Attachment does
    // not start a transfer or make its options immutable: this refresh runs before
    // `curl_multi_perform`, while neither libcurl nor a callback can be walking the old
    // list. The accepted replacement becomes the live option before the old owned list
    // is freed below.
    let code = unsafe { easy::setopt_slist(entry.curl, easy::CURLOPT_HTTPHEADER, list) };
    if code != easy::CURLE_OK {
        unsafe { easy::slist_free_all(list) };
        return;
    }
    if let Some(previous) = entry.slists.insert(easy::CURLOPT_HTTPHEADER, list) {
        unsafe { easy::slist_free_all(previous) };
    }
    entry.applied_traceparent = injected;
}

#[cfg(test)]
mod monitoring_header_tests {
    use super::{effective_http_headers, is_traceparent_header};

    /// Verifies traceparent header-name matching is whitespace tolerant and
    /// case insensitive without accepting similarly prefixed names.
    #[test]
    fn detects_explicit_traceparent_headers() {
        assert!(is_traceparent_header(b"TraceParent: 00-value"));
        assert!(is_traceparent_header(b" traceparent :"));
        assert!(is_traceparent_header(b"traceparent;"));
        assert!(!is_traceparent_header(b"x-traceparent: value"));
        assert!(!is_traceparent_header(b"traceparent"));
    }

    /// Verifies automatic propagation appends one header while preserving the
    /// original user header sequence.
    #[test]
    fn appends_automatic_traceparent() {
        let user = vec![b"accept: application/json".to_vec()];
        let trace = "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01".to_owned();
        let (headers, injected) = effective_http_headers(&user, Some(trace.clone()));
        assert_eq!(injected, Some(trace.clone()));
        assert_eq!(headers[0], user[0]);
        assert_eq!(headers[1], format!("traceparent: {trace}").into_bytes());
    }

    /// Verifies an explicit user traceparent suppresses automatic propagation.
    #[test]
    fn preserves_explicit_traceparent() {
        let user = vec![b"TRACEPARENT: user-value".to_vec()];
        let automatic = "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01".to_owned();
        let (headers, injected) = effective_http_headers(&user, Some(automatic));
        assert_eq!(headers, user);
        assert_eq!(injected, None);
    }
}

/// Applies `CURLOPT_POSTFIELDS` as libcurl's copying, length-aware pair:
/// `CURLOPT_POSTFIELDSIZE_LARGE` first (so libcurl knows how many bytes to
/// take instead of calling `strlen` and truncating at the first NUL), then
/// `CURLOPT_COPYPOSTFIELDS` (so libcurl owns the bytes and the PHP string that
/// supplied them can die immediately).
///
/// php-src does exactly this in `_php_curl_setopt`, and for the same two
/// reasons: `CURLOPT_POSTFIELDS` alone would leave libcurl holding a borrowed
/// pointer into a PHP string.
///
/// # Safety
/// `entry.curl` must be a still-live handle. `body` is only read for the
/// duration of the call; libcurl copies it.
unsafe fn set_postfields(entry: &mut EasyEntry, body: &[u8]) -> bool {
    let size = unsafe {
        easy::setopt_off_t(
            entry.curl,
            easy::CURLOPT_POSTFIELDSIZE_LARGE,
            body.len() as i64,
        )
    };
    if size != easy::CURLE_OK {
        return false;
    }
    // A zero-length body still needs a non-null pointer: libcurl treats null
    // as "unset the option" rather than "post nothing".
    let ptr = if body.is_empty() {
        c"".as_ptr()
    } else {
        body.as_ptr() as *const c_char
    };
    let code = unsafe { easy::setopt_str(entry.curl, easy::CURLOPT_COPYPOSTFIELDS, ptr) };
    code == easy::CURLE_OK
}

/// Builds a NUL-free `CString` from a caller-supplied byte pointer/length, or
/// `None` for a null pointer with nonzero length, or embedded NUL bytes.
///
/// # Safety
/// `ptr` must be valid for `len` bytes when non-null.
unsafe fn bytes_to_cstring(ptr: *const u8, len: usize) -> Option<CString> {
    if len == 0 {
        return CString::new(Vec::new()).ok();
    }
    if ptr.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
    CString::new(bytes).ok()
}

/// Extracts the NUL-terminated message libcurl wrote into a
/// `CURLOPT_ERRORBUFFER` buffer, up to (and excluding) the first NUL byte.
fn extract_error_message(buf: &[u8]) -> Vec<u8> {
    let end = buf.iter().position(|&byte| byte == 0).unwrap_or(buf.len());
    buf[..end].to_vec()
}

/// Writes `value` through `ptr` when non-null. Null-safe counterpart to the
/// `out_len`/`len` out-parameters several entry points above take.
unsafe fn write_out_len(ptr: *mut usize, value: usize) {
    if !ptr.is_null() {
        unsafe {
            *ptr = value;
        }
    }
}

/// Copies `bytes` into a caller-owned buffer, reporting the required length
/// through `out_len` unconditionally (even on failure, so the caller can
/// retry with a larger buffer) — mirrors `elephc-crypto`'s
/// `cipher::abi::publish_decrypt`. Returns `1` on success, `0` when `out_cap`
/// is too small or `out_ptr` is null for a nonempty result.
///
/// # Safety
/// `out_ptr` must be valid for `out_cap` bytes when non-null. `out_len` must
/// be valid for a write when non-null.
pub(crate) unsafe fn publish_bytes(
    bytes: &[u8],
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    unsafe {
        write_out_len(out_len, bytes.len());
    }
    if bytes.len() > out_cap || (!bytes.is_empty() && out_ptr.is_null()) {
        return 0;
    }
    if !bytes.is_empty() {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, bytes.len());
        }
    }
    1
}

/// Builds the `curl_version()` JSON blob from libcurl's own
/// `curl_version_info(CURLVERSION_NOW)`, key for key AND IN PHP'S OWN KEY ORDER —
/// a PHP array is ordered, and `serde_json`'s `preserve_order` feature (see this
/// crate's `Cargo.toml`) is what carries the insertion order below through the
/// blob and into the decoded array. Measured against PHP 8.4.20, the order is
/// `version_number, age, features, feature_list, ssl_version_number, version,
/// host, ssl_version, libz_version, protocols`, then the age-gated tail.
///
/// EVERY KEY EXCEPT THE AGE-GATED TAIL IS UNCONDITIONAL, including `ssl_version`
/// and `libz_version` for a build with no TLS or no zlib. php-src adds all four
/// string fields through one macro,
/// `#define CAAS(s, v) add_assoc_string_ex(return_value, s, sizeof(s) - 1, (char *) (v ? v : ""))`
/// (`ext/curl/interface.c`), so a null pointer becomes `""` and the key is still
/// there. (An earlier revision of this comment cited `if (d->ssl_version)` as
/// php's rule for those two; that line is from `PHP_MINFO_FUNCTION` — the
/// phpinfo() table — NOT from `PHP_FUNCTION(curl_version)`.)
///
/// THE SUB-LIBRARY FIELDS ARE GATED ON THE STRUCT'S `age`, NOT ON WHETHER THE
/// POINTER IS NULL, because that is php-src's rule and the two disagree for
/// every library this libcurl was built without. `PHP_FUNCTION(curl_version)`
/// (`ext/curl/interface.c`) adds `ares`/`ares_num` at `CURLVERSION_SECOND`,
/// `libidn` at `CURLVERSION_THIRD`, `iconv_ver_num`/`libssh_version` at
/// `CURLVERSION_FOURTH` and `brotli_ver_num`/`brotli_version` at
/// `CURLVERSION_FIFTH`, reporting a null string as `""` rather than dropping the
/// key. Measured on PHP 8.4.20 (age 11, no c-ares, no libidn, no libssh):
/// `ares => ''`, `ares_num => 0`, `libidn => ''`, `iconv_ver_num => 0`,
/// `libssh_version => ''` are ALL present. The previous null-pointer gating cost
/// `iconv_ver_num` on every build — it hung off `libssh_version`'s pointer, and
/// no libssh means no `iconv_ver_num` — which is punch-list item 5.
///
/// `feature_list` IS AN ASSOCIATIVE `name => bool` MAP, NOT A LIST OF STRINGS,
/// and it is built from php-src's OWN fixed 29-name table against the `features`
/// bitmask — never from libcurl's `feature_names` array, which is a different,
/// lower-cased, build-dependent set (`AsynchDNS` vs `asyn-dns`, and it omits
/// every feature this build lacks instead of reporting it `false`). Measured on
/// PHP 8.4.20: `var_dump(curl_version()["feature_list"])` is 29 `string => bool`
/// entries in [`PHP_FEATURE_LIST`]'s order, and their `true` bits add up to
/// `features` minus `CURL_VERSION_THREADSAFE` (1<<30), which php 8.4 does not
/// name.
fn build_global_info_json() -> String {
    let info = easy::version_info();
    let mut map = serde_json::Map::new();
    map.insert("version_number".to_string(), info.version_num.into());
    map.insert("age".to_string(), info.age.into());
    map.insert("features".to_string(), info.features.into());
    // php-src builds and adds `feature_list` HERE, between `features` and
    // `ssl_version_number`, and the position is observable through `foreach`.
    let mut features = serde_json::Map::new();
    for (name, bit) in PHP_FEATURE_LIST {
        features.insert(
            (*name).to_string(),
            serde_json::Value::Bool(info.features & bit != 0),
        );
    }
    map.insert(
        "feature_list".to_string(),
        serde_json::Value::Object(features),
    );
    map.insert(
        "ssl_version_number".to_string(),
        info.ssl_version_num.into(),
    );
    map.insert("version".to_string(), c_str_or_empty(info.version).into());
    map.insert("host".to_string(), c_str_or_empty(info.host).into());
    map.insert(
        "ssl_version".to_string(),
        c_str_or_empty(info.ssl_version).into(),
    );
    map.insert(
        "libz_version".to_string(),
        c_str_or_empty(info.libz_version).into(),
    );
    map.insert(
        "protocols".to_string(),
        c_str_array(info.protocols).into(),
    );
    // php-src's age gates, in php-src's order. `c_str_or_empty` is what turns a
    // library this build lacks into `""`, which is the value php reports for it.
    if info.age >= CURLVERSION_SECOND {
        map.insert("ares".to_string(), c_str_or_empty(info.ares).into());
        map.insert("ares_num".to_string(), info.ares_num.into());
    }
    if info.age >= CURLVERSION_THIRD {
        map.insert("libidn".to_string(), c_str_or_empty(info.libidn).into());
    }
    if info.age >= CURLVERSION_FOURTH {
        map.insert("iconv_ver_num".to_string(), info.iconv_ver_num.into());
        map.insert(
            "libssh_version".to_string(),
            c_str_or_empty(info.libssh_version).into(),
        );
    }
    if info.age >= CURLVERSION_FIFTH {
        map.insert("brotli_ver_num".to_string(), info.brotli_ver_num.into());
        map.insert(
            "brotli_version".to_string(),
            c_str_or_empty(info.brotli_version).into(),
        );
    }
    serde_json::Value::Object(map).to_string()
}

/// `CURLVERSION_SECOND`: the `curl_version_info_data` age that added
/// `ares`/`ares_num` (libcurl 7.11.1, `include/curl/curl.h`).
const CURLVERSION_SECOND: c_int = 1;
/// `CURLVERSION_THIRD`: the age that added `libidn` (libcurl 7.12.0).
const CURLVERSION_THIRD: c_int = 2;
/// `CURLVERSION_FOURTH`: the age that added `iconv_ver_num`/`libssh_version`
/// (libcurl 7.16.1).
const CURLVERSION_FOURTH: c_int = 3;
/// `CURLVERSION_FIFTH`: the age that added `brotli_ver_num`/`brotli_version`
/// (libcurl 7.57.0).
const CURLVERSION_FIFTH: c_int = 4;

/// php-src's own `curl_version()` feature table: the `feature_list` key names it
/// publishes, paired with the `CURL_VERSION_*` bit each one reports
/// (`ext/curl/interface.c`, `curl_version`). THE NAMES AND THE ORDER ARE PHP'S,
/// NOT libcurl's — several are legacy spellings for capabilities libcurl itself
/// no longer advertises (`krb4`, `NTLMWB`, `CharConv`), and php reports them
/// `false` rather than omitting them. `CURL_VERSION_THREADSAFE` (1<<30) and
/// `CURL_VERSION_CURLDEBUG` (1<<13) are deliberately absent: php 8.4 does not
/// name them, and a 30th key would be a divergence in the other direction
/// (measured: PHP 8.4.20 reports exactly these 29 keys).
pub(crate) const PHP_FEATURE_LIST: &[(&str, c_int)] = &[
    ("AsynchDNS", 1 << 7),     // CURL_VERSION_ASYNCHDNS
    ("CharConv", 1 << 12),     // CURL_VERSION_CONV
    ("Debug", 1 << 6),         // CURL_VERSION_DEBUG
    ("GSS-Negotiate", 1 << 5), // CURL_VERSION_GSSNEGOTIATE
    ("IDN", 1 << 10),          // CURL_VERSION_IDN
    ("IPv6", 1 << 0),          // CURL_VERSION_IPV6
    ("krb4", 1 << 1),          // CURL_VERSION_KERBEROS4
    ("Largefile", 1 << 9),     // CURL_VERSION_LARGEFILE
    ("libz", 1 << 3),          // CURL_VERSION_LIBZ
    ("NTLM", 1 << 4),          // CURL_VERSION_NTLM
    ("NTLMWB", 1 << 15),       // CURL_VERSION_NTLM_WB
    ("SPNEGO", 1 << 8),        // CURL_VERSION_SPNEGO
    ("SSL", 1 << 2),           // CURL_VERSION_SSL
    ("SSPI", 1 << 11),         // CURL_VERSION_SSPI
    ("TLS-SRP", 1 << 14),      // CURL_VERSION_TLSAUTH_SRP
    ("HTTP2", 1 << 16),        // CURL_VERSION_HTTP2
    ("GSSAPI", 1 << 17),       // CURL_VERSION_GSSAPI
    ("KERBEROS5", 1 << 18),    // CURL_VERSION_KERBEROS5
    ("UNIX_SOCKETS", 1 << 19), // CURL_VERSION_UNIX_SOCKETS
    ("PSL", 1 << 20),          // CURL_VERSION_PSL
    ("HTTPS_PROXY", 1 << 21),  // CURL_VERSION_HTTPS_PROXY
    ("MULTI_SSL", 1 << 22),    // CURL_VERSION_MULTI_SSL
    ("BROTLI", 1 << 23),       // CURL_VERSION_BROTLI
    ("ALTSVC", 1 << 24),       // CURL_VERSION_ALTSVC
    ("HTTP3", 1 << 25),        // CURL_VERSION_HTTP3
    ("UNICODE", 1 << 27),      // CURL_VERSION_UNICODE
    ("ZSTD", 1 << 26),         // CURL_VERSION_ZSTD
    ("HSTS", 1 << 28),         // CURL_VERSION_HSTS
    ("GSASL", 1 << 29),        // CURL_VERSION_GSASL
];

/// Converts a possibly-null `*const c_char` to an owned `String` (empty for
/// null), replacing invalid UTF-8 with the standard replacement character so
/// the JSON encoder stays infallible regardless of what libcurl reports.
fn c_str_or_empty(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Reads a NULL-terminated `const char * const *` array of C strings (the
/// shape `curl_version_info_data`'s `protocols`/`feature_names` both use)
/// into an owned `Vec<String>`. Returns an empty vec for a null array.
fn c_str_array(ptr: *const *const c_char) -> Vec<String> {
    if ptr.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cursor = ptr;
    unsafe {
        while !(*cursor).is_null() {
            out.push(c_str_or_empty(*cursor));
            cursor = cursor.add(1);
        }
    }
    out
}
