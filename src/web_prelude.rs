//! Purpose:
//! The `--web` and `--web-worker` request preludes. Under `--web`, prepends an
//! `extern "elephc_web"` declaration block (Phase 2 Task 2) and executable
//! statements that build the request superglobals ($_SERVER/$_GET/$_POST) on
//! every request (Task 5+). Under `--web-worker`, prepends the same extern block
//! plus a `__elephc_web_request_init()` function, the C-ABI trampoline
//! `elephc_worker_handle_request()`, and the six per-superglobal fill
//! functions it invokes per request, without wrapping the boot in a catch-all
//! try (the boot runs once and a crash is a startup failure the master
//! respawns from).
//!
//! Called from:
//! - `crate::pipeline::compile`, after the other preludes and before name
//!   resolution, gated on `CliConfig.web` / `CliConfig.web_worker` (the only
//!   flag-gated preludes).
//!
//! Key details:
//! - Under `--web` the injected statements run before user top-level code each
//!   request because the prelude statements are prepended and the whole
//!   top-level body re-runs per request.
//! - Under `--web-worker` the boot runs once; the trampoline re-fills
//!   superglobals per request via `__elephc_web_request_init()` and the Rust
//!   side calls `__rt_web_worker_request_reset` before invoking it.

use crate::parser::ast::{Program, Stmt, StmtKind};

/// Which request superglobals a program actually references, so the web preludes
/// build only the ones used (B1: skip the per-request cost of superglobals a
/// program never reads — a hello-world builds none).
///
/// Detection is a deliberate SAFE over-approximation: a superglobal can only be
/// referenced in PHP by its literal name (`$_SERVER`), because elephc rejects
/// variable-variables (`$$name`) at compile time and provides no `extract` /
/// `compact` / `get_defined_vars` / `$GLOBALS`. String interpolation (`"$_SERVER"`)
/// lowers to a real `Variable("_SERVER")` AST node, so it is covered too. We
/// detect a reference by looking for that node in the fully-resolved AST (after
/// `include`/`require` and autoloaded classes are inlined), so nothing that
/// contributes code to the program is missed. A spurious match (e.g. the literal
/// text inside an unrelated string) only over-builds — it never under-builds,
/// which is the only unsafe direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebSuperglobals {
    /// `$_SERVER` referenced (or required by `$_POST`/`$_COOKIE`/`$_REQUEST`).
    pub server: bool,
    /// `$_GET` referenced (or required by `$_REQUEST`).
    pub get: bool,
    /// `$_POST` or `$_FILES` referenced (they share one fill; also pulled in by
    /// `$_REQUEST`). Requires `$_SERVER` (reads `CONTENT_TYPE`).
    pub post: bool,
    /// `$_COOKIE` referenced. Requires `$_SERVER` (reads `HTTP_COOKIE`).
    pub cookie: bool,
    /// `$_REQUEST` referenced. Requires `$_GET` and `$_POST`.
    pub request: bool,
    /// `$_ENV` referenced.
    pub env: bool,
}

/// `Debug` marker for a `$_SERVER` reference in the AST dump. The value must equal
/// `format!("{:?}", ExprKind::Variable("_SERVER".into()))`; a dedicated unit test
/// asserts this so a rename of the `Variable` variant breaks the build loudly
/// instead of silently under-detecting.
const MARK_SERVER: &str = "Variable(\"_SERVER\")";
/// `Debug` marker for a `$_GET` reference. See [`MARK_SERVER`].
const MARK_GET: &str = "Variable(\"_GET\")";
/// `Debug` marker for a `$_POST` reference. See [`MARK_SERVER`].
const MARK_POST: &str = "Variable(\"_POST\")";
/// `Debug` marker for a `$_FILES` reference (built by the `$_POST` fill). See [`MARK_SERVER`].
const MARK_FILES: &str = "Variable(\"_FILES\")";
/// `Debug` marker for a `$_COOKIE` reference. See [`MARK_SERVER`].
const MARK_COOKIE: &str = "Variable(\"_COOKIE\")";
/// `Debug` marker for a `$_REQUEST` reference. See [`MARK_SERVER`].
const MARK_REQUEST: &str = "Variable(\"_REQUEST\")";
/// `Debug` marker for a `$_ENV` reference. See [`MARK_SERVER`].
const MARK_ENV: &str = "Variable(\"_ENV\")";

