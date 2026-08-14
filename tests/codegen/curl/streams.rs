//! Purpose:
//! End-to-end fixtures for the four `ext/curl` options whose value is a PHP STREAM:
//! `CURLOPT_FILE`, `CURLOPT_WRITEHEADER`, `CURLOPT_INFILE`/`CURLOPT_READDATA`, and
//! `CURLOPT_STDERR`. All four used to answer `false` + PHP's unsupported-option warning.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl::streams` through Rust's test harness.
//!
//! Key details:
//! - THE OPTIONS ARE IMPLEMENTED BY COMPOSITION, not by a new ABI. libcurl never receives
//!   a stream: the curl prelude installs an internal PHP closure in the write / header /
//!   read / debug callback slot that `fwrite()`s to (or `fread()`s from) the stream. So
//!   these fixtures are also a second, indirect test of the callback machinery.
//! - EVERY EXPECTATION HERE WAS MEASURED against the host PHP 8.4.20 + ext/curl first
//!   (transcripts in `.superpowers/sdd/curl-punchlist/wpC-report.md`). The surprising
//!   ones, none of which follow from the manual:
//!   * WRITE is one LAST-SET-WINS mode across `CURLOPT_FILE` / `CURLOPT_RETURNTRANSFER` /
//!     `CURLOPT_WRITEFUNCTION`, and clearing any of them falls back to STDOUT — never to
//!     a sibling that was set earlier.
//!   * READ is NOT last-set-wins: a `CURLOPT_READFUNCTION` outranks `CURLOPT_INFILE` in
//!     BOTH orders, and clearing it falls BACK to the stream.
//!   * `CURLOPT_STDERR` is a FALLBACK sink, not a mode: a debug callback always wins, and
//!     once `CURLOPT_DEBUGFUNCTION` has been touched at all — even with `null` —
//!     `CURLOPT_STDERR` is shadowed for the rest of the handle's life.
//! - The `CURLOPT_STDERR` format is libcurl's OWN (`lib/curl_trc.c`'s `trc_write` in the
//!   pinned 8.21.0): a two-character prefix written ONCE PER CALLBACK CALL, then the
//!   payload verbatim, and only for `CURLINFO_TEXT`/`HEADER_IN`/`HEADER_OUT`. Reproducing
//!   it through `CURLOPT_DEBUGFUNCTION` was verified byte-identical against a real
//!   `CURLOPT_STDERR` transfer on PHP 8.4.20.

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// `CURLOPT_FILE` routes the response body into a stream and `curl_exec()` answers `true`,
/// exactly as php does — the stream, not the caller, receives the bytes.
#[test]
fn curl_file_writes_the_body_to_a_stream() {
    if skip_without_curl_native("curl_file_writes_the_body_to_a_stream") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $path = tempnam(sys_get_temp_dir(), "elephc-curl-file");
        $sink = fopen($path, "w+b");
        $ch = curl_init("{url}");
        var_dump(curl_setopt($ch, CURLOPT_FILE, $sink));
        $result = curl_exec($ch);
        var_dump($result);
        curl_close($ch);
        fclose($sink);
        echo file_get_contents($path), "\n";
        unlink($path);
        "#
    ));
    assert_eq!(out, "bool(true)\nbool(true)\nhello-curl\n");
}

