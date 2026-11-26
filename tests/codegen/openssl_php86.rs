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
