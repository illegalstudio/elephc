//! Purpose:
//! End-to-end value tests for NAMING, in compiled code, a class that only a runtime include or a
//! registered autoloader can provide. Symfony's container writes `new $class` and
//! `new ReflectionClass($id)` against classes the Composer autoloader supplies, and the checker
//! used to REFUSE those programs at type-check time where `php -n` exits 0.
//!
//! Called from:
//! - `cargo test --test codegen_tests include_deferred_classes` through Rust's test harness.
//!
//! Key details:
//! - The include path is DELIBERATELY not a string literal; see `include_reflection` for why a
//!   literal path makes such a fixture prove nothing.
//! - The refusal was in the CHECKER, not in lowering. `ir_lower::stmt::includes::lower_include`
//!   has always called `ctx.apply_eval_barrier()`, so `object_construction` already lowers a
//!   `new` of an unknown class to `Op::EvalObjectNew`, which resolves the name at run time through
//!   the bridge. What was missing is that `Checker::allows_absent_runtime_class` only answered
//!   `true` inside a function body or after a literal `eval()`; at top level, after a runtime
//!   include, it still reported `Undefined class`.
//! - The NEGATIVE control is the point of the third test. Deferring must not turn a genuinely
//!   missing class into a silent success: PHP raises a catchable `Error` reading
//!   `Class "X" not found`, and that is what the binary must print too.
//! - Every expected string is `php -n` 8.5.6's output on the same files.

use crate::support::*;

/// Verifies the caller can reflect on a class a runtime include declared.
///
/// This is the twin of `include_reflection::test_caller_reflects_on_a_class_a_runtime_include_declared`
/// with the reflected name reaching a variable, and it fails the same way: the reflection
/// constructor validator reported `ReflectionClass::__construct(): undefined class 'S1Plain'`
/// where `php -n` 8.5.6 prints `loaded;S1Plain;done`.
#[test]
fn test_reflection_constructor_defers_a_class_only_the_include_declares() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\n$r = new ReflectionClass(\"S1Plain\");\necho $r->getName();\necho \";done\";\n",
            ),
            (
                "piece.php",
                "<?php\nclass S1Plain { public int $id = 4; }\necho \"loaded;\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded;S1Plain;done");
}

/// Verifies compiled code can construct a class only a runtime include declares.
///
/// `php -n` 8.5.6 prints `loaded;7;done`. Before this change the checker refused with
/// `Undefined class: S1Plain` at the `new`.
#[test]
fn test_new_defers_a_class_only_the_include_declares() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\n$o = new S1Plain();\necho $o->v;\necho \";done\";\n",
            ),
            (
                "piece.php",
                "<?php\nclass S1Plain { public int $v = 7; }\necho \"loaded;\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded;7;done");
}

/// Verifies a class that exists NOWHERE still produces PHP's runtime Error, not a silent success.
///
/// This is the control that keeps the deferral honest. `php -n` 8.5.6 prints
/// `loaded;Error: Class "NoSuchClassAnywhere" not found;done` and exits 0, because the Error is
/// catchable. A deferral that answered "constructed" here would be worse than the refusal it
/// replaces.
#[test]
fn test_a_class_that_exists_nowhere_still_raises_phps_runtime_error() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\ntry {\n    $x = new NoSuchClassAnywhere();\n    echo \"constructed\";\n} catch (\\Throwable $e) {\n    echo get_class($e), \": \", $e->getMessage();\n}\necho \";done\";\n",
            ),
            (
                "piece.php",
                "<?php\nclass S1Plain { public int $v = 7; }\necho \"loaded;\";\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "loaded;Error: Class \"NoSuchClassAnywhere\" not found;done"
    );
}

// THE AUTOLOADER ROUTE HAS NO FIXTURE HERE, and the reason is worth writing down rather than
// leaving as an absence.
//
// The checker half works: `spl_autoload_register(...)` followed by `new MadeByLoader()` used to
// be refused with `Undefined class: MadeByLoader` at type-check, and now compiles. Measured on
// the real pipeline with the release compiler.
//
// The route still does not produce PHP's value, for a reason that belongs to the autoload
// registry rather than to this deferral. `autoload::Registry::build` CONSUMES a collectable
// `spl_autoload_register` call and strips it from the program, turning it into a compile-time
// rule. A loader whose body is `eval("class …")` cannot be reduced to such a rule, and it is no
// longer registered at run time either, so the interpreter's callback chain is empty —
// `ELEPHC_EVAL_TRACE=1` reports `phase=spl_autoload_local class="MadeByLoader" callbacks=0` — and
// the binary prints `Fatal error: Uncaught Error: Class "MadeByLoader" not found` where
// `php -n` 8.5.6 prints `loaded;done`.
//
// It is not pinned here because the helper that runs a program expected to FAIL builds no
// autoload registry, so it cannot model the pipeline for this shape at all; a fixture written
// against it would prove something about the harness instead of about the compiler. Closing the
// divergence means either keeping consumed registrations live at run time for whatever the
// compiled rules cannot resolve, or only consuming registrations provably reducible to
// classmap/PSR-4 — and the fixture belongs with that change, next to a harness that models it.