impl WebSuperglobals {
    /// The empty set: no superglobal referenced (the hello-world case).
    fn none() -> Self {
        Self { server: false, get: false, post: false, cookie: false, request: false, env: false }
    }

    /// True once every directly-detectable superglobal flag is set, so the AST
    /// scan can stop early on programs that reference them all.
    fn all_direct_set(&self) -> bool {
        self.server && self.get && self.post && self.cookie && self.request && self.env
    }

    /// Expands the directly-detected flags to include the superglobals each one
    /// depends on, so the fills run in a satisfiable order: `$_REQUEST` needs
    /// `$_GET`+`$_POST`; `$_POST` and `$_COOKIE` read `$_SERVER`.
    fn close_dependencies(&mut self) {
        if self.request {
            self.get = true;
            self.post = true;
        }
        if self.post || self.cookie {
            self.server = true;
        }
    }
}

/// Detects which request superglobals `program` references (see [`WebSuperglobals`]).
///
/// Scans the `Debug` rendering of each top-level statement of the fully-resolved
/// program for the `Variable("_NAME")` markers, OR-ing the flags together and
/// stopping as soon as all are found, then closes the dependency set. Formatting
/// per statement (rather than the whole program at once) bounds peak memory and
/// lets the common hello-world case exit after the first tiny statement.
fn detect_superglobals(program: &Program) -> WebSuperglobals {
    let mut sg = WebSuperglobals::none();
    for stmt in program {
        if sg.all_direct_set() {
            break;
        }
        let dump = format!("{:?}", stmt);
        sg.server |= dump.contains(MARK_SERVER);
        sg.get |= dump.contains(MARK_GET);
        sg.post |= dump.contains(MARK_POST) || dump.contains(MARK_FILES);
        sg.cookie |= dump.contains(MARK_COOKIE);
        sg.request |= dump.contains(MARK_REQUEST);
        sg.env |= dump.contains(MARK_ENV);
    }
    sg.close_dependencies();
    sg
}

/// Assembles the classic `--web` prelude PHP source for the detected superglobal
/// set: the shared extern block, then only the inline fills for the superglobals
/// actually used (in dependency order), then the always-present `setcookie`
/// helpers. With every flag set this reproduces the former monolithic
/// `WEB_PRELUDE_SRC` byte-for-byte; with none set it emits just the extern block
/// and the cookie helpers, so a hello-world pays no per-request superglobal cost.
fn classic_prelude_src(sg: &WebSuperglobals) -> String {
    let mut s = String::from("<?php\n");
    s.push_str(WEB_EXTERN_BLOCK_SRC);
    s.push('\n');
    if sg.server {
        s.push_str(FILL_SERVER_SRC);
        s.push('\n');
    }
    if sg.get {
        s.push_str(FILL_GET_SRC);
        s.push('\n');
    }
    if sg.post {
        s.push_str(FILL_POST_SRC);
        s.push('\n');
    }
    if sg.cookie {
        s.push_str(FILL_COOKIE_SRC);
        s.push('\n');
    }
    if sg.request {
        s.push_str(FILL_REQUEST_SRC);
        s.push('\n');
    }
    if sg.env {
        s.push_str(FILL_ENV_SRC);
        s.push('\n');
    }
    s.push_str(WEB_COOKIE_FUNCS_SRC);
    s.push('\n');
    s
}

/// The catch-all wrapper: the whole handler body is placed inside its `try` so an
/// uncaught exception sets a 500 status instead of crashing the worker (the
/// process would otherwise die and the master would respawn it, dropping the
/// connection). The `0;` placeholder body is replaced with the real statements.
const WEB_WRAP_SRC: &str =
    "<?php try { $__elephc_wrap = 0; } catch (\\Throwable $__elephc_exc) { http_response_code(500); }";

