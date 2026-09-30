//! Purpose:
//! End-to-end fixtures for the curl easy handle's object surface and for
//! `curl_version()`: `curl_init()` returns a real `CurlHandle` object, serializing it
//! throws, `function_exists()` and `extension_loaded()` agree that curl is present, and
//! `curl_version()` reports the libcurl this binary actually linked.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - No fixture here touches the network. `curl_version()` reads
//!   `curl_version_info(CURLVERSION_NOW)` out of the linked library; everything else is
//!   object identity and declaration checks.
//! - The asserted version is elephc's PINNED libcurl (8.21.0), never
//!   the developer's system curl — that is the whole point of the managed native
//!   package, so this assertion is the pin's end-to-end proof.
//! - `CurlHandle` being `final` and not user-constructible is a COMPILE-TIME diagnostic,
//!   so those two checks live in `tests/error_tests/curl.rs` instead: they need only the
//!   injected prelude, never a link, and therefore run everywhere.

use crate::support::*;

/// `curl_init()` mints a real PHP object of the `CurlHandle` class, not a resource and
/// not an int: `get_class()` names it and `instanceof` agrees, exactly as PHP 8 does
/// since the resource-to-object migration.
#[test]
fn curl_init_returns_curlhandle() {
    if skip_without_curl_native("curl_init_returns_curlhandle") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo get_class($ch), "\n";
        echo ($ch instanceof CurlHandle) ? "yes\n" : "no\n";
        "#,
    );
    assert_eq!(out, "CurlHandle\nyes\n");
}

/// `curl_version()` reports the pinned libcurl the binary actually linked — 8.21.0 from
/// the managed native package, with HTTP in its protocol list — decoded from the
/// bridge's JSON blob into PHP's associative array shape.
#[test]
fn curl_version_reports_pinned_libcurl() {
    if skip_without_curl_native("curl_version_reports_pinned_libcurl") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $v = curl_version();
        echo $v['version'], "\n";
        $found = false;
        foreach ($v['protocols'] as $protocol) {
            if ($protocol === 'http') {
                $found = true;
            }
        }
        echo $found ? "http\n" : "no-http\n";
        "#,
    );
    assert!(out.starts_with("8.21.0\n"), "{out}");
    assert!(out.contains("http\n"), "{out}");
}

/// THE PINNED BUILD'S PROTOCOL SET, ASSERTED EXACTLY. `curl_version()['protocols']` is
/// the one place a recipe change is visible from PHP, so this fixture is what makes the
/// protocol matrix in `docs/php/curl.md` a checked claim rather than a comment: it pins
/// the whole list, in libcurl's own order, for curl recipe revision 2.
///
/// This is deliberately NOT a php-parity assertion, and the lists genuinely differ in
/// BOTH directions. A distro php's libcurl typically carries `ldap`/`ldaps`, which this
/// build does not; and this build carries schemes a given distro php may not — measured
/// on this machine, php 8.4.20's libcurl 8.19.0 has no `scp`/`sftp` at all. Comparing the
/// two lists would be comparing two build configurations, not two implementations. What
/// must match php is the SHAPE — a list of lowercase scheme strings in libcurl's own
/// order — and that is what `curl_version_exposes_php_key_shape` and
/// `curl_version_keys_are_in_php_s_order` cover.
///
/// `ipfs`/`ipns` are absent on purpose: curl's configure summary reports them as enabled,
/// but they are a feature of the curl TOOL (which rewrites an IPFS URL to an HTTP gateway
/// request), not schemes libcurl registers, so `curl_version_info()` never lists them.
#[test]
fn curl_version_reports_the_full_pinned_protocol_set() {
    if skip_without_curl_native("curl_version_reports_the_full_pinned_protocol_set") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        echo implode(" ", curl_version()['protocols']), "\n";
        "#,
    );
    assert_eq!(
        out,
        "dict file ftp ftps gopher gophers http https imap imaps mqtt mqtts pop3 pop3s \
         rtsp scp sftp smb smbs smtp smtps telnet tftp ws wss\n"
    );
}

