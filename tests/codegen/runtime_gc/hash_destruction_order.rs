//! Purpose:
//! PHP-observable destructor-order regressions for associative array teardown.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` through the executable codegen harness.
//!
//! Key details:
//! - Teardown follows insertion order across growth, COW and deletion.
//! - Compiled PHP and dynamically supplied eval source must both leave clean heaps.

use crate::support::{compile_and_run_with_heap_debug, compile_and_run_with_heap_debug_per_argv};

const PAYLOAD: &str = r#"class Payload {
    public function __construct(public int $id) {}
    public function __destruct() { echo $this->id, ","; }
}
"#;

/// Returns PHP cases whose physical bucket ordering differs from insertion order.
fn teardown_cases() -> Vec<(String, &'static str)> {
    [
        (r#"$a=["a"=>new Payload(1),"b"=>new Payload(2),"c"=>new Payload(3)];
unset($a); echo "end\n";"#, "1,2,3,end\n"),
        (r#"$a=[PHP_INT_MAX-2=>new Payload(1),PHP_INT_MAX-1=>new Payload(2),PHP_INT_MAX=>new Payload(3)];
unset($a); echo "end\n";"#, "1,2,3,end\n"),
        (r#"$a=["a"=>new Payload(1),"b"=>new Payload(2),"c"=>new Payload(3)];
unset($a["b"]); $a["b"]=new Payload(4); unset($a); echo "end\n";"#, "2,1,3,4,end\n"),
        (r#"$a=[]; for($i=0;$i<12;$i++){$a["k".$i]=new Payload($i);}
unset($a["k3"]); $a["k3"]=new Payload(30); unset($a); echo "end\n";"#,
         "3,0,1,2,4,5,6,7,8,9,10,11,30,end\n"),
        (r#"$a=["a"=>new Payload(1),"b"=>new Payload(2),"c"=>new Payload(3)];
$b=$a; $b["d"]=new Payload(4); unset($a); echo "shared|"; unset($b); echo "end\n";"#,
         "shared|1,2,3,4,end\n"),
        (r#"$a=["empty"=>new Payload(1)]; unset($a["empty"]); unset($a); echo "end\n";"#, "1,end\n"),
        (r#"$a=["first"=>new Payload(1),"nested"=>["b"=>new Payload(2),"c"=>new Payload(3)],"last"=>new Payload(4)];
unset($a); echo "end\n";"#, "1,2,3,4,end\n"),
    ].into_iter().map(|(body, expected)| (format!("{PAYLOAD}\n{body}"), expected)).collect()
}

/// Compiled hashes release values in the same insertion order as reference PHP.
#[test]
fn test_hash_destruction_follows_insertion_order() {
    for (source, expected) in teardown_cases() {
        let output = compile_and_run_with_heap_debug(&format!("<?php\n{source}"));
        assert!(output.success, "{source}\n{}", output.stderr);
        assert_eq!(output.stdout, expected, "{source}");
        assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
        assert!(output.stderr.contains("leak summary: clean"), "{source}\n{}", output.stderr);
    }
}

/// A destructor exception propagates after the remaining values are released in order.
#[test]
fn test_hash_destruction_preserves_order_across_throwing_destructors() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class Payload {
    public function __construct(public int $id) {}
    public function __destruct() {
        echo $this->id, ",";
        if ($this->id === 2) { throw new Exception("boom"); }
    }
}
try {
    $a=["a"=>new Payload(1),"b"=>new Payload(2),"c"=>new Payload(3)];
    unset($a);
} catch (Exception $error) { echo "caught:",$error->getMessage(),"|"; unset($error); }
echo "end\n";
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "1,2,3,caught:boom|end\n");
    assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
}

/// Native eval reaches the same hash teardown helper without changing ordering or ownership.
#[test]
fn test_dynamic_eval_hash_destruction_follows_insertion_order() {
    let cases = teardown_cases();
    let arguments = cases.iter().map(|(source, _)| vec![source.as_str()]).collect::<Vec<_>>();
    let argvs = arguments.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv("<?php eval($argv[1]);", &argvs);
    for (output, (source, expected)) in outputs.iter().zip(cases) {
        assert!(output.success, "{source}\n{}", output.stderr);
        assert_eq!(output.stdout, expected, "{source}");
        assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
        assert!(output.stderr.contains("leak summary: clean"), "{source}\n{}", output.stderr);
    }
}
