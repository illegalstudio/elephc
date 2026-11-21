//! Purpose:
//! Builds a parent-side callable that submits an already checked worker/payload template.
//!
//! Called from:
//! - Periodic Parallel submission lowering through `super::lower_parallel_submission_factory`.
//!
//! Key details:
//! - The only capture is a parent-owned immutable serialized input string.
//! - The existing typed ParallelSpawn opcode copies input into each newly owned native job.
//! - No worker address or PHP pointer becomes a user-supplied callback selector.

use super::*;
use crate::ir::LocalKind;

/// Creates a zero-visible-argument closure whose hidden capture supplies the frozen input bytes.
pub(crate) fn lower_parallel_submission_factory(
    parent: &mut LoweringContext<'_, '_>,
    serialized: crate::ir_lower::context::LoweredValue,
    callback: crate::ir::DataId,
    span: Span,
) -> crate::ir_lower::context::LoweredValue {
    let name = parent.next_closure_name();
    let mut factory = build_factory(name.clone(), callback, span);
    factory.lexical_class = parent.current_class.clone();
    parent.extend_closures([factory]);
    let data = parent.intern_string(&name);
    parent.emit_value(Op::ClosureNew, vec![serialized.value], Some(Immediate::Data(data)),
        PhpType::Callable, Op::ClosureNew.default_effects(), Some(span))
}

/// Constructs only the typed EIR body, allowing target validation before pump integration.
fn build_factory(name: String, callback: crate::ir::DataId, span: Span) -> Function {
    let signature = FunctionSig {
        params: Vec::new(), param_type_exprs: Vec::new(), param_attributes: Vec::new(),
        defaults: Vec::new(), return_type: PhpType::Int, declared_return: true,
        by_ref_return: false, ref_params: Vec::new(), declared_params: Vec::new(),
        variadic: None, deprecation: None, is_generator: false,
    };
    let mut factory = Function::new(name.clone(), IrType::I64, PhpType::Int);
    factory.flags = FunctionFlags { is_closure: true, is_internal: true, is_synthetic: true,
        ..FunctionFlags::default() };
    factory.params = closure_capture_params(&[("__periodic_payload".into(), PhpType::Str, false)]);
    factory.source_signature = Some(source_signature(&name, &signature));
    factory.signature = Some(signature);
    let mut builder = Builder::new(&mut factory);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let slot = builder.add_local(Some("__periodic_payload".into()), IrType::Str, PhpType::Str,
        LocalKind::ClosureCapture);
    let input = builder.emit_with_effects(Op::LoadLocal, Vec::new(), Some(Immediate::LocalSlot(slot)),
        IrType::Str, PhpType::Str, Ownership::Borrowed, Op::LoadLocal.default_effects(), Some(span))
        .expect("capture load produces serialized bytes");
    let job = builder.emit_with_effects(Op::ParallelSpawn, vec![input], Some(Immediate::Data(callback)),
        IrType::I64, PhpType::Int, Ownership::NonHeap, Op::ParallelSpawn.default_effects(), Some(span))
        .expect("submission produces a native job identity");
    builder.terminate(Terminator::Return { value: Some(job) });
    factory
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frozen-input submission body and hidden-capture ABI emit on every supported target.
    #[test]
    fn parallel_factory_template_emits_on_all_supported_targets() {
        let source = r#"<?php
\Elephc\Parallel\run(static function (\Elephc\Parallel\TaskGroup $tasks): void {
    $tasks->spawn(static fn(): int => 7);
});
"#;
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut module = crate::ir_lower::tests::lower_source_at_for_target(
                source, std::path::Path::new("main.php"), std::path::Path::new("."),
                crate::codegen::platform::Target::parse(name).unwrap(),
            );
            let callback = module.functions.iter().chain(&module.closures)
                .flat_map(|function| &function.instructions)
                .find_map(|inst| match (inst.op, inst.immediate.as_ref()) {
                    (Op::ParallelSpawn, Some(Immediate::Data(data))) => Some(*data), _ => None,
                }).expect("checked worker entry");
            let factory = build_factory("__periodic_factory_target_probe".into(), callback, Span::synthetic());
            assert_eq!(factory.params.len(), 1, "{name}: one hidden byte-string capture");
            assert!(factory.signature.as_ref().unwrap().params.is_empty(), "{name}: no visible arguments");
            assert_eq!(factory.instructions.iter().filter(|inst| inst.op == Op::ParallelSpawn).count(), 1);
            module.add_closure(factory);
            crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        }
    }
}
