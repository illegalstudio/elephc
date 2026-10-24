//! Purpose:
//! End-to-end coverage for the straight-line reuse of `buffer<T>`-typed property reads: repeated
//! `$this->slots[$i]` accesses in a method read the property once, resolve the handle once and
//! share the record address of one index, while property stores through another reference,
//! `buffer_free()`, calls and index writes between the reads keep them apart.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::buffer_property_reuse`.
//!
//! Key details:
//! - Textual EIR of one method pins the access shape; every fixture also runs natively with the
//!   optimizer on and off and must print the same result or die with the same fatal.
//! - `Table::matches` is the shape of Ivory's `Entities::matches()`, which `bench/ecs` calls once
//!   per entity.

use super::*;

/// A slot table owning its packed records as a private buffer property.
const TABLE_SOURCE: &str = r#"<?php
packed class Slot { public int $generation; public int $nextFree; public int $mask; public bool $alive; }

final class Table
{
    private buffer<Slot> $slots;

    public function __construct(int $n) { $this->slots = buffer_new<Slot>($n); }

    public function matches(int $index, int $components): bool
    {
        return $this->slots[$index]->alive
            && ($this->slots[$index]->mask & $components) === $components;
    }

    public function swapMasks(int $a, int $b): void
    {
        $mask = $this->slots[$a]->mask;
        $this->slots[$a]->mask = $this->slots[$b]->mask;
        $this->slots[$b]->mask = $mask;
    }

    public function countMatching(int $components): int
    {
        $n = 0;
        for ($i = 0; $i < 4; $i++) {
            if ($this->slots[$i]->alive && ($this->slots[$i]->mask & $components) === $components) {
                $n = $n + 1;
            }
        }
        return $n;
    }

    public function slots(): buffer<Slot> { return $this->slots; }
}

$t = new Table(4);
$s = $t->slots();
$s[1]->alive = true;
$s[1]->mask = 7;
$s[2]->alive = true;
$s[2]->mask = 1;
$hits = 0;
for ($i = 0; $i < 4; $i++) {
    if ($t->matches($i, 3)) {
        $hits = $hits + 1;
    }
}
echo $hits, " ", $t->countMatching(1), " ";
$t->swapMasks(1, 2);
echo $t->matches(1, 3) ? "y" : "n", $t->matches(2, 3) ? "y" : "n", " ", $s[1]->mask, $s[2]->mask, "\n";
"#;

/// The output both optimizer modes must print for `TABLE_SOURCE`.
const TABLE_OUTPUT: &str = "1 2 ny 17\n";

/// A class whose methods put a property store, a free, a call or an index write between two
/// reads of the same buffer property.
const HOLDER_CLASS: &str = r#"<?php
packed class Cell { public int $value; }

final class Holder
{
    public buffer<Cell> $cells;

    public function __construct(public buffer<Cell> $spare)
    {
        $this->cells = buffer_new<Cell>(2);
        $this->cells[0]->value = 1;
        $this->cells[1]->value = 3;
    }

    public function readAcrossAliasStore(Holder $alias): int
    {
        $first = $this->cells[0]->value;
        $alias->cells = $alias->spare;
        return $first * 10 + $this->cells[0]->value;
    }

    public function readAcrossIndexWrite(int $i): int
    {
        $first = $this->cells[$i]->value;
        $i = 1;
        return $first * 10 + $this->cells[$i]->value;
    }

    public function readAcrossFree(): int
    {
        $first = $this->cells[0]->value;
        $cells = $this->cells;
        buffer_free($cells);
        return $first + $this->cells[0]->value;
    }

    public function release(): void
    {
        $cells = $this->cells;
        buffer_free($cells);
    }

    public function readAcrossCall(): int
    {
        $first = $this->cells[0]->value;
        $this->release();
        return $first + $this->cells[0]->value;
    }
}
"#;

/// A method whose two reads of one buffer property have a property read on a nullable receiver
/// between them; on `null` that read warns and runs the error handler the driver installs.
const NULLABLE_PROBE_CLASS: &str = r#"<?php
packed class Cell { public int $value; }
final class Probe { public int $n = 0; }
final class Holder
{
    public buffer<Cell> $cells;

