//! Purpose:
//! Focused parity cases that compare legacy AST backend output with EIR backend output.
//!
//! Called from:
//! - `tests/ir_backend_parity.rs` as an integration-test module.
//!
//! Key details:
//! - Each case compiles the same PHP snippet twice at the binary level, once
//!   through the default backend and once through `--ir-backend`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug)]
enum Backend {
    Legacy,
    Ir,
}

impl Backend {
    /// Returns the short backend name used in diagnostics and temporary paths.
    fn name(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Ir => "ir",
        }
    }

    /// Adds the backend-specific CLI flags to the compile command.
    fn add_compile_flags(self, command: &mut Command) {
        if matches!(self, Self::Ir) {
            command.arg("--ir-backend");
        }
    }
}

/// Verifies scalar locals and control flow stay equivalent across the two backends.
#[test]
fn parity_scalar_control_and_builtin_baseline() {
    assert_backend_parity(
        "scalar_control",
        "<?php $x = 40; if ($argc === 1) { echo $x + 2; } else { echo 0; }",
        &[],
    );
    assert_backend_parity(
        "scalar_builtins",
        "<?php echo strlen('abc'); echo ':'; echo strtoupper('ir');",
        &[],
    );
}

/// Verifies later indexed-array foreach states do not clobber source pointers at large frame offsets.
#[test]
fn parity_repeated_indexed_foreach() {
    assert_backend_parity(
        "repeated_indexed_foreach",
        r#"<?php
$a = [1, 2];
foreach ($a as $value) { echo $value; }
echo "|";
$b = [3, 4];
foreach ($b as $value) { echo $value; }
echo "|";
$c = [5, 6];
foreach ($c as $value) { echo $value; }
"#,
        &[],
    );
}

/// Verifies recent static callable fixes match the legacy backend behavior.
#[test]
fn parity_static_callable_checks() {
    assert_backend_parity(
        "direct_fcc_is_callable",
        r#"<?php
function eir_callable_probe(): int { return 1; }
class EirCallableProbe {
    public static function hit(): int { return 1; }
}
echo is_callable(strlen(...)) ? "yes" : "no";
echo ":";
echo is_callable(eir_callable_probe(...)) ? "yes" : "no";
echo ":";
echo is_callable(EirCallableProbe::hit(...)) ? "yes" : "no";
"#,
        &[],
    );
    assert_backend_parity(
        "static_method_string_is_callable",
        r#"<?php
class EirCallableBox {
    public static function hit(): int { return 1; }
    private static function hidden(): int { return 2; }
}
echo is_callable("EirCallableBox::hit") ? "yes" : "no";
echo ":";
echo is_callable("eircallablebox::HIT") ? "yes" : "no";
echo ":";
echo is_callable("EirCallableBox::missing") ? "yes" : "no";
echo ":";
echo is_callable("MissingCallableBox::hit") ? "yes" : "no";
echo ":";
echo is_callable("EirCallableBox::hidden") ? "yes" : "no";
"#,
        &[],
    );
    assert_backend_parity(
        "static_method_array_is_callable",
        r#"<?php
class EirCallableArrayBox {
    public static function hit(): int { return 1; }
    private static function hidden(): int { return 2; }
    public function instance(): int { return 3; }
}
echo is_callable(["EirCallableArrayBox", "hit"]) ? "yes" : "no";
echo ":";
echo is_callable(["eircallablearraybox", "HIT"]) ? "yes" : "no";
echo ":";
echo is_callable(["EirCallableArrayBox", "missing"]) ? "yes" : "no";
echo ":";
echo is_callable(["MissingCallableArrayBox", "hit"]) ? "yes" : "no";
echo ":";
echo is_callable(["EirCallableArrayBox", "hidden"]) ? "yes" : "no";
echo ":";
echo is_callable(["EirCallableArrayBox", "instance"]) ? "yes" : "no";
"#,
        &[],
    );
    assert_backend_parity(
        "static_callable_local_is_callable",
        r#"<?php
function eir_local_callable_probe(): int { return 1; }
class EirLocalCallableProbe {
    public static function hit(): int { return 1; }
}
$fn = eir_local_callable_probe(...);
echo is_callable($fn) ? "yes" : "no";
echo ":";
$name = "eir_local_callable_probe";
echo is_callable($name) ? "yes" : "no";
echo ":";
$stat = EirLocalCallableProbe::hit(...);
echo is_callable($stat) ? "yes" : "no";
"#,
        &[],
    );
}