/// Prepends the web prelude when compiling with `--web` and wraps the whole
/// handler body in a catch-all `try`/`catch` so uncaught exceptions become a 500.
/// Returns the program unchanged otherwise.
///
/// Only the superglobal fills the program actually references are injected
/// (B1, via [`detect_superglobals`]), so a program that never reads a superglobal
/// pays none of its per-request build cost. Detection runs on the resolved
/// program, so `include`d/autoloaded references are seen.
pub fn inject_if_web(program: Program, web: bool) -> Program {
    if !web {
        return program;
    }
    let sg = detect_superglobals(&program);
    let src = classic_prelude_src(&sg);
    let tokens = crate::lexer::tokenize(&src).expect("web prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("web prelude must parse");
    combined.extend(program);
    partition_and_wrap(combined)
}

/// Partitions a top-level (or namespace-block) statement list into declarations,
/// namespace markers, `use` imports — all kept at their structural level — and
/// maximal runs of executable statements, each run wrapped in the catch-all
/// `try`/`catch` so an uncaught exception becomes a 500 instead of crashing the
/// worker.
///
/// Handles namespaced programs (the previous code skipped the wrap entirely for
/// any `namespace`, so namespaced uncaught throws crashed the worker with no
/// response): a `namespace X;` marker stays at top level and starts a new
/// section, so the executables that follow it are wrapped in a `try` that name
/// resolution resolves in namespace `X` (the `\Throwable` catch keeps its leading
/// backslash → global). `namespace X { … }` blocks recurse so the block's own
/// executables get their own net while its declarations stay lexically inside the
/// block. `use` imports and hoistable declarations stay outside any `try` so
/// imports resolve and externs are not nested in a `try`. With zero namespaces
/// this reduces to exactly the prior single-section wrap.
fn partition_and_wrap(stmts: Program) -> Program {
    let mut result: Program = Vec::new();
    let mut exec: Program = Vec::new();
    for stmt in stmts {
        if matches!(stmt.kind, StmtKind::NamespaceDecl { .. }) {
            // A `namespace X;` marker stays at top level and starts a new section.
            flush_exec(&mut exec, &mut result);
            result.push(stmt);
        } else if matches!(stmt.kind, StmtKind::NamespaceBlock { .. }) {
            // Recurse so the block's own executables get their own 500 net while
            // its declarations stay lexically inside the block.
            flush_exec(&mut exec, &mut result);
            let Stmt { kind, span, attributes } = stmt;
            if let StmtKind::NamespaceBlock { name, body } = kind {
                result.push(Stmt {
                    kind: StmtKind::NamespaceBlock {
                        name,
                        body: partition_and_wrap(body),
                    },
                    span,
                    attributes,
                });
            }
        } else if matches!(stmt.kind, StmtKind::UseDecl { .. }) || is_hoistable_decl(&stmt.kind) {
            // `use` imports stay at the section's top level (outside any `try`) so
            // their imports are visible to both the hoisted declarations and the
            // wrapped executables; hoistable declarations (functions/classes/
            // externs) stay outside the try so they resolve normally — externs in
            // particular are NOT resolved when nested in a try.
            result.push(stmt);
        } else {
            exec.push(stmt);
        }
    }
    flush_exec(&mut exec, &mut result);
    result
}

/// Drains the accumulated executable statements (if any) into a fresh catch-all
/// `try` wrapper and appends it to `result`. A no-op when there are no pending
/// executables, so purely declarative sections emit no empty `try`.
fn flush_exec(exec: &mut Program, result: &mut Program) {
    if exec.is_empty() {
        return;
    }
    let body = std::mem::take(exec);
    result.extend(wrap_executables_in_try(body));
}

/// Wraps `exec` in the shared `WEB_WRAP_SRC` catch-all `try`/`catch (\Throwable)`
/// (→ HTTP 500). Returns the single wrapping `try` statement; on the (unreachable)
/// event that the wrapper template's shape changed, falls back to the unwrapped
/// executables so the body still runs.
fn wrap_executables_in_try(exec: Program) -> Program {
    let wrap_tokens = crate::lexer::tokenize(WEB_WRAP_SRC).expect("web wrapper must tokenize");
    let mut wrapper = crate::parser::parse(&wrap_tokens).expect("web wrapper must parse");
    if let Some(stmt) = wrapper.first_mut() {
        if let StmtKind::Try { try_body, .. } = &mut stmt.kind {
            *try_body = exec;
            return wrapper;
        }
    }
    exec
}

/// The `extern "elephc_web"` declaration block shared by both `--web` and
/// `--web-worker` preludes. Under `--web-worker` the worker-registration bridge
/// `elephc_web_worker_register` is appended so the `elephc_worker_register`
/// builtin can hand the trampoline address to Rust.
const WEB_EXTERN_BLOCK_SRC: &str = r#"extern "elephc_web" {
    function elephc_web_method(): string;
    function elephc_web_uri(): string;
    function elephc_web_path(): string;
    function elephc_web_query_string(): string;
    function elephc_web_header_count(): int;
    function elephc_web_header_name(int $i): string;
    function elephc_web_header_value(int $i): string;
    function elephc_web_header_php_name(int $i): string;
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
    function elephc_web_register_tmp_file(string $path): void;
}"#;