/// THE WRITE SINK IS ONE MODE, AND THE LAST SETTER WINS. Nine orderings, each measured on
/// PHP 8.4.20 — this is the fixture that would catch `CURLOPT_FILE` being modelled as a
/// flag alongside `CURLOPT_RETURNTRANSFER` rather than as a fourth value of the same enum.
///
/// Each probe answers where the body went: `f` the stream, `r` `curl_exec()`'s return
/// value, `c` a user `CURLOPT_WRITEFUNCTION`, `-` none of the three — which for this
/// fixture means PHP_CURL_STDOUT, and shows up as a bare `hello-curl` in the program's
/// own output just before the `-`.
///
/// STDOUT IS DETECTED BY ELIMINATION RATHER THAN BY `ob_start()`, because elephc's curl
/// bridge writes the default sink straight to fd 1 (`write_all_stdout` in
/// `crates/elephc-curl/src/php_layer.rs`) instead of through PHP's output layer, so an
/// output buffer does not capture it the way php's does. That divergence predates the
/// stream options — it is how `CURLOPT_RETURNTRANSFER => false` has always behaved — and
/// is recorded in `docs/php/curl.md`; this fixture is written not to depend on it either
/// way, and the interleaved `hello-curl` in the expectation is that raw fd-1 write.
#[test]
fn curl_file_returntransfer_and_writefunction_share_one_last_set_wins_mode() {
    if skip_without_curl_native("curl_file_returntransfer_and_writefunction_share_one_last_set_wins_mode")
    {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // Holds whatever a user CURLOPT_WRITEFUNCTION received, so the callback sink is
        // distinguishable from the stdout sink without an output buffer.
        final class Captured {{ public string $text = ""; }}
        function probe(string $url, Captured $captured, callable $setup): string {{
            $path = tempnam(sys_get_temp_dir(), "elephc-curl-mode");
            $sink = fopen($path, "w+b");
            $captured->text = "";
            $ch = curl_init($url);
            $setup($ch, $sink);
            $result = curl_exec($ch);
            curl_close($ch);
            fclose($sink);
            $onFile = (string) file_get_contents($path);
            unlink($path);
            if ($onFile !== "") {{ return "f"; }}
            if (is_string($result) && $result !== "") {{ return "r"; }}
            if ($captured->text !== "") {{ return "c"; }}
            return "-";
        }}
        $url = "{url}";
        $captured = new Captured();
        $cb = function (CurlHandle $ch, string $data) use ($captured): int {{
            $captured->text .= $data;
            return strlen($data);
        }};
        echo probe($url, $captured, function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_FILE, $sink);
        }}), "\n";
        echo probe($url, $captured, function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_FILE, $sink);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        }}), "\n";
        echo probe($url, $captured, function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_FILE, $sink);
        }}), "\n";
        echo probe($url, $captured, function (CurlHandle $ch, $sink) use ($cb) {{
            curl_setopt($ch, CURLOPT_FILE, $sink);
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, $cb);
        }}), "\n";
        echo probe($url, $captured, function (CurlHandle $ch, $sink) use ($cb) {{
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, $cb);
            curl_setopt($ch, CURLOPT_FILE, $sink);
        }}), "\n";
        echo probe($url, $captured, function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_FILE, $sink);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, false);
        }}), "\n";
        echo probe($url, $captured, function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_FILE, $sink);
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, null);
        }}), "\n";
        echo probe($url, $captured, function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_FILE, $sink);
            curl_setopt($ch, CURLOPT_FILE, null);
        }}), "\n";
        echo probe($url, $captured, function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_FILE, $sink);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_FILE, $sink);
        }}), "\n";
        "#
    ));
    assert_eq!(
        out,
        // FILE=f / FILE+RT=r / RT+FILE=f / FILE+CB=c / CB+FILE=f /
        // FILE+RT=false, FILE+CB=null and FILE+FILE=null all fall back to STDOUT, each
        // leaking its body to fd 1 just before the `-` / FILE+RT+FILE=f.
        "f\nr\nf\nc\nf\nhello-curl-\nhello-curl-\nhello-curl-\nf\n"
    );
}