/// Verifies recent callable dispatch fixes preserve legacy backend behavior.
#[test]
fn parity_function_first_class_callable_dispatch() {
    assert_backend_parity(
        "function_fcc_call_user_func",
        r#"<?php
function fcc_sum(int $left, int $right): int {
    return $left + $right;
}
function fcc_join(string $left, string $right): string {
    return $left . ":" . $right;
}
echo call_user_func(fcc_sum(...), 2, 5);
echo "|";
echo call_user_func_array(fcc_join(...), ["go", "now"]);
"#,
        &[],
    );
    assert_backend_parity(
        "preg_replace_callback_function_fcc",
        r#"<?php
function eir_regex_replace_fcc(array $matches): string {
    return "F" . count($matches);
}
echo preg_replace_callback("/[A-Z]/", eir_regex_replace_fcc(...), "AB");
"#,
        &[],
    );
    assert_backend_parity(
        "preg_replace_callback_stored_function_fcc",
        r#"<?php
function eir_regex_replace_stored_fcc(array $matches): string {
    return "S" . count($matches);
}
$cb = eir_regex_replace_stored_fcc(...);
echo preg_replace_callback("/[A-Z]/", $cb, "AB");
"#,
        &[],
    );
    assert_backend_parity(
        "direct_first_class_callable_expr_call",
        r#"<?php
function eir_direct_fcc_add(int $left, int $right): int {
    return $left + $right;
}
class EirDirectFcc {
    public static function hit(int $value): int {
        return $value + 2;
    }
}
echo (eir_direct_fcc_add(...))(2, 5);
echo "|";
echo (strlen(...))("abcd");
echo "|";
echo (EirDirectFcc::hit(...))(3);
"#,
        &[],
    );
    assert_backend_parity(
        "direct_string_callable_expr_call",
        r#"<?php
function eir_direct_string_add(int $value): int {
    return $value + 1;
}
echo ("eir_direct_string_add")(4);
"#,
        &[],
    );
    assert_backend_parity(
        "assignment_expression_callable_call",
        r#"<?php
function eir_assign_call_add(int $value): int {
    return $value + 1;
}
echo ($fn = eir_assign_call_add(...))(4);
echo "|";
echo ($name = "eir_assign_call_add")(5);
"#,
        &[],
    );
    assert_backend_parity(
        "stored_first_class_callable_variable_call",
        r#"<?php
function eir_stored_fcc_add(int $value): int {
    return $value + 1;
}
class EirStoredFcc {
    public static function hit(int $value): int {
        return $value + 2;
    }
}
$fn = eir_stored_fcc_add(...);
echo $fn(4);
echo "|";
$len = strlen(...);
echo $len("abcd");
echo "|";
$stat = EirStoredFcc::hit(...);
echo $stat(5);
"#,
        &[],
    );
    assert_backend_parity(
        "stored_first_class_callable_call_user_func",
        r#"<?php
function eir_stored_cuf_add(int $value): int {
    return $value + 1;
}
function eir_stored_cuf_join(string $left, string $right): string {
    return $left . ":" . $right;
}
class EirStoredCuf {
    public static function hit(int $value): int {
        return $value + 2;
    }
}
$fn = eir_stored_cuf_add(...);
echo call_user_func($fn, 4);
echo "|";
echo call_user_func_array($fn, [6]);
echo "|";
$join = eir_stored_cuf_join(...);
echo call_user_func_array($join, ["right" => "R", "left" => "L"]);
echo "|";
$len = strlen(...);
echo call_user_func($len, "abcd");
echo "|";
$stat = EirStoredCuf::hit(...);
echo call_user_func($stat, 5);
"#,
        &[],
    );
    assert_backend_parity(
        "stored_string_callable_dispatch",
        r#"<?php
function eir_string_callable_add(int $value): int {
    return $value + 1;
}
function eir_string_callable_join(string $left, string $right): string {
    return $left . ":" . $right;
}
function eir_string_callable_regex(array $matches): string {
    return "C" . count($matches);
}
$fn = "eir_string_callable_add";
echo $fn(4);
echo "|";
echo call_user_func($fn, 5);
echo "|";
$join = "eir_string_callable_join";
echo call_user_func_array($join, ["right" => "R", "left" => "L"]);
echo "|";
$regex = "eir_string_callable_regex";
echo preg_replace_callback("/[A-Z]/", $regex, "AB");
"#,
        &[],
    );
    assert_backend_parity(
        "runtime_string_callable_direct_call_first",
        r#"<?php
function eir_runtime_pick_left(int $value): int {
    return $value + 1;
}
function eir_runtime_pick_right(int $value): int {
    return $value + 2;
}
$fn = $argc === 1 ? "eir_runtime_pick_left" : "eir_runtime_pick_right";
echo $fn(4);
"#,
        &[],
    );
    assert_backend_parity(
        "runtime_string_callable_direct_call_second",
        r#"<?php
function eir_runtime_pick_left(int $value): int {
    return $value + 1;
}
function eir_runtime_pick_right(int $value): int {
    return $value + 2;
}
$fn = $argc === 1 ? "eir_runtime_pick_left" : "eir_runtime_pick_right";
echo $fn(4);
"#,
        &["extra"],
    );
    assert_backend_parity(
        "runtime_string_callable_expr_call_first",
        r#"<?php
function eir_runtime_expr_left(int $value): int {
    return $value + 1;
}
function eir_runtime_expr_right(int $value): int {
    return $value + 2;
}
echo ($argc === 1 ? "eir_runtime_expr_left" : "eir_runtime_expr_right")(4);
"#,
        &[],
    );
    assert_backend_parity(
        "runtime_string_callable_expr_call_second",
        r#"<?php
function eir_runtime_expr_left(int $value): int {
    return $value + 1;
}
function eir_runtime_expr_right(int $value): int {
    return $value + 2;
}
echo ($argc === 1 ? "eir_runtime_expr_left" : "eir_runtime_expr_right")(4);
"#,
        &["extra"],
    );
    assert_backend_parity(
        "runtime_function_pipe_call_first",
        r#"<?php
function eir_runtime_pipe_left(int $value): int {
    return $value + 1;
}
function eir_runtime_pipe_right(int $value): int {
    return $value + 2;
}
$cb = $argc === 1 ? eir_runtime_pipe_left(...) : eir_runtime_pipe_right(...);
echo 4 |> $cb;
"#,
        &[],
    );
    assert_backend_parity(
        "runtime_function_pipe_call_second",
        r#"<?php
function eir_runtime_pipe_left(int $value): int {
    return $value + 1;
}
function eir_runtime_pipe_right(int $value): int {
    return $value + 2;
}
$cb = $argc === 1 ? eir_runtime_pipe_left(...) : eir_runtime_pipe_right(...);
echo 4 |> $cb;
"#,
        &["extra"],
    );
}