/// The two capability flips recipe revision 2 makes visible through php's own
/// `curl_version()` keys: `HTTP2` in `feature_list` (and its bit in `features`), and a
/// non-empty `libssh_version` — which for revision 1 was the empty string php substitutes
/// for libcurl's NULL, since there was no SSH library at all.
///
/// `HTTP3` is asserted FALSE, and that is the honest state of this pin rather than an
/// oversight: curl 8.21.0 has no standalone `openssl-quic` backend (it was removed), so
/// the only non-experimental HTTP/3 path is ngtcp2 + nghttp3 — two further pinned
/// packages this round did not take.
#[test]
fn curl_version_reports_http2_and_the_ssh_library() {
    if skip_without_curl_native("curl_version_reports_http2_and_the_ssh_library") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $v = curl_version();
        echo $v['feature_list']['HTTP2'] ? "http2\n" : "no-http2\n";
        // CURL_VERSION_HTTP2 is 1 << 16; feature_list and the bitmask must agree.
        echo ($v['features'] & (1 << 16)) ? "bit\n" : "no-bit\n";
        echo $v['feature_list']['HTTP3'] ? "http3\n" : "no-http3\n";
        echo ($v['features'] & (1 << 25)) ? "bit3\n" : "no-bit3\n";
        echo $v['feature_list']['NTLM'] ? "ntlm\n" : "no-ntlm\n";
        echo $v['libssh_version'], "\n";
        "#,
    );
    assert_eq!(
        out,
        "http2\nbit\nno-http3\nno-bit3\nntlm\nlibssh2/1.11.1\n"
    );
}

/// The protocol set is enforced by libcurl at `curl_exec()`, and the three outcomes stay
/// distinguishable — which is what makes a scheme's status observable from PHP without a
/// network:
///
/// - a built-in scheme gets as far as CONNECTING (`sftp://` to a closed local port),
///   proving libcurl knows the scheme;
/// - a scheme libcurl knows but was built without says `is disabled` (`ldap`, which needs
///   an OpenLDAP this catalog does not pin);
/// - a scheme libcurl has no handler for at all says `not supported` (`rtmp`, which curl
///   8.20.0 removed outright — see `docs/DEPRECATE.md` in the pinned tarball).
///
/// THE CONNECT HALF ACCEPTS TWO ERRNOS, ON PURPOSE. A closed loopback port normally
/// REJECTs, giving `CURLE_COULDNT_CONNECT` (7); somewhere that DROPs instead, the 200 ms
/// cap expires first and libcurl reports `CURLE_OPERATION_TIMEDOUT` (28). Pinning 7 alone
/// would make this fixture flake on hardened hosts for a reason unrelated to what it
/// tests, so the PHP below folds both into one token. The distinction that MATTERS is
/// still exact: either value proves libcurl accepted the scheme and opened a socket,
/// while a build without SFTP would answer errno 1 and never reach the network stack.
///
/// That socket is the only one this file opens — `127.0.0.1:1`, capped at 200 ms — so it
/// resolves nothing and reaches no network.
#[test]
fn built_in_disabled_and_unknown_schemes_stay_distinguishable() {
    if skip_without_curl_native("built_in_disabled_and_unknown_schemes_stay_distinguishable") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $sftp = curl_init("sftp://127.0.0.1:1/x");
        curl_setopt($sftp, CURLOPT_CONNECTTIMEOUT_MS, 200);
        curl_exec($sftp);
        // 7 = CURLE_COULDNT_CONNECT (port refused), 28 = CURLE_OPERATION_TIMEDOUT
        // (port dropped, cap expired). Both mean "scheme accepted, socket opened".
        $reached = in_array(curl_errno($sftp), [7, 28], true);
        echo $reached ? "connected\n" : ("unreached:" . curl_errno($sftp) . "\n");
        $ldap = curl_init("ldap://127.0.0.1:1/x");
        curl_exec($ldap);
        echo curl_errno($ldap), " ", curl_error($ldap), "\n";
        $rtmp = curl_init("rtmp://127.0.0.1/x");
        curl_exec($rtmp);
        echo curl_errno($rtmp), " ", curl_error($rtmp), "\n";
        "#,
    );
    assert_eq!(
        out,
        "connected\n1 Protocol \"ldap\" is disabled\n1 Protocol \"rtmp\" not supported\n"
    );
}

