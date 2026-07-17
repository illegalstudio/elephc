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
//! - Optional session functions are retained through AST reachability, while
//!   unknown dynamic calls conservatively keep the complete PHP prelude.
//! - Legacy callable-handler dispatch is injected only when user code can reach
//!   `session_set_save_handler()`.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::parser::ast::{Program, StmtKind};

mod usage;

/// Maintained PHP minor selected for version-dependent compatibility behavior.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PhpVersion {
    /// PHP 8.2 compatibility.
    Php82,
    /// PHP 8.3 compatibility.
    Php83,
    /// PHP 8.4 compatibility.
    Php84,
    /// PHP 8.5 compatibility, the default and newest maintained profile.
    #[default]
    Php85,
}

impl PhpVersion {
    /// Parses one of the maintained `major.minor` spellings.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "8.2" => Some(Self::Php82),
            "8.3" => Some(Self::Php83),
            "8.4" => Some(Self::Php84),
            "8.5" => Some(Self::Php85),
            _ => None,
        }
    }

    /// Returns PHP's numeric `PHP_VERSION_ID` representation for this profile.
    pub const fn version_id(self) -> u32 {
        match self {
            Self::Php82 => 80200,
            Self::Php83 => 80300,
            Self::Php84 => 80400,
            Self::Php85 => 80500,
        }
    }
}

