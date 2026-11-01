//! Purpose:
//! End-to-end tests for compiling a refused construct as php's RUN-TIME fault instead of
//! handing its whole class file to the runtime autoloader (`autoload::runtime_faults`).
//!
//! Called from:
//! - `cargo test --test codegen_tests autoload_runtime_faults` through Rust's test harness.
//!
//! Key details:
//! - php binds a member or a class name when the expression executes: a method nothing declares,
//!   called on a branch the program never takes, is a loadable program. The checker refuses the
//!   body, and the file used to go to the interpreter for it (doctrine/orm's `WhenClause.php`
//!   calls `$walker->walkWhenClauseExpression()`, which no `SqlWalker` declares).
//! - Every fixture registers an autoloader, so its class files are deferrable, and asserts on the
//!   compiler's stderr: the `note:` that names the demotion must be there and the deferral
//!   warning must NOT.
//! - Every expected string is `php -n` 8.5's output on the same files, except where the test
//!   says why the compiled output differs (the location a helper frame reports).

use crate::support::*;

/// A Composer-like loader: it checks the file exists, so a class nothing declares is php's
/// `Class "X" not found` rather than a failed `require`.
const AUTOLOADER: &str = "<?php\nspl_autoload_register(function (string $class): void {\n    $file = __DIR__ . \"/src/\" . $class . \".php\";\n    if (is_file($file)) { require $file; }\n});\n";

const WALKER: &str = "<?php\nclass Walker {\n    public int $v = 3;\n    public function walkOk(Clause $c): string { return \"walked\"; }\n}\n";

/// Undefined instance methods (plain and nullsafe), a static method, a static property and a
/// class constant, each on a class the compiled world holds in full, raise php's `Error` only
/// when their line runs, and the file stays compiled.
#[test]
fn test_members_nothing_declares_raise_phps_error_when_their_line_runs() {
    let main = format!(
        "{AUTOLOADER}$walker = new Walker();\n$clause = new Clause();\necho $clause->ok($walker), \"\\n\";\necho $clause->constantGuarded(), \"\\n\";\nforeach ([\"dispatch\", \"dispatchNullsafe\", \"staticMissing\", \"staticProp\", \"constantMissing\"] as $m) {{\n    try {{\n        if ($argc > 5) {{ echo \"never\\n\"; }}\n        var_dump($clause->$m($argc > 5 ? null : $walker));\n    }} catch (\\Error $e) {{\n        echo get_class($e), \": \", $e->getMessage(), \"\\n\";\n    }}\n}}\nvar_dump($clause->dispatchNullsafe(null));\necho \"done\\n\";\n"
    );
    let clause = "<?php\nclass Clause {\n    public function ok(Walker $w): string { return $w->walkOk($this); }\n    public function dispatch(?Walker $w): string { return $w->walkWhenClauseExpression($this); }\n    public function dispatchNullsafe(?Walker $w): ?string { return $w?->walkWhenClauseExpression($this); }\n    public function staticMissing(?Walker $w): string { $x = Walker::nothing(1, 2); return \"x\" . $x; }\n    public function staticProp(?Walker $w): int { return Walker::$count + 1; }\n    public function constantMissing(?Walker $w): string { return strtoupper(Walker::MISSING); }\n    public function constantGuarded(): string { return defined(\"Walker::MISSING\") ? Walker::MISSING : \"guarded\"; }\n}\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[("main.php", &main), ("src/Walker.php", WALKER), ("src/Clause.php", clause)],
        "main.php",
    );
    assert_eq!(
        out,
        "walked\nguarded\nError: Call to undefined method Walker::walkWhenClauseExpression()\nError: Call to undefined method Walker::walkWhenClauseExpression()\nError: Call to undefined method Walker::nothing()\nError: Access to undeclared static property Walker::$count\nError: Undefined constant Walker::MISSING\nNULL\ndone\n"
    );
    assert!(
        !stderr.contains("is not compiled"),
        "Clause.php was handed to the autoloader:\n{stderr}"
    );
    for expected in [
        "the call to `->walkWhenClauseExpression()` is resolved by name when it runs",
        "the call to `?->walkWhenClauseExpression()` is resolved by name when it runs",
        "php's Error `Call to undefined method Walker::nothing()` is thrown when it runs",
        "php's Error `Access to undeclared static property Walker::$count` is thrown when it runs",
        "php's Error `Undefined constant Walker::MISSING` is thrown when it runs",
    ] {
        assert!(
            stderr.contains(expected),
            "missing note `{expected}` in:\n{stderr}"
        );
    }
    assert_eq!(
        stderr.matches("note: autoload: ").count(),
        6,
        "one note per demoted construct (the guarded constant read counts too):\n{stderr}"
    );
}