/// `curl_version()` carries the rest of PHP's documented key set, and the numeric keys
/// really are numbers rather than the JSON blob's text — proof the array came through
/// `json_decode()`'s typed decoding and not a string split.
#[test]
fn curl_version_exposes_php_key_shape() {
    if skip_without_curl_native("curl_version_exposes_php_key_shape") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $v = curl_version();
        echo is_array($v) ? "array\n" : "not-array\n";
        echo is_int($v['version_number']) ? "int\n" : "not-int\n";
        echo is_int($v['features']) ? "int\n" : "not-int\n";
        echo is_string($v['host']) ? "string\n" : "not-string\n";
        echo is_array($v['protocols']) ? "protocols\n" : "no-protocols\n";
        echo array_key_exists('ssl_version', $v) ? "ssl\n" : "no-ssl\n";
        "#,
    );
    assert_eq!(out, "array\nint\nint\nstring\nprotocols\nssl\n");
}

/// PUNCH-LIST ITEMS 4 AND 5, through the real prelude/`json_decode()` path: PHP's
/// `feature_list` is an ASSOCIATIVE `name => bool` map (never a list of feature-name
/// strings), and the age-gated sub-library keys — `iconv_ver_num` above all, which used
/// to be gated on a non-null `libssh_version` and therefore never appeared — are present
/// whatever this build was compiled with. Measured on PHP 8.4.20:
/// `var_dump(curl_version()['feature_list'])` prints 29 `string => bool` pairs, and
/// `curl_version()['iconv_ver_num']` is `0` on a build with no libssh at all.
#[test]
fn curl_version_feature_list_is_an_assoc_of_bools() {
    if skip_without_curl_native("curl_version_feature_list_is_an_assoc_of_bools") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $v = curl_version();
        $list = $v['feature_list'];
        echo is_array($list) ? "array\n" : "not-array\n";
        echo count($list), "\n";
        // A list of strings would have integer keys and string values; PHP's shape is
        // the other way round for every entry.
        $strings = 0;
        $bools = 0;
        foreach ($list as $name => $enabled) {
            if (is_string($name)) { $strings++; }
            if (is_bool($enabled)) { $bools++; }
        }
        echo $strings, " ", $bools, "\n";
        echo $list['SSL'] ? "ssl\n" : "no-ssl\n";
        echo $list['libz'] ? "libz\n" : "no-libz\n";
        echo $list['krb4'] === false ? "krb4-false\n" : "krb4-wrong\n";
        echo array_key_exists('iconv_ver_num', $v) ? "iconv\n" : "no-iconv\n";
        echo array_key_exists('libssh_version', $v) ? "libssh\n" : "no-libssh\n";
        echo array_key_exists('ares', $v) ? "ares\n" : "no-ares\n";
        echo array_key_exists('libidn', $v) ? "libidn\n" : "no-libidn\n";
        // PHP arrays are ORDERED: php-src's table starts at AsynchDNS and ends at GSASL.
        $names = array_keys($list);
        echo $names[0], " ", $names[28], "\n";
        "#,
    );
    assert_eq!(
        out,
        "array\n29\n29 29\nssl\nlibz\nkrb4-false\niconv\nlibssh\nares\nlibidn\nAsynchDNS GSASL\n"
    );
}

/// `curl_version()`'s own KEY ORDER is php-src's, measured with
/// `array_keys(curl_version())` on PHP 8.4.20 — `feature_list` sits between `features` and
/// `ssl_version_number`, and the age-gated sub-library keys close the list. A PHP array is
/// ordered, so this is observable through `foreach`/`json_encode()`; the bridge's JSON
/// encoder keeps the insertion order (`preserve_order`) and the prelude's `json_decode()`
/// carries it into the array.
#[test]
fn curl_version_keys_are_in_php_s_order() {
    if skip_without_curl_native("curl_version_keys_are_in_php_s_order") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        echo implode(",", array_keys(curl_version())), "\n";
        "#,
    );
    assert_eq!(
        out,
        "version_number,age,features,feature_list,ssl_version_number,version,host,\
         ssl_version,libz_version,protocols,ares,ares_num,libidn,iconv_ver_num,\
         libssh_version,brotli_ver_num,brotli_version\n"
    );
}

/// The PHP names come from the injected prelude, so `function_exists()` sees them
/// exactly like php-src's own `ext/curl` registrations.
#[test]
fn curl_functions_are_declared() {
    if skip_without_curl_native("curl_functions_are_declared") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo function_exists('curl_init') ? "init\n" : "no-init\n";
        echo function_exists('curl_exec') ? "exec\n" : "no-exec\n";
        echo function_exists('curl_setopt') ? "setopt\n" : "no-setopt\n";
        echo function_exists('curl_version') ? "version\n" : "no-version\n";
        echo class_exists('CurlHandle') ? "class\n" : "no-class\n";
        "#,
    );
    assert_eq!(out, "init\nexec\nsetopt\nversion\nclass\n");
}