/// The worker-mode bridge extern: `elephc_web_worker_register(ptr $trampoline):
/// void`. Declared separately so it can be appended to the extern block only
/// under `--web-worker`. The trampoline pointer is the address of the compiled
/// `elephc_worker_handle_request` function.
const WORKER_REGISTER_EXTERN_SRC: &str = r#"    function elephc_web_worker_register(ptr $trampoline): void;
"#;

/// Builds `$_SERVER` from the `elephc_web_*` bridge getters. Reads no other
/// superglobal, so it can run first and independently. Extracted from the former
/// monolithic `WEB_SUPERGLOBAL_FILL_SRC` into one fill function per
/// superglobal for readability and testability.
const FILL_SERVER_SRC: &str = r#"$_SERVER = [];
$_SERVER['REQUEST_METHOD'] = elephc_web_method();
$_SERVER['REQUEST_URI']    = elephc_web_uri();
$_SERVER['QUERY_STRING']   = elephc_web_query_string();
$__elephc_hc = elephc_web_header_count();
for ($__elephc_i = 0; $__elephc_i < $__elephc_hc; $__elephc_i++) {
    $__elephc_pn = elephc_web_header_php_name($__elephc_i);
    $__elephc_hv = elephc_web_header_value($__elephc_i);
    $_SERVER[$__elephc_pn] = $__elephc_hv;
    if ($__elephc_pn === 'HTTP_CONTENT_TYPE') { $_SERVER['CONTENT_TYPE'] = $__elephc_hv; }
    if ($__elephc_pn === 'HTTP_CONTENT_LENGTH') { $_SERVER['CONTENT_LENGTH'] = $__elephc_hv; }
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
$_SERVER['SERVER_SOFTWARE']   = 'elephc';"#;

/// Builds `$_GET` by parsing the query string. Independent of other superglobals.
const FILL_GET_SRC: &str = r#"$_GET = [];
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
}"#;

/// Builds `$_POST` and `$_FILES` together (the multipart loop populates both:
/// parts without a filename go to `$_POST`, parts with a filename go to
/// `$_FILES`). Reads `$_SERVER['CONTENT_TYPE']`, so `__elephc_web_fill_server`
/// must run first.
const FILL_POST_SRC: &str = r#"$_POST = [];
$_FILES = [];
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
                elephc_web_register_tmp_file($__elephc_mptmp);
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
}"#;

/// Builds `$_COOKIE` by parsing the `Cookie` request header. Reads
/// `$_SERVER['HTTP_COOKIE']`, so `__elephc_web_fill_server` must run first.
const FILL_COOKIE_SRC: &str = r#"$_COOKIE = [];
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
}"#;

/// Builds `$_REQUEST` by merging `$_GET` then `$_POST` (PHP's request order).
/// Reads `$_GET` and `$_POST`, so those fills must run first.
const FILL_REQUEST_SRC: &str = r#"$_REQUEST = [];
foreach ($_GET as $__elephc_rqk => $__elephc_rqv) {
    $_REQUEST[$__elephc_rqk] = $__elephc_rqv;
}
foreach ($_POST as $__elephc_rqk => $__elephc_rqv) {
    $_REQUEST[$__elephc_rqk] = $__elephc_rqv;
}"#;

/// Builds `$_ENV` from the `elephc_web_env_*` bridge getters. Independent of
/// other superglobals.
const FILL_ENV_SRC: &str = r#"$_ENV = [];
$__elephc_envc = elephc_web_env_count();
for ($__elephc_envi = 0; $__elephc_envi < $__elephc_envc; $__elephc_envi++) {
    $_ENV[elephc_web_env_name($__elephc_envi)] = elephc_web_env_value($__elephc_envi);
}"#;