/// Verifies static method callback forms lower to the same direct calls as legacy codegen.
#[test]
fn parity_static_method_callable_dispatch() {
    assert_backend_parity(
        "static_method_call_user_func",
        r#"<?php
class EirStaticCallback {
    public static function hit(int $value): int {
        return $value + 1;
    }
    public static function join(string $left, string $right): string {
        return $left . ":" . $right;
    }
}
echo call_user_func(["EirStaticCallback", "hit"], 4);
echo "|";
echo call_user_func(EirStaticCallback::hit(...), 8);
echo "|";
echo call_user_func_array(["eirstaticcallback", "JOIN"], ["right" => "R", "left" => "L"]);
"#,
        &[],
    );
    assert_backend_parity(
        "static_method_array_expr_call",
        r#"<?php
class EirArrayExprCallback {
    public static function hit(int $value): int {
        return $value + 1;
    }
    public static function join(string $left, string $right): string {
        return $left . ":" . $right;
    }
}
echo (["EirArrayExprCallback", "hit"])(4);
echo "|";
echo ([EirArrayExprCallback::class, "join"])(left: "L", right: "R");
"#,
        &[],
    );
}

/// Verifies static `array_map()` callback forms over indexed literals match legacy output.
#[test]
fn parity_static_array_map_callbacks() {
    assert_backend_parity(
        "static_array_map_callbacks",
        r#"<?php
function eir_map_inc(int $value): int {
    return $value + 1;
}
class EirMapStatic {
    public static function bump(int $value): int {
        return $value + 2;
    }
}
$ints = array_map(eir_map_inc(...), [1, 2]);
echo $ints[0]; echo ":"; echo $ints[1];
echo "|";
$fn = eir_map_inc(...);
$more = array_map($fn, [3, 4]);
echo $more[0]; echo ":"; echo $more[1];
echo "|";
$len = strlen(...);
$sizes = array_map($len, ["a", "abcd"]);
echo $sizes[0]; echo ":"; echo $sizes[1];
echo "|";
$stat = array_map(EirMapStatic::bump(...), [5, 6]);
echo $stat[0]; echo ":"; echo $stat[1];
"#,
        &[],
    );
}

