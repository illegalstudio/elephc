//! Purpose:
//! End-to-end tests that `function_exists()` accepts a NON-LITERAL argument and answers over
//! exactly the same set of declarations the compile-time fold uses: user functions, injected
//! prelude functions, and catalog builtins.
//!
//! Called from:
//! - `cargo test --test function_exists_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   compile a plain executable, run it, and assert stdout — the same harness style as
//!   `extension_loaded_tests` / `opcache_ini_tests`. Host-target only (macOS aarch64 local).
//! - REGRESSION ANCHOR: `foreach (['opcache_reset', ...] as $n) function_exists($n)` used to fail
//!   the COMPILE with `EIR backend error: unsupported EIR backend feature: function_exists with
//!   non-literal function name`. `foreach_over_names_matches_literal_answers` is that repro.
//! - The literal-vs-dynamic tests deliberately ask both forms for the SAME name in one program:
//!   the dynamic path is a baked table and the literal path is a compile-time predicate, so a
//!   drift between the two sources of truth shows up as a differing pair, not as a crash.
//! - Expected values were taken from reference PHP 8.5.6 (`php -d xdebug.mode=off`), except where
//!   a name is outside elephc's builtin catalog; those cases are called out per test.
//! - Compile-failure assertions filter stderr through `elephc_diagnostics` because the system
//!   linker (GNU `ld` on Linux) emits warnings macOS does not.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Runs the compiler on `source` with extra flags and returns its raw output.
fn compile_raw(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> std::process::Output {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.arg("build");
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.args(flags).arg(&php);
    cmd.output().expect("failed to spawn elephc")
}

/// Compiles `source` to a plain executable with extra compiler flags and returns its path.
fn compile_with_flags(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> PathBuf {
    let output = compile_raw(dir, source, stem, flags);
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles `source` to a plain executable with no extra flags.
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    compile_with_flags(dir, source, stem, &[])
}

/// Runs a compiled executable and returns its stdout as a string.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Picks an ephemeral localhost port by binding :0 and releasing it.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Spawns a `--web` binary on `addr` with one worker and blocks until it accepts connections.
///
/// Both output streams go to `/dev/null`: the server is a prefork parent, so an inherited pipe
/// would be held open by the worker and could outlive the test, wedging whatever reads the test
/// harness's stdout.
fn spawn_server(bin: &Path, addr: &str) -> std::process::Child {
    let child = Command::new(bin)
        .arg("--listen")
        .arg(addr)
        .arg("--workers")
        .arg("1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn web server");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(addr).is_ok() {
            return child;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("server did not start listening on {}", addr);
}

/// Stops a spawned server and every prefork worker it left behind.
///
/// `Child::kill()` only signals the parent; the forked worker keeps the listening socket alive and
/// is reparented, so the workers are matched by the `--listen <addr>` argument. The address is used
/// rather than the binary path because `pkill -f` takes an extended regex and the temp-dir name
/// contains `ThreadId(N)` — whose parentheses would be read as a group and never match literally.
/// The ephemeral port makes the pattern unique to this test. The pattern deliberately omits the
/// leading `--` of `--listen`, which `pkill` would parse as one of its own options.
fn stop_server(server: &mut std::process::Child, addr: &str) {
    let _ = server.kill();
    let _ = server.wait();
    let _ = Command::new("pkill")
        .arg("-f")
        .arg(format!("listen {}", addr))
        .status();
}

/// Sends one HTTP/1.1 GET and returns the response with any complete chunked body decoded.
fn http_get(addr: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, addr
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    normalize_complete_http_response(String::from_utf8_lossy(&response).into_owned())
}

/// Decodes a complete chunked response body while preserving the response headers.
fn normalize_complete_http_response(response: String) -> String {
    let Some((headers, body)) = response.split_once("\r\n\r\n") else {
        return response;
    };
    let is_chunked = headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
    });
    if !is_chunked {
        return response;
    }
    let Some(decoded) = decode_complete_chunked_body(body.as_bytes()) else {
        return response;
    };
    format!("{headers}\r\n\r\n{}", String::from_utf8_lossy(&decoded))
}

/// Decodes one complete HTTP chunk stream and rejects truncated or malformed framing.
fn decode_complete_chunked_body(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::new();
    loop {
        let size_end = body.windows(2).position(|window| window == b"\r\n")?;
        let size_line = std::str::from_utf8(&body[..size_end]).ok()?;
        let size_text = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(size_text, 16).ok()?;
        body = &body[size_end + 2..];
        if size == 0 {
            return body.starts_with(b"\r\n").then_some(decoded);
        }
        let chunk = body.get(..size)?;
        body = body.get(size..)?;
        if !body.starts_with(b"\r\n") {
            return None;
        }
        decoded.extend_from_slice(chunk);
        body = &body[2..];
    }
}

/// Keeps only elephc's own diagnostics so linker chatter (GNU `ld` on Linux emits warnings
/// macOS does not) cannot make a stderr assertion platform-dependent.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("Warning: ")
                || line.starts_with("warning:")
                || line.starts_with("warning[")
                || line.contains("error")
                || line.contains("Error")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Verifies the exact program from the bug report compiles and answers like reference PHP.
///
/// Reference PHP 8.5.6 (`php -d xdebug.mode=off`) on the same source prints "1\n1\n"; the
/// dynamic lookup must find both prelude-injected OPcache functions.
#[test]
fn foreach_over_names_matches_literal_answers() {
    let dir = make_test_dir("fnexists_foreach");
    let src = "<?php \
        foreach (['opcache_reset', 'opcache_get_status'] as $n) { echo function_exists($n), \"\\n\"; }";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "1\n1\n",
        "a foreach over prelude-provided function names must report both as existing"
    );
}

/// Verifies a literal name still const-folds: the emitted program must not consult the runtime
/// lookup table at all for a literal, so a name outside the catalog folds to false and a name
/// inside it folds to true, exactly as before this feature.
///
/// Reference PHP prints "bool(true)\nbool(false)\n" for `strlen` / a nonexistent name.
#[test]
fn literal_names_still_const_fold() {
    let dir = make_test_dir("fnexists_literal");
    let src = "<?php \
        var_dump(function_exists('strlen')); \
        var_dump(function_exists('definitely_not_a_function'));";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "bool(true)\nbool(false)\n",
        "literal function_exists() must keep folding to a static boolean"
    );
}

/// Verifies a variable holding a known builtin name reports true and an unknown name reports
/// false, and that both answers match the literal form for the same names.
///
/// Reference PHP prints "1010" for `strlen` / `nope_not_here` in that order.
#[test]
fn variable_name_reports_known_and_unknown() {
    let dir = make_test_dir("fnexists_var");
    let src = "<?php \
        $known = 'strlen'; \
        $unknown = 'nope_not_here'; \
        echo function_exists($known) ? '1' : '0'; \
        echo function_exists('strlen') ? '1' : '0'; \
        echo function_exists($unknown) ? '1' : '0'; \
        echo function_exists('nope_not_here') ? '1' : '0'; \
        echo \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "1100\n",
        "dynamic and literal answers must agree for a known and an unknown name"
    );
}

/// Verifies PHP's case-insensitive function-name lookup on the dynamic path.
///
/// Reference PHP: `function_exists('STRLEN')`, `function_exists('StRlEn')` and
/// `function_exists('\\strlen')` are all true, so the expected output is "111".
#[test]
fn dynamic_lookup_is_case_insensitive_and_accepts_one_leading_separator() {
    let dir = make_test_dir("fnexists_case");
    let src = "<?php \
        foreach (['STRLEN', 'StRlEn', '\\\\strlen'] as $n) { echo function_exists($n) ? '1' : '0'; } \
        echo \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "111\n",
        "function names are matched case-insensitively and tolerate one leading backslash"
    );
}

/// Verifies the empty string and a lone separator report false through a variable, matching PHP.
///
/// Reference PHP prints "00" for `''` and `'\\'`.
#[test]
fn empty_and_separator_only_names_are_false() {
    let dir = make_test_dir("fnexists_empty");
    let src = "<?php \
        $empty = ''; \
        $slash = '\\\\'; \
        echo function_exists($empty) ? '1' : '0'; \
        echo function_exists($slash) ? '1' : '0'; \
        echo \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "00\n",
        "an empty name and a bare namespace separator must both be false"
    );
}

/// Verifies a user-declared function is found through a variable, in any letter case, and that a
/// similarly named undeclared function is not.
///
/// Reference PHP prints "1110" for `myHelper` / `myhelper` / `MYHELPER` / `myHelper2`.
#[test]
fn user_declared_function_found_through_variable() {
    let dir = make_test_dir("fnexists_user");
    let src = "<?php \
        function myHelper(int $x): int { return $x + 1; } \
        echo myHelper(1); \
        foreach (['myHelper', 'myhelper', 'MYHELPER', 'myHelper2'] as $n) { \
            echo function_exists($n) ? '1' : '0'; \
        } \
        echo \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "21110\n",
        "a user-declared function must be found through a variable, case-insensitively"
    );
}

/// Verifies CLI prelude-injected functions are visible to the dynamic lookup exactly as they are
/// to the literal fold.
///
/// Reference PHP 8.5.6 (with OPcache loaded) reports `opcache_reset`, `opcache_get_status` and
/// `opcache_invalidate` as existing; the literal column proves the dynamic answer is not merely a
/// hard-coded true, since a name outside the prelude answers `0` on both.
#[test]
fn prelude_functions_visible_through_variable() {
    let dir = make_test_dir("fnexists_prelude");
    let src = "<?php \
        foreach (['opcache_reset', 'opcache_get_status', 'opcache_invalidate', 'opcache_nope'] as $n) { \
            echo function_exists($n) ? '1' : '0'; \
        } \
        echo '|'; \
        echo function_exists('opcache_reset') ? '1' : '0'; \
        echo function_exists('opcache_get_status') ? '1' : '0'; \
        echo function_exists('opcache_invalidate') ? '1' : '0'; \
        echo function_exists('opcache_nope') ? '1' : '0'; \
        echo \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "1110|1110\n",
        "prelude-provided functions must answer identically through a variable and a literal"
    );
}

/// Verifies the `--web` session prelude is visible to the dynamic lookup inside a real request.
///
/// A `--web` binary only runs as a server, so this drives one HTTP request. `session_start` and
/// `session_regenerate_id` are injected by `src/web_prelude.rs`, not by the builtin catalog, so a
/// `1` here proves the baked table is built from THIS compilation's declarations. Reference PHP
/// (session enabled) reports both as existing.
#[test]
fn web_prelude_functions_visible_through_variable() {
    let dir = make_test_dir("fnexists_web");
    let src = "<?php \
        foreach (['session_start', 'session_regenerate_id', 'session_nope'] as $n) { \
            echo function_exists($n) ? '1' : '0'; \
        } \
        echo '|'; \
        echo function_exists('session_start') ? '1' : '0'; \
        echo function_exists('session_regenerate_id') ? '1' : '0'; \
        echo function_exists('session_nope') ? '1' : '0';";
    let bin = compile_with_flags(&dir, src, "app", &["--web"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&bin, &addr);
    let response = http_get(&addr, "/");
    stop_server(&mut server, &addr);
    assert!(
        response.contains("110|110"),
        "web prelude functions must answer identically through a variable and a literal, got:\n{}",
        response
    );
}

/// Verifies the same `--web`-only name reports false in a build without `--web`, through BOTH
/// forms — the dynamic table tracks this compilation's actual declarations rather than a fixed
/// PHP function list.
///
/// This is the sentinel for the candidate set: if the dynamic path fell back to a static list of
/// PHP function names it would report `session_regenerate_id` as existing here.
#[test]
fn web_only_function_absent_without_web_flag() {
    let dir = make_test_dir("fnexists_noweb");
    let src = "<?php \
        $n = 'session_regenerate_id'; \
        echo function_exists($n) ? '1' : '0'; \
        echo function_exists('session_regenerate_id') ? '1' : '0'; \
        echo \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "00\n",
        "a --web-only function must be absent from a non-web build in both forms"
    );
}