    public function __construct(public buffer<Cell> $spare)
    {
        $this->cells = buffer_new<Cell>(2);
        $this->cells[0]->value = 1;
    }

    public function readAcrossNullableProbe(?Probe $probe): int
    {
        $first = $this->cells[0]->value;
        $probe->n;
        return $this->cells[0]->value - $first;
    }
}
$spare = buffer_new<Cell>(1);
$spare[0]->value = 3;
$h = new Holder($spare);
"#;

/// Whole-element reads and writes through a scalar buffer property, ending with a write through
/// a handle freed before the call.
const REGISTRY_SOURCE: &str = r#"<?php
final class Registry
{
    private buffer<int> $ids;

    public function __construct(int $n) { $this->ids = buffer_new<int>($n); }

    public function assign(int $a, int $b, int $value): void
    {
        $this->ids[$a] = $value;
        $this->ids[$b] = $value;
    }

    public function readAcrossWrite(int $a): int
    {
        $before = $this->ids[$a];
        $this->ids[$a] = 9;
        return $this->ids[$a] * 100 + $before;
    }

    public function ids(): buffer<int> { return $this->ids; }
}
$r = new Registry(3);
$r->assign(0, 2, 40);
echo $r->readAcrossWrite(2), "\n";
$ids = $r->ids();
echo $ids[0], " ", $ids[1], " ", $ids[2], "\n";
buffer_free($ids);
echo "before\n";
$r->assign(1, 0, 5);
"#;

/// Emits one named function's optimized or unoptimized textual EIR.
fn function_ir(source: &str, optimized: bool, name: &str) -> String {
    let dir = make_cli_test_dir("elephc_buffer_property_reuse_ir");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write PHP fixture");
    let mut command = elephc_cli_command(&dir);
    command.arg("--emit-ir");
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let output = command.arg(&php_path).output().expect("emit EIR");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).expect("EIR is UTF-8");
    let header = format!("  function {name}(");
    let body = text.split_once(&header).expect("named function").1;
    let function = body.split("\n  function ").next().expect("function body").to_string();
    fs::remove_dir_all(dir).expect("remove fixture directory");
    function
}

/// Compiles and runs one program with the EIR optimizer on or off and returns its output.
fn run_variant(source: &str, optimized: bool) -> std::process::Output {
    let dir = make_cli_test_dir("elephc_buffer_property_reuse_run");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write PHP fixture");
    let mut command = elephc_cli_command(&dir);
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let compile = command.arg(&php_path).output().expect("compile PHP fixture");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = run_binary(&dir.join("main"), &dir);
    fs::remove_dir_all(dir).expect("remove fixture directory");
    run
}

/// Asserts both optimizer modes succeed and print `expected`.
fn assert_both_modes_print(source: &str, expected: &str) {
    for optimized in [true, false] {
        let run = run_variant(source, optimized);
        assert!(run.status.success(), "optimized={optimized}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "optimized={optimized}");
    }
}

/// Asserts both optimizer modes print `stdout` and then die with `fatal` on stderr.
fn assert_both_modes_fail_after(source: &str, stdout: &str, fatal: &str) {
    for optimized in [true, false] {
        let run = run_variant(source, optimized);
        assert!(!run.status.success(), "optimized={optimized} unexpectedly succeeded");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "optimized={optimized}");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(stderr.contains(fatal), "optimized={optimized}: {stderr}");
    }
}

/// Counts the reads of a buffer-typed property in one function's EIR.
fn buffer_property_reads(ir: &str) -> usize {
    ir.lines()
        .filter(|line| line.contains("Heap(Buffer)") && line.contains("= prop_get "))
        .count()
}

