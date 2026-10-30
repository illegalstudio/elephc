//! Purpose:
//! Builds private EIR operations for host object allocation and property exchange.
//!
//! Called from:
//! - The phpext pipeline after ordinary source lowering, before export safety validation.
//!
//! Key details:
//! - Uses the existing typed property and constructorless allocation lowerers on every target.
//! - These functions are native adapter operations, never global PHP functions.

use crate::{exports::ExportedFunction, ir::{self, Builder, Function, FunctionParam, Immediate, IrType,
    LocalKind, Module, Op, Ownership, Terminator}, types::{FunctionSig, PhpType}};
use super::classes::ExportedClass;

/// One private operation with a regular contained native export thunk.
#[derive(Clone, Debug)]
pub struct ObjectHelper {
    pub operation: ObjectOperation,
    pub export: ExportedFunction,
}

/// Object operations invoked by the host identity and property synchronization layer.
#[derive(Clone, Copy, Debug)]
pub enum ObjectOperation {
    Allocate,
    Release,
    PublishRoots,
    Collect,
    Read(usize),
    Write(usize),
    Unset(usize),
    Initialized(usize),
}

/// Describes a deterministic private ABI before the declaration pruner runs.
pub fn describe(class: &ExportedClass) -> Vec<ObjectHelper> {
    let object = PhpType::Object(class.name.clone());
    let mut operations = vec![(ObjectOperation::Allocate, Vec::new(), PhpType::Mixed)];
    for (index, (_, ty)) in class.info.properties.iter().enumerate() {
        operations.extend([
            (ObjectOperation::Read(index), vec![object.clone()], ty.clone()),
            (ObjectOperation::Write(index), vec![object.clone(), ty.clone()], PhpType::Void),
            (ObjectOperation::Unset(index), vec![object.clone()], PhpType::Void),
            (ObjectOperation::Initialized(index), vec![object.clone()], PhpType::Bool),
        ]);
    }
    operations.push((ObjectOperation::Release, vec![PhpType::Int], PhpType::Void));
    operations.push((ObjectOperation::PublishRoots, Vec::new(), PhpType::Void));
    operations.push((ObjectOperation::Collect, Vec::new(), PhpType::Void));
    operations.into_iter().enumerate().map(|(index, (operation, params, result))| {
        let signature = signature(params, result);
        let suffix = match operation { ObjectOperation::Release => "release".to_string(),
            ObjectOperation::Collect => "collect".to_string(), _ => index.to_string() };
        ObjectHelper { operation, export: ExportedFunction {
            name: format!("__elephc_phpext_object_{}_{}", class.info.class_id, suffix),
            c_name: format!("elephc_php_object_{}_{}", class.info.class_id, suffix),
            source_sig: signature.clone(), internal_sig: signature,
            span: class.info.declaration_span, php_void_return: false,
        } }
    }).collect()
}

/// Creates fixed physical signatures whose storage types already came from the checker.
fn signature(params: Vec<PhpType>, return_type: PhpType) -> FunctionSig {
    let len = params.len();
    FunctionSig {
        params: params.into_iter().enumerate().map(|(index, ty)| (format!("arg{index}"), ty)).collect(),
        param_type_exprs: vec![None; len], param_attributes: vec![Vec::new(); len],
        defaults: vec![None; len], declared_params: vec![true; len], ref_params: vec![false; len],
        return_type, declared_return: true, by_ref_return: false, variadic: None,
        deprecation: None, is_generator: false,
    }
}

/// Lowers adapter-owned operations without parsing generated PHP or duplicating native layouts.
pub fn lower(module: &mut Module, classes: &[ExportedClass]) {
    for class in classes {
        for helper in &class.helpers { lower_operation(module, class, helper); }
    }
}

