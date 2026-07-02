//! Purpose:
//! The `--web` request prelude: under `--web`, prepends an `extern "elephc_web"`
//! declaration block (Phase 2 Task 2) and executable statements that build the
//! request superglobals ($_SERVER/$_GET/$_POST) on every request (Task 5+).
//!
//! Called from:
//! - `crate::pipeline::compile`, after the other preludes and before name
//!   resolution, gated on `CliConfig.web` (NOT usage detection — it is the only
//!   flag-gated prelude).
//!
//! Key details:
//! - The injected statements run before user top-level code each request because
//!   the prelude statements are prepended and the whole top-level body re-runs
//!   per request.

use crate::parser::ast::{Program, StmtKind};

/// The PHP source prepended under `--web`. Phase 2 Task 2: extern declarations;
/// Task 5: $_SERVER; Task 6: $_GET parsed from the query string; Task 7: $_POST
/// parsed from a `application/x-www-form-urlencoded` body (read binary-safe via
/// `ptr_read_string(elephc_web_body_ptr(), elephc_web_body_len())`). The query/
/// body parsers are built inline (element-by-element into the superglobal),
/// mirroring the $_SERVER pattern, to stay within the type checker's proven
/// capabilities (a helper function returning a freshly-built assoc array trips
/// return-type inference / union widening).
const WEB_PRELUDE_SRC: &str = r#"<?php
extern "elephc_web" {
    function elephc_web_method(): string;
    function elephc_web_uri(): string;
    function elephc_web_path(): string;
    function elephc_web_query_string(): string;
    function elephc_web_header_count(): int;
    function elephc_web_header_name(int $i): string;
    function elephc_web_header_value(int $i): string;
    function elephc_web_body_ptr(): ptr;
    function elephc_web_body_len(): int;
    function elephc_web_remote_addr(): string;
    function elephc_web_remote_port(): int;
    function elephc_web_server_addr(): string;
    function elephc_web_server_port(): int;
    function elephc_web_protocol(): string;
    function elephc_web_request_time(): int;
    function elephc_web_env_count(): int;
    function elephc_web_env_name(int $i): string;
    function elephc_web_env_value(int $i): string;
    function elephc_web_multipart_count(): int;
    function elephc_web_multipart_name(int $i): string;
    function elephc_web_multipart_filename(int $i): string;
    function elephc_web_multipart_type(int $i): string;
    function elephc_web_multipart_value_ptr(int $i): ptr;
    function elephc_web_multipart_value_len(int $i): int;
    function elephc_web_session_reset(): void;
    function elephc_web_session_get_name(): string;
    function elephc_web_session_set_name(string $name): void;
    function elephc_web_session_get_id(): string;
    function elephc_web_session_set_id(string $id): int;
    function elephc_web_session_get_status(): int;
    function elephc_web_session_set_status(int $status): void;
    function elephc_web_session_get_save_path(): string;
    function elephc_web_session_set_save_path(string $path): void;
    function elephc_web_session_get_cache_limiter(): string;
    function elephc_web_session_set_cache_limiter(string $v): void;
    function elephc_web_session_get_cache_expire(): int;
    function elephc_web_session_set_cache_expire(int $v): void;
    function elephc_web_session_get_cookie_lifetime(): int;
    function elephc_web_session_get_cookie_path(): string;
    function elephc_web_session_get_cookie_domain(): string;
    function elephc_web_session_get_cookie_secure(): int;
    function elephc_web_session_get_cookie_httponly(): int;
    function elephc_web_session_get_cookie_samesite(): string;
    function elephc_web_session_set_cookie_params(
        int $lifetime, string $path, string $domain,
        int $secure, int $httponly, string $samesite
    ): void;
    function elephc_web_session_read(
        string $id, string $save_path, int $read_and_close
    ): string;
    function elephc_web_session_write(
        string $id, string $save_path, string $data
    ): int;
    function elephc_web_session_destroy(
        string $id, string $save_path
    ): int;
    function elephc_web_session_abort(
        string $id, string $save_path
    ): int;
    function elephc_web_session_create_id(string $prefix): string;
    function elephc_web_session_gc(
        string $save_path, int $maxlifetime
    ): int;
    function elephc_web_session_count_entries(string $data): int;
    function elephc_web_session_entry_key(string $data, int $idx): string;
    function elephc_web_session_entry_value(string $data, int $idx): string;
}
elephc_web_session_reset();
if (!defined('PHP_SESSION_NONE')) {
    define('PHP_SESSION_DISABLED', 0);
    define('PHP_SESSION_NONE', 1);
    define('PHP_SESSION_ACTIVE', 2);
}
$_SERVER = [];
$_SERVER['REQUEST_METHOD'] = elephc_web_method();
$_SERVER['REQUEST_URI']    = elephc_web_uri();
$_SERVER['QUERY_STRING']   = elephc_web_query_string();
$__elephc_hc = elephc_web_header_count();
for ($__elephc_i = 0; $__elephc_i < $__elephc_hc; $__elephc_i++) {
    $__elephc_hn = elephc_web_header_name($__elephc_i);
    $__elephc_hv = elephc_web_header_value($__elephc_i);
    $_SERVER['HTTP_' . strtoupper(str_replace('-', '_', $__elephc_hn))] = $__elephc_hv;
    $__elephc_up = strtoupper($__elephc_hn);
    if ($__elephc_up === 'CONTENT-TYPE') { $_SERVER['CONTENT_TYPE'] = $__elephc_hv; }
    if ($__elephc_up === 'CONTENT-LENGTH') { $_SERVER['CONTENT_LENGTH'] = $__elephc_hv; }
}
$_SERVER['REMOTE_ADDR']       = elephc_web_remote_addr();
$_SERVER['REMOTE_PORT']       = elephc_web_remote_port();
$_SERVER['SERVER_ADDR']       = elephc_web_server_addr();
$_SERVER['SERVER_PORT']       = elephc_web_server_port();
$_SERVER['SERVER_NAME']       = elephc_web_server_addr();
$_SERVER['SERVER_PROTOCOL']   = elephc_web_protocol();
$_SERVER['REQUEST_TIME']      = elephc_web_request_time();
$_SERVER['REQUEST_SCHEME']    = 'http';
$_SERVER['GATEWAY_INTERFACE'] = 'CGI/1.1';
$_SERVER['SERVER_SOFTWARE']   = 'elephc';
$_GET = [];
$__elephc_qs = elephc_web_query_string();
if ($__elephc_qs !== '') {
    $__elephc_pairs = explode('&', $__elephc_qs);
    foreach ($__elephc_pairs as $__elephc_pair) {
        $__elephc_eq = strpos($__elephc_pair, '=');
        if ($__elephc_eq === false) {
            if ($__elephc_pair !== '') {
                $_GET[rawurldecode($__elephc_pair)] = '';
            }
        } else {
            $__elephc_gk = rawurldecode(substr($__elephc_pair, 0, $__elephc_eq));
            $__elephc_gv = rawurldecode(substr($__elephc_pair, $__elephc_eq + 1));
            $_GET[$__elephc_gk] = $__elephc_gv;
        }
    }
}
$_POST = [];
$__elephc_ct = isset($_SERVER['CONTENT_TYPE']) ? $_SERVER['CONTENT_TYPE'] : '';
if (strpos(strtoupper($__elephc_ct), 'APPLICATION/X-WWW-FORM-URLENCODED') !== false) {
    $__elephc_body = ptr_read_string(elephc_web_body_ptr(), elephc_web_body_len());
    if ($__elephc_body !== '') {
        $__elephc_ppairs = explode('&', $__elephc_body);
        foreach ($__elephc_ppairs as $__elephc_ppair) {
            $__elephc_peq = strpos($__elephc_ppair, '=');
            if ($__elephc_peq === false) {
                if ($__elephc_ppair !== '') {
                    $_POST[rawurldecode($__elephc_ppair)] = '';
                }
            } else {
                $__elephc_pk = rawurldecode(substr($__elephc_ppair, 0, $__elephc_peq));
                $__elephc_pv = rawurldecode(substr($__elephc_ppair, $__elephc_peq + 1));
                $_POST[$__elephc_pk] = $__elephc_pv;
            }
        }
    }
}
$_FILES = [];
if (strpos(strtoupper($__elephc_ct), 'MULTIPART/FORM-DATA') !== false) {
    $__elephc_mpc = elephc_web_multipart_count();
    for ($__elephc_mpi = 0; $__elephc_mpi < $__elephc_mpc; $__elephc_mpi++) {
        $__elephc_mpn = elephc_web_multipart_name($__elephc_mpi);
        $__elephc_mpf = elephc_web_multipart_filename($__elephc_mpi);
        $__elephc_mpv = ptr_read_string(elephc_web_multipart_value_ptr($__elephc_mpi), elephc_web_multipart_value_len($__elephc_mpi));
        if ($__elephc_mpf === '') {
            $_POST[$__elephc_mpn] = $__elephc_mpv;
        } else {
            $__elephc_mptmp = tempnam(sys_get_temp_dir(), 'elephc_up');
            if ($__elephc_mptmp !== false) {
                file_put_contents($__elephc_mptmp, $__elephc_mpv);
                $_FILES[$__elephc_mpn] = [
                    'name' => $__elephc_mpf,
                    'type' => elephc_web_multipart_type($__elephc_mpi),
                    'tmp_name' => $__elephc_mptmp,
                    'error' => 0,
                    'size' => strlen($__elephc_mpv),
                ];
            }
        }
    }
}
$_COOKIE = [];
$__elephc_ck = isset($_SERVER['HTTP_COOKIE']) ? $_SERVER['HTTP_COOKIE'] : '';
if ($__elephc_ck !== '') {
    $__elephc_cpairs = explode(';', $__elephc_ck);
    foreach ($__elephc_cpairs as $__elephc_cpair) {
        $__elephc_ceq = strpos($__elephc_cpair, '=');
        if ($__elephc_ceq !== false) {
            $__elephc_cknm = trim(substr($__elephc_cpair, 0, $__elephc_ceq));
            $__elephc_cv = rawurldecode(trim(substr($__elephc_cpair, $__elephc_ceq + 1)));
            if ($__elephc_cknm !== '') {
                $_COOKIE[$__elephc_cknm] = $__elephc_cv;
            }
        }
    }
}
$_REQUEST = [];
foreach ($_GET as $__elephc_rqk => $__elephc_rqv) {
    $_REQUEST[$__elephc_rqk] = $__elephc_rqv;
}
foreach ($_POST as $__elephc_rqk => $__elephc_rqv) {
    $_REQUEST[$__elephc_rqk] = $__elephc_rqv;
}
$_ENV = [];
$__elephc_envc = elephc_web_env_count();
for ($__elephc_envi = 0; $__elephc_envi < $__elephc_envc; $__elephc_envi++) {
    $_ENV[elephc_web_env_name($__elephc_envi)] = elephc_web_env_value($__elephc_envi);
}
function __elephc_emit_cookie($name, $value, $expires, $path, $domain, $secure, $httponly) {
    $c = $name . '=' . $value;
    if ($expires != 0) {
        $c = $c . '; expires=' . gmdate('D, d-M-Y H:i:s', $expires) . ' GMT';
        $c = $c . '; Max-Age=' . ($expires - time());
    }
    if ($path !== '') { $c = $c . '; path=' . $path; }
    if ($domain !== '') { $c = $c . '; domain=' . $domain; }
    if ($secure) { $c = $c . '; secure'; }
    if ($httponly) { $c = $c . '; HttpOnly'; }
    header('Set-Cookie: ' . $c, false);
    return true;
}
function setcookie($name, $value = '', $expires = 0, $path = '', $domain = '', $secure = false, $httponly = false) {
    return __elephc_emit_cookie($name, rawurlencode($value), $expires, $path, $domain, $secure, $httponly);
}
function setrawcookie($name, $value = '', $expires = 0, $path = '', $domain = '', $secure = false, $httponly = false) {
    return __elephc_emit_cookie($name, $value, $expires, $path, $domain, $secure, $httponly);
}
function session_start(mixed $options = []): bool {
    $status = elephc_web_session_get_status();
    if ($status === 2) { return true; }
    $__elephc_opt_name = null;
    $__elephc_opt_save_path = null;
    $__elephc_opt_read_and_close = false;
    $__elephc_opt_cl = null;
    $__elephc_opt_cp = null;
    $__elephc_opt_cd = null;
    $__elephc_opt_cs = null;
    $__elephc_opt_ch = null;
    $__elephc_opt_css = null;
    $__elephc_opt_cachelim = null;
    $__elephc_opt_cacheexp = null;
    foreach ($options as $__elephc_ok => $__elephc_ov) {
        if ($__elephc_ok === 'name') { $__elephc_opt_name = $__elephc_ov; }
        if ($__elephc_ok === 'save_path') { $__elephc_opt_save_path = $__elephc_ov; }
        if ($__elephc_ok === 'read_and_close') { $__elephc_opt_read_and_close = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_lifetime') { $__elephc_opt_cl = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_path') { $__elephc_opt_cp = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_domain') { $__elephc_opt_cd = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_secure') { $__elephc_opt_cs = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_httponly') { $__elephc_opt_ch = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_samesite') { $__elephc_opt_css = $__elephc_ov; }
        if ($__elephc_ok === 'cache_limiter') { $__elephc_opt_cachelim = $__elephc_ov; }
        if ($__elephc_ok === 'cache_expire') { $__elephc_opt_cacheexp = $__elephc_ov; }
    }
    if ($__elephc_opt_name !== null) {
        elephc_web_session_set_name((string)$__elephc_opt_name);
    }
    if ($__elephc_opt_save_path !== null) {
        elephc_web_session_set_save_path((string)$__elephc_opt_save_path);
    }
    $read_and_close = 0;
    if ($__elephc_opt_read_and_close) {
        $read_and_close = 1;
    }
    $__elephc_ss_cl = elephc_web_session_get_cookie_lifetime();
    $__elephc_ss_cp = elephc_web_session_get_cookie_path();
    $__elephc_ss_cd = elephc_web_session_get_cookie_domain();
    $__elephc_ss_cs = elephc_web_session_get_cookie_secure();
    $__elephc_ss_ch = elephc_web_session_get_cookie_httponly();
    $__elephc_ss_css = elephc_web_session_get_cookie_samesite();
    if ($__elephc_opt_cl !== null) { $__elephc_ss_cl = (int)$__elephc_opt_cl; }
    if ($__elephc_opt_cp !== null) { $__elephc_ss_cp = (string)$__elephc_opt_cp; }
    if ($__elephc_opt_cd !== null) { $__elephc_ss_cd = (string)$__elephc_opt_cd; }
    if ($__elephc_opt_cs !== null) { $__elephc_ss_cs = (int)$__elephc_opt_cs; }
    if ($__elephc_opt_ch !== null) { $__elephc_ss_ch = (int)$__elephc_opt_ch; }
    if ($__elephc_opt_css !== null) { $__elephc_ss_css = (string)$__elephc_opt_css; }
    elephc_web_session_set_cookie_params(
        $__elephc_ss_cl, $__elephc_ss_cp, $__elephc_ss_cd,
        $__elephc_ss_cs, $__elephc_ss_ch, $__elephc_ss_css
    );
    if ($__elephc_opt_cachelim !== null) {
        elephc_web_session_set_cache_limiter((string)$__elephc_opt_cachelim);
    }
    if ($__elephc_opt_cacheexp !== null) {
        elephc_web_session_set_cache_expire((int)$__elephc_opt_cacheexp);
    }
    $name = elephc_web_session_get_name();
    $save_path = elephc_web_session_get_save_path();
    $id = elephc_web_session_get_id();
    if ($id === '') {
        if (isset($_COOKIE[$name])) {
            $id = (string)$_COOKIE[$name];
        }
    }
    if ($id === '') {
        $id = elephc_web_session_create_id('');
    }
    elephc_web_session_set_id((string)$id);
    $raw = elephc_web_session_read((string)$id, $save_path, $read_and_close);
    if ($raw !== '') {
        __elephc_session_decode($raw);
    }
    if ($read_and_close === 0) {
        __elephc_session_send_cookie();
    }
    if ($read_and_close === 1) {
        elephc_web_session_set_status(1);
    } else {
        elephc_web_session_set_status(2);
    }
    __elephc_session_send_cache_headers();
    return true;
}
function session_write_close(bool $commit = true): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    $encoded = __elephc_session_encode();
    elephc_web_session_write($id, $save_path, $encoded);
    elephc_web_session_set_status(1);
    return true;
}
function session_destroy(): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    elephc_web_session_destroy($id, $save_path);
    $_SESSION = [];
    elephc_web_session_set_status(1);
    elephc_web_session_set_id('');
    return true;
}
function session_id(?string $id = null): string|false {
    if ($id !== null && elephc_web_session_get_status() === 2) {
        return false;
    }
    $old = elephc_web_session_get_id();
    if ($id !== null) { elephc_web_session_set_id((string)$id); }
    return $old;
}
function session_name(?string $name = null): string|false {
    if ($name !== null && elephc_web_session_get_status() === 2) {
        return false;
    }
    $old = elephc_web_session_get_name();
    if ($name !== null) { elephc_web_session_set_name((string)$name); }
    return $old;
}
function session_status(): int {
    return elephc_web_session_get_status();
}
function session_unset(): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    $_SESSION = [];
    return true;
}
function session_encode(): string|false {
    if (elephc_web_session_get_status() !== 2) { return false; }
    return __elephc_session_encode();
}
function session_decode(string $data): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    __elephc_session_decode($data);
    return true;
}
function session_save_path(?string $path = null): string|false {
    if ($path !== null && elephc_web_session_get_status() === 2) { return false; }
    $old = elephc_web_session_get_save_path();
    if ($path !== null) { elephc_web_session_set_save_path((string)$path); }
    return $old;
}
function session_regenerate_id(bool $delete_old = false): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    $old_id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    if ($delete_old) {
        elephc_web_session_destroy($old_id, $save_path);
    }
    $new_id = elephc_web_session_create_id('');
    elephc_web_session_set_id($new_id);
    __elephc_session_send_cookie();
    return true;
}
function session_create_id(string $prefix = ""): string|false {
    return elephc_web_session_create_id($prefix);
}
function session_gc(): int|false {
    $save_path = elephc_web_session_get_save_path();
    return elephc_web_session_gc($save_path, 1440);
}
function session_abort(): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    elephc_web_session_abort($id, $save_path);
    $_SESSION = [];
    elephc_web_session_set_status(1);
    return true;
}
function session_reset(): bool {
    if (elephc_web_session_get_status() !== 2) { return false; }
    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    $raw = elephc_web_session_read($id, $save_path, 0);
    $_SESSION = [];
    if ($raw !== '') { __elephc_session_decode($raw); }
    return true;
}
function session_cache_limiter(?string $value = null): string|false {
    if ($value !== null && elephc_web_session_get_status() === 2) { return false; }
    $old = elephc_web_session_get_cache_limiter();
    if ($value !== null) { elephc_web_session_set_cache_limiter((string)$value); }
    return $old;
}
function session_cache_expire(?int $value = null): int|false {
    if ($value !== null && elephc_web_session_get_status() === 2) { return false; }
    $old = elephc_web_session_get_cache_expire();
    if ($value !== null) { elephc_web_session_set_cache_expire((int)$value); }
    return $old;
}
function session_get_cookie_params(): array {
    return [
        'lifetime' => elephc_web_session_get_cookie_lifetime(),
        'path' => elephc_web_session_get_cookie_path(),
        'domain' => elephc_web_session_get_cookie_domain(),
        'secure' => (bool)elephc_web_session_get_cookie_secure(),
        'httponly' => (bool)elephc_web_session_get_cookie_httponly(),
        'samesite' => elephc_web_session_get_cookie_samesite(),
    ];
}
function session_set_cookie_params(mixed ...$args): bool {
    $__elephc_scp_cl = elephc_web_session_get_cookie_lifetime();
    $__elephc_scp_cp = elephc_web_session_get_cookie_path();
    $__elephc_scp_cd = elephc_web_session_get_cookie_domain();
    $__elephc_scp_cs = elephc_web_session_get_cookie_secure();
    $__elephc_scp_ch = elephc_web_session_get_cookie_httponly();
    $__elephc_scp_css = elephc_web_session_get_cookie_samesite();
    if (count($args) === 1 && is_array($args[0])) {
        $__elephc_scp_arr = (array)$args[0];
        foreach ($__elephc_scp_arr as $__elephc_scp_k => $__elephc_scp_v) {
            if ($__elephc_scp_k === 'lifetime') { $__elephc_scp_cl = (int)$__elephc_scp_v; }
            if ($__elephc_scp_k === 'path') { $__elephc_scp_cp = (string)$__elephc_scp_v; }
            if ($__elephc_scp_k === 'domain') { $__elephc_scp_cd = (string)$__elephc_scp_v; }
            if ($__elephc_scp_k === 'secure') { $__elephc_scp_cs = (int)$__elephc_scp_v; }
            if ($__elephc_scp_k === 'httponly') { $__elephc_scp_ch = (int)$__elephc_scp_v; }
            if ($__elephc_scp_k === 'samesite') { $__elephc_scp_css = (string)$__elephc_scp_v; }
        }
    } else {
        if (count($args) > 0) { $__elephc_scp_cl = (int)$args[0]; }
        if (count($args) > 1) { $__elephc_scp_cp = (string)$args[1]; }
        if (count($args) > 2) { $__elephc_scp_cd = (string)$args[2]; }
        if (count($args) > 3) { $__elephc_scp_cs = (int)$args[3]; }
        if (count($args) > 4) { $__elephc_scp_ch = (int)$args[4]; }
        if (count($args) > 5) { $__elephc_scp_css = (string)$args[5]; }
    }
    elephc_web_session_set_cookie_params(
        $__elephc_scp_cl, $__elephc_scp_cp, $__elephc_scp_cd,
        $__elephc_scp_cs, $__elephc_scp_ch, $__elephc_scp_css
    );
    return true;
}
function session_commit(): bool {
    return session_write_close();
}
function session_register_shutdown(): void {
}
function session_module_name(?string $module = null): string|false {
    if ($module !== null && $module !== 'files') { return false; }
    return 'files';
}
function session_set_save_handler(): bool {
    return false;
}
function __elephc_session_encode(): string {
    $out = '';
    foreach ($_SESSION as $k => $v) {
        $out .= $k . '|' . serialize($v);
    }
    return $out;
}
function __elephc_session_decode(string $raw): void {
    $count = elephc_web_session_count_entries($raw);
    for ($i = 0; $i < $count; $i++) {
        $key = elephc_web_session_entry_key($raw, $i);
        $val = elephc_web_session_entry_value($raw, $i);
        $_SESSION[$key] = unserialize($val);
    }
}
function __elephc_session_send_cookie(): void {
    $name = elephc_web_session_get_name();
    $id = elephc_web_session_get_id();
    $lifetime = elephc_web_session_get_cookie_lifetime();
    $path = elephc_web_session_get_cookie_path();
    $domain = elephc_web_session_get_cookie_domain();
    $secure = (bool)elephc_web_session_get_cookie_secure();
    $httponly = (bool)elephc_web_session_get_cookie_httponly();
    $samesite = elephc_web_session_get_cookie_samesite();
    $cookie = $name . '=' . $id;
    if ($lifetime > 0) {
        $cookie .= '; expires=' . gmdate('D, d-M-Y H:i:s', time() + $lifetime) . ' GMT';
        $cookie .= '; Max-Age=' . $lifetime;
    }
    if ($path !== '') { $cookie .= '; path=' . $path; }
    if ($domain !== '') { $cookie .= '; domain=' . $domain; }
    if ($secure) { $cookie .= '; secure'; }
    if ($httponly) { $cookie .= '; HttpOnly'; }
    if ($samesite !== '') { $cookie .= '; SameSite=' . $samesite; }
    header('Set-Cookie: ' . $cookie, false);
}
function __elephc_session_send_cache_headers(): void {
    $limiter = elephc_web_session_get_cache_limiter();
    if ($limiter === 'nocache') {
        header('Cache-Control: no-store, no-cache, must-revalidate');
        header('Expires: Thu, 19 Nov 1981 08:52:00 GMT');
    } elseif ($limiter === 'public') {
        $expire = elephc_web_session_get_cache_expire();
        header('Cache-Control: public, max-age=' . ($expire * 60));
    } elseif ($limiter === 'private') {
        $expire = elephc_web_session_get_cache_expire();
        header('Cache-Control: private, max-age=' . ($expire * 60));
    } elseif ($limiter === 'private_no_expire') {
        header('Cache-Control: private, max-age=' . (elephc_web_session_get_cache_expire() * 60));
    }
}
"#;

