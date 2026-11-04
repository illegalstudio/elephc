//! Purpose:
//! End-to-end tests for `--closed-world`: a build that links no interpreter (the eval bridge).
//!
//! Called from:
//! - `cargo test --test closed_world_tests` through Rust's test harness.
//!
//! Key details:
//! - Every test drives the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir, like `opcache_preload_tests`. Host-target only.
//! - Runtime-include behaviour is compared against reference php: the value of an include, the
//!   `_once` answers and the two failed-open warnings. The one normalization is php's
//!   `(include_path='…')` suffix, which depends on the php installation and which elephc (having no
//!   include_path) does not print.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), id));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

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

/// Compiles `main.php` of `dir` with `flags`; returns the compiler's output.
fn compile(dir: &Path, flags: &[&str]) -> Output {
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.args(flags);
    cmd.arg("--php-version").arg("8.5");
    cmd.arg(dir.join("main.php"));
    cmd.output().expect("failed to spawn elephc")
}

fn compile_ok(dir: &Path, flags: &[&str]) {
    let output = compile(dir, flags);
    assert!(
        output.status.success(),
        "elephc failed with {flags:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run(dir: &Path) -> Output {
    Command::new(dir.join("main")).current_dir(dir).output().expect("failed to run main")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

// `ReflectionClass` / `ReflectionObject` / `ReflectionEnum` are table-driven and need no interpreter
// (see `reflection_table_tests`); a runtime-built `ReflectionParameter` still does, so it stands for
// "a construct only the interpreter can answer" here.
#[test]
fn closed_world_refuses_to_link_the_bridge_and_lists_every_reason() {
    let dir = make_test_dir("cw_refuse");
    fs::write(
        dir.join("main.php"),
        r#"<?php
function f(int $a) {}
$name = $argc > 5 ? 'g' : 'f';
$reflector = new ReflectionParameter($name, 0);
echo "ok\n";
"#,
    )
    .unwrap();
    // Default builds keep today's behaviour: the interpreter is linked and the program runs.
    compile_ok(&dir, &[]);
    assert_eq!(text(&run(&dir).stdout), "ok\n");
    let _ = fs::remove_file(dir.join("main"));

    let refused = compile(&dir, &["--closed-world"]);
    assert!(!refused.status.success(), "a program that needs the interpreter must not build");
    assert!(!dir.join("main").exists(), "no binary may be left behind");
    let stderr = text(&refused.stderr);
    assert!(stderr.contains("--closed-world"), "{stderr}");
    assert!(stderr.contains("still needs the interpreter"), "{stderr}");
    assert!(stderr.contains("Reflection object"), "the reason must name the construct:\n{stderr}");

    // The environment spelling asks for the same mode.
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.env("ELEPHC_CLOSED_WORLD", "1");
    cmd.current_dir(&dir);
    cmd.arg("--php-version").arg("8.5").arg(dir.join("main.php"));
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("Reflection object"));
}

#[test]
fn closed_world_non_literal_eval_raises_a_runtime_error() {
    let dir = make_test_dir("cw_eval");
    fs::write(
        dir.join("main.php"),
        r#"<?php
$code = $argc > 5 ? "return 1;" : "return 2;";
try {
    $r = eval($code);
    echo "got $r\n";
} catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
$literal = eval('return 6 * 7;');
echo "literal $literal\n";
echo "done\n";
"#,
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(
        text(&output.stdout),
        "Error: eval() is not available in a closed-world build\nliteral 42\ndone\n"
    );
    // Without the flag the same program really evaluates the computed string.
    compile_ok(&dir, &[]);
    assert_eq!(text(&run(&dir).stdout), "got 2\nliteral 42\ndone\n");
}

const PRELOAD: &str = r#"<?php
require __DIR__ . '/decl.php';
opcache_compile_file(__DIR__ . '/ret.php');
"#;

const RET: &str = "<?php\nreturn ['a' => 1, 'b' => [2, 3]];\n";

const DECL: &str = "<?php\nnamespace T;\n\nclass Decl\n{\n    const X = 7;\n}\n";

/// The include probe. `$argc` keeps every path computed at run time so no include is resolved
/// statically. The declaration file is only reached by its constant: php (with no preload) would
/// print `int(1)` for its first include, which is exactly what a closed world must NOT repeat.
const MAIN: &str = r#"<?php
$dir = __DIR__;
$ret = $argc > 9 ? 'x.php' : 'ret.php';
var_dump(include $dir . '/' . $ret);
var_dump(include_once $dir . '/' . $ret);
var_dump(include_once $dir . '/' . $ret);
var_dump(@include $dir . '/nope-' . $argc . '.php');
var_dump(include $dir . '/nope-' . $argc . '.php');
var_dump(include_once $dir . '/nope-' . $argc . '.php');
echo "end\n";
require $dir . '/nope-' . $argc . '.php';
echo "unreachable\n";
"#;

fn include_fixture(prefix: &str) -> PathBuf {
    let dir = make_test_dir(prefix);
    fs::write(dir.join("main.php"), MAIN).unwrap();
    fs::write(dir.join("ret.php"), RET).unwrap();
    fs::write(dir.join("decl.php"), DECL).unwrap();
    fs::write(dir.join("preload.php"), PRELOAD).unwrap();
    dir
}

fn closed_flags(dir: &Path) -> Vec<String> {
    vec![
        "--closed-world".to_string(),
        "--ini".to_string(),
        "opcache.enable_cli=1".to_string(),
        "--ini".to_string(),
        format!("opcache.preload={}", dir.join("preload.php").display()),
    ]
}

/// Drops php's installation-dependent ` (include_path='…')` suffix.
fn without_include_path(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(" (include_path='") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        match tail.find("')") {
            Some(end) => rest = &tail[end + 2..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn closed_world_runtime_include_runs_units_answers_once_and_fails_like_php() {
    let dir = include_fixture("cw_include");
    let flags = closed_flags(&dir);
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    compile_ok(&dir, &flags);
    let ours = run(&dir);

    let php = Command::new("php")
        .args(["-d", "display_errors=stderr", "-d", "log_errors=0", "-d", "html_errors=0"])
        .arg(dir.join("main.php"))
        .current_dir(&dir)
        .output()
        .expect("php must be installed to pin the reference behaviour");

    // The values: a compiled unit's return value, then `true` twice for `_once`, false for the
    // failed includes (the first silenced).
    assert_eq!(text(&ours.stdout), text(&php.stdout), "stdout differs from php");
    assert!(text(&ours.stdout).starts_with("array(2) {"));

    // The warnings: same text, php's include_path suffix aside, in the same order.
    let theirs = without_include_path(&text(&php.stderr));
    let warnings = |stream: &str| -> Vec<String> {
        stream
            .lines()
            .filter(|line| line.starts_with("Warning: "))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(warnings(&text(&ours.stderr)), warnings(&theirs), "warnings differ from php");
    assert_eq!(warnings(&theirs).len(), 5, "{theirs}");

    // `require` of a missing file is fatal in both, with php's message.
    let missing = format!("Failed opening required '{}/nope-1.php'", dir.display());
    assert!(text(&ours.stderr).contains(&format!("Fatal error: Uncaught Error: {missing}")));
    assert!(theirs.contains(&format!("Fatal error: Uncaught Error: {missing}")));
    assert_eq!(ours.status.code(), php.status.code());
    assert!(!text(&ours.stdout).contains("unreachable"));
}

#[test]
fn closed_world_include_of_a_declaration_file_runs_nothing() {
    let dir = make_test_dir("cw_decl");
    fs::write(
        dir.join("main.php"),
        r#"<?php
$file = __DIR__ . '/' . ($argc > 9 ? 'x.php' : 'decl.php');
var_dump(include_once $file);
var_dump(include_once $file);
echo \T\Decl::X, "\n";
"#,
    )
    .unwrap();
    fs::write(dir.join("decl.php"), DECL).unwrap();
    fs::write(
        dir.join("preload.php"),
        "<?php\nrequire __DIR__ . '/decl.php';\n",
    )
    .unwrap();
    let flags = closed_flags(&dir);
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    compile_ok(&dir, &flags);
    let output = run(&dir);
    // The preloaded file is already part of the binary: no redeclaration, `_once` says true.
    assert_eq!(text(&output.stdout), "bool(true)\nbool(true)\n7\n", "{}", text(&output.stderr));
}

#[test]
fn closed_world_program_carries_no_interpreter_symbol() {
    let dir = include_fixture("cw_symbols");
    let mut flags = closed_flags(&dir);
    flags.push("--emit-asm".to_string());
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    compile_ok(&dir, &flags);
    let asm = fs::read_to_string(dir.join("main.s")).expect("assembly was emitted");
    for forbidden in [
        "__elephc_eval_execute",
        "__elephc_eval_include",
        "__elephc_eval_register",
        "__elephc_eval_call",
        "elephc_magician",
        "magician",
    ] {
        assert!(!asm.contains(forbidden), "closed-world assembly mentions {forbidden}");
    }
    assert!(asm.contains("__rt_closed_include"), "runtime includes go through the shared routine");
    // One call per include site: the include code does not grow with the number of sites.
    let calls = asm.matches("__rt_closed_include").count();
    assert!(calls < 20, "unexpectedly many references to the shared include routine: {calls}");

    // The linked program carries no interpreter either.
    let mut flags = closed_flags(&dir);
    flags.retain(|flag| flag != "--emit-asm");
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    compile_ok(&dir, &flags);
    let binary = fs::read(dir.join("main")).unwrap();
    let hit = |needle: &str| {
        binary.windows(needle.len()).any(|window| window == needle.as_bytes())
    };
    assert!(!hit("elephc_magician"));
    assert!(!hit("__elephc_eval_execute"));
}

/// Compiles `source` as `main.php` in a fresh dir with `--closed-world --heap-debug`, runs it, and
/// returns `(stdout, heap summary line, dir)`.
fn closed_run(prefix: &str, source: &str) -> (String, String, PathBuf) {
    let dir = make_test_dir(prefix);
    fs::write(dir.join("main.php"), source).unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    let heap = text(&output.stderr)
        .lines()
        .filter(|line| line.contains("leak summary") || line.starts_with("Fatal"))
        .collect::<Vec<_>>()
        .join("\n");
    (text(&output.stdout), heap, dir)
}

/// Runs `main.php` of `dir` under php and returns its stdout.
fn php_stdout(dir: &Path) -> String {
    let output = Command::new("php")
        .args(["-d", "display_errors=stderr", "-d", "log_errors=0"])
        .arg(dir.join("main.php"))
        .current_dir(dir)
        .output()
        .expect("php must be installed to pin the reference behaviour");
    text(&output.stdout)
}

const AUTOLOAD_CHAIN: &str = r#"<?php
final class Loader
{
    public array $seen = [];
    public function load(string $class): void
    {
        $this->seen[] = $class;
        echo "Loader::load($class)\n";
    }
}
function secondLoader(string $class): void
{
    echo "secondLoader($class)\n";
}
$loader = new Loader();
var_dump(spl_autoload_register([$loader, 'load']));
var_dump(spl_autoload_register('secondLoader'));
var_dump(spl_autoload_register([$loader, 'load']));
echo count(spl_autoload_functions()), "\n";
$prepended = function (string $class): void {
    echo "prepended($class)\n";
};
spl_autoload_register($prepended, true, true);
echo count(spl_autoload_functions()), "\n";
$name = 'Missing' . ($argc > 5 ? 'A' : 'B');
var_dump(class_exists($name));
var_dump(class_exists($name, false));
var_dump(interface_exists($name));
spl_autoload_call('Direct');
var_dump(spl_autoload_unregister('secondLoader'));
var_dump(spl_autoload_unregister('secondLoader'));
echo count(spl_autoload_functions()), "\n";
var_dump(class_exists('Loader'));
var_dump(class_exists($name));
echo implode(',', $loader->seen), "\n";
"#;

#[test]
fn closed_world_autoload_chain_runs_natively_like_php() {
    let (stdout, heap, dir) = closed_run("cw_autoload", AUTOLOAD_CHAIN);
    assert_eq!(stdout, php_stdout(&dir), "autoload chain differs from php");
    assert_eq!(heap, "HEAP DEBUG: leak summary: clean");
    // The interpreter owns no part of it.
    compile_ok(&dir, &["--closed-world", "--emit-asm"]);
    let asm = fs::read_to_string(dir.join("main.s")).unwrap();
    assert!(!asm.contains("__elephc_eval_register") && !asm.contains("__elephc_eval_execute"));
}

const EXTRACT_AND_CLOSURES: &str = r#"<?php
function f(array $a, mixed $x = null, mixed $y = null)
{
    $x = "orig";
    $n = extract($a);
    return $n . ":" . $x . ":" . ($y ?? "none");
}
function g(array $a, mixed $x = null, mixed $y = null)
{
    $x = "orig";
    $n = extract($a, EXTR_SKIP);
    return $n . ":" . $x . ":" . ($y ?? "none");
}
echo f(['x' => 'new', 'y' => 'why']), "\n";
echo g(['x' => 'new', 'y' => 'why']), "\n";
function wrap(mixed $c, int $v)
{
    $f = Closure::fromCallable($c);
    return $f($v);
}
echo wrap('abs', -4), "\n";
echo wrap(fn(int $q) => $q * 2, 21), "\n";
class K { public function m(int $q) { return $q + 1; } public static function s(int $q) { return $q + 100; } }
echo wrap([new K(), 'm'], 1), "\n";
echo wrap('K::s', 1), "\n";
try {
    wrap('no_such_fn', 1);
} catch (TypeError $e) {
    echo "TypeError\n";
}
"#;

#[test]
fn closed_world_extract_and_closure_from_callable_run_natively_like_php() {
    let (stdout, heap, dir) = closed_run("cw_extract", EXTRACT_AND_CLOSURES);
    assert_eq!(stdout, php_stdout(&dir), "extract/fromCallable differ from php");
    assert_eq!(heap, "HEAP DEBUG: leak summary: clean");
}

/// `Closure::fromCallable([$context, $name])` with a method name known only at run time -- Carbon's
/// mixins and Laravel's `Component::createVariableFromMethod()` -- is an ARRAY literal, so it took
/// the eval bridge and refused the closed world. It converts like a gradual value now, keeping the
/// call's scope for a protected method and php's TypeError for a method the class lacks.
const FROM_CALLABLE_RUNTIME_PAIR: &str = r#"<?php
final class Greeter {
    public function hi(string $who): string { return "hi $who"; }
    protected function bye(string $who): string { return "bye $who"; }
    public function own(string $name): Closure { return Closure::fromCallable([$this, $name]); }
}
function make(object $context, string $name): Closure {
    return Closure::fromCallable([$context, $name]);
}
$g = new Greeter();
$names = $argc > 5 ? ['x'] : ['hi', 'bye', 'nope'];
echo $g->own($names[0])('c'), ' ', $g->own($names[1])('c'), ' ', make($g, $names[0])('a'), "\n";
try {
    make($g, $names[2]);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#;

/// A named-argument static call to a class the closed world lacks is php's `Class "X" not found`
/// like the positional form, not an eval-bridge call. Laravel's `ApplicationBuilder` calls
/// `Folio::route($pages, middleware: ...)` behind `class_exists(Folio::class)` without
/// laravel/folio installed, and the routing closure refused the closed world.
#[test]
fn closed_world_named_argument_call_to_an_absent_class_is_class_not_found() {
    let dir = make_test_dir("cw_absent_named");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . str_replace(\"\\\\\", \"/\", $class) . \".php\";\n    if (is_file($file)) { require $file; }\n});\n$b = new Builder();\necho $b->routes(\"pages\"), \"\\n\";\ntry { echo $b->force(); } catch (Error $e) { echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Builder.php"),
        "<?php\nclass Builder {\n    public function routes(?string $pages): string {\n        if (is_string($pages) && class_exists(\\Laravel\\Folio\\Folio::class)) {\n            \\Laravel\\Folio\\Folio::route($pages, middleware: [\"web\"]);\n            return \"folio\";\n        }\n        return \"none\";\n    }\n    public function force(): string { return \\Laravel\\Folio\\Folio::route(\"x\", middleware: []); }\n}\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
    assert!(text(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
}

/// A class file whose file-scope code php runs when the class is first loaded is compiled: the code
/// becomes an initializer that the first `new`, static call, static property or constant of the
/// class (or of a subclass) runs, where php's autoload would. psysh's Hoa readline files end with
/// `register_shutdown_function(...)` and `Console::advancedInteraction()`; a closed world has no
/// autoloader to hand them to, and splicing them would run that code at boot.
#[test]
fn closed_world_class_file_scope_code_runs_on_first_use() {
    let dir = make_test_dir("cw_lazy_file_scope");
    fs::create_dir_all(dir.join("src/Hoa")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . str_replace(\"\\\\\", \"/\", $class) . \".php\";\n    if (is_file($file)) { require $file; }\n});\necho \"boot\\n\";\nfunction open(): string { $f = new Hoa\\File(\"a\"); return $f->name(); }\nfunction count_streams(): int { return Hoa\\Stream::$opened; }\nfunction kind(): string { return Hoa\\Stream::KIND . \"/\" . Hoa\\Node::tag(); }\necho \"before\\n\";\necho open(), \" \", count_streams(), \" \", kind(), \"\\n\";\necho open(), \"\\n\";\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Hoa/Stream.php"),
        "<?php\nnamespace Hoa;\nabstract class Stream {\n    public const KIND = \"stream\";\n    public static int $opened = 0;\n    public function __construct(protected string $n) { self::$opened++; }\n    public static function shutdown(): void { echo \"shutdown \", self::$opened, \"\\n\"; }\n}\nclass Node { public static function tag(): string { return \"node\"; } }\n\\register_shutdown_function([Stream::class, \"shutdown\"]);\n$registry = [\"Library\"];\necho \"Stream.php loaded \", \\count($registry), \"\\n\";\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Hoa/Node.php"),
        "<?php\nnamespace Hoa;\nrequire_once __DIR__ . \"/Stream.php\";\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Hoa/File.php"),
        "<?php\nnamespace Hoa;\nfinal class File extends Stream {\n    public function name(): string { return \"file:\" . $this->n; }\n}\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
    assert!(text(&output.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", text(&output.stderr));
}

/// The php-parser shape of a lazy file: `Lexer.php` requires a file that declares a function behind
/// `if (!function_exists(...))` and then calls it to define its compatibility tokens. The guarded
/// declaration stays a declaration, and the call runs when `Lexer` is first used.
#[test]
fn closed_world_required_guarded_function_and_its_call_run_on_first_use() {
    let dir = make_test_dir("cw_lazy_required_compat");
    fs::create_dir_all(dir.join("src/P")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . str_replace(\"\\\\\", \"/\", $class) . \".php\";\n    if (is_file($file)) { require $file; }\n});\nfunction lex(): string { return (new P\\Lexer())->kind(); }\necho defined(\"T_COMPAT_X\") ? \"early\" : \"late\", \"\\n\";\necho lex(), \" \", defined(\"T_COMPAT_X\") ? T_COMPAT_X : -1, \"\\n\";\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/P/Lexer.php"),
        "<?php\nnamespace P;\nrequire __DIR__ . \"/compat.php\";\nclass Lexer { public function kind(): string { return \"lexer\"; } }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/P/compat.php"),
        "<?php\nnamespace P;\nif (!\\function_exists(\"P\\\\defineCompat\")) {\n    function defineCompat(): void { if (!\\defined(\"T_COMPAT_X\")) { \\define(\"T_COMPAT_X\", 4242); } }\n}\ndefineCompat();\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
    assert!(text(&output.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", text(&output.stderr));
}

/// A class file php itself refuses to load (an override incompatible with its parent's method)
/// and the files whose classes extend it never run in php, so they are notes, not a refused
/// build. psysh's Hoa `FileLink::create(string $name, string $target)` overrides
/// `File::create(string $name)`, and `FileLinkRead` / `FileLinkReadWrite` extend it.
#[test]
fn closed_world_class_file_php_refuses_to_load_is_a_note() {
    let dir = make_test_dir("cw_php_load_fatal");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . $class . \".php\";\n    if (is_file($file)) { require $file; }\n});\nfunction never(): string { return Worse::create('a', 'b') ? 'y' : 'n'; }\necho (new Base())->name(), \"\\n\";\nif ($argc > 5) { echo never(), \"\\n\"; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Base.php"),
        "<?php\nclass Base {\n    public static function create(string $name): bool { return true; }\n    public function name(): string { return 'base'; }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Bad.php"),
        "<?php\nclass Bad extends Base {\n    public static function create(string $name, string $target): bool { return false; }\n}\n",
    )
    .unwrap();
    fs::write(dir.join("src/Worse.php"), "<?php\nclass Worse extends Bad {}\n").unwrap();
    let output = compile(&dir, &["--closed-world", "--heap-debug"]);
    let stderr = text(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("php refuses to load it"), "{stderr}");
    let run = run(&dir);
    assert_eq!(text(&run.stdout), php_stdout(&dir));
    assert!(text(&run.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", text(&run.stderr));
}

/// Dumped cache files reached through computed paths run from the include table: a translation
/// catalogue that builds its result in locals, and a cache value holding anonymous classes. Both
/// were missing from it, so a binary with no interpreter answered `Failed to open stream`.
#[test]
fn closed_world_includes_dumped_cache_files_through_computed_paths() {
    let dir = make_test_dir("cw_dumped_cache");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("var")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::write(
        dir.join("src/Catalogue.php"),
        "<?php\nnamespace App;\nfinal class Catalogue {\n    private ?Catalogue $fallback = null;\n    public function __construct(public string $locale, private array $messages) {}\n    public function add(Catalogue $c): void { $this->fallback = $c; }\n    public function get(string $k): string { return $this->messages[$k] ?? $this->fallback?->get($k) ?? $k; }\n}\n",
    )
    .unwrap();
    fs::write(dir.join("src/Value.php"), "<?php\nnamespace App;\ninterface Value { public function value(): mixed; }\n").unwrap();
    fs::write(
        dir.join("var/catalogue.fr.php"),
        "<?php\n\nuse App\\Catalogue;\n\n$catalogue = new Catalogue('fr', array (\n  'a' => 'A-fr',\n));\n$catalogueEn = new Catalogue('en', array (\n  'b' => 'B-en',\n));\n$catalogue->add($catalogueEn);\n\nreturn $catalogue;\n",
    )
    .unwrap();
    fs::write(
        dir.join("var/metadata.php"),
        "<?php\n\n// This file has been auto-generated.\n\nreturn [['k' => 0], [0 => new class() implements \\App\\Value { public function value(): mixed { return ['deep' => 42]; } }]];\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nforeach (['fr'] as $locale) {\n    $c = include __DIR__ . '/var/catalogue.' . $locale . '.php';\n    echo $c->locale, ' ', $c->get('a'), ' ', $c->get('b'), ' ', $c->get('c'), \"\\n\";\n}\n$name = 'metadata';\n[$keys, $values] = include __DIR__ . \"/var/$name.php\";\necho json_encode($keys), ' ', json_encode($values[0]->value()), \"\\n\";\n",
    )
    .unwrap();
    let output = compile(&dir, &["--closed-world", "--heap-debug"]);
    assert!(output.status.success(), "{}{}", text(&output.stdout), text(&output.stderr));
    let run = run(&dir);
    assert_eq!(text(&run.stdout), "fr A-fr B-en c\n{\"k\":0} {\"deep\":42}\n");
    assert!(text(&run.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", text(&run.stderr));
}

/// A method or function reflected from a run-time value knows where it was declared: the Symfony
/// Demo reads its controller's source through `getFileName()` on every page, and its row carried
/// no location, so the file was `""`.
#[test]
fn closed_world_reflected_members_know_their_file_and_lines() {
    let dir = make_test_dir("cw_member_location");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::write(
        dir.join("src/Ctl.php"),
        "<?php\nnamespace App;\n\nabstract class Base {\n    public function inherited(int $a): int {\n        return $a;\n    }\n}\n\nfinal class Ctl extends Base {\n    public function index(string $x = 'd'): string\n    {\n        return $x;\n    }\n    public static function st(): int { return 1; }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nuse App\\Ctl;\nfunction plain(int $x) {\n    return $x;\n}\n$cases = [\n    new ReflectionMethod(new Ctl(), 'index'),\n    new ReflectionMethod(Ctl::class, 'st'),\n    new ReflectionMethod(new Ctl(), 'inherited'),\n    new ReflectionFunction('plain'),\n];\nforeach ($cases as $r) {\n    echo $r->getName(), ' ', basename($r->getFileName()), ' ', $r->getStartLine(), ' ', $r->getEndLine(), \"\\n\";\n}\n$m = new ReflectionMethod(new Ctl(), 'index');\necho trim(file($m->getFileName())[$m->getStartLine() - 1]), \"\\n\";\n",
    )
    .unwrap();
    let output = compile(&dir, &["--closed-world"]);
    assert!(output.status.success(), "{}{}", text(&output.stdout), text(&output.stderr));
    assert_eq!(
        text(&run(&dir).stdout),
        "index Ctl.php 11 14\nst Ctl.php 15 15\ninherited Ctl.php 5 7\nplain main.php 3 5\n\
         public function index(string $x = 'd'): string\n"
    );
}

/// A backend re-run is a new process: a file the first round classified as harmless (here, one
/// php refuses to load) stays harmless, while one it inherits with no such verdict still refuses
/// the build. `KernelBrowser` was a note in the first round of the Symfony Demo build and a
/// reason to refuse it in the second.
#[test]
fn closed_world_rerun_keeps_the_harmless_verdict_of_a_deferred_file() {
    let dir = make_test_dir("cw_rerun_harmless");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . $class . \".php\";\n    if (is_file($file)) { require $file; }\n});\nfunction never(): string { return (new Browser())->name(); }\necho \"ok\\n\";\nif ($argc > 5) { echo never(), \"\\n\"; }\n",
    )
    .unwrap();
    fs::write(dir.join("src/Browser.php"), "<?php\nclass Browser extends AbstractBrowserFromDevPackage {}\n").unwrap();
    let file = dir.join("src/Browser.php").canonicalize().unwrap();
    let rerun = |harmless: bool| {
        let mut cmd = Command::new(elephc_bin());
        cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
        cmd.env("ELEPHC_DEFERRED_CLASS_FILES", &file);
        if harmless {
            cmd.env("ELEPHC_HARMLESS_DEFERRED_CLASS_FILES", &file);
        }
        cmd.current_dir(&dir).arg("--closed-world").arg("--php-version").arg("8.5").arg(dir.join("main.php"));
        cmd.output().expect("failed to spawn elephc")
    };
    let refused = rerun(false);
    let refused_text = format!("{}{}", text(&refused.stdout), text(&refused.stderr));
    assert!(!refused.status.success(), "{refused_text}");
    assert!(refused_text.contains("handed to the runtime autoloader"), "{refused_text}");
    let kept = rerun(true);
    assert!(kept.status.success(), "{}{}", text(&kept.stdout), text(&kept.stderr));
}

/// A class file whose parent no file declares (symfony/framework-bundle's `KernelBrowser` over
/// the dev-only browser-kit) is one php refuses to load (`Class "…" not found`), so it and its
/// subclasses are notes, not a refused closed-world build.
#[test]
fn closed_world_class_file_with_a_missing_parent_is_a_note() {
    let dir = make_test_dir("cw_missing_parent");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . $class . \".php\";\n    if (is_file($file)) { require $file; }\n});\nfunction never(): string { return (new Browser2())->name(); }\necho (new Base())->name(), \"\\n\";\nif ($argc > 5) { echo never(), \"\\n\"; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Base.php"),
        "<?php\nclass Base {\n    public function name(): string { return 'base'; }\n}\n",
    )
    .unwrap();
    fs::write(dir.join("src/Browser.php"), "<?php\nclass Browser extends AbstractBrowserFromDevPackage {}\n").unwrap();
    fs::write(dir.join("src/Browser2.php"), "<?php\nclass Browser2 extends Browser {}\n").unwrap();
    let output = compile(&dir, &["--closed-world", "--heap-debug"]);
    let stderr = text(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("php refuses to load it"), "{stderr}");
    let run = run(&dir);
    assert_eq!(text(&run.stdout), php_stdout(&dir));
    assert!(text(&run.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", text(&run.stderr));
}

#[test]
fn closed_world_closure_from_callable_of_a_runtime_method_pair() {
    let (stdout, heap, dir) = closed_run("cw_fromcallable_pair", FROM_CALLABLE_RUNTIME_PAIR);
    assert_eq!(stdout, php_stdout(&dir), "fromCallable pair differs from php");
    assert_eq!(heap, "HEAP DEBUG: leak summary: clean");
}

#[test]
fn closed_world_indexes_composers_generated_runtime_classes() {
    let dir = make_test_dir("cw_composer");
    fs::write(
        dir.join("composer.json"),
        r#"{"autoload": {"psr-4": {"App\\": "src/"}}}"#,
    )
    .unwrap();
    fs::create_dir_all(dir.join("vendor/composer")).unwrap();
    fs::write(
        dir.join("vendor/composer/autoload_classmap.php"),
        "<?php\n\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\nreturn array(\n    'Composer\\\\InstalledVersions' => $vendorDir . '/composer/InstalledVersions.php',\n);\n",
    )
    .unwrap();
    fs::write(
        dir.join("vendor/composer/InstalledVersions.php"),
        "<?php\nnamespace Composer;\nclass InstalledVersions\n{\n    public static function isInstalled(string $package): bool\n    {\n        return $package === 'known/package';\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
use Composer\InstalledVersions;
$package = $argc > 5 ? 'other/package' : 'known/package';
var_dump(InstalledVersions::isInstalled($package));
var_dump(InstalledVersions::isInstalled('absent/package'));
"#,
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world"]);
    assert_eq!(text(&run(&dir).stdout), "bool(true)\nbool(false)\n");
}

const MIXED_RECEIVER_CONSTANT_DEFAULT: &str = r####"<?php
namespace K;

interface HttpKernelInterface
{
    public const MAIN_REQUEST = 1;
    public function handle(Req $request, int $type = HttpKernelInterface::MAIN_REQUEST, bool $catch = true): string;
}

final class Req {}

abstract class Base implements HttpKernelInterface
{
    public function handle(Req $request, int $type = HttpKernelInterface::MAIN_REQUEST, bool $catch = true): string
    {
        return "handled $type";
    }
}

final class App extends Base {}

final class Other
{
    public function handle(string $x = 'a'): string { return "other $x"; }
}

function make(int $n): mixed
{
    return $n > 5 ? new Other() : new App();
}

$k = make($argc);
echo $k->handle(new Req()), "\n";
"####;

/// A boxed receiver calling a method with an OMITTED parameter whose default is a constant
/// expression (`int $type = HttpKernelInterface::MAIN_REQUEST`): the dispatch folds the default
/// to its literal. Before, the candidate was dropped and a closed-world build (which has no
/// interpreter to answer the call) died as "Call to undefined method".
#[test]
fn closed_world_mixed_receiver_call_stages_a_constant_expression_default() {
    let (stdout, heap, dir) = closed_run("cw_const_default", MIXED_RECEIVER_CONSTANT_DEFAULT);
    assert_eq!(stdout, php_stdout(&dir));
    assert_eq!(stdout, "handled 1\n");
    assert!(heap.contains("clean"), "{heap}");
}

const CALLABLE_PARAMETER_THROUGH_BOXED_RECEIVER: &str = r####"<?php
class Resolver
{
    public function run(callable $callable, string $arg): string
    {
        $reflected = new ReflectionFunction($callable(...));
        return $reflected->getNumberOfParameters() . ':' . $callable($arg);
    }

    public function maybe(?callable $callable): string
    {
        return $callable === null ? 'none' : (string) $callable('x');
    }

    public function twice(string $name): string
    {
        return $name . $name;
    }
}

class Other
{
    public function run(string $a, string $b): string
    {
        return $a . $b;
    }
}

function make(int $n): mixed
{
    return $n > 5 ? new Other() : new Resolver();
}

$resolver = make($argc);
$suffix = static fn (string $s): string => $s . '!';
echo $resolver->run($suffix, 'a'), "\n";
echo $resolver->maybe($suffix), "\n";
echo $resolver->maybe(null), "\n";
"####;

/// A `mixed` receiver's candidate call must hand a `callable` parameter the descriptor inside the
/// boxed argument, not the box. The callee's `$callable(...)` and `new ReflectionFunction($callable)`
/// read the box as a descriptor: Symfony's runtime (`GenericRuntime::getResolver()`) died on it.
#[test]
fn closed_world_boxed_receiver_passes_a_callable_parameter_its_descriptor() {
    let (stdout, heap, dir) =
        closed_run("cw_callable_param", CALLABLE_PARAMETER_THROUGH_BOXED_RECEIVER);
    assert_eq!(stdout, php_stdout(&dir));
    assert!(heap.contains("clean"), "{heap}");
}

/// A class string handed to a private helper that probes and instantiates its parameter
/// (`$this->addIfExists(Pass::class)` → `if (class_exists($class)) { new $class(); }`) is part of
/// the world. symfony/framework-bundle registers a dozen compiler passes this way, and the
/// closed-world console could not build its container without them.
#[test]
fn closed_world_class_string_parameter_reaches_the_probed_class() {
    let dir = make_test_dir("cw_param_class_string");
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/Pass.php"),
        "<?php\nnamespace App;\nclass Pass { public function name(): string { return 'pass'; } }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Bundle.php"),
        "<?php\nnamespace App;\nclass Bundle {\n    private array $passes = [];\n    public function build(): array {\n        $this->addIfExists(Pass::class);\n        $this->addIfExists('App\\\\Missing');\n        return $this->passes;\n    }\n    private function addIfExists(string $class): void {\n        if (class_exists($class)) { $this->passes[] = new $class(); }\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $c): void { $f = __DIR__ . '/src/' . substr($c, 4) . '.php'; if (is_file($f)) { require $f; } });\nforeach ((new App\\Bundle())->build() as $p) { echo $p->name(), \"\\n\"; }\necho \"done\\n\";\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "pass\ndone\n");
}

/// An inherited method that derives a class name from `static::class`
/// (`substr_replace(static::class, '\Configuration', strrpos(static::class, '\\'))`, which is how
/// every Symfony bundle extension finds its `Configuration`) reaches the derived class of each
/// concrete subclass.
#[test]
fn closed_world_class_derived_from_static_class_is_part_of_the_world() {
    let dir = make_test_dir("cw_static_class_derived");
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::create_dir_all(dir.join("src/DI")).unwrap();
    fs::write(
        dir.join("src/BaseExtension.php"),
        // In two steps, as Symfony writes it: the second assignment reads the first.
        "<?php\nnamespace App;\nabstract class BaseExtension {\n    public function configuration(): ?object {\n        $class = static::class;\n        $class = substr_replace($class, '\\\\Configuration', strrpos($class, '\\\\'));\n        return class_exists($class) ? new $class() : null;\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/DI/TwigExtension.php"),
        "<?php\nnamespace App\\DI;\nclass TwigExtension extends \\App\\BaseExtension {}\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/DI/Configuration.php"),
        "<?php\nnamespace App\\DI;\nclass Configuration { public function name(): string { return 'twig config'; } }\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $c): void { $f = __DIR__ . '/src/' . str_replace('\\\\', '/', substr($c, 4)) . '.php'; if (is_file($f)) { require $f; } });\n$config = (new App\\DI\\TwigExtension())->configuration();\necho $config === null ? 'none' : $config->name(), \"\\n\";\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "twig config\n");
}

/// `new ReflectionFunction($closure)` on a closure literal written in a METHOD: its name is
/// `{closure:Bundle::load():N}`, and the `::` there is not a method closure's `Class::method`.
const METHOD_CLOSURE_REFLECTION: &str = r#"<?php
class Def { public array $tags = []; }
class Registry {
    private array $configurators = [];
    public function register(string $attr, callable $c): void { $this->configurators[$attr] = $c; }
    public function inspect(): void {
        foreach ($this->configurators as $attr => $c) {
            $r = new ReflectionFunction($c);
            echo $attr, ": ", $r->getNumberOfParameters(), " ", $r->getParameters()[0]->getType(), "\n";
        }
    }
}
class Bundle {
    public function load(Registry $reg): void {
        $reg->register("A", static function (Def $d) { $d->tags[] = "a"; });
        $reg->register("B", static function (Def $d, object $attr, \ReflectionClass|\ReflectionMethod $m) { $d->tags[] = "b"; });
    }
}
$reg = new Registry();
(new Bundle())->load($reg);
$reg->inspect();
"#;

/// A builtin named by a string that reaches `new ReflectionFunction($f)` through data (twig's
/// `new TwigFunction('max', 'max')`, reflected by `debug:twig`) has a row, and it is internal.
const BUILTIN_REFLECTION_THROUGH_DATA: &str = r#"<?php
class TwigFn { public function __construct(public string $name, public $callable) {} }
$fns = [new TwigFn("max", "max"), new TwigFn("upper", "strtoupper")];
foreach ($fns as $f) {
    $r = new ReflectionFunction($f->callable);
    echo $f->name, ":", $r->getName(), ":", $r->getNumberOfParameters(), ":", $r->isInternal() ? "internal" : "user", "\n";
}
"#;

#[test]
fn closed_world_reflection_of_a_builtin_named_through_data() {
    let (stdout, heap, dir) = closed_run("cw_builtin_reflection_data", BUILTIN_REFLECTION_THROUGH_DATA);
    assert_eq!(stdout, php_stdout(&dir), "{heap}");
    assert_eq!(stdout, "max:max:2:internal\nupper:strtoupper:1:internal\n");
}

/// `isAnonymous()` and the closure's scope class answer from the callable: a first-class callable
/// of a static method is not anonymous and is scoped to its class; a closure literal is anonymous.
const CLOSURE_ORIGIN_REFLECTION: &str = r#"<?php
class K { public static function m(): int { return 1; } }
foreach ([K::m(...), fn() => 1, strlen(...)] as $f) {
    $r = new ReflectionFunction($f);
    echo var_export($r->isAnonymous(), true), " ", var_export($r->getClosureScopeClass()?->name, true), " ", var_export($r->getClosureCalledClass()?->name, true), "\n";
}
"#;

#[test]
fn closed_world_reflection_reports_closure_origin() {
    let (stdout, heap, dir) = closed_run("cw_closure_origin_reflection", CLOSURE_ORIGIN_REFLECTION);
    assert_eq!(stdout, php_stdout(&dir), "{heap}");
}

/// An interface's doc comment is reflected (`debug:autowiring` prints the first line of
/// `Psr\Log\LoggerInterface`'s, "Describes a logger instance.").
const INTERFACE_DOC_COMMENT: &str = r#"<?php
/**
 * Describes a logger instance.
 */
interface LoggerI { public function log(): void; }
/** A class doc. */
class Impl implements LoggerI { public function log(): void {} }
$n = $argc > 5 ? "Impl" : "LoggerI";
var_dump((new ReflectionClass($n))->getDocComment(), (new ReflectionClass("Impl"))->getDocComment());
"#;

#[test]
fn closed_world_reflection_of_an_interface_doc_comment() {
    let (stdout, heap, dir) = closed_run("cw_interface_doc_comment", INTERFACE_DOC_COMMENT);
    assert_eq!(stdout, php_stdout(&dir), "{heap}");
}

/// Every `is_*` type probe is callable by a name built at run time
/// (symfony/dependency-injection's `CheckTypeDeclarationsPass` calls `sprintf('is_%s', $type)`);
/// `is_null`, `is_numeric` and `is_callable` had no wrapper and died "Call to undefined function".
const TYPE_PROBES_BY_RUNTIME_NAME: &str = r#"<?php
foreach (["int", "string", "array", "bool", "float", "object", "iterable", "callable", "null", "numeric", "scalar", "countable"] as $t) {
    $f = sprintf("is_%s", $t);
    echo $t, ":", var_export($f(5), true), " ";
}
$f = "is_callable";
echo var_export($f("strlen"), true), var_export($f("no_such_fn"), true), "\n";
"#;

#[test]
fn closed_world_type_probes_by_runtime_name() {
    let (stdout, heap, dir) = closed_run("cw_type_probes_runtime_name", TYPE_PROBES_BY_RUNTIME_NAME);
    assert_eq!(stdout, php_stdout(&dir), "{heap}");
}

#[test]
fn closed_world_reflection_of_a_closure_written_in_a_method() {
    let (stdout, heap, dir) = closed_run("cw_method_closure_reflection", METHOD_CLOSURE_REFLECTION);
    assert_eq!(stdout, php_stdout(&dir), "{heap}");
    assert_eq!(stdout, "A: 1 Def\nB: 3 Def\n");
}

/// A class named only as `X::class` is part of a closed world: a framework instantiates it from
/// the string later (`Route::compile()` calls `$class::compile()` on its `'compiler_class' =>
/// RouteCompiler::class` option), and no loader can bring it in at run time.
#[test]
fn closed_world_class_named_only_by_class_constant_is_compiled() {
    let dir = make_test_dir("cw_class_constant_probe");
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/Compiler.php"),
        "<?php\nnamespace App;\nclass Compiler { public static function compile(Route $r): string { return 'compiled ' . $r->path; } }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Route.php"),
        "<?php\nnamespace App;\nclass Route {\n    private array $options;\n    public function __construct(public string $path, array $options = []) { $this->options = array_merge(['compiler_class' => Compiler::class], $options); }\n    public function compile(): string { $class = $this->options['compiler_class']; return $class::compile($this); }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $c): void { $f = __DIR__ . '/src/' . substr($c, 4) . '.php'; if (is_file($f)) { require $f; } });\nvar_dump(class_exists('App\\\\Compiler', false));\necho (new App\\Route('/blog'))->compile(), \"\\n\";\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "bool(false)\ncompiled /blog\n");
}

/// A class named only inside a project config file -- a `return static function ($c) {...}` the
/// program includes through a computed path, as a bundle's `Resources/config/*.php` -- is part of
/// the world: the file is an include unit, and the classes its body names are walked like any
/// other reference (a Symfony container rebuild checks every service class named there).
#[test]
fn closed_world_class_named_in_a_config_file_is_compiled() {
    let dir = make_test_dir("cw_config_file_class");
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("config")).unwrap();
    fs::write(
        dir.join("src/Helper.php"),
        "<?php\nnamespace App;\nclass Helper { public function hi(): string { return 'helper'; } }\n",
    )
    .unwrap();
    fs::write(
        dir.join("config/services.php"),
        "<?php\n\nreturn static function (\\ArrayObject $c) {\n    $c['helper'] = \\App\\Helper::class;\n};\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $c): void { $f = __DIR__ . '/src/' . substr($c, 4) . '.php'; if (is_file($f)) { require $f; } });\n$name = $argc > 5 ? 'other' : 'services';\n$load = require __DIR__ . '/config/' . $name . '.php';\n$c = new ArrayObject();\n$load($c);\n$class = $c['helper'];\necho class_exists($class) ? (new $class())->hi() : 'missing', \"\\n\";\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "helper\n");
}

/// A first-class callable on a receiver typed by a class nothing declares (an uninstalled
/// package's: symfony/cache's `DoctrineDbalAdapter` does `$this->conn->executeStatement(...)` on a
/// `Doctrine\DBAL\Connection` property) compiles, and behaves as php at run time.
#[test]
fn closed_world_first_class_callable_on_an_uninstalled_class_receiver() {
    let dir = make_test_dir("cw_uninstalled_receiver");
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/Adapter.php"),
        "<?php\nnamespace App;\nclass Adapter {\n    private \\Missing\\Dependency $conn;\n    public function run(\\Closure $same): bool { return $same($this->conn->go(...)); }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $c): void { $f = __DIR__ . '/src/' . substr($c, 4) . '.php'; if (is_file($f)) { require $f; } });\n$a = new App\\Adapter();\ntry { var_dump($a->run(fn($x) => true)); } catch (\\Error $e) { echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir), "{}", text(&output.stderr));
}

/// A class file whose file scope calls `trigger_deprecation()` (symfony/http-kernel's deprecated
/// `Config/FileLocator.php`) is compiled in a closed world without that call, reported as a note,
/// instead of being left to an autoloader the binary does not have.
#[test]
fn closed_world_class_file_with_a_file_scope_deprecation_is_compiled() {
    let dir = make_test_dir("cw_file_scope_deprecation");
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/Old.php"),
        "<?php\nnamespace App;\n\ntrigger_deprecation('app/old', '1.0', 'The \"%s\" class is deprecated.', Old::class);\n\nclass Old { public function hi(): string { return 'old'; } }\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction trigger_deprecation(string $package, string $version, string $message, mixed ...$args): void { @trigger_error($package . ' ' . $version . ': ' . vsprintf($message, $args), E_USER_DEPRECATED); }\nspl_autoload_register(function (string $c): void { $f = __DIR__ . '/src/' . substr($c, 4) . '.php'; if (is_file($f)) { require $f; } });\nfunction make(): App\\Old { return new App\\Old(); }\necho make()->hi(), \"\\n\";\n",
    )
    .unwrap();
    let output = compile(&dir, &["--closed-world"]);
    let stderr = text(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("without its file-scope `trigger_deprecation()`"), "{stderr}");
    assert!(!stderr.contains("warning: autoload:"), "{stderr}");
    let run_output = run(&dir);
    assert_eq!(text(&run_output.stdout), php_stdout(&dir));
    assert_eq!(text(&run_output.stdout), "old\n");
}

/// A file the program names only through existence probes (`X::class`) that the compiler refuses
/// does not refuse a closed-world build: php would load it only if something used it. The build
/// says so in a note, not a warning.
#[test]
fn closed_world_refused_probe_only_file_is_a_note() {
    let dir = make_test_dir("cw_probe_only_refused");
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    // `Missing\Dependency` is installed nowhere, as Doctrine is not for symfony/cache's
    // `DoctrineDbalAdapter`.
    // A construct the checker refuses: a push onto an `int` property.
    fs::write(
        dir.join("src/Adapter.php"),
        "<?php\nnamespace App;\nclass Adapter {\n    private int $n = 5;\n    public function run(): int { $this->n[] = 3; return 1; }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $c): void { $f = __DIR__ . '/src/' . substr($c, 4) . '.php'; if (is_file($f)) { require $f; } });\n$known = [App\\Adapter::class => 'adapter'];\necho count($known), \"\\n\";\n",
    )
    .unwrap();
    let output = compile(&dir, &["--closed-world"]);
    let stderr = text(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("note: autoload:") && stderr.contains("Adapter.php"), "{stderr}");
    assert!(!stderr.contains("warning: autoload:"), "{stderr}");
    assert_eq!(text(&run(&dir).stdout), "1\n");
    // The second build replays the deferral from the memo the first one left: the file is still
    // only probed, so it is still a note. The replay used to warn and refuse the build.
    let output = compile(&dir, &["--closed-world"]);
    let stderr = text(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("remembered from an earlier build"), "{stderr}");
    assert!(stderr.contains("note: autoload:") && stderr.contains("Adapter.php"), "{stderr}");
    assert!(!stderr.contains("warning: autoload:"), "{stderr}");
    assert_eq!(text(&run(&dir).stdout), "1\n");
}

/// The probe-only classification reads the program the autoload pass left, not the one the check
/// rounds hand back after a refusal. A refused file whose only mention is an `X::class` inside a
/// function nothing calls (a bundle's `->set('test.client', KernelBrowser::class)`, whose parent
/// class lives in a package that is not installed) is still only probed: a note, not a refusal.
#[test]
fn closed_world_probe_only_class_in_uncalled_code_is_a_note() {
    let dir = make_test_dir("cw_probe_only_uncalled");
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    // A construct the checker refuses: a push onto an `int` property.
    fs::write(
        dir.join("src/Adapter.php"),
        "<?php\nnamespace App;\nclass Adapter {\n    private int $n = 5;\n    public function run(): int { $this->n[] = 3; return 1; }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $c): void { $f = __DIR__ . '/src/' . substr($c, 4) . '.php'; if (is_file($f)) { require $f; } });\nclass Cfg {\n    public array $s = [];\n    public function set(string $id, string $class): static { $this->s[$id] = $class; return $this; }\n}\nfunction register(Cfg $c): void {\n    $c->set('adapter', App\\Adapter::class);\n}\necho \"ok\\n\";\n",
    )
    .unwrap();
    let output = compile(&dir, &["--closed-world"]);
    let stderr = text(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("note: autoload:") && stderr.contains("Adapter.php"), "{stderr}");
    assert!(!stderr.contains("warning: autoload:"), "{stderr}");
    assert_eq!(text(&run(&dir).stdout), "ok\n");
}

/// Without `--heap-size`, a CLI program gets php's default `memory_limit` (128 MB), as a `--web`
/// one already did: the old 8 MB default exhausted while the Symfony console rebuilt its
/// container. 20 MB of live strings fit, as they do under `php -n`.
#[test]
fn cli_default_heap_is_phps_memory_limit() {
    let dir = make_test_dir("cli_default_heap");
    fs::write(
        dir.join("main.php"),
        "<?php\n$parts = [];\nfor ($i = 0; $i < 20; $i++) { $parts[] = str_repeat(chr(65 + $i % 26), 1048576); }\necho count($parts), ' ', strlen(implode('', $parts)), \"\\n\";\n",
    )
    .unwrap();
    compile_ok(&dir, &[]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "20 20971520\n");
}

/// A data file of the project (`<?php return [...];`) reached through a path computed at run time
/// is answered by a compiled unit. Symfony's kernel requires `$this->getBundlesPath()` and the
/// intl polyfills `__DIR__.'/Resources/unidata/'.$name.'.php'` through a variable; nothing at the
/// include site names the file, and the closed-world console died on both.
#[test]
fn closed_world_runtime_computed_path_reaches_a_project_data_file() {
    let dir = make_test_dir("cw_data_units");
    fs::write(dir.join("composer.json"), r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(dir.join("data/table.php"), "<?php\n\nreturn ['a' => 1, 'b' => [2, 3]];\n").unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nfunction table(string $name): array { $file = __DIR__ . '/data/' . $name . '.php'; return require $file; }\n$name = $argc > 5 ? 'other' : 'table';\n$t = table($name);\necho $t['a'], ' ', implode(',', $t['b']), \"\\n\";\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
    assert_eq!(text(&output.stdout), "1 2,3\n");
    assert!(text(&output.stderr).contains("leak summary: clean"), "{}", text(&output.stderr));
}

/// A literal `require` inside a method stays a run-time include (the resolver only inlines one
/// at the top level), and a closed-world binary has no interpreter to run it. Its file is
/// compiled as an include unit like a preloaded one. `Composer\InstalledVersions` reads
/// `installed.php` this way, and the compiled Symfony console died there on every command.
#[test]
fn closed_world_literal_require_inside_a_method_runs_its_compiled_unit() {
    let dir = make_test_dir("cw_method_require");
    fs::write(
        dir.join("data.php"),
        "<?php\nreturn ['root' => ['pretty_version' => 'v1.2'], 'n' => 3];\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nclass Versions {\n    private static $installed = null;\n    public static function get(): array {\n        if (self::$installed === null) { self::$installed = require __DIR__ . '/data.php'; }\n        return self::$installed;\n    }\n}\necho Versions::get()['root']['pretty_version'], ' ', Versions::get()['n'], \"\\n\";\n// An entry that ends with a top-level return, as bin/console does: the unit must not be\n// placed after it, where the optimizer drops it.\nreturn 1;\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
    assert_eq!(text(&output.stdout), "v1.2 3\n");
    assert!(text(&output.stderr).contains("leak summary: clean"), "{}", text(&output.stderr));
}

/// `get_defined_vars()` after a closed-world `extract()` lists the keys that named no compiled
/// local too, and never the compiler's own extract temporaries: they are variables of the frame
/// for php. A template renderer extracts its context and hands the scope on this way.
#[test]
fn closed_world_defined_vars_list_what_extract_created() {
    let source = "<?php\nfunction scope(array $context): array { $own = 1; extract($context, EXTR_SKIP); return get_defined_vars(); }\n$vars = scope(['own' => 9, 'extra' => 'x', 'n' => $argc]);\nksort($vars);\necho implode(',', array_keys($vars)), ' ', $vars['own'], $vars['extra'], \"\\n\";\n";
    let (stdout, heap, dir) = closed_run("cw_defined_vars_extract", source);
    assert_eq!(stdout, php_stdout(&dir));
    assert_eq!(stdout, "context,extra,n,own 1x\n");
    assert!(heap.contains("leak summary: clean"), "{heap}");
}

/// A method that includes a template through a computed path renders it in its own scope: the
/// template reads the extracted context, `$this` and a private method, and includes a stylesheet
/// the same way. Symfony's `HtmlErrorRenderer` renders its error pages exactly like this; the
/// closed-world binary answered every one of them with an empty 404 body.
#[test]
fn closed_world_computed_template_include_runs_in_the_callers_scope() {
    let dir = make_test_dir("cw_scoped_templates");
    fs::create_dir_all(dir.join("lib/Renderer")).unwrap();
    fs::create_dir_all(dir.join("lib/Resources/views")).unwrap();
    fs::create_dir_all(dir.join("lib/Resources/assets")).unwrap();
    fs::write(
        dir.join("lib/Renderer/Html.php"),
        "<?php\nnamespace Lib\\Renderer;\nclass Html {\n    private string $charset = 'UTF-8';\n    public function render(string $status, array $items): string { return $this->include('views/page.html.php', ['status' => $status, 'items' => $items]); }\n    private function escape(string $text): string { return htmlspecialchars($text, \\ENT_QUOTES, $this->charset); }\n    private function include(string $name, array $context = []): string {\n        extract($context, \\EXTR_SKIP);\n        ob_start();\n        include is_file(\\dirname(__DIR__).'/Resources/'.$name) ? \\dirname(__DIR__).'/Resources/'.$name : $name;\n        return trim(ob_get_clean());\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("lib/Resources/views/page.html.php"),
        "<html><head><meta charset=\"<?= $this->charset; ?>\"><style><?= $this->include('assets/page.css'); ?></style></head>\n<body><h1><?= $this->escape($status); ?></h1>\n<?php $count = 0; foreach ($items as $index => $item) { $count++; ?>\n<li data-i=\"<?= $index; ?>\"><?= $this->escape($item); ?></li>\n<?php } ?>\n<p><?= $count, ' item', 1 === $count ? '' : 's'; ?><?= isset($missing) ? ' missing' : ''; ?></p>\n</body></html>\n",
    )
    .unwrap();
    fs::write(dir.join("lib/Resources/assets/page.css"), "body { color: #333; }\n").unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nrequire __DIR__ . '/lib/Renderer/Html.php';\n$items = $argc > 5 ? [] : ['a<b', 'c'];\necho (new \\Lib\\Renderer\\Html())->render('Not Found & gone', $items), \"\\n\";\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    let stdout = text(&output.stdout);
    assert_eq!(stdout, php_stdout(&dir), "{}", text(&output.stderr));
    assert!(stdout.contains("<style>body { color: #333; }</style>"), "{stdout}");
    assert!(stdout.contains("<li data-i=\"0\">a&lt;b</li>"), "{stdout}");
    assert!(text(&output.stderr).contains("leak summary: clean"), "{}", text(&output.stderr));
}

/// `new ReflectionMethod($closure, '__invoke')` reflects `Closure::__invoke` with the closure's own
/// signature, and like any internal method's its parameters show no default value. Twig's
/// `debug:twig` reads every closure filter this way and died with `Class "Closure" does not exist`.
#[test]
fn closed_world_reflection_method_of_a_closure_invoke() {
    let source = "<?php\nclass Ext { public function filters(): array { return ['up' => function (string $value, int $times = 2, ?string $sep = null): string { return str_repeat(strtoupper($value), $times); }]; } }\nforeach ((new Ext())->filters() as $name => $cb) {\n    $refl = new \\ReflectionMethod($cb, '__invoke');\n    echo $name, ': ', $refl->getName(), ' ', $refl->class, ' ', $refl->getNumberOfParameters(), '/', $refl->getNumberOfRequiredParameters(), ' ', (string) $refl->getReturnType(), \"\\n\";\n    foreach ($refl->getParameters() as $param) { echo '  ', $param->getName(), $param->isOptional() ? '?' : '', $param->isDefaultValueAvailable() ? ' = ' . json_encode($param->getDefaultValue()) : '', \"\\n\"; }\n}\n";
    let (stdout, heap, dir) = closed_run("cw_reflection_closure_invoke", source);
    assert_eq!(stdout, php_stdout(&dir));
    assert_eq!(stdout, "up: __invoke Closure 3/1 string\n  value\n  times?\n  sep?\n");
    assert!(heap.contains("leak summary: clean"), "{heap}");
}

/// A `$this->method(...)` closure is reflected like its method: `ReflectionFunction` and
/// `ReflectionMethod($closure, '__invoke')` find it by its receiver's class. The descriptor was
/// named `object::method`, which names no class ("Function object::createtranslatable() does not
/// exist" from Twig's `debug:twig`).
#[test]
fn closed_world_reflection_of_a_method_first_class_callable() {
    let source = "<?php\nclass Ext {\n    public function getFilters(): array { return ['trans' => $this->createTranslatable(...)]; }\n    public function createTranslatable(string $message, array $parameters = [], ?string $domain = null): string { return $message . count($parameters) . ($domain ?? '-'); }\n}\nforeach ((new Ext())->getFilters() as $name => $cb) {\n    $r = new \\ReflectionMethod($cb, '__invoke');\n    echo $name, ' ', $r->getNumberOfParameters(), '/', $r->getNumberOfRequiredParameters(), \"\\n\";\n    foreach ($r->getParameters() as $p) { echo '  ', $p->getName(), \"\\n\"; }\n    $f = new \\ReflectionFunction($cb);\n    echo $f->getName(), ' ', $f->getNumberOfParameters(), ' ', $cb('m', [1]), \"\\n\";\n}\n";
    let (stdout, heap, dir) = closed_run("cw_reflection_method_callable", source);
    assert_eq!(stdout, php_stdout(&dir));
    assert_eq!(stdout, "trans 3/1\n  message\n  parameters\n  domain\ncreateTranslatable 3 m1-\n");
    assert!(heap.contains("leak summary: clean"), "{heap}");
}

/// A variadic parameter reflects no default value, builtin or user: Twig's `debug:twig` printed
/// `max(value, values = [])` where php prints `max(value, values)`.
#[test]
fn closed_world_variadic_parameters_reflect_no_default() {
    let source = "<?php\nfunction mine(int $a, string ...$rest): int { return $a + count($rest); }\nforeach (['max', 'min', 'mine', 'sprintf'] as $fn) {\n    $r = new \\ReflectionFunction($fn);\n    $parts = [];\n    foreach ($r->getParameters() as $p) {\n        $parts[] = ($p->isVariadic() ? '...' : '') . $p->getName() . ($p->isDefaultValueAvailable() ? ' = ' . json_encode($p->getDefaultValue()) : '') . ($p->isOptional() ? '?' : '');\n    }\n    echo $fn, '(', implode(', ', $parts), \")\\n\";\n}\n";
    let (stdout, heap, dir) = closed_run("cw_variadic_no_default", source);
    assert_eq!(stdout, php_stdout(&dir));
    assert!(stdout.contains("max(value, ...values?)"), "{stdout}");
    assert!(heap.contains("leak summary: clean"), "{heap}");
}

/// `['Closure', 'fromCallable']` called at run time builds the closure: Symfony's container uses it
/// as a service factory (`$factory(...$arguments)`), and `lint:container` died with "callable array
/// did not resolve to an invokable target" because `Closure` is no compiled class.
#[test]
fn closed_world_runtime_closure_from_callable_factory() {
    let source = "<?php\nclass Lazy { public static function fromCallable(callable $cb, mixed ...$args): string { return 'lazy:' . $cb(...$args); } }\nfunction make(array|string $factory, array $arguments): mixed { return $factory(...$arguments); }\n$names = $argc > 5 ? ['X', 'y'] : ['Closure', 'fromCallable'];\n$c = make($names, ['strtoupper']);\necho get_class($c), ' ', $c('abc'), \"\\n\";\necho make([Lazy::class, 'fromCallable'], ['strrev', 'xyz']), \"\\n\";\n";
    let (stdout, heap, dir) = closed_run("cw_closure_from_callable_factory", source);
    assert_eq!(stdout, php_stdout(&dir));
    assert_eq!(stdout, "Closure ABC\nlazy:zyx\n");
    assert!(heap.contains("leak summary: clean"), "{heap}");
}

/// `Closure::fromCallable([$this, 'protectedMethod'])` converts with its caller's access, whether
/// written directly over a dynamic value or reached as a run-time `['Closure', 'fromCallable']`
/// factory: Symfony's `container.getenv` service wraps the protected `[$container, 'getEnv']`,
/// and `lint:container` died with "cannot access protected method ContainerBuilder::getEnv()".
#[test]
fn closed_world_closure_from_callable_keeps_the_callers_access() {
    let source = "<?php\nclass Base {\n    protected function getEnv(string $name): string { return 'base:' . $name; }\n    public function create(array $factory, array $arguments): mixed { return $factory(...$arguments); }\n}\nclass Builder extends Base {\n    protected function getEnv(string $name): string { return 'builder:' . $name; }\n    public function wrap(mixed $callable): \\Closure { return \\Closure::fromCallable($callable); }\n    public function direct(): \\Closure { return $this->wrap($GLOBALS['argc'] > 5 ? 'strlen' : [$this, 'getEnv']); }\n    public function factory(): \\Closure { return $this->create([\\Closure::class, 'fromCallable'], [[$this, 'getEnv']]); }\n}\n$b = new Builder();\necho $b->direct()('A'), ' ', $b->factory()('B'), \"\\n\";\n";
    let (stdout, heap, dir) = closed_run("cw_from_callable_access", source);
    assert_eq!(stdout, php_stdout(&dir));
    assert_eq!(stdout, "builder:A builder:B\n");
    assert!(heap.contains("leak summary: clean"), "{heap}");
}

/// `is_a($interface, $parentInterface, true)` and `is_subclass_of()` over run-time interface NAMES
/// walk what the interface extends: they answered false, and Symfony's `lint:container` reported
/// its DI `ContainerInterface` as incompatible with PSR's `ContainerInterface`.
#[test]
fn closed_world_runtime_is_a_between_interface_names() {
    let source = "<?php\nnamespace Psr { interface Base { public function get(string $id); } }\nnamespace Sym { interface Child extends \\Psr\\Base { public function set(string $id): void; } class Impl implements Child { public function get(string $id) { return $id; } public function set(string $id): void {} } }\nnamespace {\n    function check(string $class, string $type): string { return is_a($class, $type, true) ? 'y' : 'n'; }\n    $keep = new Sym\\Impl();\n    $pairs = [['Sym\\Child', 'Psr\\Base'], ['Sym\\Impl', 'Psr\\Base'], ['Sym\\Impl', 'Sym\\Child'], ['Psr\\Base', 'Sym\\Child'], ['Sym\\Child', 'Sym\\Child'], ['Sym\\Child', 'Sym\\Impl']];\n    foreach ($pairs as [$c, $t]) { echo check($c, $t), is_subclass_of($c, $t) ? 'y' : 'n', ' '; }\n    echo get_class($keep), \"\\n\";\n}\n";
    let (stdout, heap, dir) = closed_run("cw_is_a_interface_names", source);
    assert_eq!(stdout, php_stdout(&dir));
    assert_eq!(stdout, "yy yy yy nn yn nn Sym\\Impl\n");
    assert!(heap.contains("leak summary: clean"), "{heap}");
}

/// `ReflectionClass('Closure')` and `ReflectionMethod('Closure', 'fromCallable')` reflect php's
/// signature in a program that names the factory at run time: Symfony's `lint:container` checks
/// its `['Closure', 'fromCallable']` factories this way and threw `Class "Closure" does not exist`.
#[test]
fn closed_world_reflection_of_closure_from_callable() {
    let source = "<?php\n$class = $argc > 5 ? 'X' : 'Closure';\ntry {\n    $r = new \\ReflectionClass($class);\n    echo $r->getName(), ' ', $r->isFinal() ? 'final' : '', ' ', $r->isInternal() ? 'internal' : '', \"\\n\";\n    $m = $r->getMethod('fromCallable');\n    echo $m->getName(), ' ', $m->isStatic() ? 'static' : '', ' ', $m->getNumberOfParameters(), \"\\n\";\n    foreach ($m->getParameters() as $p) { echo '  ', $p->getName(), ' ', (string) $p->getType(), \"\\n\"; }\n    echo (string) $m->getReturnType(), \"\\n\";\n    $m2 = new \\ReflectionMethod($class, 'fromCallable');\n    echo $m2->class, '::', $m2->name, \"\\n\";\n} catch (\\Throwable $e) { echo get_class($e), ': ', $e->getMessage(), \"\\n\"; }\n";
    let (stdout, heap, dir) = closed_run("cw_reflection_closure_class", source);
    assert_eq!(stdout, php_stdout(&dir));
    assert_eq!(stdout, "Closure final internal\nfromCallable static 1\n  callback callable\nClosure\nClosure::fromCallable\n");
    assert!(heap.contains("leak summary: clean"), "{heap}");
}

/// `class_alias()` with run-time names (Laravel's AliasLoader reads them from config) registers an
/// alias the by-name lookups resolve: `new $alias`, `$alias::CONST`, `class_exists($alias)`, and a
/// second alias of an alias. An unknown original or a name already in use answers false.
#[test]
fn closed_world_runtime_class_alias_resolves_through_the_by_name_lookups() {
    let dir = make_test_dir("cw_runtime_class_alias");
    fs::create_dir_all(dir.join("src/App")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . str_replace(\"\\\\\", \"/\", $class) . \".php\";\n    if (is_file($file)) { require $file; }\n});\nclass Loader {\n    public function __construct(private array $aliases) {}\n    public function register(): void {\n        foreach ($this->aliases as $alias => $class) { var_dump(@class_alias($class, $alias)); }\n    }\n}\n$aliases = ['Svc' => App\\Service::class, 'Other' => 'App\\Missing'];\nif ($argc > 5) { $aliases['Svc'] = 'App\\Nope'; }\n(new Loader($aliases))->register();\n$name = $argc > 5 ? 'x' : 'Svc';\nvar_dump(class_exists($name), class_exists('svc'), class_exists('Other'));\n$o = new $name();\necho get_class($o), \" \", $o->hello(), \" \", $name::KIND, \"\\n\";\nvar_dump(@class_alias($name, 'Svc2'), @class_alias('App\\\\Service', $name));\n$n2 = 'Svc' . ($argc > 5 ? '' : '2');\n$p = new $n2();\nvar_dump($p instanceof App\\Service);\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/App/Service.php"),
        "<?php\nnamespace App;\nclass Service {\n    public const KIND = \"svc\";\n    public function hello(): string { return \"hi\"; }\n}\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
    assert!(text(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
}

/// A class file that declares its class only under a run-time condition (nunomaduro/collision's
/// PHPUnit subscriber, under `if (class_exists(Version::class) && (int) Version::series() >= 10)`)
/// is compiled: the condition is checked on the class's first use, which raises php's
/// `Class "X" not found` when it does not hold. A closed world has no autoloader to leave it to.
#[test]
fn closed_world_conditionally_declared_class_checks_its_condition_on_first_use() {
    let dir = make_test_dir("cw_conditional_class");
    fs::create_dir_all(dir.join("src/Runner")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . str_replace(\"\\\\\", \"/\", $class) . \".php\";\n    if (is_file($file)) { require $file; }\n});\nuse Runner\\Version;\nif (class_exists(Version::class) && (int) Version::series() >= 10) {\n    echo Subscriber::register(), \"\\n\";\n}\ntry { echo Legacy::register(), \"\\n\"; } catch (Error $e) { echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Runner/Version.php"),
        "<?php\nnamespace Runner;\nfinal class Version {\n    public static function series(): string { return implode('.', array_slice(explode('.', '12.5.40'), 0, 2)); }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Subscriber.php"),
        "<?php\nuse Runner\\Version;\nif (class_exists(Version::class) && (int) Version::series() >= 10) {\n    final class Subscriber { public static function register(): string { return 'registered'; } }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Legacy.php"),
        "<?php\nuse Runner\\Version;\nif ((int) Version::series() < 10) {\n    final class Legacy { public static function register(): string { return 'legacy'; } }\n}\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
    assert!(text(&output.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", text(&output.stderr));
}

/// php-parser's `Lexer.php` requires `compatibility_tokens.php`, whose
/// `if (!function_exists('PhpParser\defineCompatibilityTokens')) { function ...; defineCompatibilityTokens(); }`
/// declares the function and calls it under one guard. The call runs on the lexer's first use,
/// as php runs it when the autoloader includes the file; the mixed `if` left the file to the
/// autoloader.
#[test]
fn closed_world_guarded_function_declared_and_called_in_one_if_runs_on_first_use() {
    let dir = make_test_dir("cw_guarded_fn_call");
    fs::create_dir_all(dir.join("src/PhpParser")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . str_replace(\"\\\\\", \"/\", $class) . \".php\";\n    if (is_file($file)) { require $file; }\n});\necho defined('T_ELEPHC_COMPAT') ? 'early' : 'late', \"\\n\";\n$lexer = new PhpParser\\Lexer();\necho $lexer->token(), \"\\n\";\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/PhpParser/compatibility_tokens.php"),
        "<?php\nnamespace PhpParser;\nif (!\\function_exists('PhpParser\\defineCompatibilityTokens')) {\n    function defineCompatibilityTokens(): void {\n        if (!\\defined('T_ELEPHC_COMPAT')) { \\define('T_ELEPHC_COMPAT', -1); }\n    }\n    defineCompatibilityTokens();\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/PhpParser/Lexer.php"),
        "<?php\nnamespace PhpParser;\nrequire __DIR__ . '/compatibility_tokens.php';\nclass Lexer {\n    public function token(): string { return 'token ' . \\constant('T_ELEPHC_COMPAT'); }\n}\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
}

/// A LITERAL name no compiled class carries (`class_exists('svc')`) is looked up in the run-time
/// `class_alias()` table too, when the program registers aliases: it was folded to false.
#[test]
fn closed_world_literal_class_exists_sees_a_runtime_alias() {
    let dir = make_test_dir("cw_literal_alias");
    fs::write(dir.join("main.php"), "<?php\nnamespace App { class Service { public const KIND = \"svc\"; public function hello(): string { return \"hi\"; } } }\nnamespace {\nclass Loader {\n    public function __construct(private array $aliases) {}\n    public function register(): void {\n        foreach ($this->aliases as $alias => $class) { var_dump(@class_alias($class, $alias)); }\n    }\n}\n$aliases = ['Svc' => App\\Service::class, 'Other' => 'App\\Missing'];\n(new Loader($aliases))->register();\n$name = $argc > 5 ? 'x' : 'Svc';\nvar_dump(class_exists($name), class_exists('svc'), class_exists('Other'));\n$o = new $name();\necho get_class($o), \" \", $o->hello(), \" \", $name::KIND, \"\\n\";\nvar_dump(@class_alias($name, 'Svc2'), @class_alias('App\\\\Service', $name));\n}\n").unwrap();
    compile_ok(&dir, &["--closed-world", "--heap-debug"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
    assert!(text(&output.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", text(&output.stderr));
}

/// nikic/php-parser's `Internal/TokenPolyfill.php` declares `class TokenPolyfill extends \PhpToken`
/// (on php 8) and `Token extends Internal\TokenPolyfill`: the parent `PhpToken`, which the compiler
/// supplies, must count as known when the file is autoloaded, or the polyfill was dropped and
/// `Token` deferred.
#[test]
fn closed_world_autoloaded_class_extending_php_token_is_compiled() {
    let dir = make_test_dir("cw_php_token");
    fs::create_dir_all(dir.join("src/Parser/Internal")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . str_replace(\"\\\\\", \"/\", $class) . \".php\";\n    if (is_file($file)) { require $file; }\n});\n$tokens = Parser\\Token::tokenize('<?php echo 1;');\necho get_class($tokens[1]), ' ', $tokens[1]->text, ' ', $tokens[1]->getEndPos(), \"\\n\";\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Parser/Internal/TokenPolyfill.php"),
        "<?php\nnamespace Parser\\Internal;\nif (\\PHP_VERSION_ID >= 80000) {\n    class TokenPolyfill extends \\PhpToken {}\n    return;\n}\nclass TokenPolyfill {\n    private const IGNORABLE = [\\T_WHITESPACE => true];\n    public function isIgnorable(): bool { return isset(self::IGNORABLE[0]); }\n    public function __toString(): string { return ''; }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Parser/Token.php"),
        "<?php\nnamespace Parser;\nclass Token extends Internal\\TokenPolyfill {\n    public function getEndPos(): int { return $this->pos + \\strlen($this->text); }\n}\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
}

/// `extract($this->code['use'], EXTR_OVERWRITE | EXTR_REFS)` (laravel/serializable-closure's
/// `Native::__unserialize()`) binds each local the function spells to the array's element, so a
/// write through the local reaches the array: it needed the interpreter's scope. No leak
/// assertion: two element references into one property array leak (bead elephc-872).
#[test]
fn closed_world_extract_by_reference_binds_the_elements() {
    let dir = make_test_dir("cw_extract_refs");
    fs::write(dir.join("main.php"), "<?php\nclass Native {\n    public $code = [];\n    public function unserialize(array $use): string {\n        $greeting = '';\n        $n = 0;\n        $this->code['use'] = $use;\n        extract($this->code['use'], EXTR_OVERWRITE | EXTR_REFS);\n        $before = $greeting . ' ' . $n;\n        $greeting .= '!';\n        $n = $n * 2;\n        return $before . ' ' . json_encode($this->code['use']);\n    }\n    public function plain(array $vars): string {\n        $x = null;\n        extract($vars, EXTR_SKIP);\n        return $x ?? 'none';\n    }\n}\n$o = new Native();\necho $o->unserialize(['greeting' => 'hi', 'n' => 21, 'other' => 1]), \"\\n\";\n\n").unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
}

/// Under an 8.5 profile php 8.6's `SortDirection` is a free name, which symfony/polyfill-php86's
/// stub declares: the autoloader loads it (it took the catalogued 8.6 enum as already declared),
/// and a reflected parameter default `SortDirection::Ascending` (Laravel's
/// `BelongsToMany::orderByPivot()`) is that enum's case.
#[test]
fn closed_world_newer_builtin_enum_name_is_autoloaded_from_its_polyfill() {
    let dir = make_test_dir("cw_sort_direction");
    fs::create_dir_all(dir.join("src/App")).unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . str_replace(\"\\\\\", \"/\", $class) . \".php\";\n    if (is_file($file)) { require $file; }\n});\n$m = new ReflectionMethod(App\\Relation::class, 'orderByPivot');\n$d = $m->getParameters()[1]->getDefaultValue();\nvar_dump($d === SortDirection::Ascending, $d->name);\necho (new App\\Relation())->orderByPivot('x'), \"\\n\";\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/SortDirection.php"),
        "<?php\nif (\\PHP_VERSION_ID < 80600 && \\PHP_VERSION_ID >= 80100) {\n    enum SortDirection\n    {\n        case Ascending;\n        case Descending;\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/App/Relation.php"),
        "<?php\nnamespace App;\nuse SortDirection;\nclass Relation {\n    public function orderByPivot($column, $direction = SortDirection::Ascending): string {\n        return $column . ' ' . ($direction === SortDirection::Ascending ? 'asc' : 'desc');\n    }\n}\n",
    )
    .unwrap();
    compile_ok(&dir, &["--closed-world", "--php-version", "8.5"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
}

/// Compiles `source` closed-world, runs it and returns (ours, php's) stdout.
fn closed_vs_php(prefix: &str, source: &str) -> (String, String) {
    let dir = make_test_dir(prefix);
    fs::write(dir.join("main.php"), source).unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    (text(&output.stdout), php_stdout(&dir))
}

/// `!$r instanceof C || $r->prop` in an `if`: the right operand runs once the left proved the
/// class, so the property read is on `C` (symfony/framework-bundle's `ConsoleProfilerListener`
/// reads `CliRequest::$command` off a `?Request`). The condition tree lowered the right operand
/// with the declared class and the backend refused "prop_get for dynamic or missing property".
#[test]
fn closed_world_instanceof_narrows_the_right_operand_of_a_condition() {
    let (ours, php) = closed_vs_php(
        "cw_cond_narrow",
        r#"<?php
class Req { public $x = 1; }
class CliReq extends Req { public function __construct(public string $command = 'a') {} }
class Stack { function cur(): ?Req { return new CliReq('a'); } }
class L {
    public function __construct(private Stack $s) {}
    function f(string $cmd) {
        $request = $this->s->cur();
        if (!$request instanceof CliReq || $request->command !== $cmd) { return 'no'; }
        return 'yes';
    }
    function g(string $cmd) {
        $request = $this->s->cur();
        if ($request instanceof CliReq && $request->command === $cmd) { return 'yes'; }
        return 'no';
    }
}
$l = new L(new Stack);
echo $l->f('a'), $l->f('b'), $l->g('a'), $l->g('b'), "\n";
"#,
    );
    assert_eq!(ours, php);
}

/// `~$value` over a `mixed` holding an integer (a `foreach` key), as `ErrorHandler::setLoggers()`
/// does. The unary lowered to a one-operand runtime call no backend implements.
#[test]
fn closed_world_bitwise_not_of_a_mixed_integer() {
    let (ours, php) = closed_vs_php(
        "cw_bitnot_mixed",
        r#"<?php
class H {
    private int $logged = 0;
    public function setLoggers(array $loggers): void {
        foreach ($loggers as $type => $log) {
            if (null === $log) { $this->logged &= ~$type; } else { $this->logged |= $type; }
        }
    }
    public function get(): int { return $this->logged; }
}
$h = new H;
$h->setLoggers([4 => 'x', 8 => 'y']);
$h->setLoggers([4 => null]);
echo $h->get(), "\n";
"#,
    );
    assert_eq!(ours, php);
}

/// `$a[$k] = $b['x'][$k] = $closure`: the string-offset arm of the nested write casts the stored
/// value to a string, which for a statically known closure has no lowering ("string cast for PHP
/// type Callable") though only a string container ever reaches it (symfony/framework-bundle's
/// `TestContainer::set()`).
#[test]
fn closed_world_chained_nested_write_of_a_closure() {
    let (ours, php) = closed_vs_php(
        "cw_nested_closure_write",
        r#"<?php
class C { public array $factories = []; }
class TC {
    private array $nonShared = ['a' => true];
    function pub(): C { static $c; return $c ??= new C; }
    function set(string $id, mixed $service): void {
        $container = $this->pub();
        if (isset($this->nonShared[$id])) {
            if (!$service instanceof \Closure) { throw new Exception('bad'); }
            $container->factories[$id] = $container->factories['service_container'][$id] = $service;
        }
    }
    function n() { return count($this->pub()->factories); }
}
$t = new TC;
$t->set('a', fn() => 1);
echo $t->n(), "\n";
"#,
    );
    assert_eq!(ours, php);
}

/// A closure bound to a class that does not declare the static property it reads through `self`
/// (symfony/routing's `Router` closures, rebound at every scope a bind site can name). PHP raises
/// `Error: Access to undeclared static property Other::$cache` only when the access runs; the
/// backend refused the whole body ("load_static_property for missing static property").
#[test]
fn closed_world_closure_bound_to_a_class_without_the_static_property_throws_like_php() {
    let source = r#"<?php
class R {
    private static ?array $cache = ['x' => 1, 'y' => 2];
    public $tag = 't';
    public function f() {
        return function ($p) {
            unset(self::$cache[$p]);
            return $this->tag . count(self::$cache);
        };
    }
}
class Other { public $tag = 'o'; }
$c = (new R)->f();
echo $c('x'), "\n";
$b = Closure::bind($c, new Other, Other::class);
try { echo $b('y'), "\n"; } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
echo $c('y'), "\n";
"#;
    let dir = make_test_dir("cw_closure_scope_static");
    fs::write(dir.join("main.php"), source).unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
}

/// `unset($this->prop[$key])` inside a closure. A closure's `$this` can be rebound by
/// `Closure::bind()`, so it is a runtime-shaped (boxed) receiver rather than an object of the
/// lexical class; the declared-array unset fetched the property for write through that boxed
/// receiver, which the backend refused ("prop_get_for_write for receiver PHP type Mixed") and
/// which kept symfony/cache's `ContractsTrait::doGet()` closure out of a closed-world build.
#[test]
fn closed_world_unset_property_element_in_a_closure_matches_php() {
    let source = r#"<?php
trait Computing {
    private array $computing = [];
    private function run(callable $cb) { $save = true; return $cb(new stdClass, $save); }
    public function doGet(string $key, callable $callback) {
        return $this->run(function ($item, bool &$save) use ($callback, $key) {
            if (isset($this->computing[$key])) { return 0; }
            $this->computing[$key] = $key;
            $this->computing['other'] = 'x';
            try {
                return $callback($item, $save) . count($this->computing);
            } finally {
                unset($this->computing[$key]);
            }
        });
    }
    public function left() { return implode(',', array_keys($this->computing)); }
}
class A { use Computing; }
class B { use Computing; }
$a = new A;
echo $a->doGet("k", fn($i, $s) => 5), " ", $a->left(), "\n";
$b = new B;
echo $b->doGet("z", fn($i, $s) => 6), " ", $b->left(), "\n";
"#;
    // No `--heap-debug`: the branch's compiler also reports a bad refcount for this program
    // (a separate, older defect in the by-ref closure parameter), so only the output is pinned.
    let dir = make_test_dir("cw_unset_closure");
    fs::write(dir.join("main.php"), source).unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
}

/// A date/time method called on a receiver whose class is only known at run time dispatches
/// through the source vtable, so its body must be lowered even though no call site names a
/// date/time class. Symfony's `AboutCommand::isExpired()` re-assigns a string parameter from
/// `DateTimeImmutable::createFromFormat()` and calls `->modify()` on it: the closed-world console
/// jumped to a null vtable slot (SIGSEGV). An eval bridge masks the defect, because it lowers the
/// whole alias surface, so this must be a `--closed-world` build.
#[test]
fn closed_world_date_method_on_a_runtime_typed_receiver_is_lowered() {
    let source = r#"<?php
function parse(string $date): mixed {
    return \DateTimeImmutable::createFromFormat('d/m/Y', '01/' . $date);
}
function shift(string $date): string {
    $date = \DateTimeImmutable::createFromFormat('d/m/Y', '01/' . $date);
    return false !== $date ? $date->modify('last day of this month 23:59:59')->format('Y-m-d H:i:s') : 'bad';
}
$d = parse($argc > 5 ? '13/2026' : '02/2024');
echo get_class($d), "\n";
echo $d->modify('last day of this month 23:59:59')->format('Y-m-d H:i:s'), "\n";
echo $d->setDate(2020, 5, 17)->format('Y-m-d'), "\n";
echo shift($argc > 5 ? '13/2026' : '12/2026'), "\n";
"#;
    let dir = make_test_dir("cw_date_dynamic_receiver");
    fs::write(dir.join("main.php"), source).unwrap();
    compile_ok(&dir, &["--closed-world"]);
    let output = run(&dir);
    assert!(output.status.success(), "run failed: {:?}", output.status);
    assert_eq!(text(&output.stdout), php_stdout(&dir));
}

/// A first-class callable of a static method on a class a closed world never has
/// (`\GuzzleHttp\UriTemplate\UriTemplate::expand(...)` behind `class_exists()`, symfony/http-client's
/// `UriTemplateHttpClient`) compiles: the guarded branch is never taken, and an unguarded one
/// raises php's `Error: Class "X" not found`. It was `Undefined class`, which handed the class
/// file to the runtime and failed the closed-world build.
#[test]
fn closed_world_static_callable_of_an_absent_class_raises_at_run_time() {
    let dir = make_test_dir("cw_absent_fcc");
    fs::write(dir.join("main.php"), r#"<?php
final class Expander {
    private function create(): \Closure
    {
        if (class_exists(\GuzzleHttp\UriTemplate\UriTemplate::class)) {
            return \GuzzleHttp\UriTemplate\UriTemplate::expand(...);
        }
        if (class_exists(\League\Uri\UriTemplate::class)) {
            return static fn (string $url, array $vars): string => (new \League\Uri\UriTemplate($url))->expand($vars);
        }
        if (class_exists(\Rize\UriTemplate::class)) {
            return (new \Rize\UriTemplate())->expand(...);
        }
        return static fn (string $url, array $vars): string => strtr($url, $vars);
    }
    public function run(): string { return ($this->create())('/{a}', ['{a}' => 'x']); }
}
echo (new Expander())->run(), "\n";
try { $f = \Missing\Tool::make(...); } catch (\Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
"#).unwrap();
    compile_ok(&dir, &["--closed-world"]);
    assert_eq!(text(&run(&dir).stdout), "/x\nError: Class \"Missing\\Tool\" not found\n");
}