/// Verifies `include_once` of an AUTOLOADED file answers "already included" instead of re-running it.
///
/// The autoload pass performs, at compile time, the inclusion PHP's autoloader performs at run
/// time: it opens the class file and splices its declarations into the program. The runtime
/// inclusion guard for that file stayed clear all the same, so an `include_once` reaching it
/// through a COMPUTED path re-ran the file and died redeclaring a class the binary already
/// carries. Symfony's compiled container does exactly this
/// (`include_once dirname(__DIR__, 4).'/vendor/.../ParameterBagInterface.php'`), and the whole
/// request died on it.
///
/// `php -n` 8.5.6 on the Composer-shaped equivalent (`scratchpad/preinc/ref.php`, this session,
/// a `spl_autoload_register` that `require`s the same file) prints
/// `first=thing` / `again=true` / `second=thing`.
#[test]
fn test_include_once_of_an_autoloaded_file_reports_it_as_already_included() {
    let out = compile_and_run_files(
        &[
            (
                "project.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
            ),
            (
                "src/Thing.php",
                "<?php\nnamespace App;\ninterface ThingContract { public function tag(): string; }\n\
                 class Thing implements ThingContract { public function tag(): string { return 'thing'; } }\n",
            ),
            (
                "main.php",
                "<?php\n$thing = new \\App\\Thing();\necho 'first=', $thing->tag(), \"\\n\";\n\
                 $path = __DIR__ . '/src/' . 'Thing' . '.php';\n$again = include_once $path;\n\
                 echo 'again=', var_export($again, true), \"\\n\";\n\
                 echo 'second=', (new \\App\\Thing())->tag(), \"\\n\";\n",
            ),
        ],
        "main.php",
    );

    assert_eq!(out, "first=thing\nagain=true\nsecond=thing\n");
}

