//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of namespaces, including namespace use function and global builtin resolution, namespace class can call global extern function, and namespace class can call pointer builtins without global prefix.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use crate::support::*;

mod polyfills;

/// Verifies `use function` aliasing and global builtin resolution inside a namespaced file.
/// Uses a two-namespace fixture: `Demo\Util\render` aliased as `paint` and global `strlen`.
/// Checks that the alias resolves correctly and global builtins are accessible without prefix.
#[test]
fn test_namespace_use_function_and_global_builtin_resolution() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\Util;
function render($value) { echo $value; }

namespace Demo\App;
use function Demo\Util\render as paint;

paint("A");
echo strlen("bc");
"#,
    );
    assert_eq!(out, "A2");
}

/// Verifies that a class inside a namespace can call a global `extern function` without
/// a namespace prefix. Regression test: extern functions must resolve globally regardless
/// of the enclosing namespace context.
#[test]
fn test_namespace_class_can_call_global_extern_function() {
    let out = compile_and_run(
        r#"<?php
extern function getpid(): int;

namespace Demo\App;

class Probe {
    public function ok(): int {
        return getpid() > 0 ? 1 : 0;
    }
}

$probe = new Probe();
echo $probe->ok();
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies that pointer builtins (`ptr_null`, `ptr_is_null`) are accessible inside a
/// namespaced class method without a global prefix. Regression test for builtin resolution
/// within namespace scope.
#[test]
fn test_namespace_class_can_call_pointer_builtins_without_global_prefix() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\App;

class Probe {
    public function ok(): int {
        $p = ptr_null();
        return ptr_is_null($p);
    }
}

$probe = new Probe();
echo $probe->ok();
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies that string builtins (`strlen`) are accessible inside a namespaced class method
/// without a global prefix. Regression test for builtin resolution within namespace scope.
#[test]
fn test_namespace_class_can_call_string_builtin_without_global_prefix() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\App;

class Probe {
    public function ok(): int {
        return strlen("hello");
    }
}

$probe = new Probe();
echo $probe->ok();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies that a declared return type (`Box`) resolves to the same-namespace class
/// inside a typed local variable declaration (`Box $box = ...`). Checks both return-type
/// resolution and typed local variable initialization.
#[test]
fn test_namespace_resolves_class_type_hints_in_functions_and_typed_locals() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\App;

class Box {
    public $value;

    public function __construct() {
        $this->value = 7;
    }
}

function make_box(): Box {
    return new Box();
}

Box $box = make_box();
echo $box->value;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies PHP's generic `object` type remains namespace-independent and
/// case-insensitive while accepting an instance of a concrete namespaced class.
#[test]
fn test_namespace_preserves_generic_object_type_hints_case_insensitively() {
    let out = compile_and_run(
        r#"<?php
namespace App;

class Payload {}

function handle(object $value): string {
    return is_object($value) ? "lower" : "bad";
}

function handle_upper(OBJECT $value): string {
    return is_object($value) ? "upper" : "bad";
}

$value = new Payload();
echo handle($value) . ":" . handle_upper($value);
"#,
    );
    assert_eq!(out, "lower:upper");
}

/// Verifies that `buffer<Vertex>` works correctly when `Vertex` is a packed class declared
/// in the same namespace. Tests buffer element access with typed buffer slots.
#[test]
fn test_namespace_resolves_packed_class_types_inside_buffers() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\App;

packed class Vertex {
    public int $x;
    public int $y;
}

class Probe {
    public function run(): int {
        buffer<Vertex> $points = buffer_new<Vertex>(1);
        $points[0]->x = 3;
        $points[0]->y = 4;
        return $points[0]->x + $points[0]->y;
    }
}

$probe = new Probe();
echo $probe->run();
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that property types converge across a chain of classes: `Box.items` holds a
/// `buffer<Point>` written by `Loader.load()`, and `Game.run()` reads `items[0]->x`.
/// Regression test for post-pass type convergence across class boundaries.
#[test]
fn test_method_post_pass_converges_property_types_across_classes() {
    let out = compile_and_run(
        r#"<?php
packed class Point {
    public int $x;
}

class Box {
    public $items;

    public function __construct() {
        $this->items = 0;
    }
}

class Loader {
    public function load(): Box {
        $box = new Box();
        buffer<Point> $items = buffer_new<Point>(1);
        $items[0]->x = 7;
        $box->items = $items;
        return $box;
    }
}

class Game {
    public $box;

    public function __construct() {
        $this->box = 0;
    }

    public function run(): int {
        $loader = new Loader();
        $this->box = $loader->load();
        return $this->box->items[0]->x;
    }
}

$game = new Game();
echo $game->run();
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that a forward class reference in a method return type (`load(): Item`) resolves
/// even when `Item` is defined after the method. Both classes are in the global namespace.
#[test]
fn test_forward_class_reference_in_method_return_type() {
    let out = compile_and_run(
        r#"<?php
class Loader {
    public function load(): Item {
        return new Item();
    }
}

class Item {
    public $value;

    public function __construct() {
        $this->value = 9;
    }
}

$loader = new Loader();
$item = $loader->load();
echo $item->value;
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies that property array access (`$this->items[0]`) works correctly when `items`
/// is an array property initialized in the constructor. Regression test for property lookup
/// followed by subscript in the same expression.
#[test]
fn test_property_array_access_after_property_lookup() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public $items;

    public function __construct() {
        $this->items = [10, 20, 30];
    }

    public function first(): int {
        return $this->items[0];
    }
}

$bag = new Bag();
echo $bag->first();
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies that a typed parameter (`string $text`) is correctly available inside the
/// method body for use in a subsequent call (`strlen($text)`). All types are in the
/// global namespace.
#[test]
fn test_typed_method_param_is_available_with_declared_type_in_body() {
    let out = compile_and_run(
        r#"<?php
class Reader {
    public function len(string $text): int {
        return strlen($text);
    }
}

$reader = new Reader();
echo $reader->len("doom");
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies that a typed constructor parameter (`string $bytes`) is not overwritten by
/// untyped property inference. The property `$bytes` should be set from the parameter,
/// and `$this->bytes` should remain accessible in the method body. Regression test for
/// constructor parameter vs. property name collision.
#[test]
fn test_typed_constructor_param_is_not_overwritten_by_untyped_property_inference() {
    let out = compile_and_run(
        r#"<?php
class Blob {
    public $bytes;

    public function __construct(string $bytes) {
        $this->bytes = $bytes;
    }

    public function len(): int {
        return strlen($this->bytes);
    }
}

$blob = new Blob("doom");
echo $blob->len();
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies that `require` inside a namespace preserves the required file's namespace
/// context (`Demo\Lib`) while the main file uses `use Demo\Lib\User`. Multi-file fixture
/// with `main.php` and `lib.php`; regression test for include-time namespace context.
#[test]
fn test_namespace_include_preserves_class_namespace_context() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
namespace Demo\App;
require "lib.php";

use Demo\Lib\User;

$user = new User();
echo $user->label();
"#,
            ),
            (
                "lib.php",
                r#"<?php
namespace Demo\Lib;

class User {
    public function label() {
        return "ok";
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

/// Verifies that `use const` aliasing and fully-qualified constant paths both resolve
/// correctly in the same namespace (`Demo\Values\ANSWER` aliased as `ANSWER` and accessed
/// via `\Demo\Values\ANSWER`). Checks both resolution paths produce the same value.
#[test]
fn test_namespace_use_const_and_fully_qualified_constant_resolution() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\Values;
const ANSWER = 42;

namespace Demo\App;
use const Demo\Values\ANSWER;

echo ANSWER;
echo \Demo\Values\ANSWER;
"#,
    );
    assert_eq!(out, "4242");
}

/// Verifies that fully-qualified `function_exists()` sees a namespaced function,
/// while `call_user_func` with a short name still resolves through the current
/// namespace's callback lookup.
#[test]
fn test_namespace_callback_string_literals_resolve_current_namespace() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\Callbacks;

function triple($value) {
    return $value * 3;
}

echo function_exists("Demo\\Callbacks\\triple");
echo call_user_func("triple", 4);
"#,
    );
    assert_eq!(out, "112");
}

/// Verifies that fully-qualified callback strings (`"Demo\\Support\\format_user"`) resolve
/// absolutely and that `call_user_func_array` works with the same path. Uses a namespaced
/// class method and `use` import; checks both single-argument and array-argument forms.
#[test]
fn test_namespace_fully_qualified_callback_strings_are_absolute() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\Support;

class User {
    public function badge() {
        return "ok";
    }
}

function format_user(User $user) {
    return "[" . $user->badge() . "]";
}

namespace Demo\App;

use Demo\Support\User;

echo function_exists("Demo\\Support\\format_user");
echo call_user_func("Demo\\Support\\format_user", new User());
echo call_user_func_array("Demo\\Support\\format_user", [new User()]);
"#,
    );
    assert_eq!(out, "1[ok][ok]");
}

/// Verifies that group use syntax (`use Demo\Lib\{User, function render as paint, const ANSWER}`)
/// resolves class, function, and const imports correctly within a namespaced file.
/// Checks static method call, const access, and aliased function call all produce expected output.
#[test]
fn test_namespace_group_use_resolves_class_function_and_const() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\Lib;

const ANSWER = 7;

function render($value) {
    return "<" . $value . ">";
}

class User {
    public static function label() {
        return "ok";
    }
}

namespace Demo\App;

use Demo\Lib\{User, function render as paint, const ANSWER};

echo User::label();
echo ANSWER;
echo paint("x");
"#,
    );
    assert_eq!(out, "ok7<x>");
}

/// EC-12 (#495): a `new <ImportedAlias>` nested inside a NAMED ARGUMENT resolves the alias —
/// the resolver's expression walk previously had no NamedArg arm, so the value expression
/// escaped rewriting entirely ("Undefined class: Url" on the ward-component-catalog
/// `new self(label: ..., url: new Url('/'))` previews pattern). Byte-parity vs PHP 8.5.
#[test]
fn test_named_argument_value_resolves_imported_alias() {
    let out = compile_and_run(
        r#"<?php

namespace App\Url;

final class Url {
    public function __construct(public string $p) {}
}

namespace App\C;

use App\Url\Url;

final class K {
    public function __construct(public string $label, public Url $t) {}

    public static function mk(): K {
        return new self(label: 'x', t: new Url('/'));
    }
}

namespace Main;

echo \App\C\K::mk()->t->p;
"#,
    );
    assert_eq!(out, "/");
}

/// `use const PHP_INT_MAX;` — the lexer eagerly tokenizes such constants, so the
/// use-declaration parser must accept the dedicated tokens as import names. Aliases
/// resolve through the seeded constant map (expression uses are not lexer-only).
#[test]
fn test_use_const_of_lexer_tokenized_constant() {
    let out = compile_and_run(
        r#"<?php

namespace App;

use const PHP_INT_MAX;
use const PHP_INT_MIN;
use const STDERR;

echo PHP_INT_MAX > 0 ? 'max' : '?', ':', PHP_INT_MIN < 0 ? 'min' : '?';
"#,
    );
    assert_eq!(out, "max:min");
}

/// `use const PHP_INT_MAX as MAX` must resolve the alias through ConstRef, not only the
/// dedicated lexer token path.
#[test]
fn test_use_const_alias_of_lexer_tokenized_constant() {
    let out = compile_and_run(
        r#"<?php
namespace App;
use const PHP_INT_MAX as MAX;
echo MAX > 0 ? 'ok' : 'no';
"#,
    );
    assert_eq!(out, "ok");
}

/// Imports multiple lexer-tokenized predefined constants in one `use const` declaration.
#[test]
fn test_use_const_multiple_lexer_tokenized_constants() {
    let out = compile_and_run(
        r#"<?php
namespace App;
use const PHP_INT_MAX as MAX, PHP_INT_MIN as MIN;
echo MAX > 0 && MIN < 0 ? 'ok' : 'no';
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies `Enum` is accepted as an import alias, type name, constructor, and scoped receiver.
#[test]
fn test_enum_soft_keyword_import_alias() {
    let out = compile_and_run(
        r#"<?php
namespace Vendor { class Legacy {} }
namespace App {
    use Vendor\Legacy as Enum;
    function imported_name(Enum $value): string { return Enum::class; }
    echo imported_name(new Enum());
}
"#,
    );
    assert_eq!(out, "Vendor\\Legacy");
}

/// Verifies that a namespace alias (`use App\Math as M;`) expands the leading segment of a
/// qualified *function* call, matching PHP's rule that qualified names are translated
/// through the class/namespace import table.
#[test]
fn test_namespace_alias_expands_qualified_function_call() {
    let out = compile_and_run(
        r#"<?php
namespace App\Math;
function double(int $x): int { return $x * 2; }

namespace App\Main;
use App\Math as M;
echo M\double(5);
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies that a namespace alias expands the leading segment of qualified *class*
/// references: static call, class constant, `new`, and `instanceof`.
#[test]
fn test_namespace_alias_expands_qualified_class_references() {
    let out = compile_and_run(
        r#"<?php
namespace App\Math;
class Thing {
    const V = 7;
    public static function m(): string { return "Thing::m"; }
    public function i(): string { return "i"; }
}

namespace App\Main;
use App\Math as M;
echo M\Thing::m();
echo M\Thing::V;
$t = new M\Thing();
echo $t->i();
echo $t instanceof M\Thing ? "yes" : "no";
"#,
    );
    assert_eq!(out, "Thing::m7iyes");
}

/// Verifies that a namespace alias expands the leading segment of a qualified *constant*
/// reference (`M\FOO`).
#[test]
fn test_namespace_alias_expands_qualified_constant() {
    let out = compile_and_run(
        r#"<?php
namespace App\Math;
const FOO = 42;

namespace App\Main;
use App\Math as M;
echo M\FOO;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that namespace-alias expansion is case-insensitive on both the alias and the
/// aliased function name, as PHP namespace/function lookups are.
#[test]
fn test_namespace_alias_expansion_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
namespace App\Math;
function double(int $x): int { return $x * 2; }

namespace App\Main;
use App\Math as M;
echo m\double(5);
echo M\DOUBLE(6);
"#,
    );
    assert_eq!(out, "1012");
}

/// Verifies that only the FIRST segment of a qualified name is alias-expanded: `A\C\g()`
/// expands `A` and keeps `C\g` verbatim even though `C` is itself an alias.
#[test]
fn test_namespace_alias_expands_only_first_segment() {
    let out = compile_and_run(
        r#"<?php
namespace Q\R;
function h(): string { return "Q\\R\\h"; }

namespace App\Main;
use Q as A;
use Zzz as R;
echo A\R\h();
"#,
    );
    assert_eq!(out, "Q\\R\\h");
}

/// Verifies that an alias imported directly for a class (`use App\Math\Thing as T;`) still
/// works as an unqualified constructor name.
#[test]
fn test_namespace_class_alias_unqualified_constructor() {
    let out = compile_and_run(
        r#"<?php
namespace App\Math;
class Thing { public function i(): string { return "i"; } }

namespace App\Main;
use App\Math\Thing as T;
$t = new T();
echo $t->i();
"#,
    );
    assert_eq!(out, "i");
}

/// Fully qualified predefined constants parse wherever an expression can stand (#1307).
///
/// The lexer turns `PHP_EOL`, `PHP_INT_MAX`, `M_PI`, `STDOUT` and 18 other predefined constants
/// into dedicated tokens, so `\PHP_EOL` reached the name parser as a backslash followed by a
/// non-identifier and failed with "Expected name". Namespaced code writes them fully qualified
/// routinely. Covers a namespace constant, a class constant, parameter defaults, a `match`
/// subject and ordinary expressions. Expected output is PHP 8.5.10's.
#[test]
fn test_fully_qualified_predefined_constants_parse_in_every_position() {
    let out = compile_and_run(
        r#"<?php
namespace App;
const LIMIT = \PHP_INT_MAX;
class K {
    const PI = \M_PI;
    public function f($x = \PHP_INT_MIN, $s = \DIRECTORY_SEPARATOR) { return [$x, $s]; }
}
echo \PHP_EOL === "\n" ? "eol" : "no", \PHP_EOL;
echo LIMIT === \PHP_INT_MAX ? "max" : "no", \PHP_EOL;
echo K::PI > 3.14 ? "pi" : "no", \PHP_EOL;
echo (new K())->f()[0] === \PHP_INT_MIN ? "min" : "no", "|", (new K())->f()[1], \PHP_EOL;
echo is_infinite(\INF) ? "inf" : "no", " ", is_nan(\NAN) ? "nan" : "no", \PHP_EOL;
echo \M_E > 2.7 ? "e" : "no", " ", \M_SQRT2 > 1.41 ? "sqrt2" : "no", " ", \PHP_FLOAT_EPSILON > 0 ? "eps" : "no", \PHP_EOL;
echo is_resource(\STDOUT) ? "stdout" : "no", " ", strlen(\PHP_OS) > 0 ? "os" : "no", \PHP_EOL;
echo match (\PHP_INT_SIZE) { 8 => "eight", default => "other" }, \PHP_EOL;
var_dump(\true, \false, \null, \TRUE);
"#,
    );
    assert_eq!(
        out,
        "eol\nmax\npi\nmin|/\ninf nan\ne sqrt2 eps\nstdout os\neight\nbool(true)\nbool(false)\nNULL\nbool(true)\n"
    );
}

/// `namespace\name` is PHP's relative name, "name in the current namespace", for a
/// function, a constant, a class in `new`, a static call, `::class`, `instanceof`, an
/// `implements` list and a parameter type. It failed to parse with
/// `Unexpected token: Namespace`. Regression for #825.
#[test]
fn test_relative_namespace_names_resolve_in_the_current_namespace() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\Sub;

const LIMIT = 7;

function helper(): int { return 1; }

class Box {
    public function __construct(public int $v = 3) {}
    public static function make(): static { return new static(5); }
}

interface Shape {}
final class Square implements namespace\Shape {}

echo namespace\helper(), "\n";
echo namespace\LIMIT, "\n";
$b = new namespace\Box();
echo $b->v, "\n";
echo namespace\Box::make()->v, "\n";
echo namespace\Box::class, "\n";
var_dump(new Square() instanceof namespace\Shape);
function takes(namespace\Box $b): int { return $b->v; }
echo takes(new Box(9)), "\n";
echo \strlen("abc"), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "1\n",
            "7\n",
            "3\n",
            "5\n",
            "Demo\\Sub\\Box\n",
            "bool(true)\n",
            "9\n",
            "3\n",
        )
    );
}

/// Each braced namespace block is its own current namespace for a relative name, and the
/// global `namespace {}` block makes `namespace\who()` the global function.
#[test]
fn test_relative_namespace_names_follow_braced_namespace_blocks() {
    let out = compile_and_run(
        r#"<?php
namespace First {
    function who(): string { return "first"; }
    echo namespace\who(), "\n";
}
namespace Second {
    function who(): string { return "second"; }
    echo namespace\who(), "\n";
}
namespace {
    function who(): string { return "global"; }
    echo namespace\who(), "\n";
    echo First\who(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "first\n",
            "second\n",
            "global\n",
            "first\n",
        )
    );
}

/// A relative name keeps resolving in the enclosing namespace after the word `namespace` is
/// used as an enum case, a method name and a member access: a token scan read `case namespace;`
/// as the declaration `namespace;` and bound every later `namespace\...` in the global
/// namespace. Also covers `extends`, `implements`, trait `use`, return, property and `catch`
/// types, an attribute (read back through Reflection), a first-class callable, and a relative
/// name in the global block that follows. Regression for #825; expected output is PHP 8.5's.
#[test]
fn test_relative_namespace_names_survive_namespace_as_a_member_name() {
    let out = compile_and_run(
        r#"<?php
namespace App {
    enum Mode { case namespace; case other; }
    #[\Attribute]
    class Tag { public function __construct(public string $v = "tag") {} }
    class Foo {
        const C = "C";
        public static function who() { return __CLASS__; }
        public function namespace() { return "method named namespace"; }
    }
    interface Shape {}
    trait Greets { public function hi() { return "hi from " . static::class; } }
    class E extends \Exception {}
    function f() { return __FUNCTION__; }
    const K = 7;
    echo namespace\f(), "\n";
    echo namespace\K, "\n";
    echo namespace\Foo::who(), "\n";
    echo namespace\Foo::C, "\n";
    echo namespace\Foo::class, "\n";
    $o = new namespace\Foo();
    echo $o->namespace(), "\n";
    var_dump($o instanceof namespace\Foo);
    var_dump(Mode::namespace === namespace\Mode::namespace);
    class Bar extends namespace\Foo implements namespace\Shape { use namespace\Greets; }
    echo (new Bar)->hi(), "\n";
    function typed(namespace\Foo $x): namespace\Foo { return $x; }
    echo get_class(typed(new Foo)), "\n";
    try { throw new namespace\E("boom"); } catch (namespace\E $e) { echo "caught ", get_class($e), "\n"; }
    #[namespace\Tag("x")]
    function attributed() { return "attributed"; }
    echo attributed(), "\n";
    $r = new \ReflectionFunction('App\attributed');
    echo $r->getAttributes()[0]->getName(), "\n";
    $cb = namespace\f(...);
    echo $cb(), "\n";
    class Holder { public namespace\Foo $p; public function __construct() { $this->p = new Foo; } }
    echo get_class((new Holder)->p), "\n";
}
namespace {
    function f() { return "global f"; }
    echo namespace\f(), "\n";
    echo namespace\App\f(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "App\\f\n",
            "7\n",
            "App\\Foo\n",
            "C\n",
            "App\\Foo\n",
            "method named namespace\n",
            "bool(true)\n",
            "bool(true)\n",
            "hi from App\\Bar\n",
            "App\\Foo\n",
            "caught App\\E\n",
            "attributed\n",
            "App\\Tag\n",
            "App\\f\n",
            "App\\Foo\n",
            "global f\n",
            "App\\f\n",
        )
    );
}

/// `eval()` resolves a relative name in the eval fragment's own namespace, once: the eval
/// parser read `namespace` as a first segment and prefixed the namespace again, so
/// `namespace\helper()` in `App` called `App\namespace\helper`. The global fragment's relative
/// name is the global function. Regression for #825; expected output is PHP 8.5's.
#[test]
fn test_eval_relative_namespace_names_resolve_in_the_fragment_namespace() {
    let out = compile_and_run(
        r#"<?php
namespace App {
    function helper() { return "App\\helper"; }
    const LIMIT = 3;
    class Box { public static function make() { return "App\\Box::make"; } }
}
namespace {
    function helper() { return "global helper"; }
    $code = $argc > 5 ? 'return 0;' : 'namespace App; echo namespace\helper(), "|", namespace\LIMIT, "|", namespace\Box::make(), "|", namespace\Box::class, "\n"; namespace\helper();';
    eval($code);
    eval('echo namespace\helper(), "\n";');
    eval('namespace App { echo get_class(new namespace\Box()), "\n"; }');
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "App\\helper|3|App\\Box::make|App\\Box\n",
            "global helper\n",
            "App\\Box\n",
        )
    );
}

/// A relative name works in the three positions the second review found refused: an
/// `insteadof` list (`namespace\A::m insteadof namespace\B`), the static-property form of
/// `instanceof` (`$x instanceof namespace\Cfg::$cls`), and a constant the lexer gives its own
/// token. Inside a namespace `namespace\NAN` is that namespace's constant; in the global
/// namespace `namespace\PHP_EOL`, `namespace\true`, `namespace\null`, `namespace\INF` and
/// `namespace\M_PI` are the global constants. Regression for #825; expected output is PHP 8.5's.
#[test]
fn test_relative_names_in_insteadof_instanceof_and_constant_tokens() {
    let out = compile_and_run(
        r#"<?php
namespace App {
    trait A { public function m() { return "a"; } }
    trait B { public function m() { return "b"; } }
    class C { use namespace\A, namespace\B { namespace\A::m insteadof namespace\B; } }
    echo (new C())->m(), "\n";
    class Cfg { public static $cls = "Exception"; }
    $x = new \Exception();
    var_dump($x instanceof namespace\Cfg::$cls);
    const NAN = "App NAN";
    echo namespace\NAN, "\n";
}
namespace {
    echo "[", namespace\PHP_EOL, "]\n";
    var_dump(namespace\true, namespace\null, namespace\INF);
    echo namespace\M_PI > 3 ? "pi" : "no", "\n";
}
"#,
    );
    assert_eq!(
        out,
        "a\nbool(true)\nApp NAN\n[\n]\nbool(true)\nNULL\nfloat(INF)\npi\n"
    );
}