/// `Table::matches()` reads `$this->slots` and resolves it once, and its second access to the
/// same index reuses the first record address; the unoptimized EIR still has both chains.
#[test]
fn test_repeated_property_buffer_reads_share_one_record() {
    let unoptimized = function_ir(TABLE_SOURCE, false, "Table::matches");
    assert_eq!(buffer_property_reads(&unoptimized), 2, "{unoptimized}");
    assert_eq!(unoptimized.matches("= buffer_get ").count(), 2, "{unoptimized}");

    let ir = function_ir(TABLE_SOURCE, true, "Table::matches");
    assert_eq!(buffer_property_reads(&ir), 1, "{ir}");
    assert_eq!(ir.matches("= buffer_get ").count(), 1, "{ir}");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_both_modes_print(TABLE_SOURCE, TABLE_OUTPUT);
}

/// Accesses to two indexes of one property read share a single descriptor, and each index keeps
/// one record address across the swap.
#[test]
fn test_property_buffer_accesses_of_two_indexes_share_one_descriptor() {
    let ir = function_ir(TABLE_SOURCE, true, "Table::swapMasks");
    assert_eq!(buffer_property_reads(&ir), 1, "{ir}");
    assert_eq!(ir.matches("= buffer_resolve ").count(), 1, "{ir}");
    assert_eq!(ir.matches("= buffer_get_resolved ").count(), 2, "{ir}");
    assert!(!ir.contains("= buffer_get "), "{ir}");
}

/// Inside a loop body, the two reads of one iteration share the property read and the record
/// address, which is the shape a caller's loop gets once such a method is inlined into it.
#[test]
fn test_property_buffer_reads_in_one_loop_iteration_share_one_record() {
    let ir = function_ir(TABLE_SOURCE, true, "Table::countMatching");
    assert_eq!(buffer_property_reads(&ir), 1, "{ir}");
    assert_eq!(ir.matches("= buffer_get ").count(), 1, "{ir}");
}

/// A store to the property through another reference to the same object makes the second read
/// see the new buffer, so it is not shared.
#[test]
fn test_property_store_through_an_alias_keeps_the_second_read() {
    let source = format!(
        "{HOLDER_CLASS}\n$spare = buffer_new<Cell>(1);\n$spare[0]->value = 2;\n$h = new Holder($spare);\necho $h->readAcrossAliasStore($h), \"\\n\";\n"
    );
    let ir = function_ir(&source, true, "Holder::readAcrossAliasStore");
    assert_eq!(ir.matches("= buffer_get ").count(), 2, "{ir}");
    assert_both_modes_print(&source, "12\n");
}

/// Writing the index between two reads makes the second address another record.
#[test]
fn test_index_write_keeps_the_second_record() {
    let source = format!(
        "{HOLDER_CLASS}\n$h = new Holder(buffer_new<Cell>(1));\necho $h->readAcrossIndexWrite(0), \"\\n\";\n"
    );
    assert_both_modes_print(&source, "13\n");
}

/// `buffer_free()` between two reads keeps the second access resolving the handle, which then
/// reports the use-after-free instead of reading freed memory.
#[test]
fn test_buffer_free_between_property_reads_keeps_the_fatal() {
    let source = format!(
        "{HOLDER_CLASS}\n$h = new Holder(buffer_new<Cell>(1));\necho \"before\\n\";\necho $h->readAcrossFree(), \"\\n\";\n"
    );
    let ir = function_ir(&source, true, "Holder::readAcrossFree");
    assert_eq!(ir.matches("= buffer_get ").count(), 2, "{ir}");
    assert_both_modes_fail_after(&source, "before\n", "use of buffer after buffer_free()");
}

/// A call between two reads may free the buffer, so the second access resolves it again.
#[test]
fn test_call_between_property_reads_keeps_the_fatal() {
    let source = format!(
        "{HOLDER_CLASS}\n$h = new Holder(buffer_new<Cell>(1));\necho \"before\\n\";\necho $h->readAcrossCall(), \"\\n\";\n"
    );
    let ir = function_ir(&source, true, "Holder::readAcrossCall");
    assert_eq!(ir.matches("= buffer_get ").count(), 2, "{ir}");
    assert_both_modes_fail_after(&source, "before\n", "use of buffer after buffer_free()");
}