/// Verifies static `array_filter()` string callbacks use the runtime helper like legacy codegen.
#[test]
fn parity_static_array_filter_callbacks() {
    assert_backend_parity(
        "static_array_filter_callbacks",
        r#"<?php
function eir_filter_odd(int $value): bool {
    return ($value % 2) === 1;
}
function eir_filter_key(int $key): bool {
    return $key === 1;
}
function eir_filter_both(int $value, int $key): bool {
    return $value > 2 && $key < 3;
}
class EirFilterStatic {
    public static function odd(int $value): bool {
        return ($value % 2) === 1;
    }
    public static function key_one(int $key): bool {
        return $key === 1;
    }
    public static function both(int $value, int $key): bool {
        return $value >= 4 && $key < 2;
    }
}
$odd = array_filter([1, 2, 3, 4], "eir_filter_odd");
echo count($odd); echo ":"; echo $odd[0]; echo ":"; echo $odd[1];
echo "|";
$keyed = array_filter([7, 8, 9], "eir_filter_key", ARRAY_FILTER_USE_KEY);
echo count($keyed); echo ":"; echo $keyed[0];
echo "|";
$both = array_filter([1, 3, 4, 2], "eir_filter_both", ARRAY_FILTER_USE_BOTH);
echo count($both); echo ":"; echo $both[0]; echo ":"; echo $both[1];
echo "|";
$mode = ARRAY_FILTER_USE_KEY;
$dynamic = array_filter([10, 20, 30], "eir_filter_key", $mode);
echo count($dynamic); echo ":"; echo $dynamic[0];
echo "|";
$fcc = array_filter([1, 2, 3], eir_filter_odd(...));
echo count($fcc); echo ":"; echo $fcc[0]; echo ":"; echo $fcc[1];
echo "|";
$static = array_filter([5, 6, 7], EirFilterStatic::odd(...));
echo count($static); echo ":"; echo $static[0]; echo ":"; echo $static[1];
echo "|";
$staticKey = array_filter([7, 8, 9], EirFilterStatic::key_one(...), ARRAY_FILTER_USE_KEY);
echo count($staticKey); echo ":"; echo $staticKey[0];
echo "|";
$staticBoth = array_filter([1, 4, 5], EirFilterStatic::both(...), ARRAY_FILTER_USE_BOTH);
echo count($staticBoth); echo ":"; echo $staticBoth[0];
"#,
        &[],
    );
}

/// Verifies static `array_reduce()` callback forms over immediate indexed literals match legacy output.
#[test]
fn parity_static_array_reduce_callbacks() {
    assert_backend_parity(
        "static_array_reduce_callbacks",
        r#"<?php
function eir_reduce_add(int $carry, int $item): int {
    return $carry + $item;
}
class EirReduceStatic {
    public static function mul(int $carry, int $item): int {
        return $carry * $item;
    }
}
echo array_reduce([1, 2, 3], "eir_reduce_add", 0);
echo "|";
$fn = eir_reduce_add(...);
echo array_reduce([4, 5], $fn, 1);
echo "|";
$stat = EirReduceStatic::mul(...);
echo array_reduce([2, 3, 4], $stat, 1);
"#,
        &[],
    );
}

/// Verifies static `array_walk()` callback forms over immediate indexed literals match legacy output.
#[test]
fn parity_static_array_walk_callbacks() {
    assert_backend_parity(
        "static_array_walk_callbacks",
        r#"<?php
function eir_walk_show(int $value): void {
    echo $value;
}
class EirWalkStatic {
    public static function show(int $value): void {
        echo $value + 1;
    }
}
array_walk([1, 2], "eir_walk_show");
echo "|";
$fn = eir_walk_show(...);
array_walk([3, 4], $fn);
echo "|";
$stat = EirWalkStatic::show(...);
array_walk([5, 6], $stat);
"#,
        &[],
    );
}

