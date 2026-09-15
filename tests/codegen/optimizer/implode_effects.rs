//! Purpose:
//! Verifies unused joins preserve string-conversion callbacks, exceptions and mutable reads.
//!
//! Called from:
//! - The optimizer codegen integration suite.
//!
//! Key details:
//! - The same runtime-dependent fixture runs with EIR optimization enabled and disabled in CI.

use super::*;

/// Compiles and runs a join-effects fixture with an explicit EIR optimizer setting.
fn run_implode_effect_fixture(source: &str, ir_opt: bool) -> String {
    let dir = make_cli_test_dir("elephc_implode_effects");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write implode effects fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir).arg(mode).arg(&php_path).output()
        .expect("failed to compile implode effects fixture");
    assert!(compile.status.success(), "ir_opt={ir_opt}: {}", String::from_utf8_lossy(&compile.stderr));
    let output = Command::new(dir.join("main")).output().expect("failed to run implode effects fixture");
    assert!(output.status.success(), "ir_opt={ir_opt}: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let _ = fs::remove_dir_all(&dir);
    stdout
}

/// Both optimization modes retain unused calls, thrown conversions and joins across array mutations.
#[test]
fn test_implode_discarded_calls_preserve_effects_and_heap_reads() {
    let source = r#"<?php
$calls = 0;
class ObservableJoinedValue {
    public function __toString(): string {
        global $calls;
        $calls = $calls + 1;
        echo "cast|";
        return "value";
    }
}
class ThrowingUnusedJoinedValue {
    public function __toString(): string { throw new RuntimeException("throw"); }
}
function discardJoin(array $items): void { implode(",", $items); }
function discardJoinAlias(array $items): void { join($items); }
discardJoin(["item" => new ObservableJoinedValue()]);
discardJoinAlias(["item" => new ObservableJoinedValue()]);
echo $calls, "|";
try { discardJoin(["item" => new ThrowingUnusedJoinedValue()]); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
function joinChangingValues(array $items, int $seed): void {
    for ($i = 0; $i < 2; $i++) {
        $items[0] = $seed + $i;
        $before = implode(",", $items);
        $items[0] = $seed + $i + 10;
        $after = implode(",", $items);
        echo $before, ":", $after, "|";
    }
}
joinChangingValues([0], $argc);
"#;
    for ir_opt in [true, false] {
        assert_eq!(run_implode_effect_fixture(source, ir_opt), "cast|cast|2|throw|1:11|2:12|", "ir_opt={ir_opt}");
    }
}
