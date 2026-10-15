//! Purpose:
//! Loaded-extension queries: the extension set of this target, `extension_loaded` and `get_loaded_extensions`.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.
//! - `crate::codegen_wasm::plan` reads `loaded_extension_answer_strings`.

use super::*;

/// The extensions `extension_loaded()` and `get_loaded_extensions()` report on `wasm32-wasi`.
///
/// The native backend's rule is not "what php-src was built with" — it answers `curl: no` where
/// the comparison host answers yes — but "what THIS BINARY provides": a core set plus whatever
/// bridges the program links. A wasm module links nothing at all, so the second half is empty by
/// construction and only the core set is in question.
///
/// The rule is therefore "does this target serve that extension", and it is applied rather than
/// copied. A representative function of `date`, `ctype`, `mbstring`, `Reflection` and
/// `Zend OPcache` is refused by this target's own audit, so reporting them loaded would break
/// the one thing the builtin is for: `if (extension_loaded('date')) { date(...); }` would take
/// the branch and then fail to COMPILE, where answering false lets the program build and take
/// its fallback.
///
/// `bcmath`, `zlib` and `pcre` are here because this target now implements them, and each is a
/// SUBSET. That is deliberate and it does not weaken the rule: a construct outside the subset is
/// refused at compile time naming itself — `bcsub`, a backreference, `gzopen` — so a guarded
/// branch that reaches one fails loudly and locally, where the whole extension being absent
/// would have sent every user of it down a fallback path it did not need.
///
/// `extension_loaded_set_is_honest` pins the list against the compiler itself, which is how
/// `pcre` came to be here: the test failed the moment `preg_match` started compiling.
pub(in crate::codegen_wasm) const WASM_LOADED_EXTENSIONS: &[&str] =
    &["Core", "standard", "SPL", "json", "bcmath", "zlib", "pcre"];

/// The Zend extensions `get_loaded_extensions(true)` reports on `wasm32-wasi`.
///
/// Empty, and for the same reason: elephc's only Zend extension is OPcache, whose surface this
/// target refuses. php-src answers `["Zend OPcache"]` and so does the native backend.
pub(in crate::codegen_wasm) const WASM_LOADED_ZEND_EXTENSIONS: &[&str] = &[];

/// Whether `name` is one of the extensions this target serves, compared php's way.
///
/// Case-insensitive on the canonical name only, which is php's own rule: `JSON` matches `json`
/// and `opcache` is NOT an alias for `Zend OPcache`.
pub(super) fn wasm_extension_is_loaded(name: &str) -> bool {
    WASM_LOADED_EXTENSIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

/// The list `get_loaded_extensions($zend_extensions)` answers, or `None` when the flag is not a
/// compile-time constant.
///
/// Single source of truth for the gate and the lowering, so they can never report different sets.
/// The flag is the only operand and php defaults it to false; anything but a literal leaves the
/// answer undecidable here, which the gate turns into a refusal rather than a guess.
pub(super) fn loaded_extension_names(function: &Function, call: &Instruction) -> Option<&'static [&'static str]> {
    let zend = match call.operands.first() {
        None => false,
        Some(operand) => {
            let inst = function
                .values
                .get(operand.as_raw() as usize)
                .and_then(|value| match value.def {
                    crate::ir::ValueDef::Instruction { inst, .. } => function.instruction(inst),
                    _ => None,
                })?;
            if inst.op != Op::ConstBool {
                return None;
            }
            match inst.immediate {
                Some(Immediate::Bool(flag)) => flag,
                _ => return None,
            }
        }
    };
    Some(if zend {
        WASM_LOADED_ZEND_EXTENSIONS
    } else {
        WASM_LOADED_EXTENSIONS
    })
}

/// Every string `get_loaded_extensions()` can answer with in this module, for the data layout.
///
/// Both lists, because which one a given call picks is the flag's business and the planner lays
/// bytes out once for the whole module.
pub(in crate::codegen_wasm) fn loaded_extension_answer_strings(module: &Module) -> Vec<String> {
    let calls = module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .any(|function| {
            function.instructions.iter().any(|inst| {
                matches!(
                    inst.immediate.as_ref(),
                    Some(Immediate::RuntimeCall(
                        RuntimeCallTarget::Function(RuntimeFnId::GetLoadedExtensions)
                            | RuntimeCallTarget::ProfiledFunction {
                                target: RuntimeFnId::GetLoadedExtensions,
                                ..
                            }
                    ))
                )
            })
        });
    if !calls {
        return Vec::new();
    }
    WASM_LOADED_EXTENSIONS
        .iter()
        .chain(WASM_LOADED_ZEND_EXTENSIONS.iter())
        .map(|name| (*name).to_string())
        .collect()
}

/// Validates `get_loaded_extensions()`: an optional constant flag in, `array<string>` out.
pub(super) fn get_loaded_extensions_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if call.operands.len() > 1 {
        return Some(format!(
            "get_loaded_extensions takes at most one flag, got {}",
            call.operands.len()
        ));
    }
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "get_loaded_extensions answers array<string>, not {:?}",
            call.result_type
        ));
    }
    if loaded_extension_names(function, call).is_none() {
        return Some(
            "get_loaded_extensions needs a literal $zend_extensions flag: the set is settled when \
             the module is built, so a runtime flag has no answer to choose between"
                .to_string(),
        );
    }
    None
}

/// Emits a fresh `array<string>` holding `names`, and leaves its pointer on the stack.
///
/// Shared by every builtin whose whole answer is a list of names settled when the module is
/// built — the loaded extensions, the declared classes, the SPL catalogue. Each name must
/// already be laid out in static data by the planner, which is what `default_str_literal`
/// resolves; a name the planner did not place has no address and fails here rather than
/// silently reading whatever sits at offset zero.
pub(super) fn emit_name_array(ctx: &mut FnCtx, names: &[String], what: &str) -> Result<String> {
    let array = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    ctx.fb.ins(
        &format!(
            "(local.set {} (call $__rt_array_new (i64.const {}) (i64.const 16)))",
            array,
            names.len().max(1)
        ),
        what,
    );
    for name in names {
        let (offset, len) = ctx.default_str_literal(name)?;
        ctx.fb.ins(
            &format!(
                "(local.set {} (call $__rt_array_push_str (local.get {}) (i32.const {}) (i64.const {})))",
                array, array, offset, len
            ),
            &format!("{name:?}"),
        );
    }
    Ok(array)
}

/// Lowers `get_loaded_extensions()` to the constant list this target serves.
pub(super) fn lower_get_loaded_extensions(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let names = loaded_extension_names(ctx.function, inst)
        .ok_or_else(|| WasmError::Unsupported("get_loaded_extensions flag is not a literal".into()))?;
    let names: Vec<String> = names.iter().map(|name| (*name).to_string()).collect();
    let array = emit_name_array(ctx, &names, "the extension list, one string slot per entry")?;
    ctx.fb.ins(&format!("local.get {}", array), "the finished list");
    store_result(ctx, inst)
}