/// Verifies static user-sort callback forms route through the same runtime helper as legacy.
#[test]
fn parity_static_user_sort_callbacks() {
    assert_backend_parity(
        "static_usort_callback",
        r#"<?php
function eir_sort_asc(int $left, int $right): int {
    return $left - $right;
}
$usorted = [5, 3, 1, 4, 2];
usort($usorted, "eir_sort_asc");
foreach ($usorted as $value) { echo $value; }
"#,
        &[],
    );
    assert_backend_parity(
        "static_uksort_callback",
        r#"<?php
function eir_sort_desc(int $left, int $right): int {
    return $right - $left;
}
$uksorted = [1, 3, 2];
uksort($uksorted, "eir_sort_desc");
foreach ($uksorted as $value) { echo $value; }
"#,
        &[],
    );
    assert_backend_parity(
        "static_uasort_callback",
        r#"<?php
function eir_sort_asc(int $left, int $right): int {
    return $left - $right;
}
$uasorted = [30, 10, 20];
uasort($uasorted, "eir_sort_asc");
foreach ($uasorted as $value) { echo $value . ":"; }
"#,
        &[],
    );
    assert_backend_parity(
        "static_usort_first_class_callback",
        r#"<?php
function eir_sort_asc(int $left, int $right): int {
    return $left - $right;
}
class EirSortStatic {
    public static function desc(int $left, int $right): int {
        return $right - $left;
    }
}
$usorted = [3, 1, 2];
usort($usorted, eir_sort_asc(...));
foreach ($usorted as $value) { echo $value; }
echo "|";
$static = [1, 3, 2];
usort($static, EirSortStatic::desc(...));
foreach ($static as $value) { echo $value; }
"#,
        &[],
    );
}

/// Verifies reflection attribute owner metadata matches the legacy backend.
#[test]
fn parity_reflection_owner_attributes() {
    assert_backend_parity(
        "reflection_owner_get_attributes",
        r#"<?php
class EirRoute {
    public function __construct(string $name) {
        echo "ctor:" . $name . ":";
    }
}
#[EirRoute("class")]
class EirReflectedController {
    #[EirRoute("method")]
    public function handle(): void {}

    #[EirRoute("property")]
    public int $id = 0;
}
$class = new ReflectionClass(EirReflectedController::class);
$classAttrs = $class->getAttributes();
$methodAttrs = (new ReflectionMethod(EirReflectedController::class, "handle"))->getAttributes();
$propertyAttrs = (new ReflectionProperty(EirReflectedController::class, "id"))->getAttributes();
echo $class->getName();
echo ":";
echo $classAttrs[0]->getName();
echo ":";
echo $classAttrs[0]->getArguments()[0];
echo ":";
$classAttrs[0]->newInstance();
$methodAttrs[0]->newInstance();
$propertyAttrs[0]->newInstance();
"#,
        &[],
    );
}

/// Verifies the supported `SplFileInfo` EIR slice matches the legacy backend.
#[test]
fn parity_spl_file_info_basics() {
    assert_backend_parity(
        "spl_file_info_basics",
        r#"<?php
$info = new SplFileInfo(".");
echo $info->getPathname();
echo ":";
echo $info->__toString();
echo ":";
echo ($info instanceof SplFileInfo) ? "C" : "x";
echo ($info instanceof Stringable) ? "I" : "x";
"#,
        &[],
    );
}

/// Verifies `SplFileInfo` path/stat helper methods match the legacy backend.
#[test]
fn parity_spl_file_info_path_stat_helpers() {
    assert_backend_parity(
        "spl_file_info_path_stat_helpers",
        r#"<?php
mkdir("docs");
file_put_contents("docs/a.txt", "one\ntwo\n");

$info = new SplFileInfo("docs/a.txt");
echo $info->getFilename();
echo "|";
echo $info->getExtension();
echo "|";
echo $info->getBasename(".txt");
echo "|";
echo $info->getPath();
echo "|";
echo $info->isFile() ? "file" : "no";
echo "|";
echo $info->getSize();

unlink("docs/a.txt");
rmdir("docs");
"#,
        &[],
    );
}

