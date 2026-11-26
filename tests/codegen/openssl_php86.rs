//! Purpose:
//! PHP 8.6's `Openssl\OpensslException`, `Openssl\Psk` and `Openssl\Session` value
//! classes.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext openssl_php86`.
//!
//! Key details:
//! - The classes are 8.6-only; the fixtures compile under `--php-version=8.6`.
//! - `Psk` is pure PHP-level state; `Session` exercises the managed `openssl-crypto`
//!   libssl bridge, so its test skips when that artifact is not installed locally.
//! - The TLS 1.2 session fixture was generated against a local OpenSSL 3.6.4 server
//!   and is committed so the test never needs a network listener.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Verifies the 8.6 OpenSSL value classes, their constants, validation and readonly state.
#[test]
fn test_openssl_psk_php86() {
    let out = compile_and_run_with_php_version(
        r#"<?php
var_dump(class_exists('Openssl\Psk'), class_exists('Openssl\OpensslException'));
var_dump(Openssl\Psk::MAX_PSK_LEN, Openssl\Psk::MAX_IDENTITY_LEN);
$p = new Openssl\Psk('secret', 'client');
echo $p->psk, '|', $p->identity, "\n";
$q = new Openssl\Psk('secret');
var_dump($q->identity);
foreach ([["", null], [str_repeat('a', 257), null], ['k', str_repeat('b', 129)]] as [$psk, $id]) {
    try { new Openssl\Psk($psk, $id); }
    catch (Throwable $e) { echo get_class($e), ':', $e->getMessage(), "\n"; }
}
try { $p->psk = 'other'; }
catch (Throwable $e) { echo get_class($e), ':', $e->getMessage(), "\n"; }
try { throw new Openssl\OpensslException('boom'); }
catch (Exception $e) { echo $e->getMessage(), "\n"; }
"#,
        PhpVersion::Php86,
    );
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nint(256)\nint(128)\nsecret|client\nNULL\n\
ValueError:Openssl\\Psk::__construct(): Argument #1 ($psk) must not be empty\n\
ValueError:Openssl\\Psk::__construct(): Argument #1 ($psk) must not exceed 256 bytes\n\
ValueError:Openssl\\Psk::__construct(): Argument #2 ($identity) must not exceed 128 bytes\n\
Error:Cannot modify readonly property Openssl\\Psk::$psk\nboom\n"
    );
}