/// `CURLOPT_WRITEHEADER` routes the response HEADERS into a stream while the body follows
/// its own sink, and a user `CURLOPT_HEADERFUNCTION` shares the same last-set-wins slot.
/// The default is DISCARD, not stdout — so clearing either one silences headers.
#[test]
fn curl_writeheader_captures_headers_and_shares_the_headerfunction_slot() {
    if skip_without_curl_native("curl_writeheader_captures_headers_and_shares_the_headerfunction_slot")
    {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        function headerProbe(string $url, callable $setup): string {{
            $path = tempnam(sys_get_temp_dir(), "elephc-curl-hdr");
            $sink = fopen($path, "w+b");
            $ch = curl_init($url);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            $setup($ch, $sink);
            ob_start();
            curl_exec($ch);
            $printed = ob_get_clean();
            curl_close($ch);
            fclose($sink);
            $onFile = (string) file_get_contents($path);
            unlink($path);
            return ($onFile === "" ? "-" : "file") . "/" . ($printed === "" ? "-" : "cb");
        }}
        $hf = function (CurlHandle $ch, string $line): int {{ echo "H"; return strlen($line); }};
        // The body still comes back through RETURNTRANSFER; only the headers are diverted.
        $path = tempnam(sys_get_temp_dir(), "elephc-curl-hdr0");
        $sink = fopen($path, "w+b");
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_WRITEHEADER, $sink);
        $body = curl_exec($ch);
        curl_close($ch);
        fclose($sink);
        $headers = file_get_contents($path);
        unlink($path);
        echo $body, "\n";
        // The status line comes first and the header block ends with a blank line.
        echo str_starts_with($headers, "HTTP/1.0 200 OK") ? "status-line\n" : "no-status\n";
        echo str_contains($headers, "Content-Type: text/plain") ? "content-type\n" : "no-ct\n";
        echo str_ends_with($headers, "\r\n\r\n") ? "blank-terminated\n" : "unterminated\n";
        echo headerProbe("{url}", function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_WRITEHEADER, $sink);
        }}), "\n";
        echo headerProbe("{url}", function (CurlHandle $ch, $sink) use ($hf) {{
            curl_setopt($ch, CURLOPT_WRITEHEADER, $sink);
            curl_setopt($ch, CURLOPT_HEADERFUNCTION, $hf);
        }}), "\n";
        echo headerProbe("{url}", function (CurlHandle $ch, $sink) use ($hf) {{
            curl_setopt($ch, CURLOPT_HEADERFUNCTION, $hf);
            curl_setopt($ch, CURLOPT_WRITEHEADER, $sink);
        }}), "\n";
        echo headerProbe("{url}", function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_WRITEHEADER, $sink);
            curl_setopt($ch, CURLOPT_HEADERFUNCTION, null);
        }}), "\n";
        echo headerProbe("{url}", function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_WRITEHEADER, $sink);
            curl_setopt($ch, CURLOPT_WRITEHEADER, null);
        }}), "\n";
        "#
    ));
    assert_eq!(
        out,
        "hello-curl\nstatus-line\ncontent-type\nblank-terminated\n\
         file/-\n-/cb\nfile/-\n-/-\n-/-\n"
    );
}

/// `CURLOPT_INFILE` + `CURLOPT_INFILESIZE` drive a sized `PUT` from a stream, with no PHP
/// callback in sight — the prelude's internal reader does the `fread()`ing.
#[test]
fn curl_infile_drives_a_put_upload() {
    if skip_without_curl_native("curl_infile_drives_a_put_upload") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $path = tempnam(sys_get_temp_dir(), "elephc-curl-up");
        file_put_contents($path, "upload-from-a-stream");
        $source = fopen($path, "rb");
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_UPLOAD, true);
        var_dump(curl_setopt($ch, CURLOPT_INFILE, $source));
        curl_setopt($ch, CURLOPT_INFILESIZE, 20);
        $echo = curl_exec($ch);
        curl_close($ch);
        fclose($source);
        unlink($path);
        echo str_contains($echo, "method=PUT") ? "put\n" : "not-put\n";
        echo str_contains($echo, "body=upload-from-a-stream") ? "body-ok\n" : "body-bad\n";
        "#
    ));
    assert_eq!(out, "bool(true)\nput\nbody-ok\n");
}

