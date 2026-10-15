//! Purpose:
//! Include bookkeeping for the wasm32-wasi backend: `include_once`/`require_once` guards and
//! the dispatchers for functions whose bodies arrive from an include.
//!
//! Called from:
//! - `crate::codegen_wasm::plan::generate()` to declare the globals and emit the dispatchers.
//! - `crate::codegen_wasm::inst` for `IncludeOnceMark`, `IncludeOnceGuard` and
//!   `FunctionVariantMark`.
//! - `crate::codegen_wasm::calls` to resolve a public group name to its dispatcher.
//!
//! Key details:
//! - Both features are one mutable i32 global apiece. The include guard holds "has this site
//!   run"; the variant slot holds WHICH body is live, as a one-based index into the group.
//! - The variant dispatcher is an if-ladder over that index, NOT a `call_indirect`: this
//!   backend carries no `funcref` table, and every other dynamic dispatch here — methods,
//!   interfaces, `__toString` — is already a ladder. Adding a table for this one feature would
//!   introduce a second dispatch mechanism to keep in sync with the first.
//! - Index ZERO is reserved for "no include has defined it yet", which is what lets the
//!   dispatcher raise PHP's own `Call to undefined function` instead of trapping.

use crate::ir::{function_variants, Function, Immediate, Instruction, Module};
use crate::names::php_symbol_key;
use std::collections::HashMap;

use super::context::{FnCtx, Result};
use super::symbols::{
    function_symbol, function_variant_active_symbol, function_variant_dispatch_symbol,
    include_once_flag_symbol,
};
use super::values::WasmRepr;
use super::wat::{Global, ValType, WatModule};
use super::WasmError;

/// Returns every `include_once` label the module marks or guards, in a deterministic order.
///
/// Marks and guards are collected TOGETHER because a site may be guarded in one function and
/// marked in another; declaring a global for only one of them would leave the other reading an
/// undeclared name.
fn include_once_labels(module: &Module) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for function in super::function::all_functions(module) {
        for inst in &function.instructions {
            if !matches!(inst.op, crate::ir::Op::IncludeOnceMark | crate::ir::Op::IncludeOnceGuard)
            {
                continue;
            }
            let Some(Immediate::Data(data)) = inst.immediate else {
                continue;
            };
            let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
                continue;
            };
            if !labels.iter().any(|seen| seen == label) {
                labels.push(label.clone());
            }
        }
    }
    labels.sort();
    labels
}

/// Declares one mutable global per include-once site and per include-variant group.
pub(super) fn plan_include_globals(wm: &mut WatModule, module: &Module) {
    for label in include_once_labels(module) {
        wm.add_global(Global {
            name: include_once_flag_symbol(&label),
            ty: ValType::I32,
            mutable: true,
            init: 0,
        });
    }
    for group in function_variants::collect_dispatch_groups(module) {
        wm.add_global(Global {
            name: function_variant_active_symbol(&group.name),
            ty: ValType::I32,
            mutable: true,
            init: 0, // no include has defined it yet
        });
    }
}

/// Returns the group whose public name matches `name`, if this module has one.
pub(super) fn dispatch_group_for(
    module: &Module,
    name: &str,
) -> Option<function_variants::FunctionVariantLabel> {
    let key = php_symbol_key(name.trim_start_matches('\\'));
    function_variants::collect_dispatch_groups(module)
        .into_iter()
        .find(|group| php_symbol_key(group.name.trim_start_matches('\\')) == key)
}

/// Returns the concrete function body for one variant name.
fn variant_body<'a>(module: &'a Module, variant: &str) -> Option<&'a Function> {
    let key = php_symbol_key(variant.trim_start_matches('\\'));
    module
        .functions
        .iter()
        .find(|function| php_symbol_key(function.name.trim_start_matches('\\')) == key)
}