/// The PHP source prepended under `--web`. Phase 2 Task 2: extern declarations;
/// Task 5: $_SERVER; Task 6: $_GET parsed from the query string; Task 7: $_POST
/// parsed from a `application/x-www-form-urlencoded` body (read binary-safe via
/// `__elephc_ptr_read_string(elephc_web_body_ptr(), elephc_web_body_len())`). The query/
/// body parsers are built inline (element-by-element into the superglobal),
/// mirroring the $_SERVER pattern, to stay within the type checker's proven
/// capabilities (a helper function returning a freshly-built assoc array trips
/// return-type inference / union widening).
pub(crate) const WEB_PRELUDE_SRC: &str = r#"<?php
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
    function elephc_web_session_get_cookie_partitioned(): int;
    function elephc_web_session_get_cookie_httponly(): int;
    function elephc_web_session_get_cookie_samesite(): string;
    function elephc_web_session_set_cookie_params(
        int $lifetime, string $path, string $domain,
        int $secure, int $partitioned, int $httponly, string $samesite
    ): void;
    function elephc_web_session_data_stage(int $len): ptr;
    function elephc_web_session_data_len(): int;
    function elephc_web_session_read_bytes(
        string $id, string $save_path, int $read_and_close
    ): ptr;
    function elephc_web_session_last_read_ok(): int;
    function elephc_web_session_write_bytes(
        string $id, string $save_path, ptr $data, int $data_len
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
    function elephc_web_session_count_entries_bytes(ptr $data, int $data_len): int;
    function elephc_web_session_entry_key_bytes(ptr $data, int $data_len, int $idx): ptr;
    function elephc_web_session_entry_value_bytes(ptr $data, int $data_len, int $idx): ptr;
    function elephc_web_session_snapshot_bytes(): ptr;
    function elephc_web_session_file_exists(string $id, string $save_path): int;
    function elephc_web_session_touch(string $id, string $save_path): int;
    function elephc_web_session_should_gc(): int;
    function elephc_web_session_get_strict_mode(): int;
    function elephc_web_session_set_strict_mode(int $v): void;
    function elephc_web_session_get_serialize_handler(): string;
    function elephc_web_session_set_serialize_handler(string $v): void;
    function elephc_web_session_get_gc_probability(): int;
    function elephc_web_session_set_gc_probability(int $v): void;
    function elephc_web_session_get_gc_divisor(): int;
    function elephc_web_session_set_gc_divisor(int $v): void;
    function elephc_web_session_get_gc_maxlifetime(): int;
    function elephc_web_session_set_gc_maxlifetime(int $v): void;
    function elephc_web_session_get_sid_length(): int;
    function elephc_web_session_set_sid_length(int $v): int;
    function elephc_web_session_get_sid_bits_per_character(): int;
    function elephc_web_session_set_sid_bits_per_character(int $v): int;
    function elephc_web_session_count_entries_bin_bytes(ptr $data, int $data_len): int;
    function elephc_web_session_entry_key_bin_bytes(ptr $data, int $data_len, int $idx): ptr;
    function elephc_web_session_entry_value_bin_bytes(ptr $data, int $data_len, int $idx): ptr;
    function elephc_web_session_get_referer_check(): string;
    function elephc_web_session_set_referer_check(string $v): void;
    function elephc_web_session_get_use_only_cookies(): int;
    function elephc_web_session_set_use_only_cookies(int $v): void;
    function elephc_web_session_get_use_cookies(): int;
    function elephc_web_session_set_use_cookies(int $v): void;
    function elephc_web_session_get_lazy_write(): int;
    function elephc_web_session_set_lazy_write(int $v): void;
    function elephc_web_session_get_use_trans_sid(): int;
    function elephc_web_session_set_use_trans_sid(int $v): void;
    function elephc_web_session_get_trans_sid_tags(): string;
    function elephc_web_session_set_trans_sid_tags(string $v): void;
    function elephc_web_session_get_trans_sid_hosts(): string;
    function elephc_web_session_set_trans_sid_hosts(string $v): void;
    function elephc_web_session_get_upload_progress_enabled(): int;
    function elephc_web_session_set_upload_progress_enabled(int $v): void;
    function elephc_web_session_get_upload_progress_cleanup(): int;
    function elephc_web_session_set_upload_progress_cleanup(int $v): void;
    function elephc_web_session_get_upload_progress_prefix(): string;
    function elephc_web_session_set_upload_progress_prefix(string $v): void;
    function elephc_web_session_get_upload_progress_name(): string;
    function elephc_web_session_set_upload_progress_name(string $v): void;
    function elephc_web_session_get_upload_progress_freq(): string;
    function elephc_web_session_set_upload_progress_freq(string $v): void;
    function elephc_web_session_get_upload_progress_min_freq(): string;
    function elephc_web_session_set_upload_progress_min_freq(string $v): void;
    function elephc_web_session_get_auto_start(): int;
    function elephc_web_session_set_auto_start(int $v): void;
}
function __elephc_php_version_id(): int { return __ELEPHC_PHP_VERSION_ID__; }
elephc_web_session_reset();
$_SERVER = [];
$_SESSION = [];
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
    $__elephc_body_len = elephc_web_body_len();
    $__elephc_body = '';
    if ($__elephc_body_len > 0) {
        $__elephc_body = __elephc_ptr_read_string(elephc_web_body_ptr(), $__elephc_body_len);
    }
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
        $__elephc_mpv_len = elephc_web_multipart_value_len($__elephc_mpi);
        $__elephc_mpv = '';
        if ($__elephc_mpv_len > 0) {
            $__elephc_mpv = __elephc_ptr_read_string(
                elephc_web_multipart_value_ptr($__elephc_mpi),
                $__elephc_mpv_len
            );
        }
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
interface SessionHandlerInterface {
    public function open(string $path, string $name): bool;
    public function close(): bool;
    public function read(string $id): string|false;
    public function write(string $id, string $data): bool;
    public function destroy(string $id): bool;
    public function gc(int $max_lifetime): int|false;
}
interface SessionIdInterface {
    public function create_sid(): string;
}
interface SessionUpdateTimestampHandlerInterface {
    public function validateId(string $id): bool;
    public function updateTimestamp(string $id, string $data): bool;
}
function __elephc_session_stage_bytes(string $data): ptr {
    $__elephc_sb_len = strlen($data);
    $__elephc_sb_ptr = elephc_web_session_data_stage($__elephc_sb_len);
    if ($__elephc_sb_len > 0) { __elephc_ptr_write_string($__elephc_sb_ptr, $data); }
    return $__elephc_sb_ptr;
}
function __elephc_session_copy_bytes(ptr $data, int $len): string {
    if ($len === 0) { return ''; }
    // `__elephc_ptr_read_string()` returns a borrowed view. Materialize it
    // through `str_repeat()` so the returned PHP string owns compiler-heap
    // storage and cannot be changed by the next bridge key/value publication
    // during session decoding.
    return str_repeat(__elephc_ptr_read_string($data, $len), 1);
}
function __elephc_session_read_file(string $id, string $save_path, int $read_and_close): string {
    $__elephc_rf_ptr = elephc_web_session_read_bytes($id, $save_path, $read_and_close);
    $__elephc_rf_len = elephc_web_session_data_len();
    return __elephc_session_copy_bytes($__elephc_rf_ptr, $__elephc_rf_len);
}
function __elephc_session_write_file(string $id, string $save_path, string $data): int {
    $__elephc_wf_len = strlen($data);
    $__elephc_wf_ptr = __elephc_session_stage_bytes($data);
    return elephc_web_session_write_bytes($id, $save_path, $__elephc_wf_ptr, $__elephc_wf_len);
}
function __elephc_session_snapshot_bytes(): string {
    $__elephc_snap_ptr = elephc_web_session_snapshot_bytes();
    $__elephc_snap_len = elephc_web_session_data_len();
    return __elephc_session_copy_bytes($__elephc_snap_ptr, $__elephc_snap_len);
}
function __elephc_session_entry_count(string $data, bool $binary): int {
    $__elephc_ec_len = strlen($data);
    $__elephc_ec_ptr = __elephc_session_stage_bytes($data);
    if ($binary) {
        return elephc_web_session_count_entries_bin_bytes($__elephc_ec_ptr, $__elephc_ec_len);
    }
    return elephc_web_session_count_entries_bytes($__elephc_ec_ptr, $__elephc_ec_len);
}
function __elephc_session_entry_bytes(string $data, int $idx, bool $value, bool $binary): string {
    $__elephc_eb_len = strlen($data);
    $__elephc_eb_ptr = __elephc_session_stage_bytes($data);
    if ($binary) {
        if ($value) {
            $__elephc_eb_out = elephc_web_session_entry_value_bin_bytes($__elephc_eb_ptr, $__elephc_eb_len, $idx);
            return __elephc_session_copy_bytes($__elephc_eb_out, elephc_web_session_data_len());
        }
        $__elephc_eb_out = elephc_web_session_entry_key_bin_bytes($__elephc_eb_ptr, $__elephc_eb_len, $idx);
        return __elephc_session_copy_bytes($__elephc_eb_out, elephc_web_session_data_len());
    }
    if ($value) {
        $__elephc_eb_out = elephc_web_session_entry_value_bytes($__elephc_eb_ptr, $__elephc_eb_len, $idx);
        return __elephc_session_copy_bytes($__elephc_eb_out, elephc_web_session_data_len());
    }
    $__elephc_eb_out = elephc_web_session_entry_key_bytes($__elephc_eb_ptr, $__elephc_eb_len, $idx);
    return __elephc_session_copy_bytes($__elephc_eb_out, elephc_web_session_data_len());
}
class SessionHandler implements SessionHandlerInterface, SessionIdInterface {
    public function open(string $path, string $name): bool {
        elephc_web_session_set_save_path($path); elephc_web_session_set_name($name); return true; }
    public function close(): bool {
        return elephc_web_session_abort(
            elephc_web_session_get_id(), elephc_web_session_get_save_path()
        ) === 1;
    }
    public function read(string $id): string|false {
        return __elephc_session_read_file($id, elephc_web_session_get_save_path(), 0); }
    public function write(string $id, string $data): bool {
        return __elephc_session_write_file($id, elephc_web_session_get_save_path(), $data) === 1; }
    public function destroy(string $id): bool {
        return elephc_web_session_destroy($id, elephc_web_session_get_save_path()) === 1; }
    public function gc(int $max_lifetime): int|false {
        return elephc_web_session_gc(elephc_web_session_get_save_path(), $max_lifetime); }
    public function create_sid(): string { return elephc_web_session_create_id(''); }
}
class __ElephcSessionState {
    public static ?SessionHandlerInterface $handler = null;
    public static bool $shutdown = true;
    public static string $snapshot = '';
    public static bool $snapshotValid = false;
    public static bool $sendCookie = false;
}
class __ElephcCallableSessionHandler implements SessionHandlerInterface, SessionIdInterface, SessionUpdateTimestampHandlerInterface {
    public mixed $openCb;
    public mixed $closeCb;
    public mixed $readCb;
    public mixed $writeCb;
    public mixed $destroyCb;
    public mixed $gcCb;
    public mixed $createSidCb;
    public mixed $validateIdCb;
    public mixed $updateTimestampCb;
    public function __construct(mixed $open, mixed $close, mixed $read, mixed $write, mixed $destroy, mixed $gc, mixed $create_sid, mixed $validate_id, mixed $update_timestamp) {
        $this->openCb = $open;
        $this->closeCb = $close;
        $this->readCb = $read;
        $this->writeCb = $write;
        $this->destroyCb = $destroy;
        $this->gcCb = $gc;
        $this->createSidCb = $create_sid;
        $this->validateIdCb = $validate_id;
        $this->updateTimestampCb = $update_timestamp;
    }
    public function open(string $path, string $name): bool {
        return (bool)call_user_func($this->openCb, $path, $name);
    }
    public function close(): bool {
        return (bool)call_user_func($this->closeCb);
    }
    public function read(string $id): string|false {
        // The callable returns Mixed; `=== false` narrows the abort sentinel and
        // any other value is coerced to the string payload (avoids a Union return).
        $__elephc_r = call_user_func($this->readCb, $id);
        if ($__elephc_r === false) { return false; }
        return (string)$__elephc_r;
    }
    public function write(string $id, string $data): bool {
        return (bool)call_user_func($this->writeCb, $id, $data);
    }
    public function destroy(string $id): bool {
        return (bool)call_user_func($this->destroyCb, $id);
    }
    public function gc(int $max_lifetime): int|false {
        $__elephc_g = call_user_func($this->gcCb, $max_lifetime);
        if ($__elephc_g === false) { return false; }
        return (int)$__elephc_g;
    }
    public function create_sid(): string {
        $__elephc_c = $this->createSidCb;
        if ($__elephc_c !== null) {
            return (string)call_user_func($__elephc_c);
        }
        return elephc_web_session_create_id('');
    }
    public function validateId(string $id): bool {
        $__elephc_vc = $this->validateIdCb;
        if ($__elephc_vc !== null) {
            return (bool)call_user_func($__elephc_vc, $id);
        }
        // php-src's procedural user module accepts the id when no explicit
        // validate_sid callback was registered.
        return true;
    }
    public function updateTimestamp(string $id, string $data): bool {
        $__elephc_uc = $this->updateTimestampCb;
        if ($__elephc_uc !== null) {
            return (bool)call_user_func($__elephc_uc, $id, $data);
        }
        // No update_timestamp callable: re-persist through write, matching PHP's
        // default lazy_write behavior for handlers without a timestamp method.
        return (bool)call_user_func($this->writeCb, $id, $data);
    }
}
# error_log(): the web-SAPI error channel. type 0 writes to STDERR (the worker's
# stderr, exactly like PHP's SAPI logger); type 3 appends verbatim to a file;
# type 1 (mail) is unsupported. Returns true on a successful write, false otherwise.
function error_log(string $message, int $message_type = 0, ?string $destination = null, ?string $additional_headers = null): bool {
    if ($message_type === 3) {
        if ($destination === null) { return false; }
        $__elephc_el_fh = fopen((string)$destination, 'a');
        if ($__elephc_el_fh === false) { return false; }
        fwrite($__elephc_el_fh, $message);
        fclose($__elephc_el_fh);
        return true;
    }
    if ($message_type === 0) {
        // Append a trailing newline only when the message does not already end
        // in one, matching PHP's SAPI logger line framing.
        $__elephc_el_m = $message;
        if ($__elephc_el_m === '' || substr($__elephc_el_m, -1) !== "\n") {
            $__elephc_el_m = $__elephc_el_m . "\n";
        }
        fwrite(STDERR, $__elephc_el_m);
        return true;
    }
    // type 1 (mail) and any other message_type are unsupported under --web. A
    // type-1 call carries a recipient ($destination) and extra headers
    // ($additional_headers); report the dropped delivery to stderr rather than
    // lose it silently.
    if ($message_type === 1) {
        fwrite(STDERR, 'error_log(): mail delivery (type 1) is not supported under --web'
            . ' [to=' . (string)$destination . ', headers=' . (string)$additional_headers . "]\n");
    }
    return false;
}
# trigger_error(): the faithful web-SAPI-to-stderr rendering of a user error.
# Maps the level to PHP's severity prefix and writes "<Prefix>: <message>\n" to
# STDERR. There is no error_reporting/display_errors layer, so it always writes.
function trigger_error(string $message, int $error_level = E_USER_NOTICE): bool {
    $__elephc_te_prefix = 'Notice';
    if ($error_level === E_USER_ERROR) {
        $__elephc_te_prefix = 'Fatal error';
    } elseif ($error_level === E_USER_WARNING || $error_level === E_WARNING) {
        $__elephc_te_prefix = 'Warning';
    } elseif ($error_level === E_USER_DEPRECATED || $error_level === E_DEPRECATED) {
        $__elephc_te_prefix = 'Deprecated';
    }
    fwrite(STDERR, $__elephc_te_prefix . ': ' . $message . "\n");
    return true;
}
function __elephc_session_start_option_known(string $key): bool {
    if ($key === 'cookie_partitioned') { return __elephc_php_version_id() >= 80500; }
    foreach ([
        'name', 'save_path', 'read_and_close', 'cookie_lifetime', 'cookie_path',
        'cookie_domain', 'cookie_secure', 'cookie_httponly', 'cookie_samesite',
        'cache_limiter', 'cache_expire', 'use_strict_mode', 'serialize_handler',
        'gc_probability', 'gc_divisor', 'gc_maxlifetime', 'sid_length',
        'sid_bits_per_character', 'referer_check', 'use_cookies',
        'use_only_cookies', 'lazy_write', 'use_trans_sid', 'trans_sid_tags',
        'trans_sid_hosts',
    ] as $__elephc_known_option) {
        if ($key === $__elephc_known_option) { return true; }
    }
    return false;
}
function session_start(mixed $options = []): bool {
    if (!is_array($options)) {
        throw new TypeError('session_start(): Argument #1 ($options) must be of type array');
    }
    $status = elephc_web_session_get_status();
    if ($status === PHP_SESSION_ACTIVE) {
        // PHP emits this as an E_NOTICE (not a warning) and returns true.
        trigger_error("session_start(): Ignoring session_start() because a session is already active", E_NOTICE);
        return true;
    }
    $__elephc_opt_name = null;
    $__elephc_opt_save_path = null;
    $__elephc_opt_read_and_close = false;
    $__elephc_opt_cl = null;
    $__elephc_opt_cp = null;
    $__elephc_opt_cd = null;
    $__elephc_opt_cs = null;
    $__elephc_opt_cpart = null;
    $__elephc_opt_ch = null;
    $__elephc_opt_css = null;
    $__elephc_opt_cachelim = null;
    $__elephc_opt_cacheexp = null;
    $__elephc_opt_strict = null;
    $__elephc_opt_serialize = null;
    $__elephc_opt_gcprob = null;
    $__elephc_opt_gcdiv = null;
    $__elephc_opt_gcmax = null;
    $__elephc_opt_sidlen = null;
    $__elephc_opt_sidbits = null;
    $__elephc_opt_referer = null;
    $__elephc_opt_usecookies = null;
    $__elephc_opt_useonly = null;
    $__elephc_opt_lazy = null;
    $__elephc_opt_transsid = null;
    $__elephc_opt_transtags = null;
    $__elephc_opt_transhosts = null;
    foreach ($options as $__elephc_ok => $__elephc_ov) {
        if (is_int($__elephc_ok)) {
            if (__elephc_php_version_id() >= 80500) {
                throw new ValueError('session_start(): Argument #1 ($options) must be of type array with keys as string');
            }
            continue;
        }
        if (!is_string($__elephc_ov) && !is_int($__elephc_ov) && !is_bool($__elephc_ov)) {
            throw new TypeError('session_start(): Option "' . $__elephc_ok . '" must be of type string|int|bool');
        }
        if (!__elephc_session_start_option_known($__elephc_ok)) {
            trigger_error('session_start(): Setting option "' . $__elephc_ok . '" failed', E_WARNING);
            continue;
        }
        if (__elephc_php_version_id() >= 80500 && $__elephc_ok === 'read_and_close'
            && is_string($__elephc_ov) && !is_numeric($__elephc_ov)) {
            throw new TypeError('session_start(): Option "read_and_close" value must be of type compatible with int');
        }
        if ($__elephc_ok === 'name') { $__elephc_opt_name = $__elephc_ov; }
        if ($__elephc_ok === 'save_path') { $__elephc_opt_save_path = $__elephc_ov; }
        if ($__elephc_ok === 'read_and_close') { $__elephc_opt_read_and_close = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_lifetime') { $__elephc_opt_cl = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_path') { $__elephc_opt_cp = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_domain') { $__elephc_opt_cd = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_secure') { $__elephc_opt_cs = $__elephc_ov; }
        if (__elephc_php_version_id() >= 80500 && $__elephc_ok === 'cookie_partitioned') { $__elephc_opt_cpart = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_httponly') { $__elephc_opt_ch = $__elephc_ov; }
        if ($__elephc_ok === 'cookie_samesite') { $__elephc_opt_css = $__elephc_ov; }
        if ($__elephc_ok === 'cache_limiter') { $__elephc_opt_cachelim = $__elephc_ov; }
        if ($__elephc_ok === 'cache_expire') { $__elephc_opt_cacheexp = $__elephc_ov; }
        if ($__elephc_ok === 'use_strict_mode') { $__elephc_opt_strict = $__elephc_ov; }
        if ($__elephc_ok === 'serialize_handler') { $__elephc_opt_serialize = $__elephc_ov; }
        if ($__elephc_ok === 'gc_probability') { $__elephc_opt_gcprob = $__elephc_ov; }
        if ($__elephc_ok === 'gc_divisor') { $__elephc_opt_gcdiv = $__elephc_ov; }
        if ($__elephc_ok === 'gc_maxlifetime') { $__elephc_opt_gcmax = $__elephc_ov; }
        if ($__elephc_ok === 'sid_length') { $__elephc_opt_sidlen = $__elephc_ov; }
        if ($__elephc_ok === 'sid_bits_per_character') { $__elephc_opt_sidbits = $__elephc_ov; }
        if ($__elephc_ok === 'referer_check') { $__elephc_opt_referer = $__elephc_ov; }
        if ($__elephc_ok === 'use_cookies') { $__elephc_opt_usecookies = $__elephc_ov; }
        if ($__elephc_ok === 'use_only_cookies') { $__elephc_opt_useonly = $__elephc_ov; }
        if ($__elephc_ok === 'lazy_write') { $__elephc_opt_lazy = $__elephc_ov; }
        if ($__elephc_ok === 'use_trans_sid') { $__elephc_opt_transsid = $__elephc_ov; }
        if ($__elephc_ok === 'trans_sid_tags') { $__elephc_opt_transtags = $__elephc_ov; }
        if ($__elephc_ok === 'trans_sid_hosts') { $__elephc_opt_transhosts = $__elephc_ov; }
    }
    if ($__elephc_opt_name !== null) {
        if (__elephc_session_name_valid((string)$__elephc_opt_name)) {
            elephc_web_session_set_name((string)$__elephc_opt_name);
        } else {
            trigger_error('session_start(): Setting option "name" failed', E_WARNING);
        }
    }
    if ($__elephc_opt_save_path !== null) {
        elephc_web_session_set_save_path((string)$__elephc_opt_save_path);
    }
    $read_and_close = 0;
    if ((int)$__elephc_opt_read_and_close > 0) {
        $read_and_close = 1;
    }
    $__elephc_ss_cl = elephc_web_session_get_cookie_lifetime();
    $__elephc_ss_cp = elephc_web_session_get_cookie_path();
    $__elephc_ss_cd = elephc_web_session_get_cookie_domain();
    $__elephc_ss_cs = elephc_web_session_get_cookie_secure();
    $__elephc_ss_cpart = elephc_web_session_get_cookie_partitioned();
    $__elephc_ss_ch = elephc_web_session_get_cookie_httponly();
    $__elephc_ss_css = elephc_web_session_get_cookie_samesite();
    if ($__elephc_opt_cl !== null) {
        if ((int)$__elephc_opt_cl >= 0) {
            $__elephc_ss_cl = (int)$__elephc_opt_cl;
        } else {
            trigger_error('session_start(): Setting option "cookie_lifetime" failed', E_WARNING);
        }
    }
    if ($__elephc_opt_cp !== null) { $__elephc_ss_cp = (string)$__elephc_opt_cp; }
    if ($__elephc_opt_cd !== null) { $__elephc_ss_cd = (string)$__elephc_opt_cd; }
    if ($__elephc_opt_cs !== null) { $__elephc_ss_cs = __elephc_session_ini_bool($__elephc_opt_cs); }
    if ($__elephc_opt_cpart !== null) { $__elephc_ss_cpart = __elephc_session_ini_bool($__elephc_opt_cpart); }
    if ($__elephc_opt_ch !== null) { $__elephc_ss_ch = __elephc_session_ini_bool($__elephc_opt_ch); }
    if ($__elephc_opt_css !== null
        && ($__elephc_opt_css === '' || $__elephc_opt_css === 'Strict'
            || $__elephc_opt_css === 'Lax' || $__elephc_opt_css === 'None')) {
        $__elephc_ss_css = (string)$__elephc_opt_css;
    } elseif ($__elephc_opt_css !== null) {
        trigger_error('session_start(): Setting option "cookie_samesite" failed', E_WARNING);
    }
    elephc_web_session_set_cookie_params(
        $__elephc_ss_cl, $__elephc_ss_cp, $__elephc_ss_cd,
        $__elephc_ss_cs, $__elephc_ss_cpart, $__elephc_ss_ch, $__elephc_ss_css
    );
    if ($__elephc_opt_cachelim !== null) {
        elephc_web_session_set_cache_limiter((string)$__elephc_opt_cachelim);
    }
    if ($__elephc_opt_cacheexp !== null) {
        elephc_web_session_set_cache_expire((int)$__elephc_opt_cacheexp);
    }
    if ($__elephc_opt_strict !== null) {
        elephc_web_session_set_strict_mode(__elephc_session_ini_bool($__elephc_opt_strict));
    }
    if ($__elephc_opt_serialize !== null) {
        if ($__elephc_opt_serialize === 'php' || $__elephc_opt_serialize === 'php_serialize'
            || $__elephc_opt_serialize === 'php_binary') {
            elephc_web_session_set_serialize_handler((string)$__elephc_opt_serialize);
        } else {
            trigger_error('session_start(): Setting option "serialize_handler" failed', E_WARNING);
        }
    }
    if ($__elephc_opt_gcprob !== null) {
        if (__elephc_php_version_id() >= 80400 && (int)$__elephc_opt_gcprob < 0) {
            trigger_error('session_start(): session.gc_probability must be greater than or equal to 0', E_WARNING);
            trigger_error('session_start(): Setting option "gc_probability" failed', E_WARNING);
        } else {
            elephc_web_session_set_gc_probability((int)$__elephc_opt_gcprob);
        }
    }
    if ($__elephc_opt_gcdiv !== null) {
        if (__elephc_php_version_id() >= 80400 && (int)$__elephc_opt_gcdiv <= 0) {
            trigger_error('session_start(): session.gc_divisor must be greater than 0', E_WARNING);
            trigger_error('session_start(): Setting option "gc_divisor" failed', E_WARNING);
        } else {
            elephc_web_session_set_gc_divisor((int)$__elephc_opt_gcdiv);
        }
    }
    if ($__elephc_opt_gcmax !== null) {
        elephc_web_session_set_gc_maxlifetime((int)$__elephc_opt_gcmax);
    }
    if ($__elephc_opt_sidlen !== null) {
        if (__elephc_php_version_id() >= 80400 && (int)$__elephc_opt_sidlen !== 32) {
            trigger_error('session_start(): session.sid_length INI setting is deprecated', E_DEPRECATED);
        }
        if (elephc_web_session_set_sid_length((int)$__elephc_opt_sidlen) !== 1) {
            trigger_error('session_start(): Setting option "sid_length" failed', E_WARNING);
        }
    }
    if ($__elephc_opt_sidbits !== null) {
        if (__elephc_php_version_id() >= 80400 && (int)$__elephc_opt_sidbits !== 4) {
            trigger_error('session_start(): session.sid_bits_per_character INI setting is deprecated', E_DEPRECATED);
        }
        if (elephc_web_session_set_sid_bits_per_character((int)$__elephc_opt_sidbits) !== 1) {
            trigger_error('session_start(): Setting option "sid_bits_per_character" failed', E_WARNING);
        }
    }
    if ($__elephc_opt_referer !== null) {
        if (__elephc_php_version_id() >= 80400 && (string)$__elephc_opt_referer !== '') {
            trigger_error('session_start(): Usage of session.referer_check INI setting is deprecated', E_DEPRECATED);
        }
        elephc_web_session_set_referer_check((string)$__elephc_opt_referer);
    }
    if ($__elephc_opt_usecookies !== null) {
        elephc_web_session_set_use_cookies(__elephc_session_ini_bool($__elephc_opt_usecookies));
    }
    if ($__elephc_opt_useonly !== null) {
        if (__elephc_php_version_id() >= 80400
            && __elephc_session_ini_bool($__elephc_opt_useonly) === 0) {
            trigger_error('session_start(): Disabling session.use_only_cookies INI setting is deprecated', E_DEPRECATED);
        }
        elephc_web_session_set_use_only_cookies(__elephc_session_ini_bool($__elephc_opt_useonly));
    }
    if ($__elephc_opt_lazy !== null) {
        elephc_web_session_set_lazy_write(__elephc_session_ini_bool($__elephc_opt_lazy));
    }
    if ($__elephc_opt_transsid !== null) {
        if (__elephc_php_version_id() >= 80400
            && __elephc_session_ini_bool($__elephc_opt_transsid) === 1) {
            trigger_error('session_start(): Enabling session.use_trans_sid INI setting is deprecated', E_DEPRECATED);
        }
        elephc_web_session_set_use_trans_sid(__elephc_session_ini_bool($__elephc_opt_transsid));
    }
    if ($__elephc_opt_transtags !== null) {
        if (__elephc_php_version_id() >= 80400
            && (string)$__elephc_opt_transtags !== 'a=href,area=href,frame=src,form=') {
            trigger_error('session_start(): Usage of session.trans_sid_tags INI setting is deprecated', E_DEPRECATED);
        }
        elephc_web_session_set_trans_sid_tags((string)$__elephc_opt_transtags);
    }
    if ($__elephc_opt_transhosts !== null) {
        if (__elephc_php_version_id() >= 80400 && (string)$__elephc_opt_transhosts !== '') {
            trigger_error('session_start(): Usage of session.trans_sid_hosts INI setting is deprecated', E_DEPRECATED);
        }
        elephc_web_session_set_trans_sid_hosts((string)$__elephc_opt_transhosts);
    }
    return __elephc_session_start_core($read_and_close);
}
// Compact default/session-open path shared by public session_start() and
// session.auto_start. Keeping option parsing in the public wrapper lets a web
// program that never calls session_start() omit that large function entirely.
function __elephc_session_start_core(int $read_and_close): bool {
    $status = elephc_web_session_get_status();
    if ($status === PHP_SESSION_ACTIVE) {
        trigger_error("session_start(): Ignoring session_start() because a session is already active", E_NOTICE);
        return true;
    }
    $__elephc_h = __ElephcSessionState::$handler;
    $name = elephc_web_session_get_name();
    $save_path = elephc_web_session_get_save_path();
    $id = elephc_web_session_get_id();
    $__elephc_use_cookies = elephc_web_session_get_use_cookies();
    $__elephc_use_only = elephc_web_session_get_use_only_cookies();
    $__elephc_supplied_id = $id !== '';
    $__elephc_from_cookie = false;
    $__elephc_from_global = false;
    if ($id === '' && $__elephc_use_cookies === 1) {
        if (isset($_COOKIE[$name])) {
            $id = (string)$_COOKIE[$name];
            $__elephc_from_cookie = true;
            $__elephc_supplied_id = true;
        }
    }
    if ($id === '' && $__elephc_use_only === 0) {
        if (isset($_GET[$name])) {
            $id = (string)$_GET[$name];
            $__elephc_supplied_id = true;
            $__elephc_from_global = true;
        } elseif (isset($_POST[$name])) {
            $id = (string)$_POST[$name];
            $__elephc_supplied_id = true;
            $__elephc_from_global = true;
        }
    }
    // session.referer_check: a cookie-supplied ID whose request Referer does not
    // contain the configured substring is treated as invalid (session-fixation
    // defense). Invalidate it here — before any data is read — so the fresh-id
    // path below mints a new one and starts an empty $_SESSION, matching PHP (and
    // the existing strict-mode invalid-id handling just below).
    $__elephc_refchk = elephc_web_session_get_referer_check();
    if ($id !== '' && $__elephc_use_only === 0 && $__elephc_refchk !== '') {
        $__elephc_referer = isset($_SERVER['HTTP_REFERER']) ? (string)$_SERVER['HTTP_REFERER'] : '';
        if (strpos($__elephc_referer, $__elephc_refchk) === false) {
            $id = '';
            $__elephc_supplied_id = false;
            $__elephc_from_cookie = false;
            $__elephc_from_global = false;
        }
    }
    if ($id !== '' && (strpos($id, "\r") !== false || strpos($id, "\n") !== false
        || strpos($id, "\t") !== false || strpos($id, ' ') !== false
        || strpos($id, '<') !== false || strpos($id, '>') !== false
        || strpos($id, "'") !== false || strpos($id, '"') !== false
        || strpos($id, '\\') !== false)) {
        $id = '';
        $__elephc_supplied_id = false;
        $__elephc_from_cookie = false;
        $__elephc_from_global = false;
    }
    // PHP reports ACTIVE from inside every save-handler callback.
    elephc_web_session_set_status(PHP_SESSION_ACTIVE);
    if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
        if (!$__elephc_h->open($save_path, $name)) {
            elephc_web_session_set_status(PHP_SESSION_NONE);
            return false;
        }
    }
    if ($id !== '' && elephc_web_session_get_strict_mode() === 1) {
        $__elephc_id_ok = elephc_web_session_file_exists($id, $save_path) === 1;
        // A custom handler implementing SessionUpdateTimestampHandlerInterface
        // owns validation. Without that interface, php-src's user module keeps
        // backwards compatibility by accepting every syntactically-valid ID.
        if ($__elephc_h !== null) {
            $__elephc_id_ok = true;
            if ($__elephc_h instanceof SessionUpdateTimestampHandlerInterface) {
                $__elephc_id_ok = $__elephc_h->validateId($id);
            }
        }
        if (!$__elephc_id_ok) {
            $id = '';
            $__elephc_supplied_id = false;
            $__elephc_from_cookie = false;
            $__elephc_from_global = false;
        }
    }
    if ($id === '') {
        $__elephc_attempt = 0;
        do {
            $id = elephc_web_session_create_id('');
            $__elephc_collision = elephc_web_session_file_exists($id, $save_path) === 1;
            $__elephc_attempt = $__elephc_attempt + 1;
        } while ($__elephc_collision && $__elephc_attempt < 3);
        // Custom SessionIdInterface handlers generate their own ids. Nested
        // instanceof (not `&&`) so the checker narrows the interface type.
        if ($__elephc_h !== null) {
            if ($__elephc_h instanceof SessionIdInterface) {
                $id = $__elephc_h->create_sid();
            }
        }
    }
    __ElephcSessionState::$sendCookie = $__elephc_use_cookies === 1
        && !$__elephc_from_cookie && !$__elephc_from_global;
    elephc_web_session_set_id((string)$id);
    $_SESSION = [];
    __ElephcSessionState::$snapshot = '';
    __ElephcSessionState::$snapshotValid = false;
    if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
        $__elephc_hraw = $__elephc_h->read((string)$id);
        if ($__elephc_hraw === false) {
            $__elephc_h->close();
            elephc_web_session_set_status(PHP_SESSION_NONE);
            return false;
        }
        __ElephcSessionState::$snapshot = (string)$__elephc_hraw;
        __ElephcSessionState::$snapshotValid = true;
        if ($__elephc_hraw !== '') { __elephc_session_decode((string)$__elephc_hraw); }
        if ($read_and_close === 1) {
            $__elephc_h->close();
        }
    } else {
        $__elephc_fraw = __elephc_session_read_file((string)$id, $save_path, $read_and_close);
        if (elephc_web_session_last_read_ok() !== 1) {
            elephc_web_session_set_status(PHP_SESSION_NONE);
            return false;
        }
        if ($__elephc_fraw !== '') {
            __elephc_session_decode($__elephc_fraw);
        }
    }
    if (elephc_web_session_should_gc() === 1) {
        session_gc();
    }
    if (__ElephcSessionState::$sendCookie) {
        if (!__elephc_session_send_cookie()) {
            session_abort();
            return false;
        }
    }
    __elephc_session_send_cache_headers();
    if ($read_and_close === 1) {
        elephc_web_session_set_status(PHP_SESSION_NONE);
    }
    return true;
}
function session_write_close(): bool {
    if (elephc_web_session_get_status() !== PHP_SESSION_ACTIVE) { return false; }
    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    $__elephc_h = __ElephcSessionState::$handler;
    $__elephc_encoded = __elephc_session_encode();
    if ($__elephc_encoded === false) {
        // A `php`-serializer key containing `|` cannot be encoded. php-src
        // writes an empty payload in this case, then closes the handler. Besides
        // matching those bytes, completing the close is essential: the files
        // handler's read lock must never survive an inactive session status.
        if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
            $__elephc_h->write($id, '');
            $__elephc_h->close();
        } else {
            __elephc_session_write_file($id, $save_path, '');
        }
        elephc_web_session_set_status(PHP_SESSION_NONE);
        return true;
    }
    $__elephc_data = (string)$__elephc_encoded;
    if ($__elephc_h !== null) {
        // lazy_write for a custom handler: unchanged data + a timestamp handler
        // → updateTimestamp instead of write. Standalone instanceof so the
        // checker narrows to SessionUpdateTimestampHandlerInterface.
        $__elephc_ts_done = false;
        if (elephc_web_session_get_lazy_write() === 1
            && __ElephcSessionState::$snapshotValid
            && $__elephc_data === __ElephcSessionState::$snapshot) {
            if ($__elephc_h instanceof SessionUpdateTimestampHandlerInterface) {
                $__elephc_h->updateTimestamp($id, $__elephc_data);
                $__elephc_ts_done = true;
            }
        }
        if (!$__elephc_ts_done) {
            $__elephc_h->write($id, $__elephc_data);
        }
        $__elephc_h->close();
    } else {
        if (elephc_web_session_get_lazy_write() === 1
            && $__elephc_data === __elephc_session_snapshot_bytes()) {
            // lazy_write (default on): unchanged since read — bump the
            // mtime instead of rewriting the file.
            elephc_web_session_touch($id, $save_path);
        } else {
            __elephc_session_write_file($id, $save_path, $__elephc_data);
        }
    }
    elephc_web_session_set_status(PHP_SESSION_NONE);
    return true;
}
function session_destroy(): bool {
    if (elephc_web_session_get_status() !== PHP_SESSION_ACTIVE) {
        trigger_error("session_destroy(): Trying to destroy uninitialized session", E_WARNING);
        return false;
    }
    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    $__elephc_h = __ElephcSessionState::$handler;
    $__elephc_destroyed = true;
    if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
        $__elephc_destroyed = $__elephc_h->destroy($id);
        $__elephc_h->close();
    } else {
        $__elephc_destroyed = elephc_web_session_destroy($id, $save_path) === 1;
    }
    // BUG-8: PHP does not clear $_SESSION (nor the cookie) on destroy.
    elephc_web_session_set_status(PHP_SESSION_NONE);
    elephc_web_session_set_id('');
    return $__elephc_destroyed;
}
function session_id(?string $id = null): string|false {
    if ($id !== null && elephc_web_session_get_status() === PHP_SESSION_ACTIVE) {
        trigger_error("session_id(): Session ID cannot be changed when a session is active", E_WARNING);
        return false;
    }
    $old = elephc_web_session_get_id();
    if ($id !== null) { elephc_web_session_set_id((string)$id); }
    return $old;
}
function session_name(?string $name = null): string|false {
    if ($name !== null && elephc_web_session_get_status() === PHP_SESSION_ACTIVE) {
        trigger_error("session_name(): Session name cannot be changed when a session is active", E_WARNING);
        return false;
    }
    $old = elephc_web_session_get_name();
    if ($name !== null) {
        if (!__elephc_session_name_valid((string)$name)) {
            trigger_error('session_name(): session.name "' . (string)$name
                . '" must not be numeric, empty, contain null bytes or any of the following characters "=,;.[ \\t\\r\\n\\013\\014"', E_WARNING);
            return $old;
        }
        elephc_web_session_set_name((string)$name);
    }
    return $old;
}
function session_status(): int {
    return elephc_web_session_get_status();
}
function session_unset(): bool {
    if (elephc_web_session_get_status() !== PHP_SESSION_ACTIVE) { return false; }
    $_SESSION = [];
    return true;
}
function session_encode(): string|false {
    if (elephc_web_session_get_status() !== PHP_SESSION_ACTIVE) { return false; }
    return __elephc_session_encode();
}
function session_decode(string $data): bool {
    if (elephc_web_session_get_status() !== PHP_SESSION_ACTIVE) { return false; }
    __elephc_session_decode($data);
    return true;
}
function session_save_path(?string $path = null): string|false {
    if ($path !== null && elephc_web_session_get_status() === PHP_SESSION_ACTIVE) {
        trigger_error("session_save_path(): Session save path cannot be changed when a session is active", E_WARNING);
        return false;
    }
    $old = elephc_web_session_get_save_path();
    if ($path !== null) { elephc_web_session_set_save_path((string)$path); }
    return $old;
}
function session_regenerate_id(bool $delete_old = false): bool {
    if (elephc_web_session_get_status() !== PHP_SESSION_ACTIVE) {
        trigger_error("session_regenerate_id(): Session ID cannot be regenerated when there is no active session", E_WARNING);
        return false;
    }
    $old_id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    $__elephc_h = __ElephcSessionState::$handler;
    $__elephc_encoded = __elephc_session_encode();
    if ($__elephc_encoded === false) { return false; }
    $__elephc_current = (string)$__elephc_encoded;
    // Finish the old storage transaction first. In particular, the files
    // handler must release the fd locked for the old ID before opening the new
    // one; otherwise the eventual write targets the old inode under a new ID.
    if ($delete_old) {
        if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
            if (!$__elephc_h->destroy($old_id)) { return false; }
        } else {
            if (elephc_web_session_destroy($old_id, $save_path) !== 1) { return false; }
        }
    } else {
        if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
            if (!$__elephc_h->write($old_id, $__elephc_current)) { return false; }
        } else {
            if (__elephc_session_write_file($old_id, $save_path, $__elephc_current) !== 1) { return false; }
        }
    }
    if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
        $__elephc_h->close();
    }

    $__elephc_attempt = 0;
    do {
        $new_id = elephc_web_session_create_id('');
        if ($__elephc_h !== null) {
            if ($__elephc_h instanceof SessionIdInterface) {
                $new_id = $__elephc_h->create_sid();
            }
        }
        $__elephc_collision = elephc_web_session_file_exists($new_id, $save_path) === 1;
        if ($__elephc_h !== null) {
            if ($__elephc_h instanceof SessionUpdateTimestampHandlerInterface) {
                $__elephc_collision = $__elephc_h->validateId($new_id);
            }
        }
        $__elephc_attempt = $__elephc_attempt + 1;
    } while ($__elephc_collision && $__elephc_attempt < 3);
    if ($new_id === '' || $__elephc_collision) { return false; }

    elephc_web_session_set_id($new_id);
    // Re-open and read the new storage record solely to establish the handler's
    // normal lock/module state. The current in-memory $_SESSION is preserved.
    if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
        if (!$__elephc_h->open($save_path, elephc_web_session_get_name())) { return false; }
        if ($__elephc_h->read($new_id) === false) { $__elephc_h->close(); return false; }
        __ElephcSessionState::$snapshot = '';
        __ElephcSessionState::$snapshotValid = false;
    } else {
        __elephc_session_read_file($new_id, $save_path, 0);
    }
    if (elephc_web_session_get_use_cookies() === 1) {
        if (!__elephc_session_send_cookie()) { return false; }
    }
    return true;
}
function session_create_id(string $prefix = ""): string|false {
    if (strpos($prefix, "\0") !== false) {
        throw new ValueError('session_create_id(): Argument #1 ($prefix) must not contain any null bytes');
    }
    if (__elephc_php_version_id() >= 80400 && strlen($prefix) > 256) {
        throw new ValueError('session_create_id(): Argument #1 ($prefix) cannot be longer than 256 characters');
    }
    $__elephc_created_id = elephc_web_session_create_id($prefix);
    if ($__elephc_created_id === '' && $prefix !== '') {
        trigger_error('session_create_id(): Prefix cannot contain special characters. Only the A-Z, a-z, 0-9, "-", and "," characters are allowed', E_WARNING);
        return false;
    }
    return $__elephc_created_id;
}
function session_gc(): int|false {
    if (elephc_web_session_get_status() !== PHP_SESSION_ACTIVE) {
        trigger_error("session_gc(): Session cannot be garbage collected when there is no active session", E_WARNING);
        return false;
    }
    $__elephc_h = __ElephcSessionState::$handler;
    $maxlifetime = elephc_web_session_get_gc_maxlifetime();
    if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
        return $__elephc_h->gc($maxlifetime);
    }
    $save_path = elephc_web_session_get_save_path();
    return elephc_web_session_gc($save_path, $maxlifetime);
}
function session_abort(): bool {
    if (elephc_web_session_get_status() !== PHP_SESSION_ACTIVE) { return false; }
    $id = elephc_web_session_get_id();
    $save_path = elephc_web_session_get_save_path();
    $__elephc_h = __ElephcSessionState::$handler;
    if ($__elephc_h !== null && $__elephc_h instanceof SessionHandlerInterface) {
        $__elephc_h->close();
    } else {
        elephc_web_session_abort($id, $save_path);
    }
    // php-src closes without writing but deliberately leaves the caller's
    // current in-memory $_SESSION values untouched.
    elephc_web_session_set_status(PHP_SESSION_NONE);
    return true;
}
function session_reset(): bool {
    if (elephc_web_session_get_status() !== PHP_SESSION_ACTIVE) { return false; }
    $__elephc_h = __ElephcSessionState::$handler;
    $_SESSION = [];
    if ($__elephc_h instanceof SessionHandlerInterface) {
        // Custom handler: re-read the original data from the backend, discarding
        // in-memory changes (PHP re-runs the read path). Standalone instanceof so
        // the checker/runtime resolves the dispatch on the static-held handler.
        $id = elephc_web_session_get_id();
        $raw = (string)$__elephc_h->read($id);
        if ($raw !== '') { __elephc_session_decode($raw); }
    } else {
        // Default files handler — BUG-1/2: use the read-time snapshot instead of
        // re-opening (and re-flock'ing) a file this process already holds locked,
        // which previously self-deadlocked.
        $raw = __elephc_session_snapshot_bytes();
        if ($raw !== '') { __elephc_session_decode($raw); }
    }
    return true;
}
function session_cache_limiter(?string $value = null): string|false {
    if ($value !== null && elephc_web_session_get_status() === PHP_SESSION_ACTIVE) { return false; }
    $old = elephc_web_session_get_cache_limiter();
    if ($value !== null) { elephc_web_session_set_cache_limiter((string)$value); }
    return $old;
}
function session_cache_expire(?int $value = null): int|false {
    if ($value !== null && elephc_web_session_get_status() === PHP_SESSION_ACTIVE) { return false; }
    $old = elephc_web_session_get_cache_expire();
    if ($value !== null) { elephc_web_session_set_cache_expire((int)$value); }
    return $old;
}
function session_get_cookie_params(): array {
    $__elephc_cookie_params = [
        'lifetime' => elephc_web_session_get_cookie_lifetime(),
        'path' => elephc_web_session_get_cookie_path(),
        'domain' => elephc_web_session_get_cookie_domain(),
        'secure' => (bool)elephc_web_session_get_cookie_secure(),
        'httponly' => (bool)elephc_web_session_get_cookie_httponly(),
        'samesite' => elephc_web_session_get_cookie_samesite(),
    ];
    if (__elephc_php_version_id() >= 80500) {
        $__elephc_cookie_params = [
            'lifetime' => elephc_web_session_get_cookie_lifetime(),
            'path' => elephc_web_session_get_cookie_path(),
            'domain' => elephc_web_session_get_cookie_domain(),
            'secure' => (bool)elephc_web_session_get_cookie_secure(),
            'partitioned' => (bool)elephc_web_session_get_cookie_partitioned(),
            'httponly' => (bool)elephc_web_session_get_cookie_httponly(),
            'samesite' => elephc_web_session_get_cookie_samesite(),
        ];
    }
    return $__elephc_cookie_params;
}
function session_set_cookie_params(mixed ...$args): bool {
    if (elephc_web_session_get_use_cookies() === 0) {
        trigger_error('session_set_cookie_params(): Session cookies cannot be used when session.use_cookies is disabled', E_WARNING);
        return false;
    }
    // PHP refuses (and warns) if a session is already active, leaving the cookie
    // params unchanged and returning false.
    if (elephc_web_session_get_status() === PHP_SESSION_ACTIVE) {
        trigger_error("session_set_cookie_params(): Session cookie parameters cannot be changed when a session is active", E_WARNING);
        return false;
    }
    $__elephc_scp_cl = elephc_web_session_get_cookie_lifetime();
    $__elephc_scp_cp = elephc_web_session_get_cookie_path();
    $__elephc_scp_cd = elephc_web_session_get_cookie_domain();
    $__elephc_scp_cs = elephc_web_session_get_cookie_secure();
    $__elephc_scp_cpart = elephc_web_session_get_cookie_partitioned();
    $__elephc_scp_ch = elephc_web_session_get_cookie_httponly();
    $__elephc_scp_css = elephc_web_session_get_cookie_samesite();
    if (count($args) === 1 && is_array($args[0])) {
        $_ENV['_elephc_scp'] = $args[0];
        $__elephc_scp_found = 0;
        foreach ($_ENV['_elephc_scp'] as $__elephc_scp_key => $__elephc_scp_value) {
            if (is_int($__elephc_scp_key)) {
                trigger_error('session_set_cookie_params(): Argument #1 ($lifetime_or_options) cannot contain numeric keys', E_WARNING);
                continue;
            }
            $__elephc_scp_normalized = strtolower((string)$__elephc_scp_key);
            if ($__elephc_scp_normalized === 'lifetime') {
                $__elephc_scp_cl = (int)$__elephc_scp_value; $__elephc_scp_found++;
            } elseif ($__elephc_scp_normalized === 'path') {
                $__elephc_scp_cp = (string)$__elephc_scp_value; $__elephc_scp_found++;
            } elseif ($__elephc_scp_normalized === 'domain') {
                $__elephc_scp_cd = (string)$__elephc_scp_value; $__elephc_scp_found++;
            } elseif ($__elephc_scp_normalized === 'secure') {
                $__elephc_scp_cs = $__elephc_scp_value ? 1 : 0; $__elephc_scp_found++;
            } elseif ($__elephc_scp_normalized === 'partitioned'
                && __elephc_php_version_id() >= 80500) {
                $__elephc_scp_cpart = $__elephc_scp_value ? 1 : 0; $__elephc_scp_found++;
            } elseif ($__elephc_scp_normalized === 'httponly') {
                $__elephc_scp_ch = $__elephc_scp_value ? 1 : 0; $__elephc_scp_found++;
            } elseif ($__elephc_scp_normalized === 'samesite') {
                $__elephc_scp_css = (string)$__elephc_scp_value; $__elephc_scp_found++;
            } else {
                trigger_error('session_set_cookie_params(): Argument #1 ($lifetime_or_options) contains an unrecognized key "' . (string)$__elephc_scp_key . '"', E_WARNING);
            }
        }
        unset($_ENV['_elephc_scp']);
        if ($__elephc_scp_found === 0) {
            throw new ValueError('session_set_cookie_params(): Argument #1 ($lifetime_or_options) must contain at least 1 valid key');
        }
    } else {
        // Positional form is capped at 5 args (lifetime, path, domain,
        // secure, httponly); PHP has no 6th positional (samesite is
        // array-form only — a literal 6th arg raises ArgumentCountError in
        // real PHP). The current samesite value is kept unchanged here.
        if (count($args) > 0) { $__elephc_scp_cl = (int)$args[0]; }
        if (count($args) > 1) { $__elephc_scp_cp = (string)$args[1]; }
        if (count($args) > 2) { $__elephc_scp_cd = (string)$args[2]; }
        if (count($args) > 3) { $__elephc_scp_cs = (int)$args[3]; }
        if (count($args) > 4) { $__elephc_scp_ch = (int)$args[4]; }
    }
    if ($__elephc_scp_cl < 0) { return false; }
    if ($__elephc_scp_css !== '' && $__elephc_scp_css !== 'Strict'
        && $__elephc_scp_css !== 'Lax' && $__elephc_scp_css !== 'None') { return false; }
    elephc_web_session_set_cookie_params(
        $__elephc_scp_cl, $__elephc_scp_cp, $__elephc_scp_cd,
        $__elephc_scp_cs, $__elephc_scp_cpart, $__elephc_scp_ch, $__elephc_scp_css
    );
    return true;
}
function session_commit(): bool {
    return session_write_close();
}
function session_register_shutdown(): void {
    __ElephcSessionState::$shutdown = true;
}
function session_module_name(?string $module = null): string|false {
    $__elephc_old_module = __ElephcSessionState::$handler !== null ? 'user' : 'files';
    if ($module === null) { return $__elephc_old_module; }
    if (elephc_web_session_get_status() === PHP_SESSION_ACTIVE) { return false; }
    if (strtolower($module) !== 'files') { return false; }
    __ElephcSessionState::$handler = null;
    return $__elephc_old_module;
}
function session_set_save_handler($handler_or_open = null, $register_or_close = true, $read = null, $write = null, $destroy = null, $gc = null, $create_sid = null, $validate_id = null, $update_timestamp = null): bool {
    if (elephc_web_session_get_status() === PHP_SESSION_ACTIVE) { return false; }
    // Object form: session_set_save_handler(SessionHandlerInterface $handler,
    // bool $register_shutdown = true). A standalone instanceof narrows
    // $handler_or_open to the interface so the assignment into the typed static
    // holder is a proper (retained) object reference.
    if ($handler_or_open instanceof SessionHandlerInterface) {
        __ElephcSessionState::$handler = $handler_or_open;
        __ElephcSessionState::$shutdown = (bool)$register_or_close;
        return true;
    }
    // Legacy callable form (deprecated in PHP 8.4): six required callables
    // ($open, $close, $read, $write, $destroy, $gc) plus optional $create_sid,
    // $validate_id, $update_timestamp. They are wrapped in an internal handler
    // object whose methods dispatch through call_user_func, so every PHP callable
    // kind is supported ("func", Closure, [$obj, "m"], "Class::m"). Each callable
    // is stored in its own property, so a given slot only ever receives one
    // callable kind — the configuration elephc's property-callable dispatch
    // requires (mixing kinds into one property is unsupported).
    if ($handler_or_open === null || $register_or_close === null || $read === null
        || $write === null || $destroy === null || $gc === null) {
        return false;
    }
    if (__elephc_php_version_id() >= 80400) {
        trigger_error('session_set_save_handler(): Providing individual callbacks instead of an object implementing SessionHandlerInterface is deprecated', E_DEPRECATED);
    }
    __ElephcSessionState::$handler = new __ElephcCallableSessionHandler(
        $handler_or_open, $register_or_close, $read, $write, $destroy, $gc,
        $create_sid, $validate_id, $update_timestamp
    );
    __ElephcSessionState::$shutdown = true;
    return true;
}
function __elephc_session_encode(): string|false {
    $__elephc_sh = elephc_web_session_get_serialize_handler();
    if ($__elephc_sh === 'php_serialize') {
        return serialize($_SESSION);
    }
    if ($__elephc_sh === 'php_binary') {
        $out = '';
        foreach ($_SESSION as $k => $v) {
            $ks = (string)$k;
            // php_binary keys are length-prefixed and therefore may contain
            // `|`; only the format's 127-byte key-length ceiling applies.
            if (strlen($ks) > 127) { continue; }
            $out .= chr(strlen($ks)) . $ks . serialize($v);
        }
        return $out;
    }
    $out = '';
    foreach ($_SESSION as $k => $v) {
        $ks = (string)$k;
        // BUG-10: a key containing '|' would corrupt the key|value framing;
        // session_encode() surfaces false, session_write_close() does not.
        if (strpos($ks, '|') !== false) {
            if (__elephc_php_version_id() >= 80500) {
                trigger_error('session_encode(): Failed to write session data. Data contains invalid key "' . $ks . '"', E_WARNING);
            }
            return false;
        }
        $out .= $ks . '|' . serialize($v);
    }
    return $out;
}
function __elephc_session_decode(string $raw): void {
    $__elephc_sh = elephc_web_session_get_serialize_handler();
    if ($__elephc_sh === 'php_serialize') {
        $__elephc_decoded = unserialize($raw);
        if (is_array($__elephc_decoded)) {
            // Copy through the session hash so each value is retained by the
            // request-global owner before the temporary unserialize result is
            // released at function exit. Assigning the Mixed root wholesale can
            // leave $_SESSION pointing at the temporary array storage.
            $_SESSION = [];
            foreach ($__elephc_decoded as $__elephc_key => $__elephc_value) {
                $_SESSION[$__elephc_key] = $__elephc_value;
            }
        }
        return;
    }
    // Stage the borrowed/raw PHP string exactly once. Passing it through the
    // count/key/value helper functions separately would let a callee consume
    // the caller's string owner between calls under the current EIR call ABI.
    $__elephc_dec_len = strlen($raw);
    $__elephc_dec_ptr = __elephc_session_stage_bytes($raw);
    if ($__elephc_sh === 'php_binary') {
        $count = elephc_web_session_count_entries_bin_bytes($__elephc_dec_ptr, $__elephc_dec_len);
        for ($i = 0; $i < $count; $i++) {
            $__elephc_key_ptr = elephc_web_session_entry_key_bin_bytes($__elephc_dec_ptr, $__elephc_dec_len, $i);
            $key = __elephc_session_copy_bytes($__elephc_key_ptr, elephc_web_session_data_len());
            $__elephc_val_ptr = elephc_web_session_entry_value_bin_bytes($__elephc_dec_ptr, $__elephc_dec_len, $i);
            $val = __elephc_session_copy_bytes($__elephc_val_ptr, elephc_web_session_data_len());
            $_SESSION[$key] = unserialize($val);
        }
        return;
    }
    $count = elephc_web_session_count_entries_bytes($__elephc_dec_ptr, $__elephc_dec_len);
    for ($i = 0; $i < $count; $i++) {
        $__elephc_key_ptr = elephc_web_session_entry_key_bytes($__elephc_dec_ptr, $__elephc_dec_len, $i);
        $key = __elephc_session_copy_bytes($__elephc_key_ptr, elephc_web_session_data_len());
        $__elephc_val_ptr = elephc_web_session_entry_value_bytes($__elephc_dec_ptr, $__elephc_dec_len, $i);
        $val = __elephc_session_copy_bytes($__elephc_val_ptr, elephc_web_session_data_len());
        $_SESSION[$key] = unserialize($val);
    }
}
function __elephc_session_send_cookie(): bool {
    $name = elephc_web_session_get_name();
    $id = elephc_web_session_get_id();
    $lifetime = elephc_web_session_get_cookie_lifetime();
    $path = elephc_web_session_get_cookie_path();
    $domain = elephc_web_session_get_cookie_domain();
    $secure = (bool)elephc_web_session_get_cookie_secure();
    $partitioned = (bool)elephc_web_session_get_cookie_partitioned();
    $httponly = (bool)elephc_web_session_get_cookie_httponly();
    $samesite = elephc_web_session_get_cookie_samesite();
    if ($partitioned && !$secure) {
        trigger_error("session_start(): Partitioned session cookie cannot be used without also configuring it as secure", E_WARNING);
        return false;
    }
    // Session IDs are restricted to PHP's cookie-safe `[A-Za-z0-9,-]`
    // alphabet, so emitting the validated value directly is equivalent to
    // php-src's URL encoding and avoids changing its bytes.
    $cookie = $name . '=' . $id;
    if ($lifetime > 0) {
        $cookie .= '; expires=' . gmdate('D, d-M-Y H:i:s', time() + $lifetime) . ' GMT';
        $cookie .= '; Max-Age=' . $lifetime;
    }
    if ($path !== '') { $cookie .= '; path=' . $path; }
    if ($domain !== '') { $cookie .= '; domain=' . $domain; }
    if ($secure) { $cookie .= '; secure'; }
    if ($partitioned) { $cookie .= '; Partitioned'; }
    if ($httponly) { $cookie .= '; HttpOnly'; }
    if ($samesite !== '') { $cookie .= '; SameSite=' . $samesite; }
    header('Set-Cookie: ' . $cookie, false);
    return true;
}
function __elephc_session_send_cache_headers(): void {
    $limiter = elephc_web_session_get_cache_limiter();
    if ($limiter === 'nocache') {
        header('Expires: Thu, 19 Nov 1981 08:52:00 GMT');
        header('Cache-Control: no-store, no-cache, must-revalidate');
        header('Pragma: no-cache');
    } elseif ($limiter === 'public') {
        $expire = elephc_web_session_get_cache_expire();
        header('Expires: ' . gmdate('D, d M Y H:i:s', time() + $expire * 60) . ' GMT');
        header('Cache-Control: public, max-age=' . ($expire * 60));
        header('Last-Modified: ' . gmdate('D, d M Y H:i:s', time()) . ' GMT');
    } elseif ($limiter === 'private') {
        $expire = elephc_web_session_get_cache_expire();
        header('Expires: Thu, 19 Nov 1981 08:52:00 GMT');
        header('Cache-Control: private, max-age=' . ($expire * 60));
        header('Last-Modified: ' . gmdate('D, d M Y H:i:s', time()) . ' GMT');
    } elseif ($limiter === 'private_no_expire') {
        $expire = elephc_web_session_get_cache_expire();
        header('Cache-Control: private, max-age=' . ($expire * 60));
        header('Last-Modified: ' . gmdate('D, d M Y H:i:s', time()) . ' GMT');
    }
}
// The canonical list of every session.* ini directive elephc exposes through
// ini_get/ini_set/ini_get_all. Single source of truth so all three stay in sync
// (each key here must have a matching arm in __elephc_ini_get_raw / ini_set).
function __elephc_ini_session_keys(): array {
    if (__elephc_php_version_id() < 80500) {
        return [
            'session.name', 'session.save_path', 'session.save_handler', 'session.cache_limiter',
            'session.cache_expire', 'session.cookie_lifetime', 'session.cookie_path',
            'session.cookie_domain', 'session.cookie_secure', 'session.cookie_httponly',
            'session.cookie_samesite', 'session.use_cookies', 'session.use_strict_mode',
            'session.use_only_cookies', 'session.lazy_write', 'session.use_trans_sid',
            'session.referer_check', 'session.trans_sid_tags', 'session.trans_sid_hosts',
            'session.serialize_handler', 'session.gc_probability', 'session.gc_divisor',
            'session.gc_maxlifetime', 'session.sid_length', 'session.sid_bits_per_character',
            'session.auto_start', 'session.upload_progress.enabled',
            'session.upload_progress.cleanup', 'session.upload_progress.prefix',
            'session.upload_progress.name', 'session.upload_progress.freq',
            'session.upload_progress.min_freq',
        ];
    }
    return [
        'session.name', 'session.save_path', 'session.save_handler', 'session.cache_limiter',
        'session.cache_expire', 'session.cookie_lifetime', 'session.cookie_path',
        'session.cookie_domain', 'session.cookie_secure', 'session.cookie_partitioned',
        'session.cookie_httponly', 'session.cookie_samesite', 'session.use_cookies',
        'session.use_strict_mode', 'session.use_only_cookies', 'session.lazy_write',
        'session.use_trans_sid', 'session.referer_check', 'session.trans_sid_tags',
        'session.trans_sid_hosts', 'session.serialize_handler', 'session.gc_probability',
        'session.gc_divisor', 'session.gc_maxlifetime', 'session.sid_length',
        'session.sid_bits_per_character', 'session.auto_start',
        'session.upload_progress.enabled', 'session.upload_progress.cleanup',
        'session.upload_progress.prefix', 'session.upload_progress.name',
        'session.upload_progress.freq', 'session.upload_progress.min_freq',
    ];
}
// True when $option is a session.* directive elephc knows about.
function __elephc_is_session_ini(string $option): bool {
    foreach (__elephc_ini_session_keys() as $__elephc_ik) {
        if ($__elephc_ik === $option) { return true; }
    }
    return false;
}
// Returns the current value of a known session.* directive as a PHP ini string:
// plain integers stringify as decimals, booleans as '1'/'' (PHP's ini_get
// convention), and strings pass through. Assumes $key is already validated.
function __elephc_ini_get_raw(string $key): string {
    // String directives pass through unchanged.
    if ($key === 'session.name') { return elephc_web_session_get_name(); }
    if ($key === 'session.save_path') { return elephc_web_session_get_save_path(); }
    if ($key === 'session.save_handler') { return (string)session_module_name(); }
    if ($key === 'session.cache_limiter') { return elephc_web_session_get_cache_limiter(); }
    if ($key === 'session.cookie_path') { return elephc_web_session_get_cookie_path(); }
    if ($key === 'session.cookie_domain') { return elephc_web_session_get_cookie_domain(); }
    if ($key === 'session.cookie_samesite') { return elephc_web_session_get_cookie_samesite(); }
    if ($key === 'session.serialize_handler') { return elephc_web_session_get_serialize_handler(); }
    if ($key === 'session.referer_check') { return elephc_web_session_get_referer_check(); }
    if ($key === 'session.trans_sid_tags') { return elephc_web_session_get_trans_sid_tags(); }
    if ($key === 'session.trans_sid_hosts') { return elephc_web_session_get_trans_sid_hosts(); }
    if ($key === 'session.upload_progress.prefix') { return elephc_web_session_get_upload_progress_prefix(); }
    if ($key === 'session.upload_progress.name') { return elephc_web_session_get_upload_progress_name(); }
    if ($key === 'session.upload_progress.freq') { return elephc_web_session_get_upload_progress_freq(); }
    if ($key === 'session.upload_progress.min_freq') { return elephc_web_session_get_upload_progress_min_freq(); }
    // Integer directives stringify as decimals.
    if ($key === 'session.cache_expire') { return (string)elephc_web_session_get_cache_expire(); }
    if ($key === 'session.cookie_lifetime') { return (string)elephc_web_session_get_cookie_lifetime(); }
    if ($key === 'session.gc_probability') { return (string)elephc_web_session_get_gc_probability(); }
    if ($key === 'session.gc_divisor') { return (string)elephc_web_session_get_gc_divisor(); }
    if ($key === 'session.gc_maxlifetime') { return (string)elephc_web_session_get_gc_maxlifetime(); }
    if ($key === 'session.sid_length') { return (string)elephc_web_session_get_sid_length(); }
    if ($key === 'session.sid_bits_per_character') { return (string)elephc_web_session_get_sid_bits_per_character(); }
    // Boolean directives render as '1' (on) or '' (off), matching PHP's ini_get.
    if ($key === 'session.cookie_secure') { return elephc_web_session_get_cookie_secure() === 1 ? '1' : ''; }
    if ($key === 'session.cookie_partitioned') { return elephc_web_session_get_cookie_partitioned() === 1 ? '1' : ''; }
    if ($key === 'session.cookie_httponly') { return elephc_web_session_get_cookie_httponly() === 1 ? '1' : ''; }
    if ($key === 'session.use_strict_mode') { return elephc_web_session_get_strict_mode() === 1 ? '1' : ''; }
    if ($key === 'session.use_cookies') { return elephc_web_session_get_use_cookies() === 1 ? '1' : ''; }
    if ($key === 'session.use_only_cookies') { return elephc_web_session_get_use_only_cookies() === 1 ? '1' : ''; }
    if ($key === 'session.lazy_write') { return elephc_web_session_get_lazy_write() === 1 ? '1' : ''; }
    if ($key === 'session.use_trans_sid') { return elephc_web_session_get_use_trans_sid() === 1 ? '1' : ''; }
    if ($key === 'session.upload_progress.enabled') { return elephc_web_session_get_upload_progress_enabled() === 1 ? '1' : ''; }
    if ($key === 'session.upload_progress.cleanup') { return elephc_web_session_get_upload_progress_cleanup() === 1 ? '1' : ''; }
    if ($key === 'session.auto_start') { return elephc_web_session_get_auto_start() === 1 ? '1' : ''; }
    return '';
}
// ini_get($option): current value of a session.* directive as a string, or false
// for a non-session/unknown directive. elephc only models the session.* surface.
function ini_get(string $option): string|false {
    if (!__elephc_is_session_ini($option)) { return false; }
    return __elephc_ini_get_raw($option);
}
function __elephc_session_ini_perdir(string $option): bool {
    return $option === 'session.auto_start'
        || $option === 'session.upload_progress.enabled'
        || $option === 'session.upload_progress.cleanup'
        || $option === 'session.upload_progress.prefix'
        || $option === 'session.upload_progress.name'
        || $option === 'session.upload_progress.freq'
        || $option === 'session.upload_progress.min_freq';
}
function __elephc_session_ini_bool(mixed $value): int {
    if (is_string($value)) {
        $__elephc_bv = strtolower(trim($value));
        if ($__elephc_bv === '' || $__elephc_bv === '0' || $__elephc_bv === 'off'
            || $__elephc_bv === 'no' || $__elephc_bv === 'false' || $__elephc_bv === 'none') {
            return 0;
        }
        return 1;
    }
    return $value ? 1 : 0;
}
function __elephc_session_name_valid(string $name): bool {
    if ($name === '' || strpos($name, "\0") !== false || is_numeric($name)) { return false; }
    foreach (['=', ',', ';', '.', '[', ' ', "\t", "\r", "\n", "\v", "\f"] as $__elephc_nc) {
        if (strpos($name, $__elephc_nc) !== false) { return false; }
    }
    return true;
}
// ini_set($option, $value): set a session.* directive, returning the OLD value as
// a string (or false for an unknown directive). $value is coerced to the
// directive's stored type: integer/boolean directives take (int)$value, string
// directives take (string)$value. PERDIR directives are rejected at runtime.
function ini_set(string $option, $value): string|false {
    $old = '';
    if (!__elephc_is_session_ini($option)) { return false; }
    if (__elephc_session_ini_perdir($option)) { return false; }
    if (elephc_web_session_get_status() === PHP_SESSION_ACTIVE) { return false; }
    if ($option === 'session.name' && !__elephc_session_name_valid((string)$value)) { return false; }
    if ($option === 'session.serialize_handler' && $value !== 'php'
        && $value !== 'php_serialize' && $value !== 'php_binary') { return false; }
    if ($option === 'session.save_handler' && $value !== 'files') { return false; }
    if ($option === 'session.cookie_samesite' && $value !== '' && $value !== 'Strict'
        && $value !== 'Lax' && $value !== 'None') { return false; }
    if ($option === 'session.cookie_lifetime' && (int)$value < 0) { return false; }
    if (__elephc_php_version_id() >= 80400
        && $option === 'session.gc_probability' && (int)$value < 0) {
        trigger_error('ini_set(): session.gc_probability must be greater than or equal to 0', E_WARNING);
        return false;
    }
    if (__elephc_php_version_id() >= 80400
        && $option === 'session.gc_divisor' && (int)$value <= 0) {
        trigger_error('ini_set(): session.gc_divisor must be greater than 0', E_WARNING);
        return false;
    }
    $old = __elephc_ini_get_raw($option);
    if (__elephc_php_version_id() >= 80400) {
        if ($option === 'session.sid_length' && (int)$value !== 32) {
            trigger_error('ini_set(): session.sid_length INI setting is deprecated', E_DEPRECATED);
        }
        if ($option === 'session.sid_bits_per_character' && (int)$value !== 4) {
            trigger_error('ini_set(): session.sid_bits_per_character INI setting is deprecated', E_DEPRECATED);
        }
        if ($option === 'session.use_only_cookies' && __elephc_session_ini_bool($value) === 0) {
            trigger_error('ini_set(): Disabling session.use_only_cookies INI setting is deprecated', E_DEPRECATED);
        }
        if ($option === 'session.use_trans_sid' && __elephc_session_ini_bool($value) === 1) {
            trigger_error('ini_set(): Enabling session.use_trans_sid INI setting is deprecated', E_DEPRECATED);
        }
        if ($option === 'session.referer_check' && (string)$value !== '') {
            trigger_error('ini_set(): Usage of session.referer_check INI setting is deprecated', E_DEPRECATED);
        }
        if ($option === 'session.trans_sid_tags'
            && (string)$value !== 'a=href,area=href,frame=src,form=') {
            trigger_error('ini_set(): Usage of session.trans_sid_tags INI setting is deprecated', E_DEPRECATED);
        }
        if ($option === 'session.trans_sid_hosts' && (string)$value !== '') {
            trigger_error('ini_set(): Usage of session.trans_sid_hosts INI setting is deprecated', E_DEPRECATED);
        }
    }
    if ($option === 'session.name') { elephc_web_session_set_name((string)$value); }
    if ($option === 'session.save_path') { elephc_web_session_set_save_path((string)$value); }
    if ($option === 'session.save_handler') { __ElephcSessionState::$handler = null; }
    if ($option === 'session.cache_limiter') { elephc_web_session_set_cache_limiter((string)$value); }
    if ($option === 'session.cookie_samesite') { elephc_web_session_set_cookie_params(elephc_web_session_get_cookie_lifetime(), elephc_web_session_get_cookie_path(), elephc_web_session_get_cookie_domain(), elephc_web_session_get_cookie_secure(), elephc_web_session_get_cookie_partitioned(), elephc_web_session_get_cookie_httponly(), (string)$value); }
    if ($option === 'session.cookie_path') { elephc_web_session_set_cookie_params(elephc_web_session_get_cookie_lifetime(), (string)$value, elephc_web_session_get_cookie_domain(), elephc_web_session_get_cookie_secure(), elephc_web_session_get_cookie_partitioned(), elephc_web_session_get_cookie_httponly(), elephc_web_session_get_cookie_samesite()); }
    if ($option === 'session.cookie_domain') { elephc_web_session_set_cookie_params(elephc_web_session_get_cookie_lifetime(), elephc_web_session_get_cookie_path(), (string)$value, elephc_web_session_get_cookie_secure(), elephc_web_session_get_cookie_partitioned(), elephc_web_session_get_cookie_httponly(), elephc_web_session_get_cookie_samesite()); }
    if ($option === 'session.cookie_lifetime') { elephc_web_session_set_cookie_params((int)$value, elephc_web_session_get_cookie_path(), elephc_web_session_get_cookie_domain(), elephc_web_session_get_cookie_secure(), elephc_web_session_get_cookie_partitioned(), elephc_web_session_get_cookie_httponly(), elephc_web_session_get_cookie_samesite()); }
    if ($option === 'session.cookie_secure') { elephc_web_session_set_cookie_params(elephc_web_session_get_cookie_lifetime(), elephc_web_session_get_cookie_path(), elephc_web_session_get_cookie_domain(), __elephc_session_ini_bool($value), elephc_web_session_get_cookie_partitioned(), elephc_web_session_get_cookie_httponly(), elephc_web_session_get_cookie_samesite()); }
    if ($option === 'session.cookie_partitioned') { elephc_web_session_set_cookie_params(elephc_web_session_get_cookie_lifetime(), elephc_web_session_get_cookie_path(), elephc_web_session_get_cookie_domain(), elephc_web_session_get_cookie_secure(), __elephc_session_ini_bool($value), elephc_web_session_get_cookie_httponly(), elephc_web_session_get_cookie_samesite()); }
    if ($option === 'session.cookie_httponly') { elephc_web_session_set_cookie_params(elephc_web_session_get_cookie_lifetime(), elephc_web_session_get_cookie_path(), elephc_web_session_get_cookie_domain(), elephc_web_session_get_cookie_secure(), elephc_web_session_get_cookie_partitioned(), __elephc_session_ini_bool($value), elephc_web_session_get_cookie_samesite()); }
    if ($option === 'session.serialize_handler') { elephc_web_session_set_serialize_handler((string)$value); }
    if ($option === 'session.referer_check') { elephc_web_session_set_referer_check((string)$value); }
    if ($option === 'session.trans_sid_tags') { elephc_web_session_set_trans_sid_tags((string)$value); }
    if ($option === 'session.trans_sid_hosts') { elephc_web_session_set_trans_sid_hosts((string)$value); }
    if ($option === 'session.upload_progress.prefix') { elephc_web_session_set_upload_progress_prefix((string)$value); }
    if ($option === 'session.upload_progress.name') { elephc_web_session_set_upload_progress_name((string)$value); }
    if ($option === 'session.upload_progress.freq') { elephc_web_session_set_upload_progress_freq((string)$value); }
    if ($option === 'session.upload_progress.min_freq') { elephc_web_session_set_upload_progress_min_freq((string)$value); }
    if ($option === 'session.cache_expire') { elephc_web_session_set_cache_expire((int)$value); }
    if ($option === 'session.gc_probability') { elephc_web_session_set_gc_probability((int)$value); }
    if ($option === 'session.gc_divisor') { elephc_web_session_set_gc_divisor((int)$value); }
    if ($option === 'session.gc_maxlifetime') { elephc_web_session_set_gc_maxlifetime((int)$value); }
    if ($option === 'session.sid_length' && elephc_web_session_set_sid_length((int)$value) !== 1) { return false; }
    if ($option === 'session.sid_bits_per_character' && elephc_web_session_set_sid_bits_per_character((int)$value) !== 1) { return false; }
    if ($option === 'session.use_strict_mode') { elephc_web_session_set_strict_mode(__elephc_session_ini_bool($value)); }
    if ($option === 'session.use_cookies') { elephc_web_session_set_use_cookies(__elephc_session_ini_bool($value)); }
    if ($option === 'session.use_only_cookies') { elephc_web_session_set_use_only_cookies(__elephc_session_ini_bool($value)); }
    if ($option === 'session.lazy_write') { elephc_web_session_set_lazy_write(__elephc_session_ini_bool($value)); }
    if ($option === 'session.use_trans_sid') { elephc_web_session_set_use_trans_sid(__elephc_session_ini_bool($value)); }
    return $old;
}
// ini_get_all($extension, $details): the session.* directives. A non-session
// $extension yields []. With $details each entry is
// ['global_value'=>v,'local_value'=>v,'access'=>7]; otherwise the plain value.
function ini_get_all(?string $extension = null, bool $details = true): array {
    if ($extension !== null && $extension !== 'session') { return []; }
    $__elephc_all = [];
    foreach (__elephc_ini_session_keys() as $__elephc_ak) {
        $__elephc_av = __elephc_ini_get_raw($__elephc_ak);
        if ($details) {
            $__elephc_access = __elephc_session_ini_perdir($__elephc_ak) ? 2 : 7;
            $__elephc_all[$__elephc_ak] = ['global_value' => $__elephc_av, 'local_value' => $__elephc_av, 'access' => $__elephc_access];
        } else {
            $__elephc_all[$__elephc_ak] = $__elephc_av;
        }
    }
    return $__elephc_all;
}
// Reset PHP-side handler state on every request. Class statics live for the
// worker process, unlike normal handler locals, so retaining these values would
// leak a prior request's object/snapshot into auto_start.
__ElephcSessionState::$handler = null;
__ElephcSessionState::$shutdown = true;
__ElephcSessionState::$snapshot = '';
__ElephcSessionState::$snapshotValid = false;
__ElephcSessionState::$sendCookie = false;
// session.auto_start runs only after every superglobal and the PHP-side session
// state above have been initialized, but still before user statements.
if (elephc_web_session_get_auto_start() === 1) { __elephc_session_start_core(0); }
"#;

