//! Purpose:
//! Native HTTP regressions for session behaviors compared with frozen PHP 8.5.11.
//!
//! Called from:
//! - `tests/web_session_tests.rs` through its shared compile/server helpers.
//!
//! Key details:
//! - Every server has private session storage; warnings are captured in PHP code.
//! - Cases exercise public session functions rather than invoking bridge helpers directly.

use super::*;

/// Runs one request with private storage and returns the response after reaping the server.
fn request(source: &str) -> String {
    let dir = make_test_dir("session_compliance");
    let bin = compile_web(&dir, source, "app");
    let addr = format!("127.0.0.1:{}", free_port());
    let storage = dir.to_str().unwrap();
    let mut server = spawn_server_with_env(&bin, &addr, "1", &[
        ("ELEPHC_SESSION_SAVE_PATH", storage), ("TMPDIR", storage),
    ]);
    let response = http_get(&addr, "/");
    let _ = server.kill();
    let _ = server.wait();
    response.split_once("\r\n\r\n").unwrap().1.to_owned()
}

/// Invalid trailing session data must cancel the session and discard already decoded values.
#[test]
fn session_compliance_invalid_decode_cancels_session() {
    let output = request(r#"<?php
ob_start();
set_error_handler(function(int $level, string $message): bool { echo $message, "\n"; return true; });
session_start(); $_SESSION['keep'] = 9;
var_dump(session_decode('x|i:1;broken'), session_status(), session_id(), $_SESSION);
ob_end_flush();
"#);
    assert_eq!(output, concat!(
        "session_decode(): Failed to decode session object. Session has been destroyed\n",
        "bool(false)\nint(1)\nstring(0) \"\"\narray(0) {\n}\n",
    ));
}

/// A false value is valid serialized input and decoding must preserve unrelated keys.
#[test]
fn session_compliance_valid_false_is_not_a_decode_failure() {
    let output = request(r#"<?php
ob_start();
session_start(); $_SESSION['keep'] = 9;
var_dump(session_decode('flag|b:0;'), $_SESSION['flag'], $_SESSION['keep']); session_destroy();
ob_end_flush();
"#);
    assert_eq!(output, "bool(true)\nbool(false)\nint(9)\n");
}

/// session_reset must read the held file again, without reopening and self-deadlocking.
#[test]
fn session_compliance_reset_rereads_modified_file() {
    let output = request(r#"<?php
ob_start();
session_start(); $id = (string)session_id(); $_SESSION['x'] = 'initial'; session_write_close();
session_id($id); session_start(); $_SESSION['x'] = 'memory';
file_put_contents(session_save_path() . '/sess_' . $id, 'x|s:7:"on-disk";');
var_dump(session_reset(), $_SESSION['x']); session_destroy();
ob_end_flush();
"#);
    assert_eq!(output, "bool(true)\nstring(7) \"on-disk\"\n");
}

/// PHP reports true for an initially active reset but deactivates on a handler read failure.
#[test]
fn session_compliance_reset_failed_handler_read_deactivates() {
    let output = request(r#"<?php
ob_start();
class H implements SessionHandlerInterface {
    public int $reads = 0;
    public function open(string $path, string $name): bool { return true; }
    public function close(): bool { return true; }
    public function read(string $id): string|false {
        $this->reads++; if ($this->reads > 1) { return false; } return 'x|i:1;';
    }
    public function write(string $id, string $data): bool { return true; }
    public function destroy(string $id): bool { return true; }
    public function gc(int $max_lifetime): int|false { return 0; }
}
set_error_handler(function(int $level, string $message): bool { echo substr($message, 0, 56), "\n"; return true; });
$h = new H(); session_set_save_handler($h); session_start(); $_SESSION['x'] = 2;
var_dump(session_reset(), session_status(), $_SESSION);
ob_end_flush();
"#);
    assert_eq!(output, concat!(
        "session_reset(): Failed to read session data: user (path",
        "\nbool(true)\nint(1)\narray(0) {\n}\n",
    ));
}

/// php and php_binary skip integer keys with warnings, while php_serialize preserves them.
#[test]
fn session_compliance_serializers_skip_numeric_keys_selectively() {
    let output = request(r#"<?php
ob_start();
set_error_handler(function(int $level, string $message): bool { echo $message, "\n"; return true; });
foreach (['php', 'php_binary', 'php_serialize'] as $handler) {
    ini_set('session.serialize_handler', $handler); session_start();
    $_SESSION = [0 => 'integer', '0x' => 'string'];
    echo bin2hex((string)session_encode()), "\n"; session_destroy();
}
ob_end_flush();
"#);
    assert_eq!(output, concat!(
        "session_encode(): Skipping numeric key 0\n30787c733a363a22737472696e67223b\n",
        "session_encode(): Skipping numeric key 0\n023078733a363a22737472696e67223b\n",
        "613a323a7b693a303b733a373a22696e7465676572223b733a323a223078223b733a363a22737472696e67223b7d\n",
    ));
}

/// An inactive decode must emit the PHP warning instead of returning false silently.
#[test]
fn session_compliance_decode_without_active_session_warns() {
    let output = request(r#"<?php
ob_start();
set_error_handler(function(int $level, string $message): bool { echo $message, "\n"; return true; });
var_dump(session_decode('x|i:1;'));
ob_end_flush();
"#);
    assert_eq!(output, concat!(
        "session_decode(): Session data cannot be decoded when there is no active session\n",
        "bool(false)\n",
    ));
}

/// Session warnings preserve engine severity bits and respect a registered handler's mask.
#[test]
fn session_compliance_warning_handler_mask_uses_engine_level() {
    let output = request(r#"<?php
ob_start();
set_error_handler(function(int $level, string $message): bool { echo 'unexpected'; return true; }, E_USER_WARNING);
var_dump(session_decode('x|i:1;')); restore_error_handler();
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; }, E_WARNING);
var_dump(session_decode('x|i:1;')); restore_error_handler();
ob_end_flush();
"#);
    assert_eq!(output, "bool(false)\n2|bool(false)\n");
}

/// An explicit empty user ID clears state while the generated-ID setter stays fail-closed.
#[test]
fn session_compliance_explicit_empty_id_clears_previous_identifier() {
    let output = request(r#"<?php
ob_start();
session_id('manual'); var_dump(session_id(''), session_id());
ob_end_flush();
"#);
    assert_eq!(output, "string(6) \"manual\"\nstring(0) \"\"\n");
}

/// References spanning session entries must retain their aliasing after one shared decode.
#[test]
fn session_compliance_decode_keeps_cross_entry_array_references() {
    let output = request(r#"<?php
ob_start();
session_start();
var_dump(session_decode('first|a:1:{i:0;i:1;}second|R:1;'));
$_SESSION['first'][0] = 99; var_dump($_SESSION['second'][0]); session_destroy();
ob_end_flush();
"#);
    assert_eq!(output, "bool(true)\nint(99)\n");
}

/// php_serialize accepts scalar roots, including a valid false result from unserialize.
#[test]
fn session_compliance_php_serialize_preserves_scalar_roots() {
    let output = request(r#"<?php
ob_start();
ini_set('session.serialize_handler', 'php_serialize'); session_start();
var_dump(session_decode('i:1;'), $_SESSION);
var_dump(session_decode('b:0;'), $_SESSION);
var_dump(session_decode('N;'), $_SESSION); session_destroy();
ob_end_flush();
"#);
    assert_eq!(output, "bool(true)\nint(1)\nbool(true)\nbool(false)\nbool(true)\narray(0) {\n}\n");
}