/// Emits one dispatcher per include-variant group.
///
/// A group whose variants disagree on the ABI is SKIPPED rather than emitted half-right — the
/// capability gate refuses calls to it, so no `call` will reference the stub that was not
/// emitted. That mirrors how the interface stubs handle the same disagreement.
pub(super) fn emit_variant_dispatchers(
    wm: &mut WatModule,
    module: &Module,
    default_strings: &HashMap<String, (u32, u32)>,
) {
    for group in function_variants::collect_dispatch_groups(module) {
        let Some(&(name_ptr, name_len)) = default_strings.get(&group.name) else {
            continue;
        };
        let Some(wat) = build_variant_dispatcher(module, &group, name_ptr, name_len) else {
            continue;
        };
        wm.add_raw_func(&wat);
    }
}

/// Returns the public names every dispatcher needs addressable, for the data layout.
///
/// The fatal composes the name at RUNTIME, so the bytes have to exist in static data even
/// though no PHP literal in the program need mention them.
pub(super) fn dispatch_group_names(module: &Module) -> Vec<String> {
    function_variants::collect_dispatch_groups(module)
        .into_iter()
        .map(|group| group.name)
        .collect()
}

/// Builds the WAT of one group's dispatcher, or `None` when the group cannot share one.
fn build_variant_dispatcher(
    module: &Module,
    group: &function_variants::FunctionVariantLabel,
    name_ptr: u32,
    name_len: u32,
) -> Option<String> {
    let bodies: Vec<&Function> = group
        .variants
        .iter()
        .map(|variant| variant_body(module, variant))
        .collect::<Option<Vec<_>>>()?;
    let signature = bodies.first()?;
    for body in &bodies {
        if body.return_type != signature.return_type || body.params.len() != signature.params.len()
        {
            return None;
        }
        if body
            .params
            .iter()
            .zip(&signature.params)
            .any(|(left, right)| left.ir_type != right.ir_type)
        {
            return None;
        }
    }

    let mut params = Vec::new();
    let mut forwards = Vec::new();
    let mut index = 0u32;
    for param in &signature.params {
        for ty in WasmRepr::val_types(param.ir_type) {
            let name = format!("$p{index}");
            params.push(format!("(param {name} {})", ty.as_str()));
            forwards.push(format!("local.get {name}"));
            index += 1;
        }
    }
    let results = WasmRepr::val_types(signature.return_type);

    let mut wat = format!(
        "(func ${}",
        function_variant_dispatch_symbol(&group.name)
    );
    for param in &params {
        wat.push_str(&format!(" {param}"));
    }
    for ty in &results {
        wat.push_str(&format!(" (result {})", ty.as_str()));
    }
    wat.push('\n');
    let active = function_variant_active_symbol(&group.name);
    for (position, body) in bodies.iter().enumerate() {
        wat.push_str(&format!(
            "  (if (i32.eq (global.get ${active}) (i32.const {})) (then\n",
            position + 1
        ));
        for forward in &forwards {
            wat.push_str(&format!("    {forward}\n"));
        }
        wat.push_str(&format!("    (return (call ${}))))\n", function_symbol(body)));
    }
    // Zero: no include has run yet, which is php's own fatal rather than a dispatch failure.
    wat.push_str(&format!(
        "  (call $__rt_fail_undefined_function (i32.const {name_ptr}) (i32.const {name_len}))\n"
    ));
    wat.push_str(
        "  unreachable ;; elephc-trap:post-noreturn:undefined-function-variant\n)\n",
    );
    Some(wat)
}

/// Lowers `IncludeOnceMark`: records that this include site has run.
pub(super) fn lower_include_once_mark(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let flag = include_once_flag_symbol(&label_of(ctx, inst)?);
    ctx.fb.ins("i32.const 1", "the value the guard reads as 'already run'");
    ctx.fb
        .ins(&format!("global.set ${flag}"), "mark this include site as run");
    Ok(())
}

/// Lowers `IncludeOnceGuard`: answers whether this is the FIRST run, and records it.
///
/// Test-and-set in one op rather than a guard the caller then marks: PHP decides inclusion at
/// the moment the guard runs, and leaving the two apart would let a second guard on the same
/// site answer "first" as well.
pub(super) fn lower_include_once_guard(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let flag = include_once_flag_symbol(&label_of(ctx, inst)?);
    ctx.fb
        .ins(&format!("global.get ${flag}"), "has this include site run?");
    ctx.fb.ins("i32.eqz", "first run is the zero flag");
    ctx.fb.ins("i64.extend_i32_u", "PHP bool travels as i64");
    ctx.fb.ins("i32.const 1", "record the run");
    ctx.fb.ins(&format!("global.set ${flag}"), "set before returning");
    super::inst::store_result(ctx, inst)
}