/// Verifies `Openssl\Session` import/export, metadata, serialization and error paths.
#[test]
fn test_openssl_session_php86() {
    if skip_without_openssl_native("test_openssl_session_php86") {
        return;
    }
    let pem = format!(
        "{}\n",
        include_str!("strings/fixtures/ssl_session_tls12.pem").trim_end()
    );
    let source = r#"<?php
$pem = <<<'PEM'
__SESSION_PEM__PEM;
$pem = rtrim($pem, "\n") . "\n";
$s = Openssl\Session::import($pem);
echo bin2hex($s->id), "|", strlen($s->id), "|", $s->getProtocol(), "|", $s->getCipher(), "|";
echo $s->isResumable() ? "resumable" : "not-resumable", "|";
echo $s->getTimeout(), "|", $s->getCreatedAt() > 0 ? "created" : "missing", "|";
echo $s->hasTicket() ? "ticket" : "no-ticket", "|", $s->getTicketLifetimeHint(), "\n";

$der = $s->export(OPENSSL_ENCODING_DER);
$fromDer = Openssl\Session::import($der, OPENSSL_ENCODING_DER);
echo bin2hex($fromDer->id) === bin2hex($s->id) ? "der-id" : "der-bad", "|";
echo $fromDer->getProtocol(), "|", $fromDer->getCipher(), "\n";

$pemAgain = $s->export();
echo $pemAgain === $pem ? "pem-same" : "pem-different", "|";
echo substr($pemAgain, 0, 38), "\n";

$copy = unserialize(serialize($s));
echo $copy->getProtocol(), "|", $copy->getCipher(), "|", $copy->hasTicket() ? "ticket" : "no-ticket", "\n";

$reflection = new ReflectionClass(Openssl\Session::class);
$empty = $reflection->newInstanceWithoutConstructor();
try { $empty->getProtocol(); }
catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
try { $empty->__unserialize([]); }
catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
try { unserialize('O:15:"Openssl\\Session":1:{s:3:"pem";s:13:"not a session";}'); }
catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
try { new Openssl\Session(); }
catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
try { Openssl\Session::import('bad'); }
catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
try { Openssl\Session::import($pem, OPENSSL_ENCODING_SMIME); }
catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
try { $s->export(OPENSSL_ENCODING_SMIME); }
catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
try { $s->__unserialize([]); }
catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
try { $s->id = "other"; }
catch (Throwable $e) { echo get_class($e), ":", $e->getMessage(), "\n"; }
"#
    .replace("__SESSION_PEM__", &pem);
    let out = compile_and_run_with_php_version(&source, PhpVersion::Php86);
    assert_eq!(
        out,
        "6012d1b555d36c328ec54c03b97e845db7608edf748ff5972948fad22a202d26|32|TLSv1.2|\
ECDHE-RSA-AES256-GCM-SHA384|resumable|7200|created|ticket|7200\n\
der-id|TLSv1.2|ECDHE-RSA-AES256-GCM-SHA384\n\
pem-same|-----BEGIN SSL SESSION PARAMETERS-----\n\
TLSv1.2|ECDHE-RSA-AES256-GCM-SHA384|ticket\n\
Openssl\\OpensslException:Session is not valid\n\
Openssl\\OpensslException:Invalid serialization data\n\
Openssl\\OpensslException:Failed to unserialize session\n\
Error:Cannot directly construct Openssl\\Session, use Openssl\\Session::import() or TLS session callbacks\n\
Openssl\\OpensslException:Failed to import session data\n\
ValueError:Openssl\\Session::import(): Argument #2 ($format) must be OPENSSL_ENCODING_DER or OPENSSL_ENCODING_PEM\n\
ValueError:Openssl\\Session::export(): Argument #1 ($format) must be OPENSSL_ENCODING_DER or OPENSSL_ENCODING_PEM\n\
Error:Cannot call Openssl\\Session::__unserialize() on an already-initialized session\n\
Error:Cannot modify readonly property Openssl\\Session::$id\n"
    );
}

/// A `session_data` value reaches the stream-context builtin as a PEM string
/// through the 8.6 wrapper while `stream_context_get_options()` keeps returning
/// the original `Openssl\Session` object.
#[test]
fn test_openssl_session_context_option_preserves_visible_object_php86() {
    if skip_without_openssl_native("test_openssl_session_context_option_preserves_visible_object_php86") {
        return;
    }
    let pem = format!(
        "{}\n",
        include_str!("strings/fixtures/ssl_session_tls12.pem").trim_end()
    );
    let source = r#"<?php
$pem = <<<'PEM'
__SESSION_PEM__PEM;
$session = Openssl\Session::import($pem);
$context = stream_context_create();
var_dump(stream_context_set_option($context, "ssl", "session_data", $session));
$stored = stream_context_get_options($context)["ssl"]["session_data"] ?? null;
echo $stored instanceof Openssl\Session ? "session" : get_debug_type($stored), "\n";
"#
    .replace("__SESSION_PEM__", &pem);
    let out = compile_and_run_with_php_version(&source, PhpVersion::Php86);
    assert_eq!(out, "bool(true)\nsession\n");
}

/// The `stream_context_create()` options array also converts `ssl.session_data`
/// to PEM for the builtin while `stream_context_get_options()` keeps the object.
#[test]
fn test_openssl_session_create_array_preserves_visible_object_php86() {
    if skip_without_openssl_native("test_openssl_session_create_array_preserves_visible_object_php86") {
        return;
    }
    let pem = format!(
        "{}\n",
        include_str!("strings/fixtures/ssl_session_tls12.pem").trim_end()
    );
    let source = r#"<?php
$pem = <<<'PEM'
__SESSION_PEM__PEM;
$session = Openssl\Session::import($pem);
$context = stream_context_create(["ssl" => ["session_data" => $session]]);
$stored = stream_context_get_options($context)["ssl"]["session_data"] ?? null;
echo $stored instanceof Openssl\Session ? "session" : get_debug_type($stored), "\n";
"#
    .replace("__SESSION_PEM__", &pem);
    let out = compile_and_run_with_php_version(&source, PhpVersion::Php86);
    assert_eq!(out, "session\n");
}

