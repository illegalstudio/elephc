//! Purpose:
//! Lowers `Op::MethodCall` and `Op::StaticMethodCall` for the wasm32-wasi backend,
//! and emits the per-(introducer, method) dispatch stubs that route virtual
//! instance-method calls to the runtime class's override.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction` dispatches the two ops here.
//! - `crate::codegen_wasm::generate` calls `emit_method_dispatch_stubs` after the
//!   class-method lowering loop, so every `call $<stub>` emitted by
//!   `lower_method_call` resolves to a defined function.
//!
//! Key details:
//! - WASM has no call-to-register, so the closed AOT class set is branched
//!   explicitly: each dispatch stub reads the receiver's `class_id` from
//!   `[obj + 0]` and walks an `i64.eq` if-ladder over the concrete subclass ids,
//!   tail-calling the matching implementation. One stub per introducing class +
//!   method key (the topmost class declaring the virtual method), so unrelated
//!   hierarchies that happen to share a method name never collide.
//! - Instance calls take the direct path when the method is non-virtual (no
//!   vtable slot, or `final`); otherwise they call the introducer's stub.
//! - True static calls push a constant `called_class_id` (i64 hidden param 0)
//!   then the user args. Lexical `self::`/`parent::` calls that resolve to an
//!   instance method forward the current `this` (slot 0) instead, which is what
//!   makes `parent::__construct()` chaining work.

use super::context::{wasm_fn_symbol, FnCtx, Result};
use super::inst::{data_immediate, operand};
use super::values::WasmRepr;
use super::wat::WatModule;
use super::WasmError;
use crate::ir::{Function, Instruction, LocalSlotId, Module};
use crate::names::php_symbol_key;
use crate::types::PhpType;
use std::collections::HashMap;

/// Lowers an `Op::MethodCall` to a direct or dispatched instance call.
///
/// `operands[0]` is the receiver; `operands[1..]` are the user arguments. The
/// receiver's `PhpType` must be `Object(class)`; `Mixed`/`Union` receivers are
/// rejected as a P6f concern. Variadic and by-reference parameters are rejected
/// here (out of P6d scope); the frontend guarantees arity for the rest.
pub(super) fn lower_method_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let method_name = ctx
        .module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .ok_or_else(|| WasmError::Unsupported(format!("method call: unknown data {:?}", data_id)))?
        .clone();
    let method_key = php_symbol_key(&method_name);

    let receiver = operand(inst, 0)?;
    let receiver_ty = ctx.value_php_type(receiver)?;
    let class_name = match receiver_ty {
        PhpType::Object(c) => c,
        PhpType::Mixed | PhpType::Union(_) => {
            return Err(WasmError::Unsupported(format!(
                "method call on {:?} receiver (P6f: mixed/union dispatch)",
                receiver_ty
            )));
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "method call on non-object receiver {:?}",
                other
            )));
        }
    };

    let ci = ctx
        .module
        .class_infos
        .get(&class_name)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", class_name)))?;
    let callee_sig = ci
        .methods
        .get(&method_key)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown method {}::{}", class_name, method_name)))?;
    if callee_sig.variadic.is_some() {
        return Err(WasmError::Unsupported(format!(
            "variadic method {}::{} (out of P6d scope)",
            class_name, method_name
        )));
    }
    if callee_sig.ref_params.iter().any(|r| *r) {
        return Err(WasmError::Unsupported(format!(
            "by-reference parameter in {}::{} (out of P6d scope)",
            class_name, method_name
        )));
    }

    let has_slot = ci.vtable_slots.contains_key(&method_key);
    let is_final = ci.final_methods.contains(&method_key);
    let dynamic = has_slot && !is_final;
    let impl_class = ci
        .method_impl_classes
        .get(&method_key)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    let callee_symbol = if dynamic {
        let introducer = resolve_vtable_introducer(ctx, &class_name, &method_key)?;
        wasm_fn_symbol(&format!("__dispatch::inst::{}::{}", introducer, method_key))
    } else {
        wasm_fn_symbol(&format!("{}::{}", impl_class, method_name))
    };
    let mode = if dynamic { "dispatch" } else { "direct" };

    let return_arity = WasmRepr::val_types(inst.result_type).len();
    ctx.emit_load_value(receiver)?;
    for &arg in inst.operands.iter().skip(1) {
        ctx.emit_load_value(arg)?;
    }
    ctx.fb.ins(
        &format!("call ${}", callee_symbol),
        &format!("{}::{} ({})", class_name, method_name, mode),
    );

    if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    } else {
        for _ in 0..return_arity {
            ctx.fb.ins("drop", "discard unused method result");
        }
    }
    Ok(())
}

