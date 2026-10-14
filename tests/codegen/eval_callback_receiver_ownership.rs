//! Purpose:
//! Heap and destructor regressions for eval array-callback receiver ownership (#1755).
//!
//! Called from:
//! - The codegen suite through `codegen::eval_callback_receiver_ownership`.
//!
//! Key details:
//! - Source comes from argv to force runtime eval rather than literal AOT lowering.
//! - Each fragment checks full stdout, successful exit and a clean final heap.

use crate::support::compile_and_run_with_heap_debug_per_argv;

/// Compiles one opaque eval harness and runs each ownership scenario independently.
fn assert_receiver_cases(cases: &[(&str, &str)]) {
    let arguments = cases.iter().map(|(source, _)| vec![*source]).collect::<Vec<_>>();
    let argvs = arguments.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv("<?php eval($argv[1]);", &argvs);
    for (output, (source, expected)) in outputs.into_iter().zip(cases) {
        assert!(output.success, "{source}\n{}", output.stderr);
        assert_eq!(output.stdout, *expected, "{source}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{source}\n{}", output.stderr);
    }
}

/// Positional, named, argument-array and variable callbacks release every receiver read.
#[test]
fn test_eval_array_callback_receiver_routes() {
    assert_receiver_cases(&[
        (r#"class C { public function f($b="b", $a="a") { return $a.$b; } public function __destruct() { echo "drop\n"; } } $c = new C(); for($i=0;$i<10;$i++){call_user_func([$c,"f"], "B", "A");} unset($c); echo "DONE\n";"#, "drop\nDONE\n"),
        (r#"class C { public function f($b="b", $a="a") { return $a.$b; } public function __destruct() { echo "drop\n"; } } $c = new C(); for($i=0;$i<10;$i++){call_user_func(a:"A", callback:[$c,"f"], b:"B");} unset($c); echo "DONE\n";"#, "drop\nDONE\n"),
        (r#"class C { public function f($b="b", $a="a") { return $a.$b; } public function __destruct() { echo "drop\n"; } } $c = new C(); for($i=0;$i<10;$i++){call_user_func_array([$c,"f"], ["a"=>"A", "b"=>"B"]);} unset($c); echo "DONE\n";"#, "drop\nDONE\n"),
        (r#"class C { public function f($b="b", $a="a") { return $a.$b; } public function __destruct() { echo "drop\n"; } } $c = new C(); $cb=[$c,"f"]; for($i=0;$i<10;$i++){$cb("B", "A");} unset($cb); unset($c); echo "DONE\n";"#, "drop\nDONE\n"),
    ]);
}

/// Validation, argument binding and throwing bodies balance their receiver leases.
#[test]
fn test_eval_array_callback_receiver_errors() {
    assert_receiver_cases(&[
        (r#"class C { private function hidden() {} public function __destruct(){echo "drop\n";} } $c=new C(); for($i=0;$i<3;$i++){try{call_user_func([$c,"hidden"]);}catch(TypeError $e){echo "caught\n";}unset($e);} unset($c); echo "DONE\n";"#, "caught\ncaught\ncaught\ndrop\nDONE\n"),
        (r#"class C { public function f($x) {} public function __destruct(){echo "drop\n";} } $c=new C(); for($i=0;$i<3;$i++){try{call_user_func([$c,"missing"]);}catch(TypeError $e){echo "caught\n";}unset($e);} unset($c); echo "DONE\n";"#, "caught\ncaught\ncaught\ndrop\nDONE\n"),
        (r#"class C { public function f($x) { echo "unexpected\n"; } public function __destruct(){echo "drop\n";} } $c=new C(); for($i=0;$i<3;$i++){try{call_user_func([$c,"f"]);}catch(ArgumentCountError $e){echo "caught\n";}unset($e);} unset($c); echo "DONE\n";"#, "caught\ncaught\ncaught\ndrop\nDONE\n"),
        (r#"class C { public function f() { throw new Exception("stop"); } public function __destruct(){echo "drop\n";} } $c=new C(); for($i=0;$i<3;$i++){try{call_user_func([$c,"f"]);}catch(Exception $e){echo "caught\n";}unset($e);} unset($c); echo "DONE\n";"#, "caught\ncaught\ncaught\ndrop\nDONE\n"),
    ]);
}

/// References, return aliases and deleting arguments keep the receiver alive until the correct point.
#[test]
fn test_eval_array_callback_receiver_lifetimes() {
    assert_receiver_cases(&[
        (r#"class C { public function bump(&$n){$n++;} public function __destruct(){echo "drop\n";} } $c=new C(); $n=1; for($i=0;$i<10;$i++){call_user_func_array([$c,"bump"],["n"=>&$n]);} echo $n,"\n";unset($c);echo "DONE\n";"#, "11\ndrop\nDONE\n"),
        (r#"class C { public function f(){global $cb;$cb=null;echo "body\n";return $this;} public function __destruct(){echo "drop\n";} } $c=new C();$cb=[$c,"f"];unset($c);$result=$cb();echo "returned\n";unset($result);echo "DONE\n";"#, "body\nreturned\ndrop\nDONE\n"),
        (r#"class C {public function f($v){echo "body:",$v,"\n";}public function __destruct(){echo "drop\n";}} function argument(){global $cb;$cb=null;echo "arg\n";return 7;} $c=new C();$cb=[$c,"f"];unset($c);$cb(argument());echo "DONE\n";"#, "arg\nbody:7\ndrop\nDONE\n"),
        (r#"class C {public function f($v){echo "unexpected\n";}public function __destruct(){echo "drop\n";}} function argument(){global $cb;$cb=null;echo "arg\n";throw new Exception("stop");} $c=new C();$cb=[$c,"f"];unset($c);try{$cb(argument());}catch(Exception $e){echo "caught\n";}echo "DONE\n";"#, "arg\ndrop\ncaught\nDONE\n"),
        (r#"class C {public function f(){return 7;}public function __destruct(){echo "drop\n";}} $c=new C();$f=Closure::fromCallable([$c,"f"]);unset($c);echo $f(),"\n";unset($f);echo "DONE\n";"#, "7\ndrop\nDONE\n"),
    ]);
}