/// The `setcookie` / `setrawcookie` helper function definitions shared by both
/// preludes. These are top-level declarations (not per-request code), so they
/// live outside the request-init function under `--web-worker`.
const WEB_COOKIE_FUNCS_SRC: &str = r#"function __elephc_emit_cookie($name, $value, $expires, $path, $domain, $secure, $httponly) {
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
}"#;

/// Builds the `--web-worker` prelude PHP source. The prelude contains:
///
/// - The shared `extern "elephc_web"` block with `elephc_web_worker_register`
///   appended (the Rust bridge entry the `elephc_worker_register` builtin calls).
/// - A dummy `$__elephc_worker_handler = function() {};` assignment so the type
///   checker records the handler slot as `Callable` in the top-level environment
///   (the trampoline's `global` declaration then picks up the Callable type).
/// - Six per-superglobal fill functions (`__elephc_web_fill_server`, …,
///   `__elephc_web_fill_env`), split per superglobal for readability and
///   testability. `__elephc_web_fill_post` also builds `$_FILES` because the
///   multipart loop populates both; there is no separate
///   `__elephc_web_fill_files`.
/// - `__elephc_web_request_init(): void` — a backward-compat wrapper that calls
///   all six fill functions in order. The trampoline calls the fill functions
///   directly (so an uncaught Throwable inside one is caught by the
///   trampoline's own try/catch), but the wrapper remains for any caller that
///   expects it.
/// - `elephc_worker_handle_request(): int` — the C-ABI trampoline the Rust
///   worker loop calls per request. It calls the six fill functions in order,
///   invokes the registered handler closure, catches `\Throwable` → HTTP 500,
///   and returns 0.
/// - The `setcookie`/`setrawcookie` helper definitions (top-level declarations).
///
/// Unlike `--web`, the boot top-level is NOT wrapped in a catch-all try: a boot
/// crash is a startup failure the master respawns from with backoff. Superglobals
/// are NOT filled at boot time — there is no request context during boot; the
/// trampoline fills them per request via the fill functions.
fn worker_prelude_src(sg: &WebSuperglobals) -> String {
    let mut src = String::from("<?php\n");
    src.push_str(WEB_EXTERN_BLOCK_SRC);
    src.push_str("\n");
    // Insert the worker-register bridge decl inside the extern block by
    // re-emitting a closing brace after appending the extra declaration. The
    // shared block above is closed already, so we append a second extern block
    // fragment carrying only the worker-register declaration.
    src.push_str("extern \"elephc_web\" {\n");
    src.push_str(WORKER_REGISTER_EXTERN_SRC);
    src.push_str("}\n");
    // Type the handler slot as Callable in the top-level env via a dummy
    // closure assignment; the real handler is stored by elephc_worker_register.
    src.push_str("$__elephc_worker_handler = function() {};\n");
    // Per-superglobal fill functions, each with a small frame — but only the ones
    // the program references (B1). Order at call sites still matters: fill_post
    // and fill_cookie read $_SERVER, and fill_request reads $_GET and $_POST, and
    // `close_dependencies` guarantees the prerequisite flags are set whenever a
    // dependent one is.
    if sg.server {
        src.push_str("function __elephc_web_fill_server(): void {\n");
        src.push_str(FILL_SERVER_SRC);
        src.push_str("\n}\n");
    }
    if sg.get {
        src.push_str("function __elephc_web_fill_get(): void {\n");
        src.push_str(FILL_GET_SRC);
        src.push_str("\n}\n");
    }
    if sg.post {
        src.push_str("function __elephc_web_fill_post(): void {\n");
        src.push_str(FILL_POST_SRC);
        src.push_str("\n}\n");
    }
    if sg.cookie {
        src.push_str("function __elephc_web_fill_cookie(): void {\n");
        src.push_str(FILL_COOKIE_SRC);
        src.push_str("\n}\n");
    }
    if sg.request {
        src.push_str("function __elephc_web_fill_request(): void {\n");
        src.push_str(FILL_REQUEST_SRC);
        src.push_str("\n}\n");
    }
    if sg.env {
        src.push_str("function __elephc_web_fill_env(): void {\n");
        src.push_str(FILL_ENV_SRC);
        src.push_str("\n}\n");
    }
    // Backward-compat wrapper: calls the enabled fill functions in order,
    // including `$_ENV` (a caller that invokes this expects every superglobal).
    src.push_str("function __elephc_web_request_init(): void {\n");
    push_fill_calls(&mut src, sg, "    ", true);
    src.push_str("}\n");
    // The trampoline: C-ABI entry the Rust worker loop calls per request. It
    // fills the referenced request superglobals via the enabled fill functions in
    // dependency order (server → get → post → cookie → request), then invokes the
    // registered handler closure. The try/catch converts uncaught Throwables
    // (thrown by the fills or by the handler) to HTTP 500 instead of crashing the
    // worker (the master would respawn, dropping the connection). `$_ENV` is NOT
    // filled here: it is built once at boot below (B2 — the process environment is
    // fixed at fork, so the trampoline skips it and the worker request-reset leaves
    // it untouched, giving Option B semantics — a boot-time snapshot that persists
    // for the worker's lifetime).
    src.push_str("function elephc_worker_handle_request(): int {\n");
    src.push_str("    global $__elephc_worker_handler;\n");
    src.push_str("    try {\n");
    push_fill_calls(&mut src, sg, "        ", false);
    src.push_str("        $__elephc_worker_handler();\n");
    src.push_str("    } catch (\\Throwable $__elephc_exc) {\n");
    src.push_str("        http_response_code(500);\n");
    src.push_str("    }\n");
    src.push_str("    return 0;\n");
    src.push_str("}\n");
    src.push_str(WEB_COOKIE_FUNCS_SRC);
    src.push_str("\n");
    // Boot-time one-shot `$_ENV` fill (B2). The worker boot top-level runs exactly
    // once before the accept loop, so building `$_ENV` here — rather than in the
    // per-request trampoline — reads the (fork-fixed) process environment a single
    // time per worker. The worker request-reset is told to leave `$_ENV` alone
    // (`env_persistent`), so this value survives every request.
    if sg.env {
        src.push_str("__elephc_web_fill_env();\n");
    }
    src
}

