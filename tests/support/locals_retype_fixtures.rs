//! Purpose:
//! PHP sources of the `codegen::locals_retype` end-to-end fixtures that claim the boxed
//! mixed-storage path, shared with the checker-side marking meta-test (issue #787).
//! Straight-line retypes and the silent capture fixture share their sources here as well.
//!
//! Called from:
//! - `tests/codegen/locals_retype.rs`, which compiles and runs each source.
//! - `tests/error_tests/type_system.rs`
//!   (`test_every_lowering_fixture_takes_the_mixed_storage_path`), which asserts the checker
//!   really marked each one.
//!
//! Key details:
//! - Included through `#[path]` by both test binaries, so it holds plain `&str` data only.
//! - The meta-test iterates [`MIXED_STORAGE_FIXTURES`]: a new mixed-storage fixture joins that
//!   list, and an edit to a source here reaches the end-to-end run and the marking check at once.
//!   The two sides used to carry hand-copied text that nothing kept in step.

/// An `if`/`else` storing an int on one arm and a string on the other.
pub const BRANCH_DIVERGENT_LOCAL: &str = "<?php if ($argc > 1) { $a = 0; } else { $a = \"ciao\"; } echo $a;";

/// A one-armed `if` that retypes the local; its condition is always true from the CLI.
pub const SINGLE_BRANCH_RETYPE_TAKEN: &str = "<?php $a = 41; if ($argc > 0) { $a = \"ciao\"; } echo $a;";

/// A one-armed `if` that retypes the local only when the program gets five arguments.
pub const SINGLE_BRANCH_RETYPE_NOT_TAKEN: &str = "<?php $a = 41; if ($argc > 5) { $a = \"ciao\"; } echo $a;";

/// A loop that re-stores a fresh concatenated string every iteration.
pub const LOOP_CARRIED_HETEROGENEOUS_LOCAL: &str = "<?php $a = 0; for ($i = 0; $i < $argc; $i++) { $a = \"s\" . $i; } echo $a;";

/// The divergent local read by `strlen`, whose checked lowering demands one operand shape.
pub const BRANCH_DIVERGENT_LOCAL_INTO_A_CHECKED_BUILTIN: &str = "<?php if ($argc > 1) { $a = 42; } else { $a = \"hello\"; } echo strlen($a);";

/// The divergent local read by several builtin and inspection surfaces.
pub const BRANCH_DIVERGENT_LOCAL_THROUGH_SEVERAL_BUILTINS: &str = r#"<?php
if ($argc > 1) { $a = 42; } else { $a = "hello"; }
echo strlen($a), "|", strtoupper($a), "|", gettype($a), "|";
var_dump(is_string($a));"#;

/// An int entry binding a loop body may overwrite with a string.
pub const ZERO_TRIP_LOOP_KEEPS_THE_ENTRY_BINDING: &str = "<?php $a = 123456789; for ($i = 1; $i < $argc; $i++) { $a = \"s\"; } var_dump($a);";

/// A divergent top-level local another function writes through `global $a`.
pub const WRITTEN_THROUGH_A_FUNCTION_GLOBAL_ALIAS: &str = r#"<?php
function q() { global $a; $a = 42; }
if ($argc > 1) { $a = 0; } else { $a = "hello"; }
echo $a, "|";
q();
echo $a, "|";
var_dump($a);"#;

/// The same cross-body write from a method.
pub const WRITTEN_THROUGH_A_METHOD_GLOBAL_ALIAS: &str = r#"<?php
class W { public function w() { global $a; $a = 42; } }
if ($argc > 1) { $a = 0; } else { $a = "hello"; }
echo $a, "|";
(new W())->w();
echo $a, "|";
var_dump($a);"#;

/// The loop-carried local with extra iterations, for the heap-debug fixture.
pub const LOOP_CARRIED_HEAP_STRINGS: &str = "<?php $a = 0; for ($i = 0; $i < $argc + 3; $i++) { $a = \"s\" . $i; } echo $a;";

/// A divergent local holding a heap string on one arm and a raw int on the other.
pub const BRANCH_DIVERGENT_HEAP_STRING: &str = "<?php if ($argc > 1) { $a = 42; } else { $a = \"hello\" . $argc; } echo $a;";

/// A divergent local captured by value by a closure that overwrites its own copy.
pub const CAPTURED_BY_VALUE_AND_OVERWRITTEN_IN_A_CLOSURE: &str = r#"<?php
if ($argc > 1) { $m = 1; } else { $m = "z"; }
$f = function (int $n) use ($m) {
    if ($n > 1) { $m = 0; } else { $m = "s"; }
    return $m;
};
var_dump($f($argc));
$g = function () use ($m) { return $m; };
var_dump($g());"#;