/// Builds one straight-line typed helper, preserving the ordinary backend ownership rules.
fn lower_operation(module: &mut Module, class: &ExportedClass, helper: &ObjectHelper) {
    let signature = &helper.export.internal_sig;
    let result_ty = signature.return_type.codegen_repr();
    let mut function = Function::new(helper.export.name.clone(), IrType::from_php(&result_ty), result_ty.clone());
    function.flags.is_internal = true;
    function.flags.is_synthetic = true;
    function.lexical_class = Some(class.name.clone());
    function.signature = Some(signature.clone());
    function.params = signature.params.iter().map(|(name, ty)| FunctionParam {
        name: name.clone(), ir_type: value_type(ty), php_type: ty.clone(), by_ref: false, variadic: false,
    }).collect();
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_block_with_params(Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let params = signature.params.iter().map(|(name, ty)| {
        let slot = builder.add_local(Some(name.clone()), value_type(ty), ty.clone(), LocalKind::BorrowedTemp);
        let ownership = if Ownership::php_type_needs_lifetime_tracking(ty) { Ownership::Borrowed } else { Ownership::NonHeap };
        builder.emit(Op::LoadLocal, Vec::new(), Some(Immediate::LocalSlot(slot)), value_type(ty), ty.clone(), ownership).unwrap()
    }).collect::<Vec<_>>();
    let result = match helper.operation {
        ObjectOperation::Allocate => {
            let data = module.data.intern_string(&class.name);
            let name = builder.emit(Op::ConstStr, Vec::new(), Some(Immediate::Data(data)), IrType::Str,
                PhpType::Str, Ownership::Persistent).unwrap();
            builder.emit(Op::DynamicObjectNewWithoutConstructorMixed, vec![name], None, value_type(&result_ty), result_ty.clone(), Ownership::Owned)
        }
        ObjectOperation::Release | ObjectOperation::PublishRoots | ObjectOperation::Collect => {
            let target = match helper.operation {
                ObjectOperation::Release => ir::RuntimeCallTarget::PhpextObjectRelease,
                ObjectOperation::Collect => ir::RuntimeCallTarget::PhpextCollectObjectCycles,
                _ => ir::RuntimeCallTarget::PhpextPublishObjectRoots,
            };
            builder.emit(Op::RuntimeCall, params, Some(Immediate::RuntimeCall(target)), IrType::Void,
                PhpType::Void, Ownership::NonHeap)
        }
        operation => {
            let (index, op) = match operation {
                ObjectOperation::Read(index) => (index, Op::PropGet),
                ObjectOperation::Write(index) => (index, Op::PropSet),
                ObjectOperation::Unset(index) => (index, Op::PropUnset),
                ObjectOperation::Initialized(index) => (index, Op::PropInitialized),
                _ => unreachable!(),
            };
            let name = &class.info.properties[index].0;
            let property = module.data.intern_string(name);
            let ownership = if op == Op::PropGet && result_ty == PhpType::Mixed { Ownership::Owned }
                else { Ownership::for_php_type(&result_ty) };
            let value = builder.emit(op, params, Some(Immediate::Data(property)), IrType::from_php(&result_ty), result_ty.clone(), ownership);
            if let Some(value) = value.filter(|_| op == Op::PropGet && result_ty != PhpType::Mixed
                && Ownership::php_type_needs_lifetime_tracking(&result_ty)) {
                builder.emit(Op::Acquire, vec![value], None, value_type(&result_ty), result_ty.clone(), Ownership::Owned)
            } else { value }
        }
    };
    builder.terminate(Terminator::Return { value: result });
    module.add_function(function);
}

/// Uses the physical representation for parameters and expression results.
fn value_type(ty: &PhpType) -> IrType {
    if matches!(ty.codegen_repr(), PhpType::Void | PhpType::Never) { IrType::I64 }
    else { IrType::from_php(&ty.codegen_repr()) }
}

/// Identifies compiler-created allocation helpers for the closed exported-class inventory.
pub(crate) fn is_allocation_helper(function: &ir::Function) -> bool {
    function.flags.is_internal && function.flags.is_synthetic
        && function.name.starts_with("__elephc_phpext_object_")
        && function.name.ends_with("_0")
}

/// Proves class membership from compiler-created helper roots rather than source attributes alone.
pub(crate) fn is_exported_class(module: &Module, class: &str) -> bool {
    module.functions.iter().any(|function| is_allocation_helper(function)
        && function.lexical_class.as_ref().is_some_and(|name| name.eq_ignore_ascii_case(class)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Emits the same constructorless allocation and typed property operations on every target.
    #[test]
    fn phpext_class_helpers_validate_on_every_supported_target() {
        let source = r#"<?php #[Export] final class Pippo {
            public int $value = 7;
            public string $label = "pippo";
            public array $items = ['label' => 'yes'];
            public ?Pippo $next = null;
            public mixed $anything = null;
            private ?Pippo $hidden = null;
            public function addValue(int $amount): int { return $this->value + $amount; }
        }"#;
        for name in ["linux-x86_64", "linux-aarch64", "macos-aarch64", "ios-arm64", "ios-sim-arm64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let tokens = crate::lexer::tokenize(source).unwrap();
            let program = crate::parser::parse(&tokens).unwrap();
            let checked = crate::types::check_with_target(&program, target).unwrap();
            let classes = super::super::classes::collect(&program, &checked.classes, target).unwrap();
            let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
            lower(&mut module, &classes);
            for function in &module.functions {
                ir::validate_function(function).unwrap_or_else(|error| panic!("{name}: {}: {error:?}", function.name));
            }
            ir::validate_module(&module).unwrap_or_else(|error| panic!("{name}: {error:?}"));
            crate::codegen::generate_user_asm_from_ir(&module, false, false)
                .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        }
    }

    /// Keeps lease retirement usable when the managed heap cannot allocate another value box.
    #[test]
    fn phpext_class_release_thunks_do_not_allocate_transport_boxes() {
        let tokens = crate::lexer::tokenize("<?php #[Export] final class Pippo {}").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        for name in ["linux-x86_64", "linux-aarch64", "macos-aarch64"] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let checked = crate::types::check_with_target(&program, target).unwrap();
            let classes = super::super::classes::collect(&program, &checked.classes, target).unwrap();
            let release = classes[0].helpers.iter().find(|helper| matches!(helper.operation, ObjectOperation::Release)).unwrap();
            let mut emitter = crate::codegen_support::emit::Emitter::new_cdylib(target);
            let mut data = crate::codegen_support::data_section::DataSection::new();
            crate::codegen_support::cdylib::emit_phpext_exports(&mut emitter, &mut data, target, &[&release.export], false, None);
            let assembly = emitter.output();
            assert!(!assembly.contains("__rt_zval_unpack"), "{name}: release boxed a raw pointer");
            assert!(!assembly.contains("__rt_zval_pack"), "{name}: void result allocated a native transport value");
            assert!(!assembly.contains("__rt_mixed_from_value"), "{name}: release allocated a Mixed result");
            assert!(assembly.contains("elephc_php_void_result"), "{name}: missing allocation-free void callback");
        }
    }


    /// Keeps the extra root-publication entry connected to its Mach-O collector atom.
    #[test]
    fn phpext_class_root_entry_uses_shared_macos_collector_label() {
        let target = crate::codegen_support::platform::Target::parse("macos-aarch64").unwrap();
        let assembly = crate::codegen_support::generate_phpext_runtime(524288, target,
            crate::codegen_support::RuntimeFeatures::none());
        assert!(assembly.contains(".alt_entry __rt_gc_phpext_scan_begin\n__rt_gc_phpext_scan_begin:"));
        assert!(assembly.contains("b __rt_gc_phpext_scan_begin"));
        assert!(!assembly.contains("L_phpext_gc_root_scan_begin"));
    }

}
