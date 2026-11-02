//! Purpose:
//! Verifies that bounded builtin string reads do not duplicate their entire subject.
//!
//! Called from:
//! - The AST-to-EIR unit test module.
//!
//! Key details:
//! - Covers property receiver owners plus plain locals on all supported targets.

/// Raw offsets avoid copies while final Mixed receivers keep their provisional cleanup.
#[test]
fn borrowed_string_offsets_follow_final_storage_on_all_targets() {
    use crate::ir::{Immediate, Op, ValueDef};
    let source = r#"<?php
function rawOffset(string $s, int $i): int { return ord($s[$i]); }
function offsetBytes(): string { return str_repeat("ab", 32); }
function widenedOffset(int $n): int {
    for ($i = 0; $i < $n; $i++) { $c = offsetBytes(); }
    $result = ord($c[2]);
    unset($c);
    return $result;
}
echo rawOffset("ab", $argc), widenedOffset($argc);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let raw = module.functions.iter().find(|f| f.name == "rawOffset").unwrap();
        assert!(raw.instructions.iter().any(|inst| inst.op == Op::StrCharAt), "{target}: raw read");
        assert!(!raw.instructions.iter().any(|inst| {
            inst.op == Op::Acquire && inst.operands.first().is_some_and(|value| {
                let ValueDef::Instruction { inst, .. } = raw.value(*value).unwrap().def else { return false; };
                raw.instruction(inst).unwrap().op == Op::StrCharAt
            })
        }), "{target}: raw ord offsets must avoid byte copies");
        let widened = module.functions.iter().find(|f| f.name == "widenedOffset").unwrap();
        assert!(widened.instructions.iter().any(|inst| {
            if inst.op != Op::Release { return false; }
            let value = widened.value(inst.operands[0]).unwrap();
            let ValueDef::Instruction { inst, .. } = value.def else { return false; };
            let load = widened.instruction(inst).unwrap();
            let Some(Immediate::LocalSlot(slot)) = load.immediate else { return false; };
            load.op == Op::LoadLocal && value.php_type == crate::types::PhpType::Str
                && widened.locals[slot.as_raw() as usize].php_type.codegen_repr() == crate::types::PhpType::Mixed
        }), "{target}: widened offset receiver must retire its detached string copy");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Literal subjects remain persistent and need no allocation for an immediately consumed slice.
#[test]
fn borrowed_string_reads_keep_literal_subject_views() {
    let module = super::lower_source(r#"<?php
function literalSubject(int $i): int { return ord(substr("abcd", $i, 1)); }
echo literalSubject($argc);
"#);
    let function = module.functions.iter().find(|f| f.name == "literalSubject").unwrap();
    assert!(!function.instructions.iter().any(|inst| inst.op == crate::ir::Op::StrPersist),
        "literal substr views must remain allocation-free");
}

/// Virtual and builtin property surfaces retain their ordinary ownership path.
#[test]
fn borrowed_string_reads_keep_virtual_property_ownership() {
    use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
    let source = r#"<?php
class ReadHook {
    public string $bytes { get { return str_repeat("ab", 2000); } }
}
class ReadMagic {
    public function __get(string $name): string { return "fallback"; }
}
class ReadScopeBase {
    private string $bytes = "base";
    public function blockedScope(ReadScopeChild $r): string { return substr($r->bytes, 0, 1); }
}
class ReadScopeChild extends ReadScopeBase { public string $bytes = "child"; }
function reflectedReadParameter(int $value): void {}
function blockedHook(ReadHook $r): string { return substr($r->bytes, 0, 1); }
function blockedMagic(ReadMagic $r): string { return substr($r->bytes, 0, 1); }
function blockedBuiltin(): string {
    $r = new ReflectionParameter("reflectedReadParameter", 0);
    return substr($r->name, 0, 1);
}
echo blockedHook(new ReadHook()), blockedMagic(new ReadMagic()), blockedBuiltin();
echo (new ReadScopeBase())->blockedScope(new ReadScopeChild());
"#;
    let module = super::lower_source(source);
    let mut reads = 0;
    for function in module.functions.iter().chain(module.class_methods.iter())
        .filter(|f| f.name.starts_with("blocked") || f.name.ends_with("::blockedScope")) {
        for call in &function.instructions {
            if !matches!(call.immediate, Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(RuntimeFnId::Substr)
                    | RuntimeCallTarget::ProfiledFunction { target: RuntimeFnId::Substr, .. }
            ))) { continue; }
            reads += 1;
            let crate::ir::ValueDef::Instruction { inst, .. } = function.value(call.operands[0]).unwrap().def else { panic!("source producer"); };
            assert_eq!(function.instruction(inst).unwrap().op, Op::Acquire,
                "{}: retain ordinary operand stabilization", function.name);
        }
    }
    assert_eq!(reads, 4);
}

/// Safe string reads borrow their subject and persist only a substring result.
#[test]
fn borrowed_string_reads_avoid_subject_copies_on_all_targets() {
    use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
    let source = r#"<?php
class BorrowedRead {
    public $bytes;
    public function __construct(string $s) { $this->bytes = $s; }
    public function __get(string $name): string { return "fallback"; }
    public function byteAt(int $i): int { return ord(substr($this->bytes, $i, 1)); }
    public function findAt(int $i): int { return (int)strpos($this->bytes, "b", $i); }
}
function localByte(string $s, int $i): int { return ord(substr($s, $i, 1)); }
function namedByte(string $s, int $i): int { return ord(\SUBSTR(length: 1, string: $s, offset: $i)); }
function localLoop(int $n): int {
    $s = str_repeat("ab", 2000);
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += ord(substr($s, $i, 1)); }
    return $sum;
}
$r = new BorrowedRead(str_repeat("ab", 2000));
echo $r->byteAt($argc), $r->findAt($argc), localByte("ab", $argc), namedByte("ab", $argc), localLoop($argc);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let mut reads = 0;
        for function in module.functions.iter().chain(module.class_methods.iter()) {
            for call in &function.instructions {
                if !matches!(call.immediate, Some(Immediate::RuntimeCall(
                    RuntimeCallTarget::Function(RuntimeFnId::Substr | RuntimeFnId::Strpos)
                        | RuntimeCallTarget::ProfiledFunction {
                            target: RuntimeFnId::Substr | RuntimeFnId::Strpos, ..
                        }
                ))) { continue; }
                reads += 1;
                let source = function.value(call.operands[0]).unwrap();
                let crate::ir::ValueDef::Instruction { inst, .. } = source.def else { panic!("{target}: source producer"); };
                assert!(matches!(function.instruction(inst).unwrap().op, Op::LoadLocal | Op::LoadRefCell | Op::PropGet),
                    "{target} {}: the read must borrow the subject without a full-string acquire (producer {:?})",
                    function.name, function.instruction(inst).unwrap().op);
            }
        }
        assert_eq!(reads, 5, "{target}: exercise every read");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