/// THE READ SOURCE IS A FIXED PRECEDENCE, NOT LAST-SET-WINS. A `CURLOPT_READFUNCTION`
/// outranks `CURLOPT_INFILE` whichever order they arrive in, clearing it falls BACK to the
/// stream, and the callback receives the `CURLOPT_INFILE` stream as its `$fd` argument —
/// which is what php-src passes and what makes `fread($fd, $length)` inside a user
/// callback work.
#[test]
fn curl_readfunction_outranks_infile_in_both_orders_and_falls_back_when_cleared() {
    if skip_without_curl_native(
        "curl_readfunction_outranks_infile_in_both_orders_and_falls_back_when_cleared",
    ) {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        function upload(string $url, callable $setup): string {{
            $path = tempnam(sys_get_temp_dir(), "elephc-curl-rd");
            file_put_contents($path, "STREAMBYTES");
            $source = fopen($path, "rb");
            $ch = curl_init($url);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_UPLOAD, true);
            curl_setopt($ch, CURLOPT_INFILESIZE, 11);
            $setup($ch, $source);
            $echo = curl_exec($ch);
            curl_close($ch);
            fclose($source);
            unlink($path);
            $marker = "body=";
            $at = strpos($echo, $marker);
            return $at === false ? "?" : trim(substr($echo, $at + strlen($marker)));
        }}
        // Answers a fixed 11 bytes so it is distinguishable from the stream's content.
        $cb = function (CurlHandle $ch, mixed $fd, int $length): string {{
            return "CALLBACKxyz";
        }};
        echo upload("{url}", function (CurlHandle $ch, $source) {{
            curl_setopt($ch, CURLOPT_INFILE, $source);
        }}), "\n";
        echo upload("{url}", function (CurlHandle $ch, $source) use ($cb) {{
            curl_setopt($ch, CURLOPT_INFILE, $source);
            curl_setopt($ch, CURLOPT_READFUNCTION, $cb);
        }}), "\n";
        echo upload("{url}", function (CurlHandle $ch, $source) use ($cb) {{
            curl_setopt($ch, CURLOPT_READFUNCTION, $cb);
            curl_setopt($ch, CURLOPT_INFILE, $source);
        }}), "\n";
        echo upload("{url}", function (CurlHandle $ch, $source) use ($cb) {{
            curl_setopt($ch, CURLOPT_READFUNCTION, $cb);
            curl_setopt($ch, CURLOPT_INFILE, $source);
            curl_setopt($ch, CURLOPT_READFUNCTION, null);
        }}), "\n";
        // php-src hands the CURLOPT_INFILE stream to the callback as `$fd`, so a callback
        // that reads from it uploads the file's bytes without ever touching the outer
        // scope. A build that passed `null` here would upload nothing.
        echo upload("{url}", function (CurlHandle $ch, $source) {{
            curl_setopt($ch, CURLOPT_INFILE, $source);
            curl_setopt($ch, CURLOPT_READFUNCTION, function (CurlHandle $ch, mixed $fd, int $length): string {{
                if (!is_resource($fd)) {{
                    return "";
                }}
                $chunk = fread($fd, $length);
                return is_string($chunk) ? $chunk : "";
            }});
        }}), "\n";
        "#
    ));
    assert_eq!(
        out,
        "STREAMBYTES\nCALLBACKxyz\nCALLBACKxyz\nSTREAMBYTES\nSTREAMBYTES\n"
    );
}

