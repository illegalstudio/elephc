//! Purpose:
//! End-to-end coverage for the count-only `count(json_decode($s, true))` sink.
//!
//! Called from:
//! - The codegen optimizer suite through `optimizer`.
//!
//! Key details:
//! - Local, borrowed and temporary sources are folded at the decode call itself; the
//!   source owner stays alive through the retargeted call and the separate count is gone.
//! - Scalar and invalid roots raise PHP's exact `count()` TypeError; escaped, numeric and
//!   oversized object keys fall back to the decoder and still count correctly.

use crate::support::*;

/// Fast counts, fallback keys and exact TypeErrors all match PHP and stay heap-clean.
#[test]
fn json_decode_count_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
echo 'temp:', count(json_decode(json_encode(['a' => 1, 'b' => 2]), true)), '|';
function run_count($json, $label) {
    try { echo $label, ':', count(json_decode($json, true)), '|'; }
    catch (TypeError $e) { echo $label, ':', $e->getMessage(), '|'; }
}
$obj = json_encode(['a' => 1, 'b' => [1, 2, 3], 'c' => 'text']);
run_count($obj, 'obj');
$arr = '[1,[2,3],{"a":4}]';
run_count($arr, 'arr');
$empty_arr = '[]';
run_count($empty_arr, 'empty_arr');
$empty_obj = '{}';
run_count($empty_obj, 'empty_obj');
$ws = '  { "a" : 1 , "b" : 2 }  ';
run_count($ws, 'ws');
$esc_value = '{"a":"x\\"y","b":"[1,2]"}';
run_count($esc_value, 'esc_value');
$esc_key = '{"a\\u0062":1,"x":2}';
run_count($esc_key, 'esc_key');
$dup = '{"a":1,"a":2,"b":3}';
run_count($dup, 'dup');
$num_key = '{"1":1,"01":2}';
run_count($num_key, 'num_key');
$utf8 = '{"é":1,"a":2}';
run_count($utf8, 'utf8');
$bad = '{';
run_count($bad, 'bad');
$int = '42';
run_count($int, 'int');
$float = '1.5';
run_count($float, 'float');
$big = '9223372036854775808';
run_count($big, 'big');
$str = '"x"';
run_count($str, 'str');
$true = 'true';
run_count($true, 'true');
$false = 'false';
run_count($false, 'false');
$null = 'null';
run_count($null, 'null');
$parts = [];
for ($i = 0; $i < 100; $i++) { $parts[] = '"k' . $i . '":1'; }
$large = '{' . implode(',', $parts) . '}';
run_count($large, 'large');
$loop = json_encode(['a' => 1, 'b' => [1, 2, 3], 'c' => 'text']);
$iters = $argc + 100;
$n = 0;
for ($i = 0; $i < $iters; $i++) { $n ^= count(json_decode($loop, true)); }
echo $n, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    let prefix = "count(): Argument #1 ($value) must be of type Countable|array, ";
    let expected = format!(
        "temp:2|obj:3|arr:3|empty_arr:0|empty_obj:0|ws:2|esc_value:2|esc_key:2|dup:2|num_key:2|utf8:2|\
         bad:bad:{prefix}null given|int:int:{prefix}int given|float:float:{prefix}float given|\
         big:big:{prefix}float given|str:str:{prefix}string given|true:true:{prefix}true given|\
         false:false:{prefix}false given|null:null:{prefix}null given|large:100|3\n"
    );
    assert_eq!(out.stdout, expected);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Emits the `main` function body for one source fixture.
fn emit_main_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_json_decode_direct_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write json count IR fixture");
    let output = elephc_cli_command(&dir)
        .arg("--emit-ir")
        .arg("--ir-opt=on")
        .arg(&php_path)
        .output()
        .expect("run elephc --emit-ir");
    assert!(
        output.status.success(),
        "emit-ir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let main = text
        .split("function main(")
        .nth(1)
        .expect("EIR contains main")
        .split("\n  function ")
        .next()
        .expect("main body")
        .to_string();
    let _ = fs::remove_dir_all(dir);
    main
}

/// A Mixed local produced by `json_encode` feeds the count helper without a per-call cast.
#[test]
fn json_decode_count_direct_mixed_source_removes_the_cast() {
    let ir = emit_main_ir(
        r#"<?php
$s = json_encode(['a' => 1, 'b' => 2]);
$n = 0;
for ($i = 0; $i < 3; $i++) { $n ^= count(json_decode($s, true)); }
echo $n, "\n";
"#,
    );
    assert!(ir.contains("runtime.json_decode_count"), "{ir}");
    assert!(!ir.contains("runtime.count"), "{ir}");
    let call = ir
        .lines()
        .find(|line| line.contains("runtime.json_decode_count"))
        .expect("count helper line");
    let operand = call
        .split("= runtime_call ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("count helper source");
    let definition = ir
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("{operand}:")))
        .expect("count helper source definition");
    assert!(definition.contains("load_local"), "{definition}");
}

/// Disabling EIR optimization leaves the exact decoder and count path.
#[test]
fn json_decode_count_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$obj = json_encode(['a' => 1, 'b' => [1, 2, 3]]);
echo count(json_decode($obj, true)), '|';
$esc = '{"a\\u0062":1,"x":2}';
echo count(json_decode($esc, true)), "\n";
"#,
        );
        assert_eq!(out, "2|2\n");
    });
}
