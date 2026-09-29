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

/// A reserved word is an ordinary segment of a qualified name, as in `Demo\Namespace` or
/// `Vendor\Default\Theme`. Namespace declarations, `use` imports, `use function`, calls,
/// `::class` and `instanceof` all have to accept it. The declaration failed with
/// `Expected identifier after '\' in qualified name`. Regression for #826 and #840.
#[test]
fn test_reserved_words_are_ordinary_qualified_name_segments() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require __DIR__ . '/kwns_lib.php';

use Vendor\Default\Theme\Example;
use Demo\Namespace\Subject as Aliased;
use function Demo\Namespace\describe;

echo (new Example())->name(), "\n";
echo get_class(new Aliased()), "\n";
echo describe(), "\n";
echo \Demo\Namespace\describe(), "\n";
echo Vendor\Default\Theme\Example::class, "\n";
var_dump(new Aliased() instanceof Demo\Namespace\Subject);
"#,
            ),
            (
                "kwns_lib.php",
                r#"<?php
namespace Vendor\Default\Theme {
    class Example { public function name(): string { return "default theme"; } }
}
namespace Demo\Namespace {
    final class Subject {}
    function describe(): string { return __NAMESPACE__; }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(
        out,
        concat!(
            "default theme\n",
            "Demo\\Namespace\\Subject\n",
            "Demo\\Namespace\n",
            "Demo\\Namespace\n",
            "Vendor\\Default\\Theme\\Example\n",
            "bool(true)\n",
        )
    );
}

/// A qualified name whose FIRST segment is a reserved word, as in `Default\Theme\Palette`,
/// must work everywhere a name can stand: calls, static calls and properties, class constants,
/// typed and nullable parameters, return and property types, `implements`, `catch`, `new`,
/// `instanceof`, `::class`, attributes, and plain, comma and group `use` imports whose first
/// segment is `function`. Before the fix these reached the keyword's own parser (statement
/// dispatch, expression prefix, type and catch parsing) and failed. A keyword separated from
/// `\` by a space stays the keyword, so `\strlen()` after `+` and `new \X` keep working.
/// Regression for #826. Expected output is PHP 8.5's.
#[test]
fn test_reserved_word_first_segments_work_in_every_name_position() {
    let out = compile_and_run(
        r#"<?php
namespace Function\Lib { class Foo {} function g() { return __FUNCTION__; } const K = 3; }
namespace Default\Theme {
    #[\Attribute]
    class Attr { }
    class Palette { public static $hits = 0; public static function accent() { return "teal"; } const X = 1; }
    interface I {}
    class E extends \Exception {}
}
namespace Static\Kit { class Factory { public function make() { return "made"; } } }
namespace Main {
use Function\Lib\Foo;
use Default\Theme\Palette, Function\Lib\Foo as Foo2;
use Default\Theme\{Attr, I};
echo get_class(new Foo), " ", get_class(new Foo2), "\n";
echo \Default\Theme\Palette::accent(), "\n";
function t(\Default\Theme\Palette $p): \Default\Theme\Palette { return $p; }
echo get_class(t(new Palette)), "\n";
}
namespace {
#[Default\Theme\Attr]
function attributed() { return "attr ok"; }
echo attributed(), "\n";
echo Default\Theme\Palette::accent(), "\n";
echo Default\Theme\Palette::X, "\n";
echo Function\Lib\g(), "\n";
echo Function\Lib\K, "\n";
Default\Theme\Palette::$hits = 5;
Default\Theme\Palette::$hits++;
echo Default\Theme\Palette::$hits, "\n";
function t2(Default\Theme\Palette $p): Default\Theme\Palette { return $p; }
echo get_class(t2(new Default\Theme\Palette)), "\n";
function t3(?Default\Theme\Palette $p = null): string { return $p === null ? "null" : "set"; }
echo t3(), " ", t3(new Default\Theme\Palette), "\n";
class Z implements Default\Theme\I {}
var_dump(new Z instanceof Default\Theme\I);
try { throw new Default\Theme\E("x"); } catch (Default\Theme\E $e) { echo "caught ", get_class($e), "\n"; }
$n = Default\Theme\Palette::class; echo $n, "\n";
echo (new Static\Kit\Factory())->make(), "\n";
var_dump((new Static\Kit\Factory()) instanceof Static\Kit\Factory);
class Holder { public Default\Theme\Palette $p; public function __construct() { $this->p = new Default\Theme\Palette; } }
echo get_class((new Holder)->p), "\n";
$f = fn(Default\Theme\Palette $p) => get_class($p);
echo $f(new Default\Theme\Palette), "\n";
echo strlen("x") + \strlen("yz"), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Function\\Lib\\Foo Function\\Lib\\Foo\n",
            "teal\n",
            "Default\\Theme\\Palette\n",
            "attr ok\n",
            "teal\n",
            "1\n",
            "Function\\Lib\\g\n",
            "3\n",
            "6\n",
            "Default\\Theme\\Palette\n",
            "null set\n",
            "bool(true)\n",
            "caught Default\\Theme\\E\n",
            "Default\\Theme\\Palette\n",
            "made\n",
            "bool(true)\n",
            "Default\\Theme\\Palette\n",
            "Default\\Theme\\Palette\n",
            "3\n",
        )
    );
}

/// A reserved word glued into a qualified name is a name in EVERY parser position, including the
/// keyword checks that run before a statement or expression is dispatched: a statement led by
/// `Default\...` or `Case\...` inside a `switch` case body (`Case\Kit\pick();` was silently
/// read as another case label), a `match` arm pattern led by `Default\...`, the `endif`, `else`
/// and `finally` checks after a body, `catch (Self\X ...)` and `Parent\X`, the `static` and
/// `readonly` member modifiers (`public Static\Factory $f` is a typed property, not a static
/// one), `insteadof`, `instanceof Name::$prop`, a builtin type word (`Int\Money`), string and
/// heredoc interpolation, and a one-word namespace declaration (`namespace Else { }`). The lexer
/// now emits such a word as an identifier, as PHP 8 lexes the whole name as one token.
/// Regression for #826 (review round 2); expected output is PHP 8.5's.
#[test]
fn test_reserved_word_names_survive_every_keyword_check() {
    let out = compile_and_run(
        r#"<?php
namespace Default\Theme {
    class Palette {
        const X = 1;
        public static $hits = 0;
        public static $cls = "Default\\Theme\\Palette";
        public static function accent() { return "teal"; }
        public function tone($n) { return "tone" . $n; }
    }
}
namespace Case\Kit { function pick() { echo "picked\n"; return 1; } }
namespace EndIf { class Marker { public static function ping() { echo "ping\n"; } } }
namespace Else { class Gate { public static function go() { echo "else-go\n"; } } }
namespace Finally\Kit { function done() { echo "done\n"; } }
namespace Self { class Boom extends \Exception {} }
namespace Parent { class Boom extends \Exception {} }
namespace Static { class Factory {} }
namespace Readonly { class Config {} }
namespace Int { class Money { public function __construct(public int $cents) {} } }
namespace List {
    trait A { public function m() { return "A"; } }
    trait B { public function m() { return "B"; } }
}
namespace {
switch ($argc) {
    case 1:
        Default\Theme\Palette::accent();
        echo "switch default-led\n";
        Case\Kit\pick();
        break;
}
$v = $argc;
echo match ($v) { Default\Theme\Palette::X => "match one", default => "match other" }, "\n";
if ($argc > 0):
    EndIf\Marker::ping();
endif;
if ($argc > 5) { echo "no\n"; } Else\Gate::go();
try { echo "try\n"; } catch (Exception $e) { } Finally\Kit\done();
try { throw new Self\Boom("s"); } catch (Self\Boom $e) { echo "caught ", get_class($e), "\n"; }
try { throw new Parent\Boom("p"); } catch (Parent\Boom | Self\Boom $e) { echo "caught ", get_class($e), "\n"; }
class Holder {
    public Static\Factory $f;
    public Readonly\Config $c;
    public function __construct() { $this->f = new Static\Factory(); $this->c = new Readonly\Config(); }
}
$h = new Holder();
$r = new ReflectionProperty("Holder", "f");
var_dump($r->isStatic());
$r = new ReflectionProperty("Holder", "c");
var_dump($r->isReadOnly());
$h->c = new Readonly\Config();
echo get_class($h->f), " ", get_class($h->c), "\n";
class Picker { use List\A, List\B { List\A::m insteadof List\B; } }
echo (new Picker())->m(), "\n";
function money(Int\Money $m): Int\Money { return $m; }
echo money(new Int\Money(250))->cents, "\n";
$p = new Default\Theme\Palette();
var_dump($p instanceof Default\Theme\Palette::$cls);
echo "{$p->tone(Default\Theme\Palette::X)}\n";
echo <<<TXT
heredoc {$p->tone(Default\Theme\Palette::X + 1)}
TXT;
echo "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "switch default-led\n",
            "picked\n",
            "match one\n",
            "ping\n",
            "else-go\n",
            "try\n",
            "done\n",
            "caught Self\\Boom\n",
            "caught Parent\\Boom\n",
            "bool(false)\n",
            "bool(false)\n",
            "Static\\Factory Readonly\\Config\n",
            "A\n",
            "250\n",
            "bool(true)\n",
            "tone1\n",
            "heredoc tone2\n",
        )
    );
}

/// `eval()` of a runtime string (the Magician interpreter, not the AOT parser) reads a reserved
/// word glued into a qualified name as a name too: `Function\Lib\g();` was dispatched as a
/// function declaration, `Static\Kit\Factory::make();` as a `static` variable, and
/// `use Function\Lib\Foo;` imported the FUNCTION `Lib\Foo`, where PHP and the compiled path
/// import the class `Function\Lib\Foo`. Regression for #826; expected output is PHP 8.5's.
#[test]
fn test_eval_reads_reserved_word_first_segments_as_names() {
    let out = compile_and_run(
        r#"<?php
namespace Function\Lib { class Foo {} function g() { return "Function\\Lib\\g"; } }
namespace Static\Kit { class Factory { public static function make() { echo "made\n"; } } }
namespace {
    $code = $argc > 5 ? 'return 0;' : 'namespace Probe; use Function\Lib\Foo; echo \Function\Lib\g(), "|", get_class(new Foo()), "\n";';
    eval($code);
    $code = $argc > 5 ? 'return 0;' : 'Static\Kit\Factory::make(); Function\Lib\g(); echo Function\Lib\g(), "\n";';
    eval($code);
}
"#,
    );
    assert_eq!(out, "Function\\Lib\\g|Function\\Lib\\Foo\nmade\nFunction\\Lib\\g\n");
}