/// Lowers `FunctionVariantMark`: makes one concrete include-loaded body active.
pub(super) fn lower_function_variant_mark(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let label = label_of(ctx, inst)?;
    let parsed = function_variants::parse_variant_label(&label).ok_or_else(|| {
        WasmError::Unsupported(format!("invalid function variant label {label:?}"))
    })?;
    let group = dispatch_group_for(ctx.module, &parsed.name).ok_or_else(|| {
        WasmError::Unsupported(format!("no dispatch group for variant label {label:?}"))
    })?;
    let variant = parsed.variants.first().ok_or_else(|| {
        WasmError::Unsupported(format!("variant label {label:?} names no variant"))
    })?;
    let variant_key = php_symbol_key(variant.trim_start_matches('\\'));
    let position = group
        .variants
        .iter()
        .position(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == variant_key)
        .ok_or_else(|| {
            WasmError::Unsupported(format!("variant {variant:?} is not in its own group"))
        })?;
    ctx.fb.ins(
        &format!("i32.const {}", position + 1),
        "one-based: zero stays reserved for 'not yet defined'",
    );
    ctx.fb.ins(
        &format!("global.set ${}", function_variant_active_symbol(&group.name)),
        "activate this include-loaded body",
    );
    Ok(())
}

/// Reads the interned label an include/variant op carries.
fn label_of(ctx: &FnCtx, inst: &Instruction) -> Result<String> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Err(WasmError::Unsupported(
            "include bookkeeping op without an interned label".to_string(),
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| {
            WasmError::Unsupported(format!("include label data {data:?} is missing"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Op;

    /// Verifies the four include-bookkeeping opcodes are admitted, and only with a real label.
    ///
    /// `Op::IncludeOnceMark` and `Op::IncludeOnceGuard` share one flag, so a mark whose label
    /// the module cannot resolve would leave the guard reading a global nothing ever sets.
    /// `Op::FunctionVariantMark` has the stricter obligation: its label must name a group this
    /// module also DISPATCHES, or the include would set a slot no call reads and the public
    /// name would fatal as undefined even though the file ran. `Op::FunctionVariantDispatch`
    /// carries no code of its own — the group's stub does.
    #[test]
    fn include_bookkeeping_opcodes_are_admitted_and_need_resolvable_labels() {
        for op in [
            Op::IncludeOnceMark,
            Op::IncludeOnceGuard,
            Op::FunctionVariantMark,
            Op::FunctionVariantDispatch,
        ] {
            assert!(
                crate::codegen_wasm::capability::op_is_supported(op),
                "{op:?} must be admitted now that it carries real storage",
            );
        }

        // An empty module has no groups, so a variant label naming one cannot resolve.
        let module = crate::ir::Module::new(crate::codegen_support::platform::Target::wasm());
        assert!(dispatch_group_for(&module, "helper").is_none());
        assert!(function_variants::parse_variant_label("helper:v1").is_some());
        assert!(function_variants::parse_variant_label("helper").is_none());
    }

    /// Verifies each generated symbol lives in its own namespace.
    ///
    /// The include flag, the active-variant slot and the dispatcher all derive from PHP text a
    /// program controls, so a shared prefix would let one impersonate another.
    #[test]
    fn include_symbols_stay_in_disjoint_namespaces() {
        let flag = include_once_flag_symbol("App\\Lib");
        let active = function_variant_active_symbol("App\\Lib");
        let dispatch = function_variant_dispatch_symbol("App\\Lib");
        assert!(flag.starts_with("__inc1_"));
        assert!(active.starts_with("__fv_"));
        assert!(dispatch.starts_with("fn_gv_"));
        assert_ne!(flag, active);
        assert_ne!(active, dispatch);
    }
}
