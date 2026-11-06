//! Purpose:
//! Regressions for temporary scalar owners in generational entity tables (task a7f30cfc).
//!
//! Called from:
//! - The codegen integration suite through `runtime_gc`.
//!
//! Key details:
//! - The standalone Ivory-shaped table uses a manually freed packed buffer.
//! - Both EIR optimizer modes must return live memory to baseline and leave a clean heap.

use crate::support::*;

const ENTITY_TABLE: &str = include_str!("../../fixtures/entity_slot_owners.php");

/// Checks the entity fixture's output and complete cleanup with EIR optimization on and off.
fn assert_entity_heap_clean(source: &str, expected: &str) {
    for optimized in [true, false] {
        let output = if optimized {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(output.success, "optimized={optimized}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "optimized={optimized}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "optimized={optimized}: {}", output.stderr);
    }
}

/// The original 10000-entity creation loop releases all cells after the table's destructor.
#[test]
fn test_entity_slot_creation_heap_clean() {
    let source = format!(r#"{ENTITY_TABLE}
$before = memory_get_usage();
run(10000);
echo memory_get_usage() === $before ? "baseline\n" : "retained\n";
"#);
    assert_entity_heap_clean(&source, "last 4294977295\nbaseline\n");
}

/// Repeated scene lifecycles stay below the default heap limit and restore live memory each time.
#[test]
fn test_entity_slot_repeated_lifecycles_heap_clean() {
    let source = format!(r#"{ENTITY_TABLE}
for ($cycle = 0; $cycle < 8; $cycle++) {{
    $before = memory_get_usage();
    run(10000);
    echo memory_get_usage() === $before ? "baseline\n" : "retained\n";
}}
"#);
    assert_entity_heap_clean(&source, &"last 4294977295\nbaseline\n".repeat(8));
}

/// Recycled slots take the other create branch without retaining their scalar index boxes.
#[test]
fn test_entity_slot_reuse_heap_clean() {
    let source = ENTITY_TABLE.replace("    public function create(): int", r#"
    public function recycle(int $index): void
    {
        $this->slots[$index]->alive = false;
        $this->slots[$index]->generation++;
        $this->slots[$index]->nextFree = $this->freeHead;
        $this->freeHead = $index;
        $this->count--;
    }

    public function create(): int"#);
    let source = format!(r#"{source}
$before = memory_get_usage();
$table = new Table(1);
for ($cycle = 0; $cycle < 1000; $cycle++) {{
    $entity = $table->create();
    $table->recycle(0);
}}
echo "last ", $entity, "\n";
unset($table);
echo memory_get_usage() === $before ? "baseline\n" : "retained\n";
"#);
    assert_entity_heap_clean(&source, "last 4294967296000\nbaseline\n");
}

/// Explicit int, float, bool and string casts retire a boxed local read after copying its value.
#[test]
fn test_entity_slot_scalar_casts_heap_clean() {
    let source = ENTITY_TABLE.replace(
        "        return ($this->slots[$index]->generation << 32) | $index;",
        r#"        echo (int) $index, ":", (float) $index, ":", (bool) $index, ":", (string) $index, "\n";
        return ($this->slots[$index]->generation << 32) | $index;"#,
    );
    let source = format!(r#"{source}
$before = memory_get_usage();
run(3);
echo memory_get_usage() === $before ? "baseline\n" : "retained\n";
"#);
    assert_entity_heap_clean(&source, "0:0::0\n1:1:1:1\n2:2:1:2\nlast 4294967298\nbaseline\n");
}

/// Later widening makes earlier Mixed loads borrow the slot and must prune their cleanup.
#[test]
fn test_entity_slot_later_mixed_storage_heap_clean() {
    let source = ENTITY_TABLE.replace(
        "        return ($this->slots[$index]->generation << 32) | $index;",
        r#"        $id = ($this->slots[$index]->generation << 32) | $index;
        $index = json_decode('"kkk"');
        echo $index, "\n";
        return $id;"#,
    );
    let source = format!(r#"{source}
$before = memory_get_usage();
run(3);
echo memory_get_usage() === $before ? "baseline\n" : "retained\n";
"#);
    assert_entity_heap_clean(&source, "kkk\nkkk\nkkk\nlast 4294967298\nbaseline\n");
}