/// Appends the enabled `__elephc_web_fill_*()` call statements (in dependency
/// order) to `src`, each prefixed with `indent`. Shared by the request-init
/// wrapper and the trampoline so both call exactly the fill functions that were
/// defined for the detected superglobal set.
///
/// `include_env` gates the `$_ENV` fill: the per-request trampoline passes `false`
/// because `$_ENV` is built once at boot and persists for the worker's lifetime
/// (B2 — the process environment is fixed at fork, so re-reading it each request
/// is wasted work). The `__elephc_web_request_init` compatibility wrapper passes
/// `true` so a caller that invokes it still gets `$_ENV` populated.
fn push_fill_calls(src: &mut String, sg: &WebSuperglobals, indent: &str, include_env: bool) {
    if sg.server {
        src.push_str(indent);
        src.push_str("__elephc_web_fill_server();\n");
    }
    if sg.get {
        src.push_str(indent);
        src.push_str("__elephc_web_fill_get();\n");
    }
    if sg.post {
        src.push_str(indent);
        src.push_str("__elephc_web_fill_post();\n");
    }
    if sg.cookie {
        src.push_str(indent);
        src.push_str("__elephc_web_fill_cookie();\n");
    }
    if sg.request {
        src.push_str(indent);
        src.push_str("__elephc_web_fill_request();\n");
    }
    if sg.env && include_env {
        src.push_str(indent);
        src.push_str("__elephc_web_fill_env();\n");
    }
}