/// Lowers an `Op::StaticMethodCall` to either a true static call or a lexical
/// instance-method call.
///
/// The immediate carries `"{Receiver}::{method}"` where `Receiver` is the
/// original-case receiver token (`self`, `parent`, a class name, …). True
/// static methods receive a constant `called_class_id` as hidden param 0;
/// `self::`/`parent::` calls that resolve to an instance method forward the
/// current `this` (slot 0) so `parent::__construct()` chains correctly. `static::`
/// late-bound dispatch is deferred (P6d scope) and rejected here.
pub(super) fn lower_static_method_call(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let data_id = data_immediate(inst)?;
    let target = ctx
        .module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .ok_or_else(|| WasmError::Unsupported(format!("static call: unknown data {:?}", data_id)))?
        .clone();
    let (receiver_label, method_name) = target
        .rsplit_once("::")
        .ok_or_else(|| WasmError::Unsupported(format!("malformed static call {}", target)))?;
    let method_key = php_symbol_key(method_name);

    if receiver_label == "static" {
        return Err(WasmError::Unsupported(format!(
            "static::{} late-bound dispatch (out of P6d scope)",
            method_name
        )));
    }

    let current_class: Option<String> = ctx
        .function
        .name
        .rsplit_once("::")
        .map(|(c, _)| c.to_string());
    let is_instance_fn = ctx.function.flags.is_method && !ctx.function.flags.is_static;

    let receiver_class = match receiver_label {
        "self" => current_class
            .clone()
            .ok_or_else(|| WasmError::Unsupported("self:: outside a method".to_string()))?,
        "parent" => {
            let cur = current_class
                .as_ref()
                .ok_or_else(|| WasmError::Unsupported("parent:: outside a method".to_string()))?;
            ctx.module
                .class_infos
                .get(cur)
                .and_then(|ci| ci.parent.clone())
                .ok_or_else(|| WasmError::Unsupported(format!("class {} has no parent", cur)))?
        }
        named => named.to_string(),
    };

    let ci = ctx
        .module
        .class_infos
        .get(&receiver_class)
        .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", receiver_class)))?;

    let true_static = ci.static_methods.contains_key(&method_key);
    let lexical_instance = !true_static
        && (receiver_label == "self" || receiver_label == "parent")
        && is_instance_fn
        && ci.methods.contains_key(&method_key);

    let return_arity = WasmRepr::val_types(inst.result_type).len();

    if true_static {
        let impl_class = ci
            .static_method_impl_classes
            .get(&method_key)
            .cloned()
            .unwrap_or_else(|| receiver_class.clone());
        let callee_symbol = wasm_fn_symbol(&format!("{}::{}", impl_class, method_name));
        ctx.fb.ins(
            &format!("i64.const {}", ci.class_id as i64),
            &format!("{}::{} called_class_id", receiver_class, method_name),
        );
        for &arg in &inst.operands {
            ctx.emit_load_value(arg)?;
        }
        ctx.fb.ins(
            &format!("call ${}", callee_symbol),
            &format!("{}::{} (static)", receiver_class, method_name),
        );
    } else if lexical_instance {
        let impl_class = ci
            .method_impl_classes
            .get(&method_key)
            .cloned()
            .unwrap_or_else(|| receiver_class.clone());
        let callee_symbol = wasm_fn_symbol(&format!("{}::{}", impl_class, method_name));
        // Forward the current `this` (slot 0) as the receiver of the instance method.
        ctx.emit_load_slot(LocalSlotId::from_raw(0))?;
        for &arg in &inst.operands {
            ctx.emit_load_value(arg)?;
        }
        ctx.fb.ins(
            &format!("call ${}", callee_symbol),
            &format!("{}::{} (lexical instance via {}::)", impl_class, method_name, receiver_label),
        );
    } else {
        return Err(WasmError::Unsupported(format!(
            "unresolvable static call {} (static method not found; lexical instance fallback \
             not applicable)",
            target
        )));
    }

    if let Some(r) = inst.result {
        ctx.emit_store_value(r)?;
    } else {
        for _ in 0..return_arity {
            ctx.fb.ins("drop", "discard unused static method result");
        }
    }
    Ok(())
}

