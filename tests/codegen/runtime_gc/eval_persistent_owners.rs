//! Purpose:
//! Verifies teardown of persistent values owned by an opaque eval context.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - Static locals, static properties and materialized class constants outlive eval execution.
//! - A destructor exception must reach native PHP only after every persistent owner retires.

use crate::support::*;

/// Persistent cells retire before context unregister, including when an eval destructor throws.
#[test]
fn test_eval_context_releases_persistent_cell_owners_bundle() {
    let args: Vec<_> = (0..7).map(|count| vec!["x"; count]).collect();
    let argvs: Vec<_> = args.iter().map(Vec::as_slice).collect();
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(r#"<?php
function runPersistentEvalOwners(bool $throwing, int $argumentCount): void {
    $source = $argumentCount > 0 ? '
class PersistentOwnerHolder {
    public static mixed $value;
    public const TOKEN = "constant";
}
class PersistentOwnerProbe {
    public function __construct(public string $label, public bool $throwing = false) {}
    public function __destruct() {
        echo $this->label, "|";
        if ($this->throwing) { throw new RuntimeException("caught"); }
    }
}
function keepPersistentOwnerLocal(): void {
    static $value;
    $value = new PersistentOwnerProbe("local");
}
keepPersistentOwnerLocal();
PersistentOwnerHolder::$value = new PersistentOwnerProbe("property", $throwing);
echo strlen(PersistentOwnerHolder::TOKEN), "|";
' : '';
    if ($argumentCount === 3) { $source = ''; }
    elseif ($argumentCount === 4) {
        $source = str_replace('echo strlen(PersistentOwnerHolder::TOKEN), "|";', 'echo PersistentOwnerHolder::TOKEN, "|";', $source);
    } elseif ($argumentCount === 5) {
        $source = str_replace('keepPersistentOwnerLocal();', '', $source);
    } elseif ($argumentCount === 6) {
        $source = str_replace('PersistentOwnerHolder::$value = new PersistentOwnerProbe("property", $throwing);', '', $source);
    } elseif ($argumentCount === 7) {
        $source = str_replace('echo strlen(PersistentOwnerHolder::TOKEN), "|";', 'echo "ready|";', $source);
    }
    eval($source);
}
try {
    runPersistentEvalOwners($argc > 1, $argc);
    echo "after|";
} catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
"#, &argvs);
    let mut failures = Vec::new();
    for (mode, out) in runs.iter().enumerate() {
        let expected = match mode {
            0 => "8|local|property|after|",
            1 => "8|local|property|caught|",
            2 => "after|",
            3 => "constant|local|property|caught|",
            4 => "8|property|caught|",
            5 => "8|local|after|",
            _ => "ready|local|property|caught|",
        };
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("mode={mode} stdout={:?} expected={expected:?}\n{}", out.stdout, out.stderr));
        }
    }
    assert!(failures.is_empty(), "{}\nPersistent context owner assembly:\n{}", failures.join("\n"),
        selected_function_assembly(&assembly, &["_main".into(), elephc::names::function_symbol("runPersistentEvalOwners")]));
}