/// The catch-all wrapper: the whole handler body is placed inside its `try` so an
/// uncaught exception sets a 500 status instead of crashing the worker (the
/// process would otherwise die and the master would respawn it, dropping the
/// connection). The `0;` placeholder body is replaced with the real statements.
const WEB_WRAP_SRC: &str =
    "<?php try { $__elephc_wrap = 0; } catch (\\Throwable $__elephc_exc) { http_response_code(500); } finally { if (elephc_web_session_get_status() === 2) { session_write_close(); } }";

/// Prepends the web prelude when compiling with `--web` and wraps the whole
/// handler body in a catch-all `try`/`catch` so uncaught exceptions become a 500.
/// Returns the program unchanged otherwise.
pub fn inject_if_web(program: Program, web: bool) -> Program {
    if !web {
        return program;
    }
    let tokens = crate::lexer::tokenize(WEB_PRELUDE_SRC).expect("web prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("web prelude must parse");
    combined.extend(program);

    // The catch-all try wrap below reorders the top level (declarations hoisted
    // out, executables wrapped). That reordering is unsafe across namespace
    // boundaries: a `namespace X;` / `namespace X { … }` would be separated from
    // the declarations it scopes, leaving them in the wrong namespace. For
    // namespaced programs (e.g. a framework with `App\…` classes) skip the wrap
    // entirely — such programs do their own error handling — and keep B1's
    // uncaught-exception → 500 net only for flat, non-namespaced programs.
    if combined
        .iter()
        .any(|s| matches!(s.kind, StmtKind::NamespaceDecl { .. } | StmtKind::NamespaceBlock { .. }))
    {
        return combined;
    }

    // Partition the top level: hoistable declarations (functions, classes, externs)
    // stay outside the try so they resolve normally — externs in particular are NOT
    // resolved when nested in a try. Everything executable goes inside a catch-all
    // try so an uncaught exception becomes a 500 instead of crashing the worker.
    let mut decls: Program = Vec::new();
    let mut exec: Program = Vec::new();
    for stmt in combined {
        if is_hoistable_decl(&stmt.kind) {
            decls.push(stmt);
        } else {
            exec.push(stmt);
        }
    }

    let wrap_tokens = crate::lexer::tokenize(WEB_WRAP_SRC).expect("web wrapper must tokenize");
    let mut wrapper = crate::parser::parse(&wrap_tokens).expect("web wrapper must parse");
    if let Some(stmt) = wrapper.first_mut() {
        if let StmtKind::Try { try_body, .. } = &mut stmt.kind {
            *try_body = exec;
            decls.extend(wrapper);
            return decls;
        }
    }
    // Parser invariant changed unexpectedly; fall back to the unwrapped body.
    decls.extend(exec);
    decls
}

/// Returns true for top-level statement kinds that are position-independent
/// declarations (hoisted by the resolver), so they can be kept outside the
/// catch-all `try` that wraps the executable handler body.
fn is_hoistable_decl(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. }
    )
}