/// Walks the parent chain from `class_name` upward and returns the topmost class
/// whose `vtable_slots` contains `method_key`.
///
/// That class is the *introducer* of the virtual method: the one whose dispatch
/// stub enumerates the whole subtree of possible runtime receiver class ids. All
/// callers whose static type sits in that subtree resolve to the same stub.
fn resolve_vtable_introducer(ctx: &FnCtx, class_name: &str, method_key: &str) -> Result<String> {
    let mut current = class_name.to_string();
    loop {
        let ci = ctx
            .module
            .class_infos
            .get(&current)
            .ok_or_else(|| WasmError::Unsupported(format!("unknown class {}", current)))?;
        match &ci.parent {
            Some(parent) => {
                let parent_ci = ctx
                    .module
                    .class_infos
                    .get(parent)
                    .ok_or_else(|| WasmError::Unsupported(format!("unknown parent {}", parent)))?;
                if parent_ci.vtable_slots.contains_key(method_key) {
                    current = parent.clone();
                    continue;
                }
                return Ok(current);
            }
            None => return Ok(current),
        }
    }
}

/// Emits one dispatch stub per (introducer, method key), for every virtual
/// (non-final) method in the module's class set.
///
/// Each stub's if-ladder covers exactly the concrete classes in the introducer's
/// subtree that carry the slot, tail-calling the implementation resolved via
/// `method_impl_classes`. Stubs with no concrete implementer are skipped (such a
/// method is never dispatched in a valid program); stubs are non-exported and
/// reached only through `call $<stub>`.
pub(super) fn emit_method_dispatch_stubs(wm: &mut WatModule, module: &Module) -> Result<()> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for (name, ci) in &module.class_infos {
        if let Some(parent) = &ci.parent {
            children
                .entry(parent.clone())
                .or_default()
                .push(name.clone());
        }
    }

    for (introducer, ci) in &module.class_infos {
        for method_key in ci.vtable_slots.keys() {
            let method_key = method_key.as_str();
            if ci.final_methods.contains(method_key) {
                continue;
            }
            // Only the introducer emits the stub: the parent must not also carry the slot.
            let is_introducer = match &ci.parent {
                None => true,
                Some(parent) => module
                    .class_infos
                    .get(parent)
                    .map(|p| !p.vtable_slots.contains_key(method_key))
                    .unwrap_or(true),
            };
            if !is_introducer {
                continue;
            }

            let subtree = collect_concrete_subtree(module, &children, introducer, method_key);
            let mut arms: Vec<(u64, String)> = Vec::new();
            let mut sig_fn: Option<&Function> = None;
            for class_name in &subtree {
                let class_ci = module
                    .class_infos
                    .get(class_name)
                    .ok_or_else(|| WasmError::Unsupported(format!("missing class {}", class_name)))?;
                let impl_class = class_ci
                    .method_impl_classes
                    .get(method_key)
                    .cloned()
                    .unwrap_or_else(|| class_name.clone());
                if let Some(f) = find_method_function(&module.class_methods, &impl_class, method_key)
                {
                    arms.push((class_ci.class_id, wasm_fn_symbol(&f.name)));
                    if sig_fn.is_none() {
                        sig_fn = Some(f);
                    }
                }
            }
            let Some(sig_f) = sig_fn else {
                // No concrete implementer in the subtree: the method is never
                // dispatched at runtime, so no stub is needed.
                continue;
            };

            let stub_symbol =
                wasm_fn_symbol(&format!("__dispatch::inst::{}::{}", introducer, method_key));
            let wat = build_dispatch_stub(&stub_symbol, sig_f, &arms);
            wm.add_raw_func(&wat);
        }
    }
    Ok(())
}