/// A divergent top-level local a function reads back through `global $a`.
pub const READ_BACK_THROUGH_A_GLOBAL_ALIAS: &str = r#"<?php
function q() { global $a; var_dump($a); }
if ($argc > 1) { $a = 0; } else { $a = "hello"; }
q();
$a = 42;
q();"#;

/// A divergent top-level local a closure writes through `global $a`.
pub const CLOSURE_DECLARED_GLOBAL: &str = r#"<?php
$w = function () { global $a; $a = 42; };
if ($argc > 1) { $a = 0; } else { $a = "hello"; }
echo $a, "|";
$w();
echo $a, "|";"#;

/// A divergent local re-assigned to a literal, then read by `strlen`.
pub const REASSIGNED_TO_A_LITERAL_INTO_A_CHECKED_BUILTIN: &str = r#"<?php
if ($argc > 1) { $a = 42; } else { $a = "hello"; }
$a = 99;
echo strlen($a);"#;

/// The same re-assigned local read by the builtins whose lowering failed in other ways.
pub const REASSIGNED_TO_A_LITERAL_THROUGH_SEVERAL_BUILTINS: &str = r#"<?php
if ($argc > 1) { $a = 42; } else { $a = "hello"; }
$a = 99;
echo str_repeat($a, 2), "|", strlen($a), "|", strtoupper($a), "|", gettype($a), "|";
var_dump($a);"#;

/// The concat-widened form of the re-assigned shape.
pub const CONCAT_MARKED_REASSIGNED_TO_A_LITERAL: &str = r#"<?php
$a = 0;
if ($argc > 1) { $a = "s" . $argc; }
$a = 5;
echo strlen($a), "|", strtoupper($a), "|";
var_dump($a);"#;

/// The re-assigned shape inside a function body and through non-builtin consumers.
pub const REASSIGNED_TO_A_LITERAL_INSIDE_A_FUNCTION_BODY: &str = r#"<?php
function f(int $n) {
    if ($n > 1) { $a = 42; } else { $a = "hello"; }
    $a = 7;
    return strlen($a) . "|" . strtoupper($a) . "|" . gettype($a);
}
echo f($argc), "\n";
if ($argc > 1) { $c = 1; } else { $c = "x"; }
$c = 3;
switch ($c) { case 3: echo "three|"; break; default: echo "other|"; }
echo ($c == 3 ? "eq" : "ne"), "|", $c + 1, "|";
var_dump($c);"#;

/// A marked store in the `default` arm of a fallthrough single-case `switch`.
pub const STORED_IN_A_FALLTHROUGH_SWITCH_DEFAULT: &str = "<?php $a = 0; switch ($argc) { case 1: echo \"one|\"; default: $a = \"ciao\" . $argc; } echo $a, \"|\"; var_dump($a);";

/// Every fixture above, in the order the marking meta-test checks them.
pub const MIXED_STORAGE_FIXTURES: &[&str] = &[
    BRANCH_DIVERGENT_LOCAL,
    SINGLE_BRANCH_RETYPE_TAKEN,
    SINGLE_BRANCH_RETYPE_NOT_TAKEN,
    LOOP_CARRIED_HETEROGENEOUS_LOCAL,
    BRANCH_DIVERGENT_LOCAL_INTO_A_CHECKED_BUILTIN,
    BRANCH_DIVERGENT_LOCAL_THROUGH_SEVERAL_BUILTINS,
    ZERO_TRIP_LOOP_KEEPS_THE_ENTRY_BINDING,
    WRITTEN_THROUGH_A_FUNCTION_GLOBAL_ALIAS,
    WRITTEN_THROUGH_A_METHOD_GLOBAL_ALIAS,
    LOOP_CARRIED_HEAP_STRINGS,
    BRANCH_DIVERGENT_HEAP_STRING,
    CAPTURED_BY_VALUE_AND_OVERWRITTEN_IN_A_CLOSURE,
    READ_BACK_THROUGH_A_GLOBAL_ALIAS,
    CLOSURE_DECLARED_GLOBAL,
    REASSIGNED_TO_A_LITERAL_INTO_A_CHECKED_BUILTIN,
    REASSIGNED_TO_A_LITERAL_THROUGH_SEVERAL_BUILTINS,
    CONCAT_MARKED_REASSIGNED_TO_A_LITERAL,
    REASSIGNED_TO_A_LITERAL_INSIDE_A_FUNCTION_BODY,
    STORED_IN_A_FALLTHROUGH_SWITCH_DEFAULT,
];

/// Shared source for the int to string retype regression.
pub const RETYPE_INT_TO_STRING: &str = "<?php $a = $argc; $a = \"ciao\"; echo $a;";

/// Shared source for the string to int retype regression.
pub const RETYPE_STRING_TO_INT: &str = "<?php $a = \"ciao\" . $argc; $a = 7; echo $a;";

