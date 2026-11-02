//! Purpose:
//! Verifies packed integer JSON prefixes, generic widening and consumer ownership.
//!
//! Called from:
//! - The JSON codegen integration suite on executable supported targets.
//!
//! Key details:
//! - Every PHP fixture uses runtime source and runs with IR optimization on/off and heap diagnostics.
//! - PHP output is checked independently when ELEPHC_PHP_CHECK is enabled.

use super::*;

/// Checks both optimizer modes and requires every parser/consumer owner to be retired.
fn check_packed(source: &str, expected: &str) {
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        assert_eq!(super::error_location::php_cli(source.strip_prefix("<?php").unwrap()).trim_end(),
            expected.trim_end(), "real PHP differs");
    }
    for off in [false, true] {
        let run = || {
            let output = compile_and_run_with_heap_debug(source);
            assert!(output.success, "{}", output.stderr);
            assert_eq!(output.stdout, expected, "{}", output.stderr);
            assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        };
        if off { without_ir_opt(run); } else { run(); }
    }
}

/// Preserves PHP values and clean ownership across grammar and bigint flags.
#[test]
fn test_json_packed_integer_grammar_and_bigint_flags() {
    check_packed(r#"<?php
$pad=$argc>0 ? " \t\r\n" : "";
foreach(['[0,-0,1,-1,9223372036854775807,-9223372036854775808]',
 '[1,2,3.5]', '[1,2,true,false,null,"x"]', '[[1,2],[3,-4]]',
 '[1,2,9223372036854775808]', '[1,2,-9223372036854775809]',
 '[1,2,1e2]', '[1,2,-0.0]', '[1,2,[]]'] as $token){
 foreach([0,JSON_BIGINT_AS_STRING] as $flags){
  $v=json_decode($pad.$token.$pad,true,512,$flags);
  echo json_encode($v,JSON_PRESERVE_ZERO_FRACTION),"|",json_last_error(),"\n";
 }
}
"#, r#"[0,0,1,-1,9223372036854775807,-9223372036854775808]|0
[0,0,1,-1,9223372036854775807,-9223372036854775808]|0
[1,2,3.5]|0
[1,2,3.5]|0
[1,2,true,false,null,"x"]|0
[1,2,true,false,null,"x"]|0
[[1,2],[3,-4]]|0
[[1,2],[3,-4]]|0
[1,2,9.223372036854776e+18]|0
[1,2,"9223372036854775808"]|0
[1,2,-9.223372036854776e+18]|0
[1,2,"-9223372036854775809"]|0
[1,2,100.0]|0
[1,2,100.0]|0
[1,2,-0.0]|0
[1,2,-0.0]|0
[1,2,[]]|0
[1,2,[]]|0
"#);
}

/// Preserves PHP values and clean ownership across cow references and consumers.
#[test]
fn test_json_packed_integer_cow_references_and_consumers() {
    check_packed(r#"<?php
$a=(array)json_decode(str_repeat('[3,1,2]',$argc),true);
$b=$a;$a[0]='x';echo json_encode($a),'|',json_encode($b),"\n";
foreach($b as &$v){$v+=10;}unset($v);sort($b);echo json_encode($b),"\n";
$c=(array)json_decode(str_repeat('[3,1,2]',$argc),true);
array_walk($c,function(&$v){$v+=1;});echo json_encode($c),"\n";
$d=array_map(function($v){return $v*2;},$c);
echo json_encode($d),'|',json_encode(array_filter($d,function($v){return $v>4;})),"\n";
echo serialize($c),'|',implode(',',array_slice($c,1)),"\n";
unset($c[1]);$c[]=9;echo json_encode($c),"\n";
$e=(array)json_decode(str_repeat('[3,1,2]',$argc),true);$original=$e;
$e['k']='v';echo json_encode($e),'|',json_encode($original),"\n";
$f=(array)json_decode(str_repeat('[3,1,2]',$argc),true);
echo json_encode(array_flip($f)),"\n";
echo array_pop($f),':',array_shift($f),"\n";
"#, r#"["x",1,2]|[3,1,2]
[11,12,13]
[4,2,3]
[8,4,6]|{"0":8,"2":6}
a:3:{i:0;i:4;i:1;i:2;i:2;i:3;}|2,3
{"0":4,"2":3,"3":9}
{"0":3,"1":1,"2":2,"k":"v"}|[3,1,2]
{"3":0,"1":1,"2":2}
2:3
"#);
}

/// Preserves PHP values and clean ownership across array return property and nested cow.
#[test]
fn test_json_packed_integer_array_return_property_and_nested_cow() {
    check_packed(r#"<?php
class Holder{public array $items=[];}
function load(string $s):array{return (array)json_decode($s,true);}
function change(array &$a):void{$a[0]='changed';}
$h=new Holder();$ref=&$h->items;$ref=load(str_repeat('[3,1,2]',$argc));
$copy=$h->items;change($ref);echo json_encode($h->items),'|',json_encode($copy),"\n";
$a=(array)json_decode(str_repeat('[[1,2],[3,4]]',$argc),true);$b=$a;
$a[0][1]='x';$a[1][]=5;echo json_encode($a),'|',json_encode($b),"\n";
$o=json_decode(str_repeat('{"items":[3,1,2]}',$argc));$a=$o->items;$a[0]='x';
echo json_encode($o),'|',json_encode($a),"\n";
"#, r#"["changed",1,2]|[3,1,2]
[[1,"x"],[3,4,5]]|[[1,2],[3,4]]
{"items":[3,1,2]}|["x",1,2]
"#);
}

/// Preserves PHP values and clean ownership across partial cleanup depth and syntax.
#[test]
fn test_json_packed_integer_partial_cleanup_depth_and_syntax() {
    check_packed(r#"<?php
$pad=$argc>0?" ":"";
foreach(['[0,1,2,3,4,5,6,7]','[0,1,2,3,4,5,6,7,null]',
 '[0,1,2,3,4,5,6,7,{"n":9}]','[0,1,2,3,4,]', '[0,1,2,3,4,01]',
 '[[1,2,3],]','[0,1,2,3,4,+5]','[0,1,2,3,4,1x]', '[0,1,2,3,4,-]'] as $token){
 $v=json_decode($pad.$token,true);echo json_last_error(),':',json_encode($v),"\n";
}
foreach(['[1,2]','[[1,2]]','[[[1,2]]]'] as $token){
 try{$v=json_decode($pad.$token,true,2,JSON_THROW_ON_ERROR);echo json_encode($v),"\n";}
 catch(JsonException $e){echo 'error:',$e->getCode(),"\n";}
}
"#, r#"0:[0,1,2,3,4,5,6,7]
0:[0,1,2,3,4,5,6,7,null]
0:[0,1,2,3,4,5,6,7,{"n":9}]
4:null
4:null
4:null
4:null
4:null
4:null
[1,2]
error:1
error:1
"#);
}

/// Mixed-boxed packed arrays survive runtime eval import, mutation and alias separation.
#[test]
fn test_json_packed_integer_eval_bridge() {
    check_packed(r#"<?php
$a=json_decode(str_repeat('[1,2,3]',$argc),true);$copy=$a;
$code=str_repeat('$a[0]=$a[0]+5;$a[]="x";',$argc);eval($code);
echo json_encode($a),'|',json_encode($copy);
"#, "[6,2,3,\"x\"]|[1,2,3]");
}

/// Cycle collection also releases packed leaves retained only by an unreachable object.
#[test]
fn test_json_packed_integer_cycle_leaf_ownership() {
    check_packed(r#"<?php
class PackedCycle {public mixed $link=null;public mixed $values=null;}
for($i=0;$i<3;$i++){
 $node=new PackedCycle();$node->values=json_decode(str_repeat('[1,2,3]',$argc),true);
 echo json_encode($node->values),'|';$node->link=$node;unset($node);gc_collect_cycles();
}
$node=new PackedCycle();$node->values=json_decode(str_repeat('[[1,2],[3,4]]',$argc),true);
echo json_encode($node->values),'|';$node->link=$node;unset($node);gc_collect_cycles();
$outside=(array)json_decode(str_repeat('[4,5,6]',$argc),true);
$node=new PackedCycle();$node->values=$outside;$node->link=$node;unset($node);gc_collect_cycles();
echo json_encode($outside);unset($outside);
"#, "[1,2,3]|[1,2,3]|[1,2,3]|[[1,2],[3,4]]|[4,5,6]");
}
