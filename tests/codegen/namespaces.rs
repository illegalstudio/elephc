//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of namespaces, including namespace use function and global builtin resolution, namespace class can call global extern function, and namespace class can call pointer builtins without global prefix.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use crate::support::*;

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