/// Shared source for the rhs reads old value retype regression.
pub const RETYPE_RHS_READS_OLD_VALUE: &str = "<?php $a = $argc; $a = \"n=\" . $a; echo $a;";

/// Shared source for the after loop retype regression.
pub const RETYPE_AFTER_LOOP: &str = "<?php $a = \"x\"; for ($i = 0; $i < $argc; $i++) { $a .= \"y\"; } $a = 7; echo $a;";

/// Shared source for the capture before retype retype regression.
pub const RETYPE_CAPTURE_BEFORE_RETYPE: &str = "<?php $a = $argc; $f = function() use ($a) { return $a; }; $a = \"x\"; echo $f() . $a;";

/// Shared source for the constant retype retype regression.
pub const RETYPE_CONSTANT_RETYPE: &str = "<?php $a = 3; $a = \"ciao\"; echo $a;";

/// Shared source for the after conditional unset retype regression.
pub const RETYPE_AFTER_CONDITIONAL_UNSET: &str = "<?php $a = \"s\" . $argc; if ($argc > 1) { unset($a); } $a = 7; echo $a;";

/// Shared source for the compound assign retype retype regression.
pub const RETYPE_COMPOUND_ASSIGN_RETYPE: &str = "<?php $x = $argc; $x .= \"a\"; echo $x;";

/// Shared source for the array to string retype regression.
pub const RETYPE_ARRAY_TO_STRING: &str = "<?php $a = [1, $argc]; $a = \"str\" . $argc; echo $a;";

/// Shared source for the object to string retype regression.
pub const RETYPE_OBJECT_TO_STRING: &str = "<?php\nclass Box {\n    public int $v;\n    public function __construct(int $v) { $this->v = $v; }\n    public function __destruct() { echo \"bye|\"; }\n}\n$o = new Box($argc);\necho $o->v, \"|\";\n$o = \"gone\" . $argc;\necho $o;";

/// Shared source for the scalar to object retype regression.
pub const RETYPE_SCALAR_TO_OBJECT: &str = "<?php\nclass Box {\n    public int $v;\n    public function __construct(int $v) { $this->v = $v; }\n    public function __destruct() { echo \"bye|\"; }\n}\n$x = $argc;\necho $x, \"|\";\n$x = new Box($argc);\necho $x->v;";

/// Shared source for the in function body retype regression.
pub const RETYPE_IN_FUNCTION_BODY: &str = "<?php\nfunction probe(int $n): string {\n    $a = $n;\n    $a = \"ciao\" . $n;\n    return $a;\n}\necho probe($argc);";

/// Shared source for the object and array epilogue retype regression.
pub const RETYPE_OBJECT_AND_ARRAY_EPILOGUE: &str = "<?php\nclass Box {\n    public int $v;\n    public function __construct(int $v) { $this->v = $v; }\n    public function __destruct() { echo \"bye|\"; }\n}\nfunction probe(int $n): string {\n    $o = new Box($n);\n    $arr = [1, $n];\n    echo $o->v, \"|\", $arr[1], \"|\";\n    $o = \"s\" . $n;\n    $arr = \"t\" . $n;\n    return $o . $arr;\n}\necho probe($argc);";

/// Shared source for the by value parameter retype regression.
pub const RETYPE_BY_VALUE_PARAMETER: &str = "<?php\nfunction probe($a, int $n): string {\n    $a = \"grown\" . $n;\n    return $a;\n}\necho probe([1, 2], $argc);";

/// Shared source for the two retype tail sinking retype regression.
pub const RETYPE_TWO_RETYPE_TAIL_SINKING: &str = "<?php $q = \"a\" . $argc; if ($argc > 5) { echo \"x\"; } echo $q; $q = 1; $q = \"s\"; echo \"|\", $q;";

/// Shared source for the if tail sinking retype regression.
pub const RETYPE_IF_TAIL_SINKING: &str = "<?php $q = \"a\" . $argc; if ($argc > 5) { echo \"x\"; } echo $q; $q = 1; echo \"|\", $q;";

/// Shared source for the global after retype retype regression.
pub const RETYPE_GLOBAL_AFTER_RETYPE: &str = "<?php function w() { global $a; $a = 5; } $a = \"x\"; $a = 2; w(); echo $a;";

/// Shared source for the retype and array element retype regression.
pub const RETYPE_RETYPE_AND_ARRAY_ELEMENT: &str = "<?php\n$a = [1, $argc];\n$b = $argc;\n$b = $argc > 0 ? \"yes\" : \"no\";\n$a[0] = \"s\";\necho $b, \"|\", $a[0], \"|\", $a[1];";

/// Shared source for the copy across tail sinking retype regression.
pub const RETYPE_COPY_ACROSS_TAIL_SINKING: &str = "<?php\nfunction probe(int $n): string {\n    $q = \"a\" . $n;\n    if ($n > 5) { echo \"x\"; }\n    $r = $q;\n    $q = 1;\n    return $r . \"|\" . $q;\n}\necho probe($argc);";