/// A curl-using program reports `curl` as a loaded extension, because the bridge that
/// backs it is linked. The negative half of this contract (a curl-free program still
/// reports curl unloaded) lives in `tests/extension_loaded_tests.rs`.
#[test]
fn extension_loaded_curl_is_true_for_a_curl_program() {
    if skip_without_curl_native("extension_loaded_curl_is_true_for_a_curl_program") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo extension_loaded('curl') ? "loaded\n" : "unloaded\n";
        "#,
    );
    assert_eq!(out, "loaded\n");
}

/// Serializing a `CurlHandle` throws, exactly as php-src does
/// (`Exception: Serialization of 'CurlHandle' is not allowed`) — elephc could not
/// reproduce a live libcurl handle from a serialized blob, so failing loudly is the
/// honest answer rather than emitting a plausible-looking dead object.
#[test]
fn curl_handle_serialization_throws() {
    if skip_without_curl_native("curl_handle_serialization_throws") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        try {
            $s = serialize($ch);
            echo "serialized\n";
        } catch (\Exception $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );
    assert_eq!(out, "Serialization of 'CurlHandle' is not allowed\n");
}

/// PHP function names are case-insensitive and namespace-qualified spellings resolve to
/// the global function, so `CURL_INIT()` and `\curl_init()` reach the same prelude
/// wrapper the lowercase spelling does.
#[test]
fn curl_names_are_case_insensitive_and_namespace_qualified() {
    if skip_without_curl_native("curl_names_are_case_insensitive_and_namespace_qualified") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $a = CURL_INIT();
        $b = \curl_init();
        echo get_class($a), "\n";
        echo get_class($b), "\n";
        "#,
    );
    assert_eq!(out, "CurlHandle\nCurlHandle\n");
}

/// `curl_close()` is a no-op in PHP 8: the handle stays usable afterwards and the object
/// still frees its libcurl handle exactly once when it goes out of scope.
#[test]
fn curl_close_is_a_no_op() {
    if skip_without_curl_native("curl_close_is_a_no_op") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        curl_close($ch);
        echo ($ch instanceof CurlHandle) ? "alive\n" : "dead\n";
        echo curl_errno($ch), "\n";
        "#,
    );
    assert_eq!(out, "alive\n0\n");
}

/// A handle that never performed a transfer reports `CURLE_OK` and an empty error
/// string, matching PHP's own initial state for a fresh handle.
#[test]
fn fresh_handle_reports_no_error() {
    if skip_without_curl_native("fresh_handle_reports_no_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo curl_errno($ch), "\n";
        echo strlen(curl_error($ch)), "\n";
        "#,
    );
    assert_eq!(out, "0\n0\n");
}

/// `curl_setopt()` forwards the options this build really supports and reports libcurl's
/// own acceptance, and rejects a value type it cannot carry with PHP 8's `TypeError`
/// (php-src raises a TypeError, not a ValueError, for a wrong-typed `$value`).
#[test]
fn curl_setopt_forwards_and_rejects_honestly() {
    if skip_without_curl_native("curl_setopt_forwards_and_rejects_honestly") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo curl_setopt($ch, 10002, "http://127.0.0.1:1/") ? "url\n" : "no-url\n";
        echo curl_setopt($ch, 19913, true) ? "capture\n" : "no-capture\n";
        echo curl_setopt($ch, 52, 1) ? "follow\n" : "no-follow\n";
        try {
            curl_setopt($ch, 10002, [1, 2]);
            echo "accepted\n";
        } catch (\TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );
    assert_eq!(
        out,
        "url\ncapture\nfollow\n\
         curl_setopt(): Argument #3 ($value) must be of type string|int|float|bool, array given\n"
    );
}