/// `fopen('https://...')` resumes a TLS 1.2 session supplied through the PHP 8.6
/// `ssl.session_data` context option against a local OpenSSL server.
#[test]
fn test_openssl_https_session_resumption_php86() {
    if skip_without_openssl_native("test_openssl_https_session_resumption_php86") {
        return;
    }
    if skip_without_openssl_cli("test_openssl_https_session_resumption_php86") {
        return;
    }
    let server = OpensslSessionServer::start(
        crate::codegen::io::streams::TEST_HTTPS_CERT_PEM,
        crate::codegen::io::streams::TEST_HTTPS_KEY_PEM,
    )
    .expect("openssl CLI is available after its skip gate");
    let session_pem = server.seed_session();
    assert!(
        server.wait_for_state("write certificate", 1, std::time::Duration::from_secs(5)),
        "initial handshake transcript missing: {}",
        server.state_log()
    );
    let source = format!(
        r#"<?php
$pem = <<<'PEM'
{session_pem}PEM;
$session = Openssl\Session::import($pem);
$context = stream_context_create(["ssl" => ["verify_peer" => "0", "session_data" => $session]]);
$response = file_get_contents("https://127.0.0.1:{port}/", false, $context);
echo is_string($response) && strpos($response, "Master-Key:") !== false
    ? "resumed-page\n"
    : "response-missing\n";
"#,
        session_pem = session_pem,
        port = server.port()
    );
    let out = compile_and_run_with_php_version(&source, PhpVersion::Php86);
    assert_eq!(out, "resumed-page\n");
    assert!(
        server.wait_for_state("read client hello", 2, std::time::Duration::from_secs(5)),
        "second handshake was not logged: {}",
        server.state_log()
    );
    let log = server.state_log();
    assert_eq!(
        log.matches("write certificate").count(),
        1,
        "the session_data connection must resume, not perform a second full handshake: {log}"
    );
}

/// `ssl.psk_client_cb` re-enters compiled PHP during a TLS 1.3 handshake and
/// authenticates against a certificate-less external-PSK OpenSSL server.
#[test]
fn test_openssl_tls13_psk_client_callback_php86() {
    if skip_without_openssl_native("test_openssl_tls13_psk_client_callback_php86") {
        return;
    }
    if skip_without_openssl_psk_cli("test_openssl_tls13_psk_client_callback_php86") {
        return;
    }
    let server = OpensslPskServer::start("736563726574", "client")
        .expect("openssl CLI is available after its skip gate");
    let source = r#"<?php
$context = stream_context_create([
    "ssl" => [
        "verify_peer" => "0",
        "psk_client_cb" => function ($stream) {
            return new Openssl\Psk("secret", "client");
        },
    ],
]);
$body = file_get_contents("https://127.0.0.1:PHP_TEST_PORT/", false, $context);
echo is_string($body) && strpos($body, "Cipher") !== false ? "psk-ok\n" : "psk-failed\n";
"#
    .replace("PHP_TEST_PORT", &server.port().to_string());
    let out = compile_and_run_with_php_version(&source, PhpVersion::Php86);
    assert_eq!(out, "psk-ok\n");
    assert!(
        server.wait_for_state("SSL_accept:SSLv3/TLS read finished", 1, std::time::Duration::from_secs(5)),
        "PSK handshake was not completed: {}",
        server.state_log()
    );
}

