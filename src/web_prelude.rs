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
"#;

/// The catch-all wrapper: the whole handler body is placed inside its `try` so an
/// uncaught exception sets a 500 status instead of crashing the worker (the
/// process would otherwise die and the master would respawn it, dropping the
/// connection). The `0;` placeholder body is replaced with the real statements.
const WEB_WRAP_SRC: &str =
    "<?php try { $__elephc_wrap = 0; } catch (\\Throwable $__elephc_exc) { http_response_code(500); }";

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