/// `CURLOPT_STDERR` + `CURLOPT_VERBOSE` writes libcurl's own trace to the stream, in
/// libcurl's own format: `"* "` for `CURLINFO_TEXT`, `"> "` for the outgoing request
/// header block, `"< "` for each incoming response header — each prefix emitted ONCE per
/// trace record, so only the FIRST line of the multi-line request block carries `"> "`.
/// Body data is never traced.
///
/// Without `CURLOPT_VERBOSE` nothing is written at all, which is libcurl's own gate
/// (`trc_write` early-returns unless `data->set.verbose`).
#[test]
fn curl_stderr_writes_libcurls_verbose_trace_to_the_stream() {
    if skip_without_curl_native("curl_stderr_writes_libcurls_verbose_trace_to_the_stream") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        function verboseTrace(string $url, bool $verbose): string {{
            $path = tempnam(sys_get_temp_dir(), "elephc-curl-err");
            $sink = fopen($path, "w+b");
            $ch = curl_init($url);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            if ($verbose) {{
                curl_setopt($ch, CURLOPT_VERBOSE, true);
            }}
            curl_setopt($ch, CURLOPT_STDERR, $sink);
            curl_exec($ch);
            curl_close($ch);
            fclose($sink);
            $trace = file_get_contents($path);
            unlink($path);
            return is_string($trace) ? $trace : "";
        }}
        $trace = verboseTrace("{url}", true);
        echo str_contains($trace, "* ") ? "text\n" : "no-text\n";
        echo str_contains($trace, "> GET /hello HTTP/1.1") ? "request\n" : "no-request\n";
        echo str_contains($trace, "< HTTP/1.0 200 OK") ? "response\n" : "no-response\n";
        // The request block is ONE trace record: its continuation lines carry no prefix.
        echo str_contains($trace, "\r\nHost: 127.0.0.1") ? "one-prefix\n" : "per-line-prefix\n";
        // CURLINFO_DATA_IN is dropped, so the body never appears in the trace.
        echo str_contains($trace, "hello-curl") ? "body-leaked\n" : "no-body\n";
        echo verboseTrace("{url}", false) === "" ? "silent-without-verbose\n" : "noisy\n";
        "#
    ));
    assert_eq!(
        out,
        "text\nrequest\nresponse\none-prefix\nno-body\nsilent-without-verbose\n"
    );
}

/// `CURLOPT_DEBUGFUNCTION` ALWAYS WINS OVER `CURLOPT_STDERR`, in either order, because
/// libcurl's `trc_write` consults `set.fdebug` before `set.err`. And once the option has
/// been TOUCHED — even by setting it to `null` — `CURLOPT_STDERR` stays shadowed for the
/// rest of the handle's life, because php-src never removes its C trampoline. Both
/// measured on PHP 8.4.20.
#[test]
fn curl_debugfunction_shadows_stderr_permanently() {
    if skip_without_curl_native("curl_debugfunction_shadows_stderr_permanently") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        function debugProbe(string $url, callable $setup): string {{
            $path = tempnam(sys_get_temp_dir(), "elephc-curl-dbg");
            $sink = fopen($path, "w+b");
            $ch = curl_init($url);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_VERBOSE, true);
            ob_start();
            $setup($ch, $sink);
            curl_exec($ch);
            $printed = ob_get_clean();
            curl_close($ch);
            fclose($sink);
            $trace = file_get_contents($path);
            unlink($path);
            return ($trace === "" ? "-" : "stderr") . "/" . ($printed === "" ? "-" : "cb");
        }}
        $df = function (CurlHandle $ch, int $type, string $data): int {{ echo "D"; return 0; }};
        echo debugProbe("{url}", function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_STDERR, $sink);
        }}), "\n";
        echo debugProbe("{url}", function (CurlHandle $ch, $sink) use ($df) {{
            curl_setopt($ch, CURLOPT_STDERR, $sink);
            curl_setopt($ch, CURLOPT_DEBUGFUNCTION, $df);
        }}), "\n";
        echo debugProbe("{url}", function (CurlHandle $ch, $sink) use ($df) {{
            curl_setopt($ch, CURLOPT_DEBUGFUNCTION, $df);
            curl_setopt($ch, CURLOPT_STDERR, $sink);
        }}), "\n";
        // Cleared, but the sink stays shadowed: NEITHER receives the trace.
        echo debugProbe("{url}", function (CurlHandle $ch, $sink) use ($df) {{
            curl_setopt($ch, CURLOPT_STDERR, $sink);
            curl_setopt($ch, CURLOPT_DEBUGFUNCTION, $df);
            curl_setopt($ch, CURLOPT_DEBUGFUNCTION, null);
        }}), "\n";
        // Even a bare `null`, with no callback ever installed, shadows the stream.
        echo debugProbe("{url}", function (CurlHandle $ch, $sink) {{
            curl_setopt($ch, CURLOPT_STDERR, $sink);
            curl_setopt($ch, CURLOPT_DEBUGFUNCTION, null);
        }}), "\n";
        "#
    ));
    assert_eq!(out, "stderr/-\n-/cb\n-/cb\n-/-\n-/-\n");
}