/// Prepends the `--web-worker` prelude when compiling with `--web-worker`.
///
/// Unlike `inject_if_web`, the boot top-level is NOT wrapped in a catch-all try
/// (a boot crash is a startup failure the master respawns from). The prelude
/// declares the extern bridge, the request-init function, the trampoline, the
/// cookie helpers, and only the fill functions for the superglobals the program
/// references (B1, via [`detect_superglobals`]), then the user program follows.
/// Returns the program unchanged when `web_worker` is false.
pub fn inject_if_web_worker(program: Program, web_worker: bool) -> Program {
    if !web_worker {
        return program;
    }
    let sg = detect_superglobals(&program);
    let prelude = worker_prelude_src(&sg);
    let tokens = crate::lexer::tokenize(&prelude).expect("web-worker prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("web-worker prelude must parse");
    combined.extend(program);
    combined
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
    //! Unit tests for the B1 superglobal-usage detection that gates which web
    //! prelude fills are injected.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Detection is a safe over-approximation; these tests pin the empty case,
    //!   the dependency closure, and the `Debug`-marker coupling that would
    //!   otherwise silently under-detect if the `Variable` AST variant changed.

    use super::*;
    use crate::parser::ast::ExprKind;

    /// Parses PHP source into a `Program` for detection tests.
    fn parse(src: &str) -> Program {
        let tokens = crate::lexer::tokenize(src).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// The `Debug`-scan markers must equal the real `Debug` rendering of a
    /// `Variable` node, so a rename of the AST variant breaks this test instead of
    /// silently making detection under-report (which would omit a needed fill).
    #[test]
    fn debug_markers_match_variable_debug() {
        assert_eq!(MARK_SERVER, format!("{:?}", ExprKind::Variable("_SERVER".into())));
        assert_eq!(MARK_GET, format!("{:?}", ExprKind::Variable("_GET".into())));
        assert_eq!(MARK_POST, format!("{:?}", ExprKind::Variable("_POST".into())));
        assert_eq!(MARK_FILES, format!("{:?}", ExprKind::Variable("_FILES".into())));
        assert_eq!(MARK_COOKIE, format!("{:?}", ExprKind::Variable("_COOKIE".into())));
        assert_eq!(MARK_REQUEST, format!("{:?}", ExprKind::Variable("_REQUEST".into())));
        assert_eq!(MARK_ENV, format!("{:?}", ExprKind::Variable("_ENV".into())));
    }

    /// A program that references no superglobal detects the empty set, so a
    /// hello-world pays no per-request superglobal build cost.
    #[test]
    fn detects_none_for_plain_program() {
        let sg = detect_superglobals(&parse("<?php echo \"ok\";"));
        assert_eq!(sg, WebSuperglobals::none());
    }

    /// `$_GET` alone builds only `$_GET` (it has no dependency on `$_SERVER`).
    #[test]
    fn get_is_independent() {
        let sg = detect_superglobals(&parse("<?php echo $_GET[\"x\"];"));
        assert!(sg.get);
        assert!(!sg.server && !sg.post && !sg.cookie && !sg.request && !sg.env);
    }

    /// `$_POST` pulls in `$_SERVER` (the fill reads `CONTENT_TYPE`) but nothing else.
    #[test]
    fn post_pulls_in_server() {
        let sg = detect_superglobals(&parse("<?php echo $_POST[\"x\"];"));
        assert!(sg.post && sg.server);
        assert!(!sg.get && !sg.cookie && !sg.request && !sg.env);
    }

    /// `$_FILES` is served by the `$_POST` fill, so it enables `post` (and thus
    /// `$_SERVER`), even without a literal `$_POST` reference.
    #[test]
    fn files_enables_post_fill() {
        let sg = detect_superglobals(&parse("<?php echo $_FILES[\"f\"][\"name\"];"));
        assert!(sg.post && sg.server);
    }

    /// `$_REQUEST` transitively pulls in `$_GET`, `$_POST` and `$_SERVER`, but not
    /// `$_ENV` or `$_COOKIE`.
    #[test]
    fn request_closes_over_get_post_server() {
        let sg = detect_superglobals(&parse("<?php echo $_REQUEST[\"x\"];"));
        assert!(sg.request && sg.get && sg.post && sg.server);
        assert!(!sg.cookie && !sg.env);
    }

    /// A superglobal referenced only inside a string interpolation is detected,
    /// because interpolation lowers to a real `Variable` node.
    #[test]
    fn detects_interpolated_reference() {
        let sg = detect_superglobals(&parse("<?php echo \"host=$_SERVER\";"));
        assert!(sg.server);
    }

    /// A superglobal referenced only inside a function body is detected, because
    /// detection scans the whole statement subtree (function bodies included).
    #[test]
    fn detects_reference_inside_function_body() {
        let sg = detect_superglobals(&parse("<?php function f() { return $_ENV[\"HOME\"]; }"));
        assert!(sg.env);
    }
}