/// Verifies extended `SplFileInfo` stat/access/link helper methods match the legacy backend.
#[test]
fn parity_spl_file_info_extended_stat_helpers() {
    assert_backend_parity(
        "spl_file_info_extended_stat_helpers",
        r##"<?php
mkdir("docs");
file_put_contents("docs/a.txt", "one\n");
file_put_contents("docs/run.sh", "#!/bin/sh\n");
chmod("docs/run.sh", 0755);
symlink("a.txt", "docs/link.txt");

$file = new SplFileInfo("docs/a.txt");
$dir = new SplFileInfo("docs");
$exec = new SplFileInfo("docs/run.sh");
$link = new SplFileInfo("docs/link.txt");

echo ($file->getPerms() !== false) ? "P" : "x";
echo ($file->getInode() !== false) ? "I" : "x";
echo ($file->getOwner() !== false) ? "O" : "x";
echo ($file->getGroup() !== false) ? "G" : "x";
echo ($file->getATime() !== false) ? "A" : "x";
echo ($file->getMTime() > 0) ? "M" : "x";
echo ($file->getCTime() !== false) ? "C" : "x";
echo $file->getType();
echo ":";
echo $file->isWritable() ? "W" : "x";
echo $file->isWriteable() ? "w" : "x";
echo $file->isReadable() ? "R" : "x";
echo $exec->isExecutable() ? "X" : "x";
echo $dir->isDir() ? "D" : "x";
echo $link->isLink() ? "L" : "x";
echo ":";
echo $link->getLinkTarget();
echo ":";
echo ($file->getRealPath() === false) ? "x" : "P";

unlink("docs/link.txt");
unlink("docs/run.sh");
unlink("docs/a.txt");
rmdir("docs");
"##,
        &[],
    );
}

/// Verifies dynamic `SplFileInfo` factories match the legacy backend.
#[test]
fn parity_spl_file_info_dynamic_factories() {
    assert_backend_parity(
        "spl_file_info_dynamic_factories",
        r#"<?php
class EirInfo extends SplFileInfo {}

$info = new SplFileInfo(".");
$file = $info->getFileInfo();
$path = $info->getPathInfo();
$customFile = $info->getFileInfo("EirInfo");
$customPath = $info->getPathInfo("EirInfo");

echo ($file instanceof SplFileInfo) ? "F" : "x";
echo ":";
echo $file->getPathname();
echo ":";
echo ($path instanceof SplFileInfo) ? "P" : "x";
echo ":";
echo $path->getPathname();
echo ":";
echo ($customFile instanceof EirInfo) ? "E" : "x";
echo ":";
echo $customFile->getPathname();
echo ":";
echo ($customPath instanceof EirInfo) ? "Q" : "x";
echo ":";
echo $customPath->getPathname();
"#,
        &[],
    );
}

/// Verifies `setInfoClass()` stored factory overrides match the legacy backend.
#[test]
fn parity_spl_file_info_stored_info_class() {
    assert_backend_parity(
        "spl_file_info_stored_info_class",
        r#"<?php
class EirInfo extends SplFileInfo {}

$info = new SplFileInfo(".");
$info->setInfoClass(EirInfo::class);
$file = $info->getFileInfo();
$path = $info->getPathInfo();
echo ($file instanceof EirInfo) ? "F" : "x";
echo ":";
echo ($path instanceof EirInfo) ? "P" : "x";
"#,
        &[],
    );
}

/// Compiles and runs a PHP snippet through both backends and compares stdout.
fn assert_backend_parity(name: &str, source: &str, args: &[&str]) {
    let legacy = compile_and_run_backend(name, source, args, Backend::Legacy);
    let ir = compile_and_run_backend(name, source, args, Backend::Ir);
    assert_eq!(ir, legacy, "IR backend stdout differed from legacy for {name}");
}

/// Compiles a PHP snippet with one backend, runs the produced binary, and returns stdout.
fn compile_and_run_backend(name: &str, source: &str, args: &[&str], backend: Backend) -> String {
    let dir = temp_case_dir(name, backend);
    fs::create_dir_all(&dir).expect("failed to create parity test directory");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write parity PHP fixture");

    let mut compile_command = Command::new(elephc_cli_bin());
    compile_command
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir);
    backend.add_compile_flags(&mut compile_command);
    let compile = compile_command
        .arg(&php_path)
        .output()
        .expect("failed to run elephc CLI for parity fixture");
    assert!(
        compile.status.success(),
        "elephc {} backend failed for {name}: stderr={}",
        backend.name(),
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(dir.join("main"))
        .current_dir(&dir)
        .args(args)
        .output()
        .expect("failed to run parity binary");
    assert!(
        run.status.success(),
        "{} backend binary failed for {name}: stderr={}",
        backend.name(),
        String::from_utf8_lossy(&run.stderr)
    );

    let stdout = String::from_utf8(run.stdout).expect("parity binary stdout should be utf8");
    let _ = fs::remove_dir_all(&dir);
    stdout
}

/// Builds a unique temporary directory path for one backend run.
fn temp_case_dir(name: &str, backend: Backend) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "elephc_ir_parity_{}_{}_{}_{}",
        name,
        backend.name(),
        std::process::id(),
        id
    ))
}

/// Returns the path to the cargo-built `elephc` binary.
fn elephc_cli_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}
