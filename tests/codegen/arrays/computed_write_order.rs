//! Purpose:
//! Regression tests for computed keys and late receiver fetches in array writes (#1653).
//!
//! Called from:
//! - The arrays integration suite in `codegen_tests`.
//!
//! Key details:
//! - Computed keys precede the RHS; bare variable keys are read at store time.
//! - Temporary key owners survive receiver replacement and unwind on exceptions.

use crate::support::*;

/// Checks the PHP result and balanced ownership for one write-order witness.
fn assert_order_and_heap(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
}

/// Freezes computed compound keys before the RHS while bare variables remain late.
#[test]
fn test_compound_expression_keys() {
    assert_order_and_heap(
        r#"<?php
$a=[10,20];$k=0;$v=($a[$k+0]+=($k=1));echo $v,"|",json_encode($a),"\n";
class C {public array $a=[10,20];} $o=new C();$k=0;$v=($o->a[$k+0]+=($k=1));echo $v,"|",json_encode($o->a),"\n";
class S {public static array $a=[10,20];} $k=0;$v=(S::$a[$k+0]+=($k=1));echo $v,"|",json_encode(S::$a),"\n";
$a=[10,20];$k=0;$v=($a[$k]+=($k=1));echo $v,"|",json_encode($a),"\n";
"#,
        "11|[11,20]\n11|[11,20]\n11|[11,20]\n21|[10,21]\n",
    );
}

/// Keeps computed keys early and releases the boxed numeric result after publication.
#[test]
fn test_compound_receiver_reassignment() {
    assert_order_and_heap(
        r#"<?php
function keyname():int {echo "[key]";return 0;}
$a=[10];$a[keyname()]+=(($a=[100])?1:0);echo json_encode($a),"\n";
class C {public array $a=[10];public static array $s=[10];}
$o=new C();$o->a[keyname()]+=(($o->a=[100])?1:0);echo json_encode($o->a),"\n";
C::$s[keyname()]+=((C::$s=[100])?1:0);echo json_encode(C::$s),"\n";
"#,
        "[key][101]\n[key][101]\n[key][101]\n",
    );
}

/// Evaluates computed keys left to right and reads a bare key after root reassignment.
#[test]
fn test_nested_computed_keys() {
    assert_order_and_heap(
        r#"<?php
function keyname(string $s,int $n):string {echo "[",$s,"]";return str_repeat($s,$n);}
function nested(mixed $m,int $n):string {$i=0;$m[keyname("a",$n)][keyname("b",$n)][$i]=($m=["seed"=>2])?($i=1):0;return json_encode($m);}
echo nested([],$argc),"\n";
"#,
        "[a][b]{\"seed\":2,\"a\":{\"b\":{\"1\":1}}}\n",
    );
}

/// Fetches instance and static nested receivers after the right-hand side.
#[test]
fn test_nested_property_receivers() {
    assert_order_and_heap(
        r#"<?php
function keyname():string {echo "[key]";return "a";}
class C {public mixed $m=[];public static mixed $s=[];}
$o=new C();$o->m[keyname()]["b"]=($o->m=["a"=>["c"=>1]])?5:6;echo json_encode($o->m),"\n";
C::$s[keyname()]["b"]=(C::$s=["a"=>["c"=>2]])?7:8;echo json_encode(C::$s),"\n";
"#,
        "[key]{\"a\":{\"c\":1,\"b\":5}}\n[key]{\"a\":{\"c\":2,\"b\":7}}\n",
    );
}

/// Does not autovivify the parent before evaluating a right-hand side observer.
#[test]
fn test_nested_rhs_observer() {
    assert_order_and_heap(
        r#"<?php
function keyname(): string { echo "[key]"; return "a"; }
function value(): int { global $m; echo "[value:", json_encode($m), "]"; return 5; }
$m = []; $m[keyname()]["b"] = value(); echo json_encode($m), "\n";
"#,
        "[key][value:[]]{\"a\":{\"b\":5}}\n",
    );
}

/// Pins a borrowed computed key across replacement of the array that owned it.
#[test]
fn test_nested_borrowed_key() {
    assert_order_and_heap(
        r#"<?php
function nested(mixed $m): string {
 $m[$m["key"]]["b"] = ($m = ["a" => ["c" => 1]]) ? 5 : 6;
 return json_encode($m);
}
echo nested(["key" => "a"]), "\n";
"#,
        "{\"a\":{\"c\":1,\"b\":5}}\n",
    );
}

/// Retires the owned computed key when the value throws before the receiver fetch.
#[test]
fn test_nested_rhs_throw() {
    assert_order_and_heap(
        r#"<?php
function keyname(int $n): string { return str_repeat("a", 32+$n); }
function value(): int { throw new RuntimeException("stop"); }
function nested(mixed $m,int $n): string {
 try { $m[keyname($n)]["b"] = value(); } catch (RuntimeException $e) { echo "caught|"; }
 return json_encode($m);
}
echo nested(["seed" => 1],$argc), "\n";
"#,
        "caught|{\"seed\":1}\n",
    );
}

/// Unwinds an earlier owned key when a later computed key throws.
#[test]
fn test_nested_key_throw() {
    assert_order_and_heap(
        r#"<?php
function first(int $n):string {return str_repeat("a",32+$n);}
function second():string {throw new RuntimeException("stop");}
function nested(mixed $m,int $n):string {try {$m[first($n)][second()]=5;}catch(RuntimeException $e){echo "caught|";}return json_encode($m);}
echo nested(["seed"=>1],$argc),"\n";
"#,
        "caught|{\"seed\":1}\n",
    );
}