/// Shared source for the int to heap string retype regression.
pub const RETYPE_INT_TO_HEAP_STRING: &str = "<?php $a = $argc; $a = \"ciao\" . $argc; echo strlen($a), \"|\", $a;";

/// Shared source for the rhs reads old heap value retype regression.
pub const RETYPE_RHS_READS_OLD_HEAP_VALUE: &str = "<?php $a = \"n\" . $argc; $a = strlen($a); echo $a;";

/// Shared source for the new value contains old retype regression.
pub const RETYPE_NEW_VALUE_CONTAINS_OLD: &str = "<?php $a = \"s\" . $argc; $a = [$a]; echo $a[0];";

/// Shared source for the if else tail sinking retype regression.
pub const RETYPE_IF_ELSE_TAIL_SINKING: &str = "<?php $q = \"a\" . $argc; if ($argc > 5) { echo \"x\"; } else { echo \"y\"; } echo $q; $q = 1; echo \"|\", $q;";

/// Shared source for the switch tail sinking retype regression.
pub const RETYPE_SWITCH_TAIL_SINKING: &str = "<?php $q = \"a\" . $argc; switch ($argc) { case 9: echo \"x\"; break; default: echo \"y\"; } echo $q; $q = 1; echo \"|\", $q;";

/// Shared source for the try tail sinking retype regression.
pub const RETYPE_TRY_TAIL_SINKING: &str = "<?php $q = \"a\" . $argc; try { echo \"t\"; } catch (Exception $e) { echo \"c\"; } echo $q; $q = 1; echo \"|\", $q;";

/// Shared source for the scalar tail sinking retype regression.
pub const RETYPE_SCALAR_TAIL_SINKING: &str = "<?php $n = $argc; if ($argc > 5) { echo \"x\"; } echo $n; $n = \"s\" . $argc; echo \"|\", $n;";

/// Shared source for the without prior read retype regression.
pub const RETYPE_WITHOUT_PRIOR_READ: &str = "<?php $q = \"a\" . $argc; if ($argc > 5) { echo \"x\"; } $q = 1; echo \"|\", $q;";

/// Shared source for the foreach target retype retype regression.
pub const RETYPE_FOREACH_TARGET_RETYPE: &str = "<?php $v = $argc; $arr = [1, 2, 3]; foreach ($arr as $v) { } $v = \"ciao\" . $argc; echo $v;";

/// Straight-line retypes checked for explicit retype sites and warnings before lowering.
pub const RETYPE_FIXTURES: &[&str] = &[
    RETYPE_INT_TO_STRING,
    RETYPE_STRING_TO_INT,
    RETYPE_RHS_READS_OLD_VALUE,
    RETYPE_AFTER_LOOP,
    RETYPE_CAPTURE_BEFORE_RETYPE,
    RETYPE_CONSTANT_RETYPE,
    RETYPE_AFTER_CONDITIONAL_UNSET,
    RETYPE_COMPOUND_ASSIGN_RETYPE,
    RETYPE_ARRAY_TO_STRING,
    RETYPE_OBJECT_TO_STRING,
    RETYPE_SCALAR_TO_OBJECT,
    RETYPE_IN_FUNCTION_BODY,
    RETYPE_OBJECT_AND_ARRAY_EPILOGUE,
    RETYPE_BY_VALUE_PARAMETER,
    RETYPE_TWO_RETYPE_TAIL_SINKING,
    RETYPE_IF_TAIL_SINKING,
    RETYPE_GLOBAL_AFTER_RETYPE,
    RETYPE_RETYPE_AND_ARRAY_ELEMENT,
    RETYPE_COPY_ACROSS_TAIL_SINKING,
    RETYPE_INT_TO_HEAP_STRING,
    RETYPE_RHS_READS_OLD_HEAP_VALUE,
    RETYPE_NEW_VALUE_CONTAINS_OLD,
    RETYPE_IF_ELSE_TAIL_SINKING,
    RETYPE_SWITCH_TAIL_SINKING,
    RETYPE_TRY_TAIL_SINKING,
    RETYPE_SCALAR_TAIL_SINKING,
    RETYPE_WITHOUT_PRIOR_READ,
    RETYPE_FOREACH_TARGET_RETYPE,
];

/// A closure silently boxes its captured local without marking the outer ternary binding.
pub const SILENT_MIXED_CAPTURE: &str = "<?php\n$m = $argc > 1 ? 1 : \"z\";\n$f = function (int $n) use ($m) { if ($n > 1) { $m = 0; } else { $m = \"s\"; } return $m; };\nvar_dump($f($argc));\n$g = function () use ($m) { return $m; };\nvar_dump($g());";