/// A NON-STREAM VALUE IS A `TypeError` and a READ-ONLY stream handed to a WRITE sink is a
/// `ValueError`, both with php-src's own messages. `null` is the one non-resource value
/// all four accept, answering `true` and clearing the sink.
///
/// `CURLOPT_INFILE` gets NO writability check — php accepts even a write-only handle
/// there (measured), so this build must not be stricter than php.
#[test]
fn curl_stream_options_reject_non_streams_and_unwritable_sinks() {
    if skip_without_curl_native("curl_stream_options_reject_non_streams_and_unwritable_sinks") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $options = [
            "CURLOPT_FILE" => CURLOPT_FILE,
            "CURLOPT_WRITEHEADER" => CURLOPT_WRITEHEADER,
            "CURLOPT_STDERR" => CURLOPT_STDERR,
            "CURLOPT_INFILE" => CURLOPT_INFILE,
        ];
        $ch = curl_init("http://127.0.0.1:1/");
        foreach ($options as $name => $option) {
            foreach (["int" => 42, "string" => "not-a-stream", "array" => []] as $kind => $value) {
                try {
                    curl_setopt($ch, $option, $value);
                    echo $name, "/", $kind, ": accepted\n";
                } catch (\TypeError $e) {
                    echo $name, "/", $kind, ": ", $e->getMessage(), "\n";
                }
            }
            // `null` clears the sink and answers true for every one of the four.
            echo $name, "/null: ", var_export(curl_setopt($ch, $option, null), true), "\n";
        }
        // A read-only stream is a ValueError for the three WRITE sinks...
        $path = tempnam(sys_get_temp_dir(), "elephc-curl-ro");
        file_put_contents($path, "seed");
        foreach (["CURLOPT_FILE" => CURLOPT_FILE, "CURLOPT_WRITEHEADER" => CURLOPT_WRITEHEADER,
                  "CURLOPT_STDERR" => CURLOPT_STDERR] as $name => $option) {
            $readOnly = fopen($path, "rb");
            try {
                curl_setopt($ch, $option, $readOnly);
                echo $name, "/read-only: accepted\n";
            } catch (\ValueError $e) {
                echo $name, "/read-only: ", $e->getMessage(), "\n";
            }
            fclose($readOnly);
        }
        // ...but NOT for CURLOPT_INFILE, which php never checks.
        $readOnly = fopen($path, "rb");
        echo "CURLOPT_INFILE/read-only: ", var_export(curl_setopt($ch, CURLOPT_INFILE, $readOnly), true), "\n";
        fclose($readOnly);
        curl_close($ch);
        unlink($path);
        "#,
    );
    let type_error = "curl_setopt(): supplied argument is not a valid File-Handle resource";
    let value_error = "curl_setopt(): The provided file handle must be writable";
    let mut expected = String::new();
    for name in ["CURLOPT_FILE", "CURLOPT_WRITEHEADER", "CURLOPT_STDERR", "CURLOPT_INFILE"] {
        for kind in ["int", "string", "array"] {
            expected.push_str(&format!("{name}/{kind}: {type_error}\n"));
        }
        expected.push_str(&format!("{name}/null: true\n"));
    }
    for name in ["CURLOPT_FILE", "CURLOPT_WRITEHEADER", "CURLOPT_STDERR"] {
        expected.push_str(&format!("{name}/read-only: {value_error}\n"));
    }
    expected.push_str("CURLOPT_INFILE/read-only: true\n");
    assert_eq!(out, expected);
}

