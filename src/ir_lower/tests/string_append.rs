//! Purpose:
//! Checks consuming string owner handoff, fallback gates and malformed EIR rejection.
//!
//! Called from:
//! - `cargo test --lib ir_lower::tests::string_append`.
//!
//! Key details:
//! - Fixtures keep runtime loop bounds so AST folding cannot erase the append.
//! - All targets run through lowering and the complete EIR optimizer.

use crate::ir::{Immediate, LocalKind, Op, Ownership, ValidationError};

/// Counts admitted append transactions across the source functions and closures.
fn appends(module: &crate::ir::Module) -> usize {
    module.functions.iter().chain(&module.closures).chain(&module.class_methods)
        .flat_map(|function| &function.instructions).filter(|inst| inst.op == Op::StrAppend).count()
}

/// Same-local accumulation has an adjacent owned replacement store on every target.
#[test]
fn consuming_string_append_lowers_and_optimizes_all_targets() {
    for source in [
        "<?php $s='';for($i=0;$i<$argc;$i++){$s.='x';}echo$s;",
        "<?php $s='';$rhs=(string)$argc;for($i=0;$i<$argc;$i++){$s=$s.$rhs;}echo$s;",
        "<?php function build(string $part,int $n):string{$s='';for($i=0;$i<$n;$i++){$s.=$part;}return$s;}echo build('x',$argc);",
    ] {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(name).unwrap());
            assert!(appends(&module) > 0, "{name}: {source}");
            crate::ir_passes::optimize_module(&mut module);
            crate::ir::validate_module(&module).unwrap();
            assert!(appends(&module) > 0, "{name}: optimized append disappeared");
        }
    }
}

/// Parameters, reference bindings, opaque RHS calls and ordinary concat keep existing ownership.
#[test]
fn consuming_string_append_rejects_unproved_ownership_or_rhs() {
    for source in [
        "<?php function update(string $s,int $n):string{for($i=0;$i<$n;$i++){$s.='x';}return$s;}echo update('a',$argc);",
        "<?php $s='a';$r=&$s;for($i=0;$i<$argc;$i++){$s.='x';}echo$r;",
        "<?php function part():string{return 'x';}$s='';for($i=0;$i<$argc;$i++){$s.=part();}echo$s;",
        "<?php $s='';for($i=0;$i<$argc;$i++){$t=$s.'x';}echo$t;",
        "<?php $s='a';$f=function()use($s){$s.='x';return$s;};echo$f();",
    ] {
        assert_eq!(appends(&super::lower_source(source)), 0, "{source}");
    }
}

/// Later writable captures must invalidate earlier consuming transactions on every target.
#[test]
fn consuming_string_append_falls_back_after_final_slot_widening() {
    for source in [
        "<?php $s=(string)$argc;$s.='x';$f=function()use(&$s){$s='z';};$f();echo$s;",
        "<?php function build(int $n):void{$s=(string)$n;$s.='x';$f=function()use(&$s){$s='z';};$f();echo$s;}build($argc);",
        "<?php $s=(string)$argc;for($i=0;$i<$argc;$i++){$s.='x';}$f=function()use(&$s){$s='z';};$f();echo$s;",
    ] {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
                std::path::Path::new("."), crate::codegen::platform::Target::parse(name).unwrap());
            assert_eq!(appends(&module), 0, "{name}: {source}");
            crate::ir_passes::optimize_module(&mut module);
            crate::ir::validate_module(&module).unwrap();
        }
    }
}

/// The validator rejects borrowed results, borrowed destinations and incorrect consumed operands.
#[test]
fn consuming_string_append_validator_rejects_invalid_transfers() {
    let source = "<?php $s='';for($i=0;$i<$argc;$i++){$s.='x';}echo$s;";
    let module = super::lower_source(source);
    let template = module.functions.iter().find(|function| function.flags.is_main).unwrap();
    for corruption in 0..4 {
        let mut function = template.clone();
        let index = function.instructions.iter().position(|inst| inst.op == Op::StrAppend).unwrap();
        let append = function.instructions[index].clone();
        let Some(Immediate::LocalSlot(slot)) = append.immediate else { panic!("append slot"); };
        match corruption {
            0 => function.value_mut(append.result.unwrap()).unwrap().ownership = Ownership::Borrowed,
            1 => function.locals[slot.as_raw() as usize].kind = LocalKind::BorrowedTemp,
            2 => function.instructions[index].operands.swap(0, 1),
            _ => {
                let mut observer = function.instructions.iter().find(|inst| inst.op == Op::EchoValue)
                    .expect("string echo observer").clone();
                observer.operands = vec![append.operands[0]];
                let id = crate::ir::InstId::from_raw(function.instructions.len() as u32);
                function.instructions.push(observer);
                let block = function.blocks.iter_mut().find(|block| block.instructions
                    .iter().any(|id| id.as_raw() as usize == index)).unwrap();
                block.instructions.push(id);
            }
        }
        let validation = crate::ir::validate_function(&function);
        let rejected = match corruption {
            0 => matches!(validation, Err(ValidationError::OwnershipTypeMismatch(_))),
            _ => matches!(validation, Err(ValidationError::InvalidStringAppendTransfer(_))),
        };
        assert!(rejected, "corruption {corruption}: {validation:?}");
    }
}
