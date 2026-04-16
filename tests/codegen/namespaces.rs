use crate::support::*;

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

#[test]
fn test_namespace_callback_string_literals_resolve_current_namespace() {
    let out = compile_and_run(
        r#"<?php
namespace Demo\Callbacks;

function triple($value) {
    return $value * 3;
}

echo function_exists("triple");
echo call_user_func("triple", 4);
"#,
    );
    assert_eq!(out, "112");
}

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
