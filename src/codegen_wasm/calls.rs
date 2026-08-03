//! Purpose:
//! Resolves direct WASM call targets and classifies the local-storage forms
//! supported by the direct by-reference call ABI.
//!
//! Called from:
//! - `crate::codegen_wasm::capability` validates `Op::Call` against the exact
//!   target selected here.
//! - `crate::codegen_wasm::inst` consumes the same target and by-ref source
//!   classifications while emitting the call.
//!
//! Key details:
//! - PHP user-function names are case-insensitive; synthetic closure-body names
//!   are compiler-generated and therefore matched exactly.
//! - Missing and ambiguous targets are rejected. There is deliberately no
//!   guessed-symbol fallback.

use super::symbols::{closure_body_symbol, function_symbol};
use super::WasmError;
use crate::ir::{
    Function, Immediate, Instruction, LocalSlotId, Module, Op, ValueDef, ValueId,
};

/// One direct call target selected from the emitted function collections.
pub(super) struct DirectCallTarget<'a> {
    /// The resolved EIR body whose parameters and return shape define the ABI.
    pub(super) function: &'a Function,
    /// The canonical WAT identifier emitted for the resolved body.
    pub(super) symbol: String,
    /// The source-facing function or synthetic closure name.
    pub(super) name: &'a str,
}

/// The local-storage form of a direct by-reference argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ByRefSource {
    /// The local is already backed by a shared reference cell.
    AlreadyRefBound(u32),
    /// A plain local must be mirrored through a temporary reference cell.
    FreshLocal(LocalSlotId),
    /// No supported stable local storage could be proven.
    NonLocal,
}

/// Resolves an `Op::Call` to exactly one emitted user function or closure body.
///
/// User functions follow PHP's case-insensitive symbol rules and exclude the
/// compiler entry function. Synthetic closure names use exact matching because
/// they are internal identities. A collision across either collection is
/// rejected instead of choosing by collection order.
pub(super) fn resolve_direct_call<'a>(
    module: &'a Module,
    inst: &Instruction,
) -> Result<DirectCallTarget<'a>, WasmError> {
    let data = match inst.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => {
            return Err(WasmError::Unsupported(
                "direct call is missing its function-name Data immediate".to_string(),
            ))
        }
    };
    let name = module
        .data
        .function_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| {
            WasmError::Unsupported(format!(
                "direct call references unknown function-name data {:?}",
                data
            ))
        })?;
    let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    let functions: Vec<&Function> = module
        .functions
        .iter()
        .filter(|function| {
            !function.flags.is_main
                && crate::names::php_symbol_key(function.name.trim_start_matches('\\')) == key
        })
        .collect();
    let closures: Vec<&Function> = module
        .closures
        .iter()
        .filter(|function| function.name == name)
        .collect();
    match (functions.as_slice(), closures.as_slice()) {
        ([function], []) => Ok(DirectCallTarget {
            function,
            symbol: function_symbol(function),
            name,
        }),
        ([], [function]) => Ok(DirectCallTarget {
            function,
            symbol: closure_body_symbol(&function.name),
            name,
        }),
        ([], []) => Err(WasmError::Unsupported(format!(
            "direct call target {name:?} is missing"
        ))),
        _ => Err(WasmError::Unsupported(format!(
            "direct call target {name:?} is ambiguous across {} user function(s) and {} closure body/bodies",
            functions.len(),
            closures.len()
        ))),
    }
}

/// Classifies the defining storage of one direct by-reference argument.
///
/// Only `LoadLocal` and `LoadRefCell` are implemented by the WASM writeback
/// path. Literals, temporaries, block parameters, properties, and container
/// elements remain a clean unsupported L2 boundary.
pub(super) fn classify_by_ref_source(function: &Function, arg: ValueId) -> ByRefSource {
    let Some(value) = function.value(arg) else {
        return ByRefSource::NonLocal;
    };
    let ValueDef::Instruction { inst, .. } = value.def else {
        return ByRefSource::NonLocal;
    };
    let Some(defining) = function.instruction(inst) else {
        return ByRefSource::NonLocal;
    };
    match (defining.op, &defining.immediate) {
        (Op::LoadRefCell, Some(Immediate::LocalSlot(slot))) => {
            ByRefSource::AlreadyRefBound(slot.as_raw())
        }
        (Op::LoadLocal, Some(Immediate::LocalSlot(slot))) => ByRefSource::FreshLocal(*slot),
        _ => ByRefSource::NonLocal,
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_direct_call;
    use crate::codegen::platform::Target;
    use crate::ir::{Function, Immediate, Instruction, IrType, Module, Op, Ownership};
    use crate::types::PhpType;

    /// Builds a direct-call instruction naming one function-pool entry.
    fn direct_call(module: &mut Module, name: &str) -> Instruction {
        let data = module.data.intern_function_name(name);
        Instruction::new(
            Op::Call,
            Vec::new(),
            Some(Immediate::Data(data)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::Call.default_effects(),
            None,
        )
    }

    /// Verifies PHP case-insensitive user-function resolution returns its
    /// canonical emitted body and symbol.
    #[test]
    fn resolves_user_functions_case_insensitively() {
        let mut module = Module::new(Target::wasm());
        let call = direct_call(&mut module, "\\fOo");
        module.add_function(Function::new(
            "Foo".to_string(),
            IrType::Void,
            PhpType::Void,
        ));

        let target = resolve_direct_call(&module, &call).expect("case-insensitive target");

        assert_eq!(target.function.name, "Foo");
        assert_eq!(
            target.symbol,
            super::super::symbols::function_symbol(target.function)
        );
    }

    /// Verifies duplicate PHP symbol keys are rejected instead of selecting the
    /// first function in module order.
    #[test]
    fn rejects_case_folded_user_function_collisions() {
        let mut module = Module::new(Target::wasm());
        let call = direct_call(&mut module, "foo");
        module.add_function(Function::new(
            "Foo".to_string(),
            IrType::Void,
            PhpType::Void,
        ));
        module.add_function(Function::new(
            "\\fOO".to_string(),
            IrType::Void,
            PhpType::Void,
        ));

        let error = resolve_direct_call(&module, &call)
            .err()
            .expect("ambiguous target");

        assert!(error.to_string().contains("ambiguous"), "{error}");
    }

    /// Verifies synthetic closure-body names are matched exactly and resolved to
    /// the closure symbol rather than a guessed free-function symbol.
    #[test]
    fn resolves_synthetic_closure_names_exactly() {
        let mut module = Module::new(Target::wasm());
        let name = "__eir_closure_owner_0";
        let call = direct_call(&mut module, name);
        let mut closure = Function::new(name.to_string(), IrType::Void, PhpType::Void);
        closure.flags.is_closure = true;
        module.add_closure(closure);

        let target = resolve_direct_call(&module, &call).expect("synthetic closure target");

        assert_eq!(
            target.symbol,
            super::super::symbols::closure_body_symbol(name)
        );
    }

    /// Verifies a missing target produces a clean diagnostic and never a
    /// best-effort symbol fallback.
    #[test]
    fn rejects_missing_direct_call_targets() {
        let mut module = Module::new(Target::wasm());
        let call = direct_call(&mut module, "missing");

        let error = resolve_direct_call(&module, &call)
            .err()
            .expect("missing target");

        assert!(error.to_string().contains("is missing"), "{error}");
    }
}
