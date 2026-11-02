//! Purpose:
//! Executes resolved method-loop counters and their ownership and overflow refusal cases.
//!
//! Called from:
//! - The focused optimizer codegen integration suite.
//!
//! Key details:
//! - Every runtime case checks both IR modes, heap validation and optional real PHP output.
//! - The all-target fixture covers the same nested-loop storage proof as native execution.

use super::*;

/// Runs one mode with strict heap validation and the installed PHP oracle when requested.
fn run(source: &str, optimized: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_method_counter");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--heap-debug", "--gc-stats"]);
    if !optimized { command.arg("--no-ir-opt"); }
    let compiled = command.arg(&path).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&path).output().unwrap();
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(output.stdout, php.stdout, "real PHP output");
    }
    fs::remove_dir_all(dir).unwrap();
    (String::from_utf8(output.stdout).unwrap(), stderr)
}

/// The original nested method-loop reproducer returns the PHP hit count in both modes.
#[test]
fn test_method_loop_counters_nested_hits_match_php() {
    for optimized in [false, true] {
        assert_eq!(run(include_str!("../../../examples/method-loop-counter/main.php"), optimized).0,
            "22500");
    }
}

/// A typed by-value method observes a bounded counter without allocating its cell per iteration.
#[test]
fn test_method_loop_counters_typed_observations_bound_allocations() {
    let source = r#"<?php
final class ReadCounter { public function read(int $value): int { return $value&3; } }
function visit(ReadCounter $box, int $count): int {
    $sum=0;
    for($index=0;$index<$count;$index++){ $sum=$sum^$box->read($index); }
    return $sum;
}
echo visit(new ReadCounter(),$argc*1000);
"#;
    assert_eq!(run(source, false).0, "0");
    let (large, stats) = run(source, true);
    let (small, small_stats) = run(&source.replace("*1000", "*10"), true);
    assert_eq!(large, "0"); assert_eq!(small, "1");
    let (allocs, frees) = parse_gc_stats(&stats);
    let (small_allocs, _) = parse_gc_stats(&small_stats);
    assert_eq!(allocs, frees);
    assert!(allocs.abs_diff(small_allocs) <= 8,
        "method counter allocations must be bounded, small={small_allocs}, large={allocs}");
}

/// Escaping snapshots, references and overflow retain their ordinary PHP values and exceptions.
#[test]
fn test_method_loop_counters_preserve_snapshots_references_and_overflow() {
    let source = r#"<?php
final class CounterObserver {
    public array $seen=[];
    public function save(mixed $value): void { $this->seen[]=$value; }
    public function stop(int $value): void {
        if($value===1){ throw new RuntimeException('stop'); }
    }
    public function change(&$value): void { $value=PHP_INT_MAX; }
    public function step(&$value): void { $value+=2; }
}
function snapshots(CounterObserver $box, int $count): void {
    for($index=0;$index<$count;$index++){ $box->save($index); }
    echo $box->seen[0],':',$box->seen[$count-1],'|';
}
function unbounded(CounterObserver $box, int $count): void {
    $value=PHP_INT_MAX;
    for($index=0;$index<$count;$index++){ $box->save($value); $value++; }
    echo gettype($value),':',gettype($box->seen[0]),':',gettype($box->seen[1]),'|';
}
function references(CounterObserver $box, int $count): void {
    $value=0;
    for($index=0;$index<$count;$index++){ $box->change($value); $value++; }
    echo gettype($value),'|';
}
function throwingCounter(CounterObserver $box, int $count): void {
    for($index=0;$index<$count;$index++){ $box->stop($index); }
}
function referenceCounter(CounterObserver $box, int $count): void {
    for($index=0;$index<$count;$index++){ $box->step($index); }
    echo $index,'|';
}
snapshots(new CounterObserver(),$argc+2);
unbounded(new CounterObserver(),$argc+1);
references(new CounterObserver(),$argc+1);
referenceCounter(new CounterObserver(),$argc+4);
try { throwingCounter(new CounterObserver(),$argc+1); } catch(RuntimeException $error) { echo 'caught'; }
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "0:2|double:integer:double|double|6|caught");
    }
}

/// Both optimizer modes emit the nested method counter on every supported ABI.
#[test]
fn test_method_loop_counters_emit_on_all_supported_targets() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for optimized in [false, true] {
            let dir = make_cli_test_dir("elephc_method_counter_target");
            let path = dir.join("main.php");
            fs::write(&path, include_str!("../../../examples/method-loop-counter/main.php")).unwrap();
            let mut command = elephc_cli_command(&dir);
            command.args(["--emit-asm", "--target", target]);
            if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
            if !optimized { command.arg("--no-ir-opt"); }
            let output = command.arg(&path).output().unwrap();
            assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
            assert!(!fs::read_to_string(path.with_extension("s")).unwrap().is_empty());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