/// An option php-src RECOGNIZES but this build cannot carry is rejected BEFORE it
/// reaches libcurl: `curl_setopt()` answers `false` and emits PHP's warning (locked
/// decision 7), rather than the inert `true` libcurl would have produced after being
/// handed a PHP value as a function pointer / `struct curl_blob *` / PHP stream.
///
/// This is a memory-safety regression test, not a politeness one. Every option here used
/// to be forwarded verbatim: 20200 (`CURLOPT_FNMATCH_FUNCTION`) overwrote a libcurl
/// callback slot with the address `1`, 20312 (`CURLOPT_PREREQFUNCTION`) the same, and
/// 40291 (`CURLOPT_SSLCERT_BLOB`) mis-read a PHP string as a `struct curl_blob *`. The
/// transfer at the end is what proves nothing was corrupted: the handle still performs and
/// still reports its own error, so none of the rejected options reached libcurl's state.
///
/// THREE FAMILIES HAVE GRADUATED OUT OF THIS LIST, each to a fixture that also pins its
/// own HONEST rejection — a wrongly-typed value is a `TypeError`, never a silent `false`:
/// 20011 (`CURLOPT_WRITEFUNCTION`) and the rest of the first callback wave to
/// `tests/codegen/curl/callbacks.rs`; 10100 (`CURLOPT_SHARE`) to
/// `tests/codegen/curl/share.rs`; and 10001 (`CURLOPT_FILE`) with the other three PHP
/// stream options to `tests/codegen/curl/streams.rs`. `CURLOPT_FILE` in particular can no
/// longer appear here at all: `curl_setopt($ch, 10001, 1)` is now a `TypeError` for the
/// non-stream `1`, which is php's own answer, so probing it with an int would abort the
/// program rather than print `rejected`.
#[test]
fn unsupported_options_are_rejected_before_libcurl() {
    if skip_without_curl_native("unsupported_options_are_rejected_before_libcurl") {
        return;
    }
    let output = compile_and_run_capture(
        r#"<?php
        $ch = curl_init();
        echo curl_setopt($ch, 20200, 1) ? "accepted\n" : "rejected\n";
        echo curl_setopt($ch, 40291, "blob") ? "accepted\n" : "rejected\n";
        echo curl_setopt($ch, 20312, 1) ? "accepted\n" : "rejected\n";
        curl_setopt($ch, 10002, "file:///nonexistent-elephc-curl-probe");
        curl_setopt($ch, 19913, true);
        $body = curl_exec($ch);
        echo ($body === false) ? "exec-false\n" : "exec-ok\n";
        echo (curl_errno($ch) !== 0) ? "errno\n" : "no-errno\n";
        "#,
    );
    assert_eq!(
        output.stdout,
        "rejected\nrejected\nrejected\nexec-false\nerrno\n"
    );
    for option in ["20200", "40291", "20312"] {
        assert!(
            output.diagnostics.contains(&format!(
                "Warning: curl_setopt(): Option {option} is not supported by this build"
            )),
            "each rejected option must warn; diagnostics were: {}",
            output.diagnostics
        );
    }
}

/// An option number that is not a cURL option AT ALL raises php-src's own
/// `ValueError: curl_setopt(): Argument #2 ($option) is not a valid cURL option`, which
/// is a DIFFERENT answer from the `false` + warning an unsupported-but-real option gets.
///
/// php-src's `_php_curl_setopt` ends its switch with exactly this
/// `zend_argument_value_error(2, ...)`, so a program that mistypes an option number finds
/// out immediately instead of watching a `false` it may not even check. The last case is
/// the one a 32-bit option parameter would have got wrong: `4294967298` truncates onto
/// option `2`, which IS a real option (`CURLINFO_HEADER_OUT`), and must still throw.
#[test]
fn invalid_option_numbers_raise_a_value_error() {
    if skip_without_curl_native("invalid_option_numbers_raise_a_value_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        foreach ([9998, 30005, 40077, 4294967298] as $option) {
            try {
                curl_setopt($ch, $option, 1);
                echo "accepted\n";
            } catch (\ValueError $e) {
                echo $e->getMessage(), "\n";
            }
        }
        echo curl_setopt($ch, 52, 1) ? "follow\n" : "no-follow\n";
        "#,
    );
    let message = "curl_setopt(): Argument #2 ($option) is not a valid cURL option";
    assert_eq!(
        out,
        format!("{message}\n{message}\n{message}\n{message}\nfollow\n")
    );
}