/// The catch-all wrapper: the whole handler body is placed inside its `try` so an
/// uncaught exception sets a 500 status instead of crashing the worker (the
/// process would otherwise die and the master would respawn it, dropping the
/// connection). The `0;` placeholder body is replaced with the real statements.
pub(crate) const WEB_WRAP_SRC: &str =
    "<?php try { $__elephc_wrap = 0; } catch (\\Throwable $__elephc_exc) { http_response_code(500); } finally { if (elephc_web_session_get_status() === PHP_SESSION_ACTIVE && __ElephcSessionState::$shutdown) { session_write_close(); } }";

/// Prepends the web prelude when compiling with `--web` and wraps the whole
/// handler body in a catch-all `try`/`catch` so uncaught exceptions become a 500.
/// Returns the program unchanged otherwise.
pub fn inject_if_web(program: Program, web: bool, php_version: PhpVersion) -> Program {
    if !web {
        return program;
    }
    let user_usage = usage::collect(&program);
    let needs_callable_session_handler = user_usage.references("session_set_save_handler")
        || user_usage.dynamic_function_call;
    let prelude = WEB_PRELUDE_SRC.replace(
        "__ELEPHC_PHP_VERSION_ID__",
        &php_version.version_id().to_string(),
    );
    let tokens = crate::lexer::tokenize(&prelude).expect("web prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("web prelude must parse");
    if !needs_callable_session_handler {
        combined.retain(|stmt| !is_callable_session_handler_decl(&stmt.kind));
    }
    prune_unreachable_prelude_functions(&mut combined, &user_usage);
    combined.extend(program);

    // The catch-all try wrap below reorders the top level (declarations hoisted
    // out, executables wrapped). That reordering is unsafe across namespace
    // boundaries: a `namespace X;` / `namespace X { … }` would be separated from
    // the declarations it scopes, leaving them in the wrong namespace. For
    // namespaced programs (e.g. a framework with `App\…` classes) skip the wrap
    // entirely — such programs do their own error handling — and keep B1's
    // uncaught-exception → 500 net only for flat, non-namespaced programs.
    if combined.iter().any(|s| {
        matches!(
            s.kind,
            StmtKind::NamespaceDecl { .. } | StmtKind::NamespaceBlock { .. }
        )
    }) {
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

/// Removes compiler-owned prelude functions that cannot be reached from user
/// code, executable bootstrap statements, retained class methods, or the web
/// exception/finalization wrapper. Unknown dynamic calls keep every declaration
/// so PHP runtime-name dispatch and `eval()` remain conservative.
fn prune_unreachable_prelude_functions(prelude: &mut Program, user_usage: &usage::Usage) {
    let mut dependencies = HashMap::new();
    let mut roots = user_usage.clone();
    for stmt in prelude.iter() {
        if let StmtKind::FunctionDecl { name, body, .. } = &stmt.kind {
            dependencies.insert(crate::names::php_symbol_key(name), usage::collect(body));
        } else {
            roots.merge(usage::collect_stmt(stmt));
        }
    }

    let wrap_tokens = crate::lexer::tokenize(WEB_WRAP_SRC).expect("web wrapper must tokenize");
    let wrapper = crate::parser::parse(&wrap_tokens).expect("web wrapper must parse");
    roots.merge(usage::collect(&wrapper));

    if roots.dynamic_function_call {
        return;
    }

    let mut reachable = HashSet::new();
    let mut pending = roots
        .functions
        .iter()
        .filter(|name| dependencies.contains_key(*name))
        .cloned()
        .collect::<VecDeque<_>>();
    while let Some(name) = pending.pop_front() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        let Some(function_usage) = dependencies.get(&name) else {
            continue;
        };
        if function_usage.dynamic_function_call {
            return;
        }
        for dependency in &function_usage.functions {
            if dependencies.contains_key(dependency) && !reachable.contains(dependency) {
                pending.push_back(dependency.clone());
            }
        }
    }

    prelude.retain(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => {
            reachable.contains(&crate::names::php_symbol_key(name))
        }
        _ => true,
    });
}

/// Returns true for the heavy legacy callable-handler declarations that ordinary
/// web/session programs do not need. The detector keeps both declarations when
/// user code can reach `session_set_save_handler()` or another dynamic callable
/// surface; otherwise omitting them avoids compiling ten boxed-Mixed callback
/// dispatchers into every `--web` binary.
fn is_callable_session_handler_decl(kind: &StmtKind) -> bool {
    match kind {
        StmtKind::ClassDecl { name, .. } => name == "__ElephcCallableSessionHandler",
        StmtKind::FunctionDecl { name, .. } => {
            name.eq_ignore_ascii_case("session_set_save_handler")
        }
        _ => false,
    }
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

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for web-prelude pay-for-use declaration selection.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Bootstrap session support remains while optional APIs and callable handlers are pruned.

    use super::*;

    /// Parses a PHP fixture before web-prelude injection.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// Returns whether an injected program declares a free function.
    fn declares_function(program: &Program, expected: &str) -> bool {
        program.iter().any(|stmt| {
            matches!(
                &stmt.kind,
                StmtKind::FunctionDecl { name, .. } if name.eq_ignore_ascii_case(expected)
            )
        })
    }

    /// Returns whether an injected program declares a class.
    fn declares_class(program: &Program, expected: &str) -> bool {
        program.iter().any(|stmt| {
            matches!(
                &stmt.kind,
                StmtKind::ClassDecl { name, .. } if name == expected
            )
        })
    }

    /// Plain web programs keep auto-start/finalization roots but shed optional APIs.
    #[test]
    fn plain_web_program_prunes_optional_session_declarations() {
        let injected = inject_if_web(parse("<?php echo 'ok';"), true, PhpVersion::Php85);
        assert!(declares_function(
            &injected,
            "__elephc_session_start_core"
        ));
        assert!(!declares_function(&injected, "session_start"));
        assert!(declares_function(&injected, "session_write_close"));
        assert!(!declares_function(&injected, "session_regenerate_id"));
        assert!(!declares_function(&injected, "session_set_save_handler"));
        assert!(!declares_class(
            &injected,
            "__ElephcCallableSessionHandler"
        ));
    }

    /// A direct session API call roots that function and its transitive helpers.
    #[test]
    fn direct_session_api_call_keeps_requested_declaration() {
        let injected = inject_if_web(
            parse("<?php session_start(); session_regenerate_id(true);"),
            true,
            PhpVersion::Php85,
        );
        assert!(declares_function(&injected, "session_regenerate_id"));
    }

    /// Literal availability probes retain the queried PHP-visible function.
    #[test]
    fn function_exists_probe_keeps_session_save_handler() {
        let injected = inject_if_web(
            parse("<?php echo function_exists('session_set_save_handler');"),
            true,
            PhpVersion::Php85,
        );
        assert!(declares_function(&injected, "session_set_save_handler"));
        assert!(declares_class(
            &injected,
            "__ElephcCallableSessionHandler"
        ));
    }

    /// Unknown runtime calls keep the complete prelude conservatively.
    #[test]
    fn dynamic_call_disables_prelude_function_pruning() {
        let injected = inject_if_web(
            parse("<?php $name = 'session_regenerate_id'; $name();"),
            true,
            PhpVersion::Php85,
        );
        assert!(declares_function(&injected, "session_regenerate_id"));
        assert!(declares_function(&injected, "session_set_save_handler"));
    }

    /// Unknown availability probes keep the complete prelude conservatively.
    #[test]
    fn dynamic_function_probe_disables_prelude_function_pruning() {
        let injected = inject_if_web(
            parse("<?php $name = 'session_regenerate_id'; echo function_exists($name);"),
            true,
            PhpVersion::Php85,
        );
        assert!(declares_function(&injected, "session_regenerate_id"));
        assert!(declares_function(&injected, "session_set_save_handler"));
    }

    /// Non-web compilation leaves the user program untouched.
    #[test]
    fn non_web_program_is_unchanged() {
        let program = parse("<?php echo 'ok';");
        assert_eq!(
            inject_if_web(program.clone(), false, PhpVersion::Php85),
            program
        );
    }
}
