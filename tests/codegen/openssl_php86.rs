//! Purpose:
//! PHP 8.6's `Openssl\OpensslException` and `Openssl\Psk` value classes.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext openssl_php86`.
//!
//! Key details:
//! - The classes are 8.6-only; the fixture compiles under `--php-version=8.6`.
//! - `Psk` is the pure PHP-level half of the release's OpenSSL additions; the TLS session
//!   transport remains tracked separately.

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
