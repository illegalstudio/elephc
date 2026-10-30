//! Purpose:
//! Native eval regressions for the lifetime of array-reference keys.
//!
//! Called from:
//! - `tests/codegen/eval.rs` through the executable codegen test harness.
//!
//! Key details:
//! - PHP source comes from argv, ensuring the interpreter bridge executes it.
//! - Output and clean heaps cover key snapshots, copies, nesting and binary strings.

use crate::support::compile_and_run_with_heap_debug_per_argv;

/// Array references retain PHP key values while retiring their temporary runtime cells.
#[test]
fn test_dynamic_eval_array_reference_keys_are_snapshotted_and_released() {
    let cases = [
        (r#"$v=[2]; $a=[&$v[0]]; unset($a); echo $v[0];"#, "2"),
        (r#"function offset() { echo "K"; return 0; }
$v=[2]; $a=[&$v[offset()]]; unset($a); echo $v[0];"#, "K2"),
        (r#"$v=["k"=>2]; $a=[&$v["k"]]; $a[0]=3; unset($a); echo $v["k"];"#, "3"),
        (r#"$v=[10,20]; $key=0; $refs=[&$v[$key]]; $key=1;
$pressure=str_repeat("x",300); $refs[0]=30; echo $v[0],":",$v[1];"#, "30:20"),
        (r#"$v=["00"=>10,0=>20]; $key="00"; $refs=[&$v[$key]]; $key="0";
$pressure=str_repeat("x",300); $refs[0]=30; echo $v["00"],":",$v[0];"#, "30:20"),
        (r#"$v=[[10,20]]; $key=0; $refs=[&$v[$key][$key]]; $key=1;
$refs[0]=30; echo $v[0][0],":",$v[0][1];"#, "30:20"),
        (r#"$key="\xff\0"; $v=[$key=>7]; $refs=[&$v[$key]]; $key="other";
$refs[0]=9; echo bin2hex("\xff\0"),":",$v["\xff\0"];"#, "ff00:9"),
        (r#"function update(&$value) { $value=30; }
$v=[10]; update($v[0]); echo $v[0];"#, "30"),
    ];
    let arguments = cases.iter().map(|(source, _)| vec![*source]).collect::<Vec<_>>();
    let argvs = arguments.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv("<?php eval($argv[1]);", &argvs);
    for (output, (source, expected)) in outputs.iter().zip(cases) {
        assert!(output.success, "{source}\n{}", output.stderr);
        assert_eq!(output.stdout, expected, "{source}");
        assert!(!output.stderr.contains("Warning:"), "{source}\n{}", output.stderr);
        assert!(output.stderr.contains("leak summary: clean"), "{source}\n{}", output.stderr);
    }
}

/// Repeated reference copies keep their source writable without accumulating key leases.
#[test]
fn test_dynamic_eval_array_reference_key_copies_remain_bounded() {
    let fragment = r#"function offset() { echo "K"; return 0; }
$sum=0;
for ($i=0; $i<40; $i++) {
    $v=[10]; $refs=[&$v[offset()]]; $copy=$refs; unset($refs);
    $copy[0]=30; $sum += $v[0]; unset($copy,$v);
    $pressure=str_repeat("x",300); unset($pressure);
}
echo "\n",$sum,"\n";"#;
    let outputs = compile_and_run_with_heap_debug_per_argv("<?php eval($argv[1]);", &[&[fragment]]);
    let output = &outputs[0];
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "K".repeat(40) + "\n1200\n");
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}
/// Resource offsets keep integer IDs through calls, mutation and stream closure.
/// Each fixture disables diagnostics to isolate key conversion from offset warnings.
#[test]
fn test_dynamic_eval_resource_array_reference_keys_preserve_integer_ids() {
    let cases = [
        (r#"error_reporting(0); function show($value){echo $value,"\n";} $h=fopen("php://memory","w+"); $a=[(int)$h=>"ok"]; show($a[$h]); fclose($h); unset($a,$h);"#, "ok\n"),
        (r#"error_reporting(0); function show($value){echo $value,"\n";} $h=fopen("php://memory","w+"); $a=[(int)$h=>["value"=>"nested"]]; show($a[$h]["value"]); fclose($h); unset($a,$h);"#, "nested\n"),
        (r#"error_reporting(0); function update(&$value,&$handle){fclose($handle);$handle=99;$value="after";} $h=fopen("php://memory","w+"); $id=(int)$h; $a=[$id=>"before"]; update($a[$h],$h); echo $a[$id],"|",$h,"\n"; unset($a,$h,$id);"#, "after|99\n"),
        (r#"error_reporting(0); $h=fopen("php://memory","w+"); $id=(int)$h; $a=[$id=>"before"]; $refs=[&$a[$h]]; fclose($h); unset($h); $refs[0]="after"; echo $a[$id],"\n"; unset($refs,$a,$id);"#, "after\n"),
    ];
    let arguments = cases.iter().map(|(source, _)| vec![*source]).collect::<Vec<_>>();
    let argvs = arguments.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv("<?php eval($argv[1]);", &argvs);
    for (output, (source, expected)) in outputs.iter().zip(cases) {
        assert!(output.success, "{source}\n{}", output.stderr);
        assert_eq!(output.stdout, expected, "{source}");
        assert!(!output.stderr.contains("Warning:"), "{source}\n{}", output.stderr);
        assert!(output.stderr.contains("leak summary: clean"), "{source}\n{}", output.stderr);
    }
}