/// Collects the introducer plus all transitive subclasses that are concrete and
/// carry `method_key` in their vtable slots.
///
/// The result is exactly the set of runtime class ids a receiver typed anywhere
/// in the subtree can have, which is what the stub's if-ladder must cover.
fn collect_concrete_subtree(
    module: &Module,
    children: &HashMap<String, Vec<String>>,
    introducer: &str,
    method_key: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = vec![introducer.to_string()];
    while let Some(name) = queue.pop() {
        let ci = match module.class_infos.get(&name) {
            Some(c) => c,
            None => continue,
        };
        if !ci.is_abstract && ci.vtable_slots.contains_key(method_key) {
            out.push(name.clone());
        }
        if let Some(kids) = children.get(&name) {
            for k in kids {
                queue.push(k.clone());
            }
        }
    }
    out
}

/// Finds the class-method `Function` that implements `method_key` for `impl_class`,
/// matching case-insensitively on the method name.
///
/// Returns the `Function` (whose `name` is `"{impl_class}::{original_method}"`)
/// so the caller can both form the call symbol and read the authoritative
/// parameter/result IR types for the stub signature.
fn find_method_function<'a>(
    class_methods: &'a [Function],
    impl_class: &str,
    method_key: &str,
) -> Option<&'a Function> {
    class_methods.iter().find(|f| match f.name.rsplit_once("::") {
        Some((cls, m)) => cls == impl_class && php_symbol_key(m) == method_key,
        None => false,
    })
}

/// Builds the raw WAT body of a dispatch stub from the signature function and the
/// concrete (class_id, call symbol) arms.
///
/// The stub re-declares `this` plus the user parameters (skipping the signature's
/// `$this` param 0), reads the runtime class id, and branches to each arm. The
/// fall-through is `unreachable` because the closed class set guarantees a match.
fn build_dispatch_stub(stub_symbol: &str, sig_fn: &Function, arms: &[(u64, String)]) -> String {
    let mut wat = String::new();
    wat.push_str(&format!("(func ${}\n", stub_symbol));

    let mut param_decls: Vec<String> = Vec::new();
    let mut forward_loads: Vec<String> = Vec::new();
    let mut user_counter = 0u32;
    for (pi, p) in sig_fn.params.iter().enumerate() {
        for (vi, vt) in WasmRepr::val_types(p.ir_type).iter().enumerate() {
            let name = if pi == 0 && vi == 0 {
                "$this".to_string()
            } else {
                user_counter += 1;
                format!("$p{}", user_counter)
            };
            param_decls.push(format!("(param {} {})", name, vt.as_str()));
            forward_loads.push(format!("local.get {}", name));
        }
    }
    for pd in &param_decls {
        wat.push_str(&format!("  {}\n", pd));
    }

    let result_types = WasmRepr::val_types(sig_fn.return_type);
    if !result_types.is_empty() {
        let rstr = result_types
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        wat.push_str(&format!("  (result {})\n", rstr));
    }
    wat.push_str("  (local $cid i64)\n");

    wat.push_str("  ;; read the runtime class id from the object payload at +0\n");
    wat.push_str("  local.get $this\n");
    wat.push_str("  i64.load offset=0\n");
    wat.push_str("  local.set $cid\n");

    for (class_id, fn_symbol) in arms {
        wat.push_str(&format!(
            "  ;; dispatch arm for class id {}\n",
            class_id
        ));
        wat.push_str(&format!("  local.get $cid\n  i64.const {}\n  i64.eq\n  (if (then\n", *class_id as i64));
        for load in &forward_loads {
            wat.push_str(&format!("    {}\n", load));
        }
        wat.push_str(&format!("    call ${}\n    return))\n", fn_symbol));
    }

    wat.push_str("  ;; closed class set guarantees an arm matched\n");
    wat.push_str("  unreachable\n)\n");
    wat
}