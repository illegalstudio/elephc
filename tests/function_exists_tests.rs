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

/// Compiles `source` with an INCLUDED file's conditional declarations bound at execution time.
///
/// That behaviour is gated off by default — see `conditional_include_declarations_are_bound` in
/// `src/resolver/declarations.rs` for the reason and for what has to land before the gate can go.
/// These tests are what will show the gate can be removed.
fn compile_binding_conditional_includes(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .env("ELEPHC_BIND_CONDITIONAL_INCLUDE_DECLARATIONS", "1")
        .current_dir(dir)
        .arg(&php)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
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

/// Verifies a session function answers the same through BOTH forms in a build without `--web`,
/// and a name no build declares answers false -- the dynamic table tracks this compilation's
/// actual declarations rather than a fixed PHP function list.
///
/// `session_regenerate_id` used to be the `--web`-only sentinel here (`00`). The session surface
/// is now declared for CLI builds too (`session_prelude`), because php's CLI SAPI ships
/// `ext/session`: `php -n` answers `11`, and so must this build.
#[test]
fn session_function_present_without_web_flag_in_both_forms() {
    let dir = make_test_dir("fnexists_noweb");
    let src = "<?php \
        $n = 'session_regenerate_id'; \
        echo function_exists($n) ? '1' : '0'; \
        echo function_exists('session_regenerate_id') ? '1' : '0'; \
        $m = 'session_no_such_function'; \
        echo function_exists($m) ? '1' : '0'; \
        echo \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "110\n",
        "the CLI session surface exists in both forms, and an undeclared name does not"
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

/// Verifies a BOXED argument — `mixed`, the shape real code actually holds — reaches the runtime
/// lookup instead of being refused.
///
/// Symfony's `ControllerResolver::getController()` ends in `\function_exists($controller)` on a
/// value typed `mixed`, after guards that have already returned for the array and object cases.
/// The `string` parameter boundary converts a boxed source, so the lookup gets a real string; the
/// concrete non-string representations it cannot convert stay rejected (see the test below).
///
/// Reference PHP 8.5.6 prints `fn;no;array;object;no` for this program.
#[test]
fn boxed_gradual_argument_is_resolved_at_runtime() {
    let dir = make_test_dir("fnexists_gradual");
    let src = "<?php \
        function probe(mixed $c): string { \
            if (is_array($c)) { return 'array'; } \
            if (is_object($c)) { return 'object'; } \
            if (function_exists($c)) { return 'fn'; } \
            return 'no'; \
        } \
        echo probe('strlen'), ';', probe('nope_nope'), ';', probe([1]), ';', \
             probe(new stdClass()), ';', probe(5);";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "fn;no;array;object;no",
        "a boxed name must be converted at the string parameter boundary and looked up at runtime"
    );
}

/// Verifies a CONCRETE non-string argument is still rejected, with a source-located compiler
/// diagnostic rather than a backend-internal "unsupported EIR backend feature" message.
///
/// The string parameter boundary converts a boxed source only (see
/// `boxed_gradual_argument_is_resolved_at_runtime`); a value whose static representation is a raw
/// `int` reaches the lookup unconverted, so the checker refuses it rather than letting codegen
/// fail. PHP would answer `false` here, which is the remaining gap this diagnostic marks.
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

// ---------------------------------------------------------------------------------------------
// Conditionally declared functions.
//
// php binds an unconditional top-level `function f() {}` when the file is COMPILED and one nested
// in a conditional statement only when that statement EXECUTES. Every polyfill in PHP is written
// against the second rule, and the two halves of the compiler must agree about it: a program must
// never be told a function exists and then fatal on calling it, nor be told it does not exist when
// it does. Each expectation below was taken from php 8.5.10 running the same source.
// ---------------------------------------------------------------------------------------------

/// Verifies the guarded-declaration shape every PHP polyfill uses: a name this build does not
/// provide, declared inside `if (!function_exists(...))`, is reported as existing AND is callable.
///
/// php 8.5.10 prints "bool(true)\nPHI\n". Before conditional declarations were bound at execution
/// time, `function_exists()` answered true here and the very next call raised
/// `Call to undefined function` — the two halves disagreeing about one name in one program.
#[test]
fn guarded_declaration_of_an_unprovided_name_is_callable() {
    let dir = make_test_dir("fnexists_guarded");
    let src = "<?php \
        if (!function_exists('elephc_probe_upper')) { \
            function elephc_probe_upper(string $s): string { return 'P' . strtoupper($s); } \
        } \
        var_dump(function_exists('elephc_probe_upper')); \
        echo elephc_probe_upper('hi'), \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "bool(true)\nPHI\n",
        "a function declared behind a function_exists() guard must be callable"
    );
}

/// Verifies `function_exists()` answers false BEFORE the declaring statement runs and true after,
/// which is the difference between php's compile-time and execution-time binding.
///
/// php 8.5.10 prints "bool(false)\nbool(true)\nL\n".
#[test]
fn conditional_declaration_is_invisible_until_its_statement_runs() {
    let dir = make_test_dir("fnexists_beforeafter");
    let src = "<?php \
        var_dump(function_exists('elephc_probe_later')); \
        if (count([1]) === 1) { \
            function elephc_probe_later(): string { return 'L'; } \
        } \
        var_dump(function_exists('elephc_probe_later')); \
        echo elephc_probe_later(), \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "bool(false)\nbool(true)\nL\n",
        "a conditional declaration must not exist before its statement executes"
    );
}

/// Verifies a declaration whose branch never runs does not exist, and that calling it raises a
/// CATCHABLE `Error` rather than a process-level fatal.
///
/// php 8.5.10 prints "bool(false)\ncaught: Call to undefined function elephc_probe_never()\n".
/// The guard reads the process environment so no constant folder can decide it.
#[test]
fn an_unexecuted_conditional_declaration_does_not_exist() {
    let dir = make_test_dir("fnexists_never");
    let src = "<?php \
        $argc = count($_SERVER['argv'] ?? []); \
        if ($argc > 100) { \
            function elephc_probe_never(): string { return 'N'; } \
        } \
        var_dump(function_exists('elephc_probe_never')); \
        try { elephc_probe_never(); } catch (Error $e) { echo 'caught: ', $e->getMessage(), \"\\n\"; }";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "bool(false)\ncaught: Call to undefined function elephc_probe_never()\n",
        "an unexecuted conditional declaration must be absent and its call must throw"
    );
}

/// Verifies two mutually exclusive branches may declare the SAME name, which php allows because
/// only one of them ever binds.
///
/// php 8.5.10 prints "bool(true)\nFIRST\n". Before this was modelled, both bodies were emitted
/// under one symbol and the build failed in the assembler with `symbol '_fn_...' is already
/// defined`.
#[test]
fn two_branches_may_declare_the_same_name() {
    let dir = make_test_dir("fnexists_branches");
    let src = "<?php \
        if (count([1]) === 1) { \
            function elephc_probe_pick(): string { return 'FIRST'; } \
        } else { \
            function elephc_probe_pick(): string { return 'SECOND'; } \
        } \
        var_dump(function_exists('elephc_probe_pick')); \
        echo elephc_probe_pick(), \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "bool(true)\nFIRST\n",
        "only the branch that runs may bind, and the build must not see a duplicate symbol"
    );
}

/// Verifies two separate files each guarding the SAME name link together and the first one wins.
///
/// This is the shape two packages polyfilling one function produce, and the specific hazard a
/// naive fix creates: one symbol defined twice is a link failure. php 8.5.10 prints
/// "bool(true)\nA\n" — the second file's guard is false by the time it runs, so its body is skipped.
#[test]
fn two_files_guarding_one_name_link_and_the_first_wins() {
    let dir = make_test_dir("fnexists_twofiles");
    fs::write(
        dir.join("poly_a.php"),
        "<?php if (!function_exists('elephc_probe_shared')) { \
            function elephc_probe_shared(): string { return 'A'; } }",
    )
    .unwrap();
    fs::write(
        dir.join("poly_b.php"),
        "<?php if (!function_exists('elephc_probe_shared')) { \
            function elephc_probe_shared(): string { return 'B'; } }",
    )
    .unwrap();
    let src = "<?php \
        require __DIR__ . '/poly_a.php'; \
        require __DIR__ . '/poly_b.php'; \
        var_dump(function_exists('elephc_probe_shared')); \
        echo elephc_probe_shared(), \"\\n\";";
    let bin = compile_binding_conditional_includes(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "bool(true)\nA\n",
        "two files guarding one name must link, and the first declaration must win"
    );
}

/// Verifies a guarded declaration inside an INCLUDED file binds when the include executes.
///
/// php 8.5.10 prints "bool(false)\nbool(true)\nPHI\n": the name does not exist before the require
/// and does after. The included file's conditional declaration used to be dropped outright, so
/// `function_exists()` answered false on both sides and the call raised `Call to undefined
/// function`.
#[test]
fn a_guarded_declaration_in_an_included_file_binds_at_the_include() {
    let dir = make_test_dir("fnexists_include");
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::write(
        dir.join("lib/bootstrap.php"),
        "<?php if (!function_exists('elephc_probe_included')) { \
            function elephc_probe_included(string $s): string { return 'P' . strtoupper($s); } }",
    )
    .unwrap();
    let src = "<?php \
        var_dump(function_exists('elephc_probe_included')); \
        require __DIR__ . '/lib/bootstrap.php'; \
        var_dump(function_exists('elephc_probe_included')); \
        echo elephc_probe_included('hi'), \"\\n\";";
    let bin = compile_binding_conditional_includes(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "bool(false)\nbool(true)\nPHI\n",
        "a conditional declaration in an included file must bind when the include runs"
    );
}

/// Verifies a conditional declaration of a name this build PROVIDES as a builtin is left alone:
/// the builtin keeps answering and the build does not refuse the program.
///
/// The guard reads the process environment, so the body survives constant folding and the pass has
/// to decide about it for real. Turning such a declaration into a variant group would put a user
/// symbol in front of the builtin at every call site — and the checker refuses a group over a
/// builtin name outright with `Cannot redeclare built-in function`.
///
/// php 8.5.10 prints "bool(true)\n3\n": it has `strlen` too, and the branch does not run there
/// either.
#[test]
fn a_guarded_declaration_of_a_builtin_name_leaves_the_builtin_in_place() {
    let dir = make_test_dir("fnexists_builtin_guard");
    let src = "<?php \
        $argc = count($_SERVER['argv'] ?? []); \
        if ($argc > 100) { \
            function strlen($s) { return 99; } \
        } \
        var_dump(function_exists('strlen')); \
        echo strlen('abc'), \"\\n\";";
    let bin = compile(&dir, src, "app");
    assert_eq!(
        run_binary(&bin),
        "bool(true)\n3\n",
        "a builtin this build provides must keep answering past a conditional redeclaration"
    );
}