/// A refused construct inside an `if` condition stops the checker before it installs what the
/// condition's `||` assigns, so the read after the `if` is "undefined" too. That second error is
/// a cascade of the first: once the call is demoted the file checks clean, and it must not be
/// deferred for an error the rewrite already removed (symfony/framework-bundle's
/// `DebugAutowiringCommand::getFileLink()` is the shape).
#[test]
fn test_an_error_cascading_from_a_demoted_condition_does_not_defer_the_file() {
    let main = format!(
        "{AUTOLOADER}$linker = new Linker();\necho \"[\", $linker->link(null), \"]\\n\";\ntry {{ echo $linker->link(new Walker()), \"\\n\"; }} catch (\\Error $e) {{ echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }}\necho \"done\\n\";\n"
    );
    let walker = "<?php\nclass Walker {\n    public int $v = 3;\n}\n";
    let linker = "<?php\nclass Linker {\n    public function link(?Walker $w): string {\n        if (null === $w || (null === $r = $w->findReflection(\"x\"))) {\n            return \"\";\n        }\n        return $r . \"!\";\n    }\n}\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[("main.php", &main), ("src/Walker.php", walker), ("src/Linker.php", linker)],
        "main.php",
    );
    assert_eq!(
        out,
        "[]\nError: Call to undefined method Walker::findReflection()\ndone\n"
    );
    assert!(
        !stderr.contains("is not compiled"),
        "Linker.php was handed to the autoloader:\n{stderr}"
    );
    assert!(
        stderr.contains("the call to `->findReflection()` is resolved by name when it runs"),
        "{stderr}"
    );
}

/// A property the receiver's class does not declare reads null with php's Warning.
///
/// php prints `Warning: Undefined property: Walker::$missing in .../Clause.php on line 3` on
/// stdout; the compiled program writes its warnings to stderr (every compiled warning does), and
/// names the helper's frame rather than `Clause.php:3`, so the Warning is checked on stderr by
/// message only. The values are php's.
#[test]
fn test_a_property_nothing_declares_warns_and_reads_null() {
    let main = format!(
        "{AUTOLOADER}$clause = new Clause();\nvar_dump($clause->readMissing(new Walker()));\nvar_dump($clause->readMissingNullsafe(null));\necho \"done\\n\";\n"
    );
    let clause = "<?php\nclass Clause {\n    public function readMissing(Walker $w): mixed { return $w->missing; }\n    public function readMissingNullsafe(?Walker $w): mixed { return $w?->missing; }\n}\n";
    let (stderr, out, run_stderr) = compile_cli_files_and_run_with_compile_and_run_stderr(
        &[("main.php", &main), ("src/Walker.php", WALKER), ("src/Clause.php", clause)],
        "main.php",
    );
    assert_eq!(out, "NULL\nNULL\ndone\n");
    assert!(
        run_stderr.contains("Warning: Undefined property: Walker::$missing in "),
        "php's Warning is missing:\n{run_stderr}"
    );
    assert!(!stderr.contains("is not compiled"), "{stderr}");
    assert!(
        stderr.contains("the read of `->missing` is resolved by name when it runs")
            && stderr.contains("the read of `?->missing` is resolved by name when it runs"),
        "{stderr}"
    );
}