/// `ssl.session_new_cb` receives the TLS 1.3 ticket as an `Openssl\Session`
/// object through the libssl-to-PHP callback adapter.
#[test]
fn test_openssl_session_new_callback_php86() {
    if skip_without_openssl_native("test_openssl_session_new_callback_php86") {
        return;
    }
    if skip_without_openssl_cli("test_openssl_session_new_callback_php86") {
        return;
    }
    let server = OpensslSessionServer::start_tls13_early(
        crate::codegen::io::streams::TEST_HTTPS_CERT_PEM,
        crate::codegen::io::streams::TEST_HTTPS_KEY_PEM,
    )
    .expect("openssl CLI is available after its skip gate");
    let source = r#"<?php
$captured = null;
$context = stream_context_get_default();
stream_context_set_option($context, "ssl", "verify_peer", "0");
stream_context_set_option($context, "ssl", "session_new_cb", function ($stream, $session) use (&$captured) {
    $captured = $session;
});
$body = file_get_contents("https://127.0.0.1:PHP_TEST_PORT/", false, $context);
echo $captured instanceof Openssl\Session ? $captured->getProtocol() : "none", "\n";
echo is_string($body) && strlen($body) > 0 ? "body\n" : "empty\n";
"#
    .replace("PHP_TEST_PORT", &server.port().to_string());
    let out = compile_and_run_with_php_version(&source, PhpVersion::Php86);
    assert_eq!(out, "TLSv1.3\nbody\n");
    assert!(
        server.wait_for_state("write session ticket", 1, std::time::Duration::from_secs(5)),
        "server did not issue a TLS 1.3 ticket: {}",
        server.state_log()
    );
}

/// A TLS 1.3 session resumption sends `ssl.early_data` as 0-RTT; the local
/// OpenSSL server reports `read end of early data` for the connection.
#[test]
fn test_openssl_tls13_session_early_data_php86() {
    if skip_without_openssl_native("test_openssl_tls13_session_early_data_php86") {
        return;
    }
    if skip_without_openssl_cli("test_openssl_tls13_session_early_data_php86") {
        return;
    }
    let server = OpensslSessionServer::start_tls13_early(
        crate::codegen::io::streams::TEST_HTTPS_CERT_PEM,
        crate::codegen::io::streams::TEST_HTTPS_KEY_PEM,
    )
    .expect("openssl CLI is available after its skip gate");
    let session_pem = server.seed_session_tls13();
    assert!(
        session_pem.contains("BEGIN SSL SESSION PARAMETERS"),
        "s_client did not export a TLS 1.3 session"
    );
    let source = r#"<?php
$pem = <<<'PEM'
__SESSION_PEM__PEM;
$session = Openssl\Session::import($pem);
$context = stream_context_get_default();
stream_context_set_option($context, "ssl", "verify_peer", "0");
stream_context_set_option($context, "ssl", "session_data", $session);
stream_context_set_option($context, "ssl", "early_data", "hello-0rtt");
$stream = fopen("https://127.0.0.1:PHP_TEST_PORT/", "r");
$meta = stream_get_meta_data($stream);
echo $meta["crypto"]["early_data"] ?? "missing", "\n";
echo stream_get_contents($stream) !== "" ? "body\n" : "empty\n";
fclose($stream);
"#
    .replace("__SESSION_PEM__", &session_pem)
    .replace("PHP_TEST_PORT", &server.port().to_string());
    let out = compile_and_run_with_php_version(&source, PhpVersion::Php86);
    assert_eq!(out, "accepted\nbody\n");
    assert!(
        server.wait_for_state(
            "TLSv1.3 read end of early data",
            1,
            std::time::Duration::from_secs(5)
        ),
        "server did not read TLS 1.3 early data: {}",
        server.state_log()
    );
}