/// The documented example reads its slot table once per `matches()` call and prints the same
/// result with the optimizer on and off.
#[test]
fn test_buffer_property_table_example_shares_property_reads() {
    let source = include_str!("../../../examples/buffer-property-table/main.php");
    for method in ["Entities::matches", "Entities::destroy"] {
        let ir = function_ir(source, true, method);
        assert_eq!(buffer_property_reads(&ir), 1, "{method}: {ir}");
        assert_eq!(ir.matches("= buffer_get ").count(), 1, "{method}: {ir}");
    }
    assert_both_modes_print(source, "entities=10; moving=5\n");
}

/// A warning between two reads (a property read on a `null` receiver) runs the error handler,
/// which here replaces the buffer property, so the second read must see the new buffer.
#[test]
fn test_error_handler_replacing_the_property_keeps_the_second_read() {
    let source = format!(
        "{NULLABLE_PROBE_CLASS}set_error_handler(function (int $errno, string $message) use ($h): bool {{\n    $h->cells = $h->spare;\n    return $errno > 0 || $message !== '';\n}});\necho $h->readAcrossNullableProbe(null), \"\\n\";\n"
    );
    let ir = function_ir(&source, true, "Holder::readAcrossNullableProbe");
    assert_eq!(buffer_property_reads(&ir), 2, "{ir}");
    assert_eq!(ir.matches("= buffer_get ").count(), 2, "{ir}");
    assert_both_modes_print(&source, "2\n");
}

/// An error handler that frees the buffer between two reads makes the second one report the
/// use-after-free in both optimizer modes.
#[test]
fn test_error_handler_freeing_the_buffer_keeps_the_fatal() {
    let source = format!(
        "{NULLABLE_PROBE_CLASS}set_error_handler(function (int $errno, string $message) use ($h): bool {{\n    $cells = $h->cells;\n    buffer_free($cells);\n    return $errno > 0 || $message !== '';\n}});\necho \"before\\n\";\necho $h->readAcrossNullableProbe(null), \"\\n\";\n"
    );
    assert_both_modes_fail_after(&source, "before\n", "use of buffer after buffer_free()");
}

/// A class declaring `__get` anywhere in the program turns the reuse off, since a property read
/// could then run user code.
#[test]
fn test_magic_get_in_the_program_disables_property_reuse() {
    let source = format!(
        "{TABLE_SOURCE}\nfinal class Magic {{ public function __get(string $name): int {{ return strlen($name); }} }}\necho (new Magic())->size, \"\\n\";\n"
    );
    let ir = function_ir(&source, true, "Table::matches");
    assert_eq!(buffer_property_reads(&ir), 2, "{ir}");
    assert_eq!(ir.matches("= buffer_get ").count(), 2, "{ir}");
    assert_both_modes_print(&source, &format!("{TABLE_OUTPUT}4\n"));
}

/// Whole-element writes through one property read share a descriptor, a scalar element read after
/// a write to the same element still loads the new value, and a write through a handle freed
/// before the call still dies with the use-after-free fatal at the first access.
#[test]
fn test_scalar_property_buffer_accesses_share_one_descriptor() {
    let assign = function_ir(REGISTRY_SOURCE, true, "Registry::assign");
    assert_eq!(buffer_property_reads(&assign), 1, "{assign}");
    assert_eq!(assign.matches("= buffer_resolve ").count(), 1, "{assign}");
    assert_eq!(assign.matches(" buffer_set_resolved ").count(), 2, "{assign}");
    assert!(!assign.contains(" buffer_set "), "{assign}");

    let read = function_ir(REGISTRY_SOURCE, true, "Registry::readAcrossWrite");
    assert_eq!(read.matches("= buffer_resolve ").count(), 1, "{read}");
    assert_eq!(read.matches("= buffer_get_resolved ").count(), 2, "{read}");
    assert_eq!(read.matches(" buffer_set_resolved ").count(), 1, "{read}");
    assert_both_modes_fail_after(
        REGISTRY_SOURCE,
        "940\n40 0 9\nbefore\n",
        "use of buffer after buffer_free()",
    );
}