/// The read sits in a TRAIT the deferred class uses: Laravel's `ResolvesDumpSource` reads
/// `$this->compiledViewPath`, which its `Exceptions\Renderer\Frame` user does not declare and its
/// dumper users do. The trait's node is rewritten, so `Frame` keeps compiling, warns and reads
/// null as php does, and a user that declares the property still reads its value.
#[test]
fn test_a_property_read_in_a_used_trait_is_demoted() {
    let main = format!(
        "{AUTOLOADER}var_dump((new Dumper())->isView('/views/a.php'));\nvar_dump((new Frame())->isView('/views/a.php'));\necho \"done\\n\";\n"
    );
    let resolves = "<?php\ntrait ResolvesDumpSource {\n    public function isView(string $file): bool { return str_starts_with($file, (string) $this->compiledViewPath); }\n}\n";
    let frame = "<?php\nclass Frame {\n    use ResolvesDumpSource;\n}\n";
    let dumper = "<?php\nclass Dumper {\n    use ResolvesDumpSource;\n    protected string $compiledViewPath = '/views';\n}\n";
    let (stderr, out, run_stderr) = compile_cli_files_and_run_with_compile_and_run_stderr(
        &[
            ("main.php", &main),
            ("src/ResolvesDumpSource.php", resolves),
            ("src/Frame.php", frame),
            ("src/Dumper.php", dumper),
        ],
        "main.php",
    );
    assert_eq!(out, "bool(true)\nbool(true)\ndone\n");
    assert!(
        run_stderr.contains("Warning: Undefined property: Frame::$compiledViewPath in "),
        "php's Warning is missing:\n{run_stderr}"
    );
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// The member is resolved against the RUN-TIME object: a subclass the compiler never saw (here
/// declared by `eval()`, so only the interpreter has it) that declares the method or property
/// is dispatched to, exactly as php does, while the base class still gets php's Error. This is
/// what rules out compiling the call as an unconditional `throw`.
#[test]
fn test_a_runtime_subclass_declaring_the_member_is_dispatched_to() {
    let main = format!(
        "{AUTOLOADER}eval(\"class LateWalker extends Walker {{ public \\$extra = \\\"late\\\"; public function walkWhenClauseExpression(Clause \\$c): string {{ return \\\"late-walked\\\"; }} }}\");\n$clause = new Clause();\necho $clause->dispatch(new LateWalker()), \"\\n\";\nvar_dump($clause->readMissing(new LateWalker()));\ntry {{ echo $clause->dispatch(new Walker()), \"\\n\"; }} catch (\\Error $e) {{ echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }}\necho \"done\\n\";\n"
    );
    let walker = "<?php\nclass Walker {\n    public int $v = 3;\n}\n";
    let clause = "<?php\nclass Clause {\n    public function dispatch(Walker $w): string { return $w->walkWhenClauseExpression($this); }\n    public function readMissing(Walker $w): mixed { return $w->extra; }\n}\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[("main.php", &main), ("src/Walker.php", walker), ("src/Clause.php", clause)],
        "main.php",
    );
    assert_eq!(
        out,
        "late-walked\nstring(4) \"late\"\nError: Call to undefined method Walker::walkWhenClauseExpression()\ndone\n"
    );
    assert!(!stderr.contains("is not compiled"), "{stderr}");
    assert_eq!(stderr.matches("note: autoload: ").count(), 2, "{stderr}");
}

