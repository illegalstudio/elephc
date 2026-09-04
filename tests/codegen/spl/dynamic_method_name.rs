//! Purpose:
//! Integration tests for `$object->$name()` on a BUILTIN class, where the method to call is
//! decided at run time.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - MEASURED: `foreach (["getFilename", "getSize"] as $call) { $info->$call(); }` died with
//!   `Fatal error: callable array did not resolve to an invokable target` where php answers both.
//!   The same loop worked as soon as each name had ALSO been called statically somewhere in the
//!   program — which is what said the dispatch ladder is bounded by the bodies that were EMITTED,
//!   not by the names the program mentions.
//! - A dynamic call carries no method name in its instruction, so the discovery pass that lowers
//!   builtin SPL method bodies on demand had nothing to read. The widening is bounded by the
//!   classes the program CONSTRUCTS: a program with no dynamic invoke pays nothing.
//! - A USER class was never affected — its methods are lowered because the class is declared.
//!   That asymmetry is what made this look like a dispatch bug rather than a missing body.

use crate::support::*;

/// Verifies a run-time method name reaches a builtin class's implementation.
#[test]
fn a_runtime_method_name_reaches_a_builtin_classs_body() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("f.txt", "1234");
$i = new SplFileInfo("f.txt");
foreach (["getFilename", "getSize", "getExtension"] as $call) {
    echo $call, "=", var_export($i->$call(), true), ";";
}
echo "\n";
$a = new ArrayObject([1, 2, 3]);
foreach (["count", "getArrayCopy"] as $call) {
    echo $call, "=", json_encode($a->$call()), ";";
}
echo "\n";
"#,
    );
    assert_eq!(
        out,
        "getFilename='f.txt';getSize=4;getExtension='txt';\n\
         count=3;getArrayCopy=[1,2,3];\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies the same for a USER class, which has always worked and must keep working.
///
/// It shares the ladder with the builtin case, so widening one must not disturb the other.
#[test]
fn a_runtime_method_name_still_reaches_a_user_class() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
class Box {
    public function one(): string { return "1"; }
    public function two(): string { return "2"; }
}
$b = new Box();
foreach (["one", "two"] as $name) { echo $b->$name(); }
echo "|";
foreach (["two", "one"] as $name) { echo call_user_func([$b, $name]); }
echo "\n";
"#,
    );
    assert_eq!(out, "12|21\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies a virtual call through a PARENT-typed receiver reaches the subclass's own body.
///
/// The discovery pass climbs from the receiver's static type to the class that DECLARES the
/// method, which is the right answer for the type and the wrong one for the object in it.
/// MEASURED: `function walk(SplFileObject $o) { $o->eof(); }` called with an `SplTempFileObject`
/// SEGFAULTED on a null vtable slot, and adding an unrelated `$temp->eof()` elsewhere in the same
/// program made it work — the gap was EMISSION, not dispatch.
#[test]
fn a_call_through_a_parent_type_reaches_the_subclasss_body() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function ask(SplFileObject $o): string {
    return var_export($o->eof(), true) . "/" . $o->getExtension();
}
$t = new SplTempFileObject();
$t->fwrite("a\n");
echo ask($t), "|";
file_put_contents("p.txt", "a\n");
echo ask(new SplFileObject("p.txt")), "\n";
"#,
    );
    assert_eq!(out, "false/|false/txt\n");
    let _ = std::fs::remove_dir_all(dir);
}