/// `curl_reset()` clears all four stream options — a reset handle prints its body to
/// stdout again rather than writing to the stream it was pointed at. `curl_copy_handle()`
/// does the OPPOSITE and carries them onto the duplicate, which is php's behaviour for all
/// four (php-src copies its handler structs).
///
/// The copy case is also what proves the internal closures read their stream off the `$ch`
/// they are CALLED with: the bridge re-registers them against the duplicate, so a closure
/// that had captured the original's stream would write the copy's body to the wrong place —
/// or, worse, keep the original's handle alive in a cycle.
#[test]
fn curl_reset_clears_stream_options_and_copy_carries_them() {
    if skip_without_curl_native("curl_reset_clears_stream_options_and_copy_carries_them") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // curl_reset(): the stream is forgotten and the body goes back to stdout.
        $path = tempnam(sys_get_temp_dir(), "elephc-curl-reset");
        $sink = fopen($path, "w+b");
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_FILE, $sink);
        curl_reset($ch);
        curl_setopt($ch, CURLOPT_URL, "{url}");
        // The body now goes to the default stdout sink, which the bridge writes straight
        // to fd 1 — so it appears in this program's output right here, before the two
        // markers below. That raw write is why this fixture does not use ob_start().
        curl_exec($ch);
        curl_close($ch);
        fclose($sink);
        echo "\n", file_get_contents($path) === "" ? "reset-file-empty\n" : "reset-file-written\n";
        unlink($path);

        // curl_copy_handle(): the duplicate writes to the SAME stream.
        $path = tempnam(sys_get_temp_dir(), "elephc-curl-copy");
        $sink = fopen($path, "w+b");
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_FILE, $sink);
        $copy = curl_copy_handle($ch);
        var_dump(curl_exec($copy));
        curl_close($copy);
        curl_close($ch);
        fclose($sink);
        echo file_get_contents($path), "\n";
        unlink($path);

        // The same for the header sink and the upload source.
        $path = tempnam(sys_get_temp_dir(), "elephc-curl-copyh");
        $sink = fopen($path, "w+b");
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_WRITEHEADER, $sink);
        $copy = curl_copy_handle($ch);
        curl_exec($copy);
        curl_close($copy);
        curl_close($ch);
        fclose($sink);
        echo str_contains((string) file_get_contents($path), "HTTP/1.0 200 OK") ? "copy-headers\n" : "no-copy-headers\n";
        unlink($path);

        $path = tempnam(sys_get_temp_dir(), "elephc-curl-copyu");
        file_put_contents($path, "copied-upload");
        $source = fopen($path, "rb");
        $ch = curl_init("{echo_url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_UPLOAD, true);
        curl_setopt($ch, CURLOPT_INFILESIZE, 13);
        curl_setopt($ch, CURLOPT_INFILE, $source);
        $copy = curl_copy_handle($ch);
        $echoed = curl_exec($copy);
        curl_close($copy);
        curl_close($ch);
        fclose($source);
        unlink($path);
        echo str_contains($echoed, "body=copied-upload") ? "copy-upload\n" : "no-copy-upload\n";
        "#,
        url = url,
        echo_url = server.url("/echo"),
    ));
    assert_eq!(
        out,
        // `hello-curl` first: the reset handle's body reaching fd 1 directly.
        "hello-curl\nreset-file-empty\nbool(true)\nhello-curl\ncopy-headers\ncopy-upload\n"
    );
}