/// Verifies `class_exists($name, false)` reports a probe-only autoloaded class as NOT LOADED.
///
/// A closed-world build declares everything it compiled from the first instruction, so a class
/// the autoload pass pulled in solely to answer `class_exists(X::class)` answered "loaded" where
/// php answers "not loaded". `symfony/runtime` reads exactly that difference to decide whether
/// `symfony/dotenv` is installed:
///
/// ```php
/// class_exists(MissingDotenv::class, false) || class_exists(Dotenv::class) || class_exists(MissingDotenv::class);
/// // ...
/// if (... && !class_exists(MissingDotenv::class, false)) { $dotenv->bootEnv(...); }
/// ```
///
/// php never loads `MissingDotenv` when dotenv IS installed, so `.env` is read. elephc answered
/// `true`, skipped `bootEnv()`, and every `%env(...)%` parameter then failed to resolve --
/// `EnvNotFoundException: Environment variable not found: "DEFAULT_URI"` on the first request.
///
/// A class the program actually USES stays loaded: php would have loaded it too.
///
/// `php -n` 8.5.6 on the Composer-shaped equivalent (`scratchpad/probeonly/ref.php`, this
/// session) prints `before=false` / `probe=true` / `after=true` / `used=optional/true`.
#[test]
fn test_class_exists_without_autoload_reports_a_probe_only_class_as_unloaded() {
    let out = compile_and_run_files(
        &[
            ("project.json", r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#),
            ("src/Fallback.php", "<?php\nnamespace App;\nclass Fallback {}\n"),
            (
                "src/Optional.php",
                "<?php\nnamespace App;\nclass Optional { public function tag(): string { return 'optional'; } }\n",
            ),
            (
                "main.php",
                "<?php\n\
                 echo 'before=', (class_exists(\\App\\Fallback::class, false) ? 'true' : 'false'), \"\\n\";\n\
                 echo 'probe=', (class_exists(\\App\\Fallback::class) ? 'true' : 'false'), \"\\n\";\n\
                 echo 'after=', (class_exists(\\App\\Fallback::class, false) ? 'true' : 'false'), \"\\n\";\n\
                 $optional = new \\App\\Optional();\n\
                 echo 'used=', $optional->tag(), '/', (class_exists(\\App\\Optional::class, false) ? 'true' : 'false'), \"\\n\";\n",
            ),
        ],
        "main.php",
    );

    assert_eq!(
        out,
        "before=false\nprobe=true\nafter=true\nused=optional/true\n"
    );
}

/// The Composer-shaped autoloader these tests share: an object whose `loadClass` includes the
/// class file through a path built at run time.
const DEFERRAL_LOADER: &str = r#"<?php
class Loader {
    public function __construct(private string $dir) {}
    public function loadClass($class) {
        $file = $this->dir . "/src/" . $class . ".php";
        if (is_file($file)) { include $file; return true; }
        return null;
    }
}
$l = new Loader(__DIR__);
spl_autoload_register([$l, "loadClass"], true, true);
echo (new Good())->hi(), "|", (new Lazy())->hi(), "|", (new Refs())->hi(), "\n";
"#;

/// An autoloaded class file the compiler cannot type-check or parse is handed to the runtime
/// autoloader instead of failing the build, and its classes still work.
///
/// php parses a class file only when something asks for the class. `Lazy` has a method php
/// accepts and only fails when CALLED (`: int` returning a string); elephc's checker refuses it
/// at compile time. `Refs` uses a construct elephc's parser refuses (a reference element in an
/// array literal). Both used to stop the whole build — for the Symfony demo, one such file among
/// hundreds (`Translator.php`) did. Now each is left out of the compiled world with a warning,
/// the runtime autoloader includes it, and the interpreter runs it. Expected output is php 8.5's.
#[test]
fn test_autoloaded_class_the_compiler_refuses_is_loaded_at_run_time() {
    let out = compile_cli_files_and_run(
        &[
            ("main.php", DEFERRAL_LOADER),
            ("src/Good.php", "<?php\nclass Good { public function hi() { return \"good\"; } }\n"),
            (
                "src/Lazy.php",
                "<?php\nclass Lazy {\n    public function bad(): int { return \"not an int\"; }\n    public function hi() { return \"lazy\"; }\n}\n",
            ),
            (
                "src/Refs.php",
                "<?php\nclass Refs { public function hi() { $x = \"refs\"; $a = [&$x]; $a[0] .= \"!\"; return $x; } }\n",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "good|lazy|refs!\n");
}

/// A class file whose top-level guard throws because an optional package is missing is loaded
/// only when something asks for its class, and the exception it throws then is catchable.
///
/// This is Symfony's `AmpHttpClient.php` (`if (!interface_exists(DelegateHttpClient::class))
/// { throw new \LogicException(...); }`), named by `HttpClient::create()` behind a probe that
/// is false without amphp. Compiled into the program, the guard ran at boot and every request of
/// the Symfony demo died on "the amphp/http-client package is not installed". The file is now
/// left to the runtime autoloader, and the exception it throws when a `new` does demand it
/// reaches the caller's `catch`, as in php. Expected output is php 8.5's.
#[test]
fn test_class_file_whose_guard_throws_loads_only_on_demand() {
    let out = compile_cli_files_and_run(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"Lib\\":"src/"}}}"#),
            (
                "src/Amp.php",
                "<?php\nnamespace Lib;\n\nif (!interface_exists(MissingDep::class)) {\n    throw new \\LogicException('Amp needs MissingDep, which is not installed');\n}\n\nfinal class Amp implements Client {}\n",
            ),
            ("src/Native.php", "<?php\nnamespace Lib;\n\nfinal class Native implements Client {}\n"),
            ("src/Client.php", "<?php\nnamespace Lib;\n\ninterface Client {}\n"),
            (
                "main.php",
                r#"<?php
class Loader {
    public function __construct(private string $dir) {}
    public function loadClass($class) {
        $file = $this->dir . '/src/' . substr($class, 4) . '.php';
        if (str_starts_with($class, 'Lib\\') && is_file($file)) {
            require $file;
        }
    }
}
spl_autoload_register([new Loader(__DIR__), 'loadClass']);

function make_client(bool $fancy): Lib\Client {
    if ($fancy) {
        return new Lib\Amp();
    }
    return new Lib\Native();
}

echo get_class(make_client($argc > 5)), "\n";
try {
    make_client($argc < 5);
} catch (\LogicException $e) {
    echo "caught: ", $e->getMessage(), "\n";
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Lib\\Native\ncaught: Amp needs MissingDep, which is not installed\n");
}

/// A class file that declares its class inside a run-time `if` is left to the runtime
/// autoloader, and the class it declares is thrown, caught and type-tested from compiled code.
///
/// This is `symfony/cache`'s `Exception/InvalidArgumentException.php`, which declares one of two
/// class bodies depending on `interface_exists(SimpleCacheInterface::class)`. The compiled world
/// has no run-time class declaration, so both branches lost their class while the file stayed
/// claimed as included; the autoloader then skipped the include and `new InvalidArgumentException`
/// from a compiled cache pool died with `Class "...\InvalidArgumentException" not found` in the
/// middle of the Symfony demo's homepage. Expected output is php 8.5.10's.
#[test]
fn test_class_file_declaring_its_class_conditionally_loads_at_run_time() {
    let out = compile_cli_files_and_run(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"Lib\\":"src/"}}}"#),
            ("src/Marker.php", "<?php\nnamespace Lib;\n\ninterface Marker {}\n"),
            (
                "src/BadKey.php",
                "<?php\nnamespace Lib;\n\nif (interface_exists(Marker::class)) {\n    class BadKey extends \\InvalidArgumentException implements Marker {}\n} else {\n    class BadKey extends \\InvalidArgumentException {}\n}\n",
            ),
            (
                "src/Pool.php",
                "<?php\nnamespace Lib;\n\nclass Pool {\n    public function get(string $key): string {\n        if (str_contains($key, ':')) {\n            throw new BadKey(sprintf('Key \"%s\" is reserved', $key));\n        }\n        return 'value:' . $key;\n    }\n}\n",
            ),
            (
                "main.php",
                r#"<?php
class Loader {
    public function __construct(private string $dir) {}
    public function loadClass($class) {
        $file = $this->dir . '/src/' . substr($class, 4) . '.php';
        if (str_starts_with($class, 'Lib\\') && is_file($file)) {
            require $file;
        }
    }
}
spl_autoload_register([new Loader(__DIR__), 'loadClass']);

$pool = new Lib\Pool();
echo $pool->get('ok'), "\n";
try {
    echo $pool->get('a:b'), "\n";
} catch (Lib\BadKey $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
    echo $e instanceof Lib\Marker ? "marker" : "no marker", "\n";
    echo $e instanceof \InvalidArgumentException ? "invalid argument" : "other", "\n";
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        "value:ok\nLib\\BadKey: Key \"a:b\" is reserved\nmarker\ninvalid argument\n"
    );
}

/// An enum the runtime autoloader declares is usable, case for case and by identity, from a
/// class another autoloaded file declares.
///
/// Each file a compiled autoloader includes runs in an eval context of its own. The enum's
/// declaration reached the other contexts, its case singletons did not, so Symfony's
/// `ReflectionExtractor::MAP_TYPES = ['integer' => TypeIdentifier::INT->value, ...]` died on
/// "unsupported ClassConstantFetch expression". Expected output is php 8.5.10's.
#[test]
fn test_enum_cases_are_shared_between_autoloaded_files() {
    let out = compile_cli_files_and_run(
        &[
            (
                "src/TypeId.php",
                "<?php\nnamespace Lib;\n\nenum TypeId: string {\n    case INT = 'int';\n    case FLOAT = 'float';\n}\n",
            ),
            (
                "src/Extractor.php",
                "<?php\nnamespace Lib;\n\nclass Extractor {\n    private const MAP_TYPES = [\n        'integer' => TypeId::INT->value,\n        'double' => TypeId::FLOAT->value,\n    ];\n    public static function map(): array { return self::MAP_TYPES; }\n    public static function int(): TypeId { return TypeId::INT; }\n}\n",
            ),
            (
                "main.php",
                r#"<?php
class Loader {
    public function __construct(private string $dir) {}
    public function loadClass($class) {
        $file = $this->dir . '/src/' . substr($class, 4) . '.php';
        if (str_starts_with($class, 'Lib\\') && is_file($file)) {
            require $file;
        }
    }
}
spl_autoload_register([new Loader(__DIR__), 'loadClass']);
$code = 'echo implode(",", \Lib\Extractor::map()), "\n";'
    . 'var_dump(\Lib\Extractor::int() === \Lib\TypeId::INT, \Lib\TypeId::from("float") === \Lib\TypeId::FLOAT);'
    . ($argc > 5 ? '' : '');
eval($code);
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "int,float\nbool(true)\nbool(true)\n");
}

/// A class file that picks its class-like by an autoloading existence probe is compiled, not
/// handed to the interpreter.
///
/// `if (!class_exists(X::class)) { trait T {...} } else { trait T {...} }` (Doctrine's
/// `GetReflectionClassImplementation`) and `if (interface_exists(Y::class)) { class E ... } else
/// { class E ... }` (Symfony Cache's exceptions) run when the autoloader includes the file, and the
/// probe's answer is whether X can be autoloaded -- which the compile-time registry decides. Left
/// undecided, both files went to the runtime autoloader, and so did every class using the trait
/// (`ClassMetadata`) or extending the exception. Expected output is php 8.5.10's.
#[test]
fn test_class_file_choosing_its_class_like_by_an_existence_probe_is_compiled() {
    let (compile_stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"Lib\\":"src/"}}}"#),
            ("src/Marker.php", "<?php\nnamespace Lib;\ninterface Marker {}\n"),
            (
                "src/Impl.php",
                "<?php\nnamespace Lib;\nif (!class_exists(Missing\\StaticService::class)) {\n    trait Impl { public function kind(): string { return \"runtime:\" . $this->name; } }\n} else {\n    trait Impl { public function kind(): ?string { return null; } }\n}\n",
            ),
            (
                "src/BadKey.php",
                "<?php\nnamespace Lib;\nif (interface_exists(Marker::class)) {\n    class BadKey extends \\InvalidArgumentException implements Marker {}\n} else {\n    class BadKey extends \\InvalidArgumentException {}\n}\n",
            ),
            (
                "src/Meta.php",
                "<?php\nnamespace Lib;\nclass Meta { use Impl; public function __construct(public string $name) {} }\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function ($class) {
    $file = __DIR__ . '/src/' . substr($class, 4) . '.php';
    if (str_starts_with($class, 'Lib\\') && is_file($file)) {
        require $file;
    }
});
$m = new Lib\Meta('post');
echo $m->kind(), "\n";
try {
    throw new Lib\BadKey('bad');
} catch (Lib\BadKey $e) {
    echo get_class($e), $e instanceof Lib\Marker ? ' marker' : ' plain', "\n";
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "runtime:post\nLib\\BadKey marker\n");
    assert!(
        !compile_stderr.contains("is not compiled"),
        "a class file was handed to the runtime autoloader: {compile_stderr}"
    );
}

/// A static property whose default names a constant of a class left to the runtime autoloader
/// sends its own class file to the runtime too, instead of failing the build.
///
/// Symfony's `BaseDateTimeTransformer::$formats` lists `\IntlDateFormatter::NONE` and friends;
/// without ext-intl that class comes from the ICU polyfill, which elephc defers. The default
/// then had no compile-time value (`array` of `mixed`), and since static defaults are written
/// by the entry program's prologue the refusal was recorded against `main`, which no autoloader
/// can supply: "static property initializer … with PHP type Mixed" stopped the whole build.
/// php evaluates the default when the class is first used, and the interpreter does the same.
/// Expected output is php 8.5's.
#[test]
fn test_static_default_naming_a_deferred_class_constant_defers_its_class() {
    let out = compile_cli_files_and_run(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"Lib\\":"src/"}}}"#),
            (
                "src/Fmt.php",
                "<?php\nnamespace Lib;\nclass Fmt {\n    public const NONE = -1;\n    public const FULL = 0;\n    public const LONG = 1;\n    public function bad(): int { return \"not an int\"; }\n}\n",
            ),
            (
                "src/Transformer.php",
                "<?php\nnamespace Lib;\nabstract class Transformer {\n    protected static array $formats = [\n        \\Lib\\Fmt::NONE,\n        \\Lib\\Fmt::FULL,\n        \\Lib\\Fmt::LONG,\n    ];\n    public function has(int $f): bool { return in_array($f, self::$formats, true); }\n    public static function formats(): array { return static::$formats; }\n}\n",
            ),
            ("src/Concrete.php", "<?php\nnamespace Lib;\nclass Concrete extends Transformer {}\n"),
            (
                "main.php",
                r#"<?php
class Loader {
    public function __construct(private string $dir) {}
    public function loadClass($class) {
        $file = $this->dir . '/src/' . substr($class, 4) . '.php';
        if (str_starts_with($class, 'Lib\\') && is_file($file)) {
            require $file;
        }
    }
}
spl_autoload_register([new Loader(__DIR__), 'loadClass']);
$t = new Lib\Concrete();
var_dump($t->has(1), $t->has(5));
echo implode(",", Lib\Concrete::formats()), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "bool(true)\nbool(false)\n-1,0,1\n");
}