/// Verifies a `Class::method` string is not a function, matching PHP, where
/// `function_exists('Foo::bar') === false` even though `is_callable('Foo::bar')` is true.
#[test]
fn static_method_string_is_not_a_function() {
    let dir = make_test_dir("fnexists_static");
    let src = "<?php \
        class Foo { public static function bar(): int { return 1; } } \
        $n = 'Foo::bar'; \
        echo function_exists($n) ? '1' : '0'; \
        echo function_exists('Foo::bar') ? '1' : '0'; \
        echo \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "00\n",
        "function_exists() must reject Class::method strings on both paths"
    );
}

/// Verifies the dynamic answer survives being computed inside a called function, where the name
/// arrives as a parameter and therefore cannot be const-folded by any upstream pass.
///
/// Reference PHP prints "1\n0\n" for `array_map` and `not_a_real_function`.
#[test]
fn parameter_name_is_resolved_at_runtime() {
    let dir = make_test_dir("fnexists_param");
    let src = "<?php \
        function probe(string $name): string { return function_exists($name) ? \"1\\n\" : \"0\\n\"; } \
        echo probe('array_map'); \
        echo probe('not_a_real_function');";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "1\n0\n",
        "a name arriving as a parameter must be resolved through the runtime lookup"
    );
}

/// Verifies a non-string argument is still rejected, with a source-located compiler diagnostic
/// rather than a backend-internal "unsupported EIR backend feature" message: the lowering has no
/// runtime string conversion, so accepting an int would be a silent miscompile.
#[test]
fn non_string_argument_is_rejected_with_a_clean_diagnostic() {
    let dir = make_test_dir("fnexists_nonstring");
    let src = "<?php $x = 5; var_dump(function_exists($x));";
    let output = compile_raw(&dir, src, "app", &[]);
    assert!(
        !output.status.success(),
        "a non-string function_exists() argument must fail to compile"
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let diagnostics = elephc_diagnostics(&stderr);
    assert!(
        diagnostics.contains("function_exists() first argument must be a string in AOT mode"),
        "expected the checker's string-argument diagnostic, got:\n{}",
        diagnostics
    );
    assert!(
        !diagnostics.contains("EIR backend error"),
        "the rejection must come from the checker, not the backend:\n{}",
        diagnostics
    );
}

/// Verifies the procedural date/time aliases the name resolver rewrites (`date_create`, `idate`,
/// `timezone_open`, …) are visible through a variable, and that a QUALIFIED spelling is not.
///
/// These names never reach the builtin catalog (their call sites are desugared before codegen), so
/// they are the source most likely to drift between the compile-time predicate and the baked table.
/// Reference PHP 8.5.6: `date_create`/`idate`/`\date_create` are true, `\foo\bar\idate` is false.
#[test]
fn date_procedural_aliases_visible_through_variable() {
    let dir = make_test_dir("fnexists_dates");
    let src = "<?php \
        foreach (['date_create', 'idate', 'timezone_open', 'gmstrftime', '\\\\date_create', 'IDATE', '\\\\foo\\\\bar\\\\idate'] as $n) { \
            echo function_exists($n) ? '1' : '0'; \
        } \
        echo '|'; \
        echo function_exists('date_create') ? '1' : '0'; \
        echo function_exists('idate') ? '1' : '0'; \
        echo function_exists('timezone_open') ? '1' : '0'; \
        echo function_exists('gmstrftime') ? '1' : '0'; \
        echo function_exists('\\\\date_create') ? '1' : '0'; \
        echo function_exists('IDATE') ? '1' : '0'; \
        echo function_exists('\\\\foo\\\\bar\\\\idate') ? '1' : '0'; \
        echo \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "1111110|1111110\n",
        "date/time procedural aliases resolve identically through a variable and a literal, \
         and a qualified spelling is false on both"
    );
}