/// A property read through a union of classes where one member lacks it (Doctrine's
/// `SchemaTool::gatherColumnOptions()`: `isset($mapping->enumType)` over
/// `JoinColumnMapping|FieldMapping|...`) is resolved by name when it runs: the declaring member
/// answers, the other raises php's Warning and reads null. The file stays compiled.
/// Value-checked against `php -n` (stdout; the Warning goes to stderr in a compiled program).
#[test]
fn test_a_property_some_union_members_lack_is_read_by_name() {
    let main = format!(
        "{AUTOLOADER}$s = new Schema();\necho json_encode($s->options(new FieldMapping())), json_encode($s->options(new JoinColumnMapping())), \"\\n\";\nvar_dump($s->raw(new JoinColumnMapping()));\n"
    );
    let field = "<?php\nfinal class FieldMapping { public ?string $enumType = \"Suit\"; public array $options = [\"a\" => 1]; }\n";
    let join = "<?php\nfinal class JoinColumnMapping { public array $options = [\"b\" => 2]; }\n";
    let schema = "<?php\nfinal class Schema {\n    public function options(JoinColumnMapping|FieldMapping $mapping): array {\n        $o = $mapping->options ?? [];\n        if (isset($mapping->enumType)) { $o[\"enumType\"] = $mapping->enumType; }\n        return $o;\n    }\n    public function raw(JoinColumnMapping|FieldMapping $mapping): mixed { return $mapping->enumType; }\n}\n";
    let (stderr, out, run_stderr) = compile_cli_files_and_run_with_compile_and_run_stderr(
        &[
            ("main.php", &main),
            ("src/FieldMapping.php", field),
            ("src/JoinColumnMapping.php", join),
            ("src/Schema.php", schema),
        ],
        "main.php",
    );
    assert_eq!(out, "{\"a\":1,\"enumType\":\"Suit\"}{\"b\":2}\nNULL\n");
    assert!(run_stderr.contains("Undefined property: JoinColumnMapping::$enumType"), "{run_stderr}");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// A constant of a PDO driver the build leaves out (`PDO::SQLSRV_ENCODING_BINARY` in Doctrine
/// DBAL's `PDO\SQLSrv\Statement`) is php's "Undefined constant" Error when the line runs, as on a
/// php without `pdo_sqlsrv`; the file stays compiled. Value-checked against `php`.
#[test]
fn test_a_disabled_pdo_driver_constant_raises_phps_error_when_read() {
    let main = format!(
        "{AUTOLOADER}$s = new SqlsrvStatement();\necho $s->name(), \"\\n\";\ntry {{ echo $s->encoding(), \"\\n\"; }} catch (\\Error $e) {{ echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }}\n"
    );
    let statement = "<?php\nclass SqlsrvStatement {\n    public function name(): string { return 'sqlsrv'; }\n    public function encoding(): int { return \\PDO::SQLSRV_ENCODING_BINARY; }\n}\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[("main.php", &main), ("src/SqlsrvStatement.php", statement)],
        "main.php",
    );
    assert_eq!(out, "sqlsrv\nError: Undefined constant PDO::SQLSRV_ENCODING_BINARY\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// A `static::` constant is bound to the called class when the line runs, so no verdict against
/// the lexical class is php's: the read becomes the run-time lookup
/// `constant(static::class . '::TOKEN')`, which answers the subclass's constant and raises php's
/// own Error for a class without one. The file stays compiled (Symfony's `FirewallAwareTrait`
/// reads `static::FIREWALL_OPTION` behind `defined(static::class.'::FIREWALL_OPTION')`).
/// Value-checked against `php -n`.
#[test]
fn test_a_late_bound_static_constant_is_looked_up_on_the_called_class() {
    let main = format!(
        "{AUTOLOADER}echo (new Child())->read(), \"\\n\";\ntry {{ echo (new Base())->read(), \"\\n\"; }} catch (\\Error $e) {{ echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }}\necho (new Base())->guarded(), (new Child())->guarded(), \"\\n\";\n"
    );
    let base = "<?php\nclass Base {\n    public function read(): string { return static::TOKEN; }\n    public function guarded(): string { return defined(static::class . '::TOKEN') ? static::TOKEN : 'none'; }\n}\n";
    let child = "<?php\nclass Child extends Base {\n    public const TOKEN = \"child\";\n}\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[("main.php", &main), ("src/Base.php", base), ("src/Child.php", child)],
        "main.php",
    );
    assert_eq!(out, "child\nError: Undefined constant Base::TOKEN\nnonechild\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
    assert!(
        stderr.contains("`static::TOKEN` is looked up on the called class when it runs"),
        "{stderr}"
    );
}

/// `isset(self::$emptyDefaults)` in a trait used by a class that declares no such static
/// (Symfony's `HttpClientTrait` in `CachingHttpClient`) is php's `false`, and the guarded read
/// is php's Error only when it runs. Both files stay compiled. Value-checked against `php -n`.
#[test]
fn test_isset_of_a_static_the_using_class_lacks_is_false() {
    let main = format!(
        "{AUTOLOADER}echo json_encode((new WithDefaults())->opts()), json_encode((new Without())->opts()), \"\\n\";\n"
    );
    let defaults = "<?php\ntrait Defaults {\n    public function opts(): array {\n        $options = ['a' => 1];\n        if (isset(self::$emptyDefaults)) {\n            foreach (self::$emptyDefaults as $k => $v) { $options[$k] ??= $v; }\n        }\n        return $options;\n    }\n}\n";
    let with = "<?php\nclass WithDefaults { use Defaults; private static array $emptyDefaults = ['b' => 2]; }\n";
    let without = "<?php\nclass Without { use Defaults; }\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("main.php", &main),
            ("src/Defaults.php", defaults),
            ("src/WithDefaults.php", with),
            ("src/Without.php", without),
        ],
        "main.php",
    );
    assert_eq!(out, "{\"a\":1,\"b\":2}{\"a\":1}\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// Carbon's `Translator.php` picks its lazy parent with
/// `require (new ReflectionMethod(<interface>, 'trans'))->hasReturnType() ? A : B;` after a
/// `class_exists()` choice of the class (false for an interface, so the fixture reaches it through
/// the else branch). The autoload fold answers both from the installed
/// sources, so the file is compiled with the branch php takes. Value-checked against `php -n`.
#[test]
fn test_reflection_return_type_require_ternary_is_folded() {
    let main = format!("{AUTOLOADER}echo (new Tr())->kind(), \"\\n\";\n");
    let interface = "<?php\ninterface TrI { public function trans(string $id): string; }\n";
    let translator = "<?php\n$m = new ReflectionMethod(class_exists(TrI::class) ? 'Missing' : TrI::class, 'trans');\nrequire $m->hasReturnType() ? __DIR__ . '/lazy/Strong.php' : __DIR__ . '/lazy/Weak.php';\nclass Tr extends LazyTr {}\n";
    let strong = "<?php\nif (!class_exists(LazyTr::class, false)) { abstract class LazyTr { public function kind(): string { return 'strong'; } } }\n";
    let weak = "<?php\nif (!class_exists(LazyTr::class, false)) { abstract class LazyTr { public function kind(): string { return 'weak'; } } }\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("main.php", &main),
            ("src/TrI.php", interface),
            ("src/Tr.php", translator),
            ("src/lazy/Strong.php", strong),
            ("src/lazy/Weak.php", weak),
        ],
        "main.php",
    );
    assert_eq!(out, "strong\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// `isset(static::$factory)` and the read it guards, in the trait copy of a leaf class that
/// declares no such static (Laravel's `HasFactory::newFactory()` in `App\Models\User`): php's
/// `false`, and the read is php's Error only when it runs. A user declaring the static still
/// reads it. Both files stay compiled. Value-checked against `php -n`.
#[test]
fn test_late_bound_static_the_leaf_class_lacks_is_unset() {
    let main = format!("{AUTOLOADER}var_dump(User::factoryName(), Post::factoryName());\n");
    let factory = "<?php\nclass Factory { public static function new(): string { return \"made by \" . static::class; } }\n";
    let user_factory = "<?php\nclass UserFactory extends Factory {}\n";
    let has_factory = "<?php\ntrait HasFactory {\n    public static function factoryName(): ?string {\n        if (isset(static::$factory)) {\n            return static::$factory::new();\n        }\n        return null;\n    }\n}\n";
    let user = "<?php\nclass User { use HasFactory; }\n";
    let post = "<?php\nclass Post { use HasFactory; protected static $factory = UserFactory::class; }\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("main.php", &main),
            ("src/Factory.php", factory),
            ("src/UserFactory.php", user_factory),
            ("src/HasFactory.php", has_factory),
            ("src/User.php", user),
            ("src/Post.php", post),
        ],
        "main.php",
    );
    assert_eq!(out, "NULL\nstring(19) \"made by UserFactory\"\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// A call passing too few arguments to a method of a class the compiled world holds in full is
/// php's `ArgumentCountError` when it RUNS: Laravel's `HandleExceptions` calls
/// `$instance->enable()` without the argument the installed PHPUnit requires, on the branch a
/// `version_compare()` rules out. The file stays compiled and the message is php's, file and
/// line included. Value-checked against `php -n`.
#[test]
fn test_too_few_arguments_raise_argument_count_error_when_their_line_runs() {
    let main = format!(
        "{AUTOLOADER}echo (new Boot())->run(false), \"\\n\";\ntry {{ echo (new Boot())->run(true), \"\\n\"; }} catch (\\ArgumentCountError $e) {{ echo get_class($e), \": \", $e->getMessage(), \"\\n\"; }}\n"
    );
    let handler = "<?php\nfinal class Handler { public function enable(string $case): string { return \"on:\" . $case; } }\n";
    let boot = "<?php\nclass Boot {\n    public function run(bool $old): string {\n        $h = new Handler();\n        if ($old) {\n            return $h->enable();\n        }\n        return $h->enable(\"t\");\n    }\n}\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[("main.php", &main), ("src/Handler.php", handler), ("src/Boot.php", boot)],
        "main.php",
    );
    assert!(out.starts_with("on:t\nArgumentCountError: Too few arguments to function Handler::enable(), 0 passed in "), "{out}");
    assert!(out.ends_with("Boot.php on line 6 and exactly 1 expected\n"), "{out}");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// The same file reached only from a function body, as Laravel reaches Carbon's `Translator`:
/// the folded `require` left `$m = new ReflectionMethod(...)` behind as file-scope code with an
/// effect, and the file was deferred for it. The binding is a local of the including function
/// once its only read is decided, so it is dropped.
#[test]
fn test_folded_reflection_binding_does_not_defer_a_function_reached_file() {
    let main = format!(
        "{AUTOLOADER}function kind(): string {{ return (new Tr())->kind(); }}\necho kind(), \"\\n\";\n"
    );
    let interface = "<?php\ninterface TrI { public function trans(string $id): string; }\n";
    let translator = "<?php\n$m = new ReflectionMethod(class_exists(TrI::class) ? 'Missing' : TrI::class, 'trans');\nrequire $m->hasReturnType() ? __DIR__ . '/lazy/Strong.php' : __DIR__ . '/lazy/Weak.php';\nclass Tr extends LazyTr {}\n";
    let strong = "<?php\nif (!class_exists(LazyTr::class, false)) { abstract class LazyTr { public function kind(): string { return 'strong'; } } }\n";
    let weak = "<?php\nif (!class_exists(LazyTr::class, false)) { abstract class LazyTr { public function kind(): string { return 'weak'; } } }\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("main.php", &main),
            ("src/TrI.php", interface),
            ("src/Tr.php", translator),
            ("src/lazy/Strong.php", strong),
            ("src/lazy/Weak.php", weak),
        ],
        "main.php",
    );
    assert_eq!(out, "strong\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// An autoloaded interface extending `Serializable` binds: the builtin was missing from the class
/// catalog the autoloader checks a declaration's parents against, so ramsey/uuid's
/// `Type\TypeInterface` and `Fields\FieldsInterface` were judged unbindable and dropped without a
/// word, and every uuid class implementing them went to the runtime autoloader.
#[test]
fn test_autoloaded_interface_extending_serializable_is_compiled() {
    let main = format!(
        "{AUTOLOADER}$h = new Hex(\"ff\");\n$s = serialize($h);\necho $h->toString(), \" \", $s, \" \", unserialize($s)->toString(), \" \", $h instanceof Serializable ? \"S\" : \"-\", \"\\n\";\n"
    );
    let interface = "<?php\ninterface TypeI extends JsonSerializable, Serializable\n{\n    public function toString(): string;\n}\n";
    let hex = "<?php\nfinal class Hex implements TypeI\n{\n    public function __construct(private string $value) {}\n    public function toString(): string { return $this->value; }\n    public function jsonSerialize(): string { return $this->value; }\n    public function serialize(): string { return $this->value; }\n    public function unserialize(string $data): void { $this->value = $data; }\n    public function __serialize(): array { return [\"s\" => $this->value]; }\n    public function __unserialize(array $data): void { $this->value = $data[\"s\"]; }\n}\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[("main.php", &main), ("src/TypeI.php", interface), ("src/Hex.php", hex)],
        "main.php",
    );
    assert_eq!(out, "ff O:3:\"Hex\":1:{s:1:\"s\";s:2:\"ff\";} ff S\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// `require $m->getParameters()[0]->hasType() ? A : B;` is folded from the installed sources like
/// `hasReturnType()`: Carbon's `MessageFormatterMapper.php` picks its lazy base class by whether
/// symfony/translation's `MessageFormatterInterface::format()` types `$message`, and left to run
/// time the file declared `MessageFormatterMapper` nowhere the compiler could take it.
#[test]
fn test_folded_parameter_type_reflection_require_compiles_the_file() {
    let main = format!(
        "{AUTOLOADER}function kind(): string {{ return (new Mapper())->kind(); }}\necho kind(), \"\\n\";\n"
    );
    let interface = "<?php\ninterface Fmt { public function format(string $message, string $locale): string; }\n";
    let mapper = "<?php\n$m = new ReflectionMethod(Fmt::class, 'format');\nrequire $m->getParameters()[0]->hasType() ? __DIR__ . '/lazy/Strong.php' : __DIR__ . '/lazy/Weak.php';\nfinal class Mapper extends LazyFmt {}\n";
    let strong = "<?php\nif (!class_exists(LazyFmt::class, false)) { abstract class LazyFmt { public function kind(): string { return 'strong'; } } }\n";
    let weak = "<?php\nif (!class_exists(LazyFmt::class, false)) { abstract class LazyFmt { public function kind(): string { return 'weak'; } } }\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("main.php", &main),
            ("src/Fmt.php", interface),
            ("src/Mapper.php", mapper),
            ("src/lazy/Strong.php", strong),
            ("src/lazy/Weak.php", weak),
        ],
        "main.php",
    );
    assert_eq!(out, "strong\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// A trait construct demoted to its run-time fault is demoted in EVERY class using the trait: the
/// copies share the trait's span, and laravel/pao's `TestResultParsable` (used by two `Starter`
/// classes) had its second copy taken for the first one's error coming back, so that file was
/// deferred.
#[test]
fn test_a_fault_in_a_trait_used_by_two_classes_is_demoted_in_both_copies() {
    let main = format!(
        "{AUTOLOADER}$s = new Starter();\n$t = new Starter2();\necho $s->resolve() ?? \"null\", \" \", $t->resolve() ?? \"null\", \"\\n\";\n"
    );
    let parsable = "<?php\ntrait Parsable {\n    private function resolveTestResult(): ?Result {\n        if (class_exists(Missing\\Runner::class, false)\n            && Missing\\Runner::$result instanceof Result) {\n            return Missing\\Runner::$result;\n        }\n        return null;\n    }\n    public function resolve(): ?string { return $this->resolveTestResult()?->name; }\n}\n";
    let result = "<?php\nclass Result { public string $name = \"r\"; }\n";
    let starter = "<?php\nclass Starter { use Parsable; }\n";
    let starter2 = "<?php\nclass Starter2 { use Parsable; }\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[
            ("main.php", &main),
            ("src/Parsable.php", parsable),
            ("src/Result.php", result),
            ("src/Starter.php", starter),
            ("src/Starter2.php", starter2),
        ],
        "main.php",
    );
    assert_eq!(out, "null null\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}

/// psysh's `VarDumper/Dumper.php` stores reflection probes in variables and declares a different
/// trait in each arm of `if ($typed) { if ($returnTyped) ... } elseif ($returnTyped) ... else`:
/// the probes are answered from the installed sources and the kept arm's trait is compiled. The
/// file was deferred for declaring a class-like inside a run-time condition.
#[test]
fn test_reflection_probes_stored_in_variables_fold_a_conditional_trait() {
    let main = format!(
        "{AUTOLOADER}echo (new Dumper())->enterHash(1, 2), \"\\n\";\n"
    );
    let base = "<?php\nclass Base { public function enterHash($cursor, int $type): void {} }\n";
    let dumper = "<?php\n$method = new \\ReflectionMethod(Base::class, 'enterHash');\n$typed = $method->getParameters()[1]->hasType();\n$returnTyped = $method->hasReturnType();\nif ($typed) {\n    if ($returnTyped) {\n        trait Shim { public function enterHash($cursor, int $type): string { return 'typed void'; } }\n    } else {\n        trait Shim { public function enterHash($cursor, int $type): string { return 'typed'; } }\n    }\n} elseif ($returnTyped) {\n    trait Shim { public function enterHash($cursor, $type): string { return 'void'; } }\n} else {\n    trait Shim { public function enterHash($cursor, $type): string { return 'untyped'; } }\n}\nclass Dumper { use Shim; }\n";
    let (stderr, out) = compile_cli_files_and_run_with_compile_stderr(
        &[("main.php", &main), ("src/Base.php", base), ("src/Dumper.php", dumper)],
        "main.php",
    );
    assert_eq!(out, "typed void\n");
    assert!(!stderr.contains("is not compiled"), "{stderr}");
}