/// The production `elephc build` pipeline selects the libssl TLS bridge for a
/// program that names `ssl.psk_client_cb` and completes a TLS 1.3 external-PSK
/// handshake against a local OpenSSL server.
#[test]
fn test_openssl_tls13_psk_client_callback_cli_php86() {
    if skip_without_openssl_native("test_openssl_tls13_psk_client_callback_cli_php86") {
        return;
    }
    if skip_without_openssl_psk_cli("test_openssl_tls13_psk_client_callback_cli_php86") {
        return;
    }
    let server = OpensslPskServer::start("736563726574", "client")
        .expect("openssl CLI is available after its skip gate");
    let dir = make_cli_test_dir("elephc_openssl_tls13_psk_client_callback");
    fs::write(
        dir.join("main.php"),
        r#"<?php
$context = stream_context_create([
    "ssl" => [
        "verify_peer" => "0",
        "psk_client_cb" => function ($stream) {
            return new Openssl\Psk("secret", "client");
        },
    ],
]);
$body = file_get_contents("https://127.0.0.1:PHP_TEST_PORT/", false, $context);
echo is_string($body) && strpos($body, "Cipher") !== false ? "psk-ok\n" : "psk-failed\n";
"#
        .replace("PHP_TEST_PORT", &server.port().to_string()),
    )
    .expect("write CLI PSK fixture");
    let Some(mut command) = openssl_cli_command(&dir) else {
        eprintln!("ELEPHC_OPENSSL_NATIVE_SKIP_GATE skipping CLI PSK fixture");
        return;
    };
    let compile = command
        .args(["--php-version", "8.6", "--quiet", "main.php"])
        .output()
        .expect("compile CLI PSK fixture");
    assert!(
        compile.status.success(),
        "CLI PSK link failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(dir.join("main"))
        .output()
        .expect("run CLI PSK fixture");
    assert!(
        run.status.success(),
        "CLI PSK fixture failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "psk-ok\n");
    assert!(
        server.wait_for_state(
            "SSL_accept:SSLv3/TLS read finished",
            1,
            std::time::Duration::from_secs(5)
        ),
        "CLI PSK handshake was not completed: {}",
        server.state_log()
    );
}

/// The production `elephc build` pipeline selects the libssl TLS bridge for a
/// program that names `ssl.session_data`, links the managed `openssl-crypto`
/// package, and resumes the imported session against a local OpenSSL server.
#[test]
fn test_openssl_https_session_resumption_cli_php86() {
    if skip_without_openssl_native("test_openssl_https_session_resumption_cli_php86") {
        return;
    }
    if skip_without_openssl_cli("test_openssl_https_session_resumption_cli_php86") {
        return;
    }
    let server = OpensslSessionServer::start(
        crate::codegen::io::streams::TEST_HTTPS_CERT_PEM,
        crate::codegen::io::streams::TEST_HTTPS_KEY_PEM,
    )
    .expect("openssl CLI is available after its skip gate");
    let session_pem = server.seed_session();
    let dir = make_cli_test_dir("elephc_openssl_https_session_resumption");
    fs::write(
        dir.join("main.php"),
        format!(
            r#"<?php
$pem = <<<'PEM'
{session_pem}PEM;
$session = Openssl\Session::import($pem);
$context = stream_context_create();
stream_context_set_option($context, ["ssl" => ["verify_peer" => "0", "session_data" => $session]]);
$response = file_get_contents("https://127.0.0.1:{port}/", false, $context);
echo is_string($response) && strpos($response, "Master-Key:") !== false
    ? "resumed-page\n"
    : "response-missing\n";
"#,
            session_pem = session_pem,
            port = server.port()
        ),
    )
    .expect("write CLI session resumption fixture");
    let Some(mut command) = openssl_cli_command(&dir) else {
        eprintln!("ELEPHC_OPENSSL_NATIVE_SKIP_GATE skipping CLI session fixture");
        return;
    };
    let compile = command
        .args(["--php-version", "8.6", "--quiet", "main.php"])
        .output()
        .expect("compile CLI session resumption fixture");
    assert!(
        compile.status.success(),
        "CLI session resumption link failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(dir.join("main"))
        .output()
        .expect("run CLI session resumption fixture");
    assert!(
        run.status.success(),
        "CLI session resumption fixture failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "resumed-page\n");
    assert!(
        server.wait_for_state("read client hello", 2, std::time::Duration::from_secs(5)),
        "second handshake was not logged: {}",
        server.state_log()
    );
    let log = server.state_log();
    assert_eq!(
        log.matches("write certificate").count(),
        1,
        "the CLI session_data connection must resume: {log}"
    );
}