/// A transfer against a closed loopback port fails honestly: `curl_exec()` returns
/// `false`, `curl_errno()` reports a real non-zero `CURLcode`, and `curl_error()`
/// carries libcurl's own message. No network beyond localhost is involved.
#[test]
fn failed_transfer_reports_curl_error() {
    if skip_without_curl_native("failed_transfer_reports_curl_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init("http://127.0.0.1:1/");
        curl_setopt($ch, 19913, true);
        $body = curl_exec($ch);
        echo ($body === false) ? "false\n" : "not-false\n";
        echo (curl_errno($ch) !== 0) ? "errno\n" : "no-errno\n";
        echo (strlen(curl_error($ch)) > 0) ? "message\n" : "no-message\n";
        "#,
    );
    assert_eq!(out, "false\nerrno\nmessage\n");
}

/// A complete transfer round-trips through the whole chain — `curl_setopt` →
/// `curl_exec` → captured body — over the `file://` protocol, so the assertion covers
/// perform, the RETURNTRANSFER capture, and the borrowed-buffer copy without touching a
/// socket. `file` is one of the protocols this build's libcurl recipe enables.
#[test]
fn file_protocol_transfer_returns_the_body() {
    if skip_without_curl_native("file_protocol_transfer_returns_the_body") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $path = tempnam(sys_get_temp_dir(), "elephc_curl_");
        file_put_contents($path, "hello from file
");
        $ch = curl_init("file://" . $path);
        curl_setopt($ch, 19913, true);
        $body = curl_exec($ch);
        echo is_string($body) ? "string
" : "not-string
";
        echo $body;
        echo curl_errno($ch), "
";
        unlink($path);
        "#,
    );
    assert_eq!(out, "string
hello from file
0
");
}

/// Without `CURLOPT_RETURNTRANSFER`, `curl_exec()` writes the body straight to stdout and
/// returns `true` — PHP CLI's default shape, and the other half of `curl_exec()`'s
/// three-way return contract.
#[test]
fn transfer_without_returntransfer_writes_to_stdout() {
    if skip_without_curl_native("transfer_without_returntransfer_writes_to_stdout") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $path = tempnam(sys_get_temp_dir(), "elephc_curl_");
        file_put_contents($path, "streamed body
");
        $ch = curl_init("file://" . $path);
        $result = curl_exec($ch);
        echo ($result === true) ? "true
" : "not-true
";
        unlink($path);
        "#,
    );
    assert_eq!(out, "streamed body
true
");
}

/// `curl_setopt_array()` applies every option and stops at the first rejection, matching
/// PHP's documented short-circuit.
#[test]
fn curl_setopt_array_applies_every_option() {
    if skip_without_curl_native("curl_setopt_array_applies_every_option") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $path = tempnam(sys_get_temp_dir(), "elephc_curl_");
        file_put_contents($path, "batched
");
        $ch = curl_init();
        $ok = curl_setopt_array($ch, [10002 => "file://" . $path, 19913 => true]);
        echo $ok ? "ok
" : "failed
";
        echo curl_exec($ch);
        unlink($path);
        "#,
    );
    assert_eq!(out, "ok
batched
");
}

/// The `CurlHandle` object owns its libcurl handle and releases it exactly once: the
/// heap is balanced after a scope that creates and drops several handles, so the
/// Mixed-cell destructor path really does reach `__rt_curl_easy_free`.
#[test]
fn curl_handles_free_their_native_handle() {
    if skip_without_curl_native("curl_handles_free_their_native_handle") {
        return;
    }
    let output = compile_and_run_with_gc_stats(
        r#"<?php
        function make(): void {
            $ch = curl_init("http://127.0.0.1:1/");
            unset($ch);
        }
        make();
        make();
        make();
        echo "done\n";
        "#,
    );
    assert_eq!(output.stdout, "done\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(allocs, frees, "curl handle scope must be heap-balanced");

    // The balanced count above proves nothing leaked; heap debug proves nothing was freed
    // TWICE or used after release, which is the failure mode a second free path (an
    // over-eager `curl_close`, or a `__destruct` beside the Mixed cell) would introduce.
    let debug = compile_and_run_with_heap_debug(
        r#"<?php
        $ch = curl_init("http://127.0.0.1:1/");
        curl_setopt($ch, 19913, true);
        echo curl_errno($ch), "\n";
        unset($ch);
        echo "done\n";
        "#,
    );
    assert_eq!(debug.stdout, "0\ndone\n", "stderr: {}", debug.stderr);
}
