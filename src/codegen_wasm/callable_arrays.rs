//! Purpose:
//! Array-callable dispatch for the wasm32-wasi backend: PHP's `[$object, "method"]` and
//! `["Class", "method"]` callables, which `Op::CallableDescriptorInvoke` carries as an ARRAY
//! operand rather than the `I64` Callable descriptor `closures.rs` dispatches.
//!
//! Called from:
//! - `plan::lower_module` — `emit_callable_array_dispatch` emits the wrappers and the two
//!   ladders, after class methods are lowered (a wrapper calls the method body) and before
//!   `wm.render()`.
//! - `closures::lower_callable_descriptor_invoke` — routes an array-typed callable operand
//!   into `__rt_cbarr_invoke_mixed` / `__rt_cbarr_invoke_str`.
//! - `capability::callable_descriptor_invoke_shape_issue` — `array_form` and
//!   `unsupported_target_issue` decide, before any code is emitted, whether this module's
//!   candidate set can actually be lowered.
//!
//! Key details:
//! - There is no `funcref` table in this backend, so dispatch is an IF-LADDER: the receiver's
//!   `class_id` (or the class-name bytes) and the method-name bytes are compared arm by arm,
//!   and the first match calls that method's wrapper. The candidate set is every PUBLIC method
//!   of every class in `module.class_infos` whose body this module actually compiles — the same
//!   selection the native backend makes in `codegen::lower_inst::callables`.
//! - Two element layouts, because the two PHP spellings give the array two different EIR types:
//!   `[$obj, "m"]` is `array<mixed>` (`value_type` 7 — 16-byte slots each holding a Mixed CELL
//!   pointer), while `["C", "m"]` is `array<string>` (`value_type` 1 — 16-byte slots holding
//!   `(ptr, len)` inline). Reading one with the other's layout would silently dispatch on
//!   garbage, so each ladder validates the `value_type` it expects before touching a slot.
//! - Method names are compared BYTE-EXACT, mirroring native. PHP itself resolves method names
//!   case-insensitively; neither backend does, so this is a shared gap rather than a wasm one,
//!   and matching native keeps the two columns of the parity sweep honest.
//! - Name comparison is emitted INLINE (a length test then one `i32.load8_u` per byte, each
//!   exiting the arm through `br_if`) instead of interning the name in a data segment. The
//!   `br_if` chain short-circuits, so a shorter string is never read past its end.
//! - Every rejection funnels into `__rt_fail_callable_dispatch`, whose `unreachable` carries the
//!   `elephc-trap:post-noreturn:` marker `traps.rs` requires.

use std::collections::BTreeSet;

use crate::ir::{Function, Instruction, IrHeapKind, IrType, Module, Op};
use crate::names::php_symbol_key;
use crate::parser::ast::Visibility;
use crate::types::PhpType;

use super::closures::{append_arg_array_guard, box_function_result_wat, build_fcc_wrapper, unbox_arg_wat};
use super::symbols::function_symbol;
use super::wat::WatModule;
use super::context::Result;
use super::WasmError;

/// Mixed-cell field offsets (`mixed.rs`: `[P+0 tag][P+8 lo][P+16 hi]`).
const CELL_TAG: u32 = 0;
const CELL_LO: u32 = 8;
const CELL_HI: u32 = 16;

/// Mixed tags this dispatch reads (`mixed.rs`).
const TAG_STRING: i64 = 1;
const TAG_OBJECT: i64 = 6;

/// Indexed-array payload: length `+0`, capacity `+8`, stride `+16`, elements from `+24`.
const ARRAY_ELEMENTS: u32 = 24;
const ARRAY_SLOT_BYTES: u32 = 16;

/// `value_type` of an `array<string>` (inline `(ptr, len)`) and of an `array<mixed>`
/// (one Mixed-cell pointer per slot).
const VALUE_TYPE_STRING: i32 = 1;
const VALUE_TYPE_MIXED_CELL: i32 = 7;

/// Which array spelling a callable operand uses, decided from its EIR PHP type alone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ArrayCallableForm {
    /// `[$object, "method"]` — an `array<mixed>` of Mixed cells.
    InstanceMixed,
    /// `["Class", "method"]` — an `array<string>` of inline strings.
    StaticString,
}

/// Returns the array-callable form of `php`, or `None` when it is not an array callable.
pub(super) fn array_form(php: &PhpType) -> Option<ArrayCallableForm> {
    match php.codegen_repr() {
        PhpType::Array(element) => match element.codegen_repr() {
            PhpType::Mixed => Some(ArrayCallableForm::InstanceMixed),
            PhpType::Str => Some(ArrayCallableForm::StaticString),
            _ => None,
        },
        _ => None,
    }
}

/// The ladder entry point a given form dispatches through.
pub(super) fn ladder_symbol(form: ArrayCallableForm) -> &'static str {
    match form {
        ArrayCallableForm::InstanceMixed => "__rt_cbarr_invoke_mixed",
        ArrayCallableForm::StaticString => "__rt_cbarr_invoke_str",
    }
}

/// One ladder arm: a public method whose body this module compiles.
struct Target<'a> {
    /// The class the receiver must be an instance OF — the `class_infos` key, so an inherited
    /// method contributes one arm per inheriting class, each with that class's own id.
    class_name: String,
    class_id: u64,
    /// The method name as DECLARED, which is what the runtime string is compared against.
    method_name: String,
    /// The body to call, which for an inherited method belongs to the implementing class.
    function: &'a Function,
}

/// Collects every public method of every class whose body this module compiles, split by
/// staticness.
///
/// Mirrors the native selection (`runtime_array_instance_method_targets_for_descriptor` and
/// `runtime_static_method_descriptor_cases`): walk `class_infos` in name order, keep public
/// methods, resolve each through `method_impl_classes` so an inherited method dispatches to the
/// implementing body, and drop any whose body this module never lowered.
fn collect_targets(module: &Module) -> (Vec<Target<'_>>, Vec<Target<'_>>) {
    let mut instance = Vec::new();
    let mut statics = Vec::new();
    let mut classes: Vec<_> = module.class_infos.iter().collect();
    classes.sort_by(|left, right| left.0.cmp(right.0));
    for (class_name, class_info) in classes {
        // Instance and static methods live in PARALLEL maps, each with its own visibility and
        // implementing-class table; walking only `methods` finds no static method at all.
        let mut names: Vec<(&String, bool)> = class_info
            .methods
            .keys()
            .map(|name| (name, false))
            .chain(class_info.static_methods.keys().map(|name| (name, true)))
            .collect();
        names.sort();
        for (method_name, is_static) in names {
            let (visibilities, impl_classes) = if is_static {
                (
                    &class_info.static_method_visibilities,
                    &class_info.static_method_impl_classes,
                )
            } else {
                (
                    &class_info.method_visibilities,
                    &class_info.method_impl_classes,
                )
            };
            if !visibilities
                .get(method_name)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            {
                continue;
            }
            let method_key = php_symbol_key(method_name);
            let impl_class = impl_classes
                .get(&method_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            let Some(function) = lowered_method(module, &impl_class, &method_key) else {
                continue;
            };
            let target = Target {
                class_name: class_name.clone(),
                class_id: class_info.class_id,
                method_name: method_name.clone(),
                function,
            };
            // The BODY's own flag decides, not the map it was listed in: those must agree, and
            // if they ever disagree the wrapper shape has to follow the body it calls.
            if function.flags.is_static {
                statics.push(target);
            } else {
                instance.push(target);
            }
        }
    }
    (instance, statics)
}

/// Finds the lowered body for `Class::method`, or `None` when this module never compiled it.
fn lowered_method<'a>(module: &'a Module, class: &str, method_key: &str) -> Option<&'a Function> {
    let want = php_symbol_key(&format!("{}::{}", class.trim_start_matches('\\'), method_key));
    module
        .class_methods
        .iter()
        .find(|f| php_symbol_key(f.name.trim_start_matches('\\')) == want)
}

/// Returns why `target` cannot be given a wrapper, or `None` when it can.
///
/// The wrapper unboxes each argument from the positional buffer, so a by-ref or variadic
/// parameter — which has no slot to unbox from — is refused rather than silently dropped. An
/// instance target additionally needs its `$this` parameter to be the object it dispatches on.
fn target_issue(target: &Target<'_>) -> Option<String> {
    let function = target.function;
    for param in &function.params {
        if param.by_ref || param.variadic {
            return Some(format!(
                "array-callable target {}::{} has by-ref/variadic param {}",
                target.class_name, target.method_name, param.name
            ));
        }
    }
    if function.flags.is_static {
        if function.params.is_empty() {
            return Some(format!(
                "static array-callable target {}::{} has no called-class parameter",
                target.class_name, target.method_name
            ));
        }
    } else {
        match function.params.first() {
            Some(param) if param.ir_type == IrType::Heap(IrHeapKind::Object) => {}
            _ => {
                return Some(format!(
                    "instance array-callable target {}::{} does not take an object receiver",
                    target.class_name, target.method_name
                ))
            }
        }
    }
    None
}

/// Returns the first reason this module's array-callable ladder cannot be built, or `None`.
///
/// Called by the capability audit so a module is refused BEFORE emission rather than producing
/// a `.wat` that references a wrapper no pass ever wrote.
pub(super) fn unsupported_target_issue(module: &Module, form: ArrayCallableForm) -> Option<String> {
    let (instance, statics) = collect_targets(module);
    let targets = match form {
        ArrayCallableForm::InstanceMixed => instance,
        ArrayCallableForm::StaticString => statics,
    };
    if targets.is_empty() {
        return Some(format!(
            "array-callable invoke has no {} targets in this module",
            match form {
                ArrayCallableForm::InstanceMixed => "public instance-method",
                ArrayCallableForm::StaticString => "public static-method",
            }
        ));
    }
    targets.iter().find_map(target_issue)
}

/// Whether any instruction in the module invokes an array callable of `form`.
///
/// Gates emission: a module with no such site must not carry the ladder, both to keep the
/// binary at its floor and because the ladder's wrappers would otherwise be the only reference
/// to method bodies the linker could then never drop.
fn module_uses(module: &Module, form: ArrayCallableForm) -> bool {
    super::function::all_functions(module)
        .any(|function| {
            function
                .instructions
                .iter()
                .any(|inst| invoke_form(function, inst) == Some(form))
        })
}

/// The array-callable form of one instruction, or `None` when it is not such an invoke.
pub(super) fn invoke_form(function: &Function, inst: &Instruction) -> Option<ArrayCallableForm> {
    if inst.op != Op::CallableDescriptorInvoke {
        return None;
    }
    let callable = *inst.operands.first()?;
    array_form(&function.value(callable)?.php_type)
}

/// Emits the array-callable wrappers and ladders this module actually uses.
///
/// Must run after class methods are lowered: a wrapper calls the method body by symbol.
pub(super) fn emit_callable_array_dispatch(wm: &mut WatModule, module: &Module) -> Result<()> {
    let uses_instance = module_uses(module, ArrayCallableForm::InstanceMixed);
    let uses_static = module_uses(module, ArrayCallableForm::StaticString);
    if !uses_instance && !uses_static {
        return Ok(());
    }
    let (instance, statics) = collect_targets(module);

    // One wrapper symbol per target, deduplicated: two classes inheriting the same method
    // resolve to the SAME body, and emitting its wrapper twice is a duplicate definition.
    let mut emitted: BTreeSet<String> = BTreeSet::new();

    let mut instance_arms = Vec::new();
    if uses_instance {
        for target in &instance {
            if let Some(issue) = target_issue(target) {
                return Err(WasmError::Unsupported(issue));
            }
            let symbol = instance_wrapper_symbol(target.function);
            if emitted.insert(symbol.clone()) {
                wm.add_raw_func(&build_instance_wrapper(&symbol, target.function)?);
            }
            instance_arms.push((target.class_id, target.method_name.clone(), symbol));
        }
        wm.add_raw_func(&build_instance_ladder(&instance_arms));
    }

    if uses_static {
        let mut static_arms = Vec::new();
        for target in &statics {
            if let Some(issue) = target_issue(target) {
                return Err(WasmError::Unsupported(issue));
            }
            let symbol = static_wrapper_symbol(target.function);
            if emitted.insert(symbol.clone()) {
                let class_id = i64::try_from(target.class_id).map_err(|_| {
                    WasmError::Unsupported(format!(
                        "class id of {} exceeds the WASM called-class range",
                        target.class_name
                    ))
                })?;
                wm.add_raw_func(&build_fcc_wrapper(&symbol, target.function, Some(class_id))?);
            }
            static_arms.push((
                target.class_name.clone(),
                target.method_name.clone(),
                symbol,
            ));
        }
        wm.add_raw_func(&build_static_ladder(&static_arms));
    }
    Ok(())
}

/// Wrapper symbol for an instance target (receiver passed separately from the arg buffer).
fn instance_wrapper_symbol(function: &Function) -> String {
    format!("fn_cbarr_inst_{}", function_symbol(function))
}

/// Wrapper symbol for a static target (called-class id forwarded, like a first-class callable).
fn static_wrapper_symbol(function: &Function) -> String {
    format!("fn_cbarr_static_{}", function_symbol(function))
}

/// Builds the wrapper for one instance method.
///
/// `(param $recv i32) (param $args i32) (result i32)`: the receiver arrives as its own
/// parameter — it lives in the callable ARRAY, not the argument buffer — and the remaining
/// parameters are unboxed from arg slots `0..n-1`. The body OWNS its `$this`, so the receiver
/// is increfed here exactly as `unbox_arg_wat` increfs an object argument.
fn build_instance_wrapper(wrapper_symbol: &str, f: &Function) -> Result<String> {
    let body_symbol = function_symbol(f);
    let mut wat = String::new();
    wat.push_str(&format!(
        "(func ${} (param $recv i32) (param $args i32) (result i32)\n",
        wrapper_symbol
    ));
    wat.push_str("  (local $ub_tag i64) (local $ub_lo i64) (local $ub_hi i64)\n");
    wat.push_str(
        "  (local $rb_i64 i64) (local $rb_f64 f64) (local $rb_ptr i32) (local $rb_len i64)\n",
    );
    wat.push_str("  (local $args_len i64) (local $args_capacity i64) (local $args_size i32)\n");

    // The receiver occupies parameter 0, so the buffer only has to carry the rest.
    let required_count = i64::try_from(f.params.len() - 1).map_err(|_| {
        WasmError::Unsupported(format!("array-callable {} parameter count exceeds i64", f.name))
    })?;
    append_arg_array_guard(&mut wat, required_count);

    // Folded form: `call $f (local.get $x)` would be read as a plain `call` (whose operand is
    // not on the stack yet) followed by a stray push.
    wat.push_str(&super::closures::wat_ins(
        "(call $__rt_incref (local.get $recv))",
        "the body owns its $this",
    ));
    wat.push_str(&super::closures::wat_ins(
        "local.get $recv",
        "push the receiver as parameter 0",
    ));
    for (i, p) in f.params.iter().skip(1).enumerate() {
        let slot_off = ARRAY_ELEMENTS as usize + i * ARRAY_SLOT_BYTES as usize;
        wat.push_str(&format!(
            "  ;; unbox arg {} (param {} : {:?}) from arg slot +{}\n",
            i, p.name, p.ir_type, slot_off
        ));
        wat.push_str(&format!(
            "  (i32.wrap_i64 (i64.load offset={} (local.get $args)))\n",
            slot_off
        ));
        wat.push_str(&unbox_arg_wat(&p.ir_type, &p.php_type)?);
    }
    wat.push_str(&format!("  call ${}\n", body_symbol));
    wat.push_str("  ;; box the body result into a Mixed cell (result i32)\n");
    wat.push_str(&box_function_result_wat(f)?);
    wat.push_str(")\n");
    Ok(wat)
}

/// Emits the header validation every callable-array ladder runs before reading a slot.
///
/// Checks the same invariants as the argument-buffer guard — heap range, alignment, indexed
/// kind, live refcount, payload bounds — plus the two this dispatch depends on: at least two
/// elements, and the `value_type` matching the layout the caller is about to read.
fn append_callable_array_guard(wat: &mut String, value_type: i32, tag: &str) {
    let fail = |what: &str| {
        format!(
            "    (then (call $__rt_fail_callable_dispatch) unreachable)) ;; elephc-trap:post-noreturn:cbarr-{}-{}\n",
            tag, what
        )
    };
    wat.push_str("  ;; validate the callable array header before reading a slot\n");
    wat.push_str(
        "  (if (i32.lt_u (local.get $arr) (i32.add (global.get $__heap_base) (i32.const 16)))\n",
    );
    wat.push_str(&fail("before-heap"));
    wat.push_str("  (if (i32.ge_u (local.get $arr) (global.get $__heap_ptr))\n");
    wat.push_str(&fail("after-heap"));
    wat.push_str("  (if (i32.ne (i32.and (local.get $arr) (i32.const 7)) (i32.const 0))\n");
    wat.push_str(&fail("alignment"));
    wat.push_str(
        "  (if (i32.ne (i32.and (i32.wrap_i64 (i64.load (i32.sub (local.get $arr) (i32.const 8)))) (i32.const 255)) (i32.const 2))\n",
    );
    wat.push_str(&fail("kind"));
    wat.push_str("  (if (i32.eqz (i32.load (i32.sub (local.get $arr) (i32.const 12))))\n");
    wat.push_str(&fail("refcount"));
    wat.push_str("  (local.set $arr_size (i32.load (i32.sub (local.get $arr) (i32.const 16))))\n");
    wat.push_str(&format!(
        "  (if (i32.lt_u (local.get $arr_size) (i32.const {}))\n",
        ARRAY_ELEMENTS + 2 * ARRAY_SLOT_BYTES
    ));
    wat.push_str(&fail("header-size"));
    wat.push_str(
        "  (if (i64.gt_u (i64.add (i64.extend_i32_u (local.get $arr)) (i64.extend_i32_u (local.get $arr_size))) (i64.extend_i32_u (global.get $__heap_ptr)))\n",
    );
    wat.push_str(&fail("payload-bounds"));
    wat.push_str("  (if (i64.lt_u (i64.load (local.get $arr)) (i64.const 2))\n");
    wat.push_str(&fail("arity"));
    wat.push_str(&format!(
        "  (if (i64.ne (i64.load offset=16 (local.get $arr)) (i64.const {}))\n",
        ARRAY_SLOT_BYTES
    ));
    wat.push_str(&fail("stride"));
    wat.push_str(&format!(
        "  (if (i32.ne (i32.and (i32.wrap_i64 (i64.shr_u (i64.load (i32.sub (local.get $arr) (i32.const 8))) (i64.const 8))) (i32.const 127)) (i32.const {}))\n",
        value_type
    ));
    wat.push_str(&fail("value-type"));
}

/// Byte offset of element `index` in an indexed array.
fn slot(index: u32) -> u32 {
    ARRAY_ELEMENTS + index * ARRAY_SLOT_BYTES
}

/// Emits `br_if $label` guards that leave the arm unless the string at `(ptr, len)` equals
/// `name` ASCII case-insensitively.
///
/// php resolves class and method names without regard to ASCII case, so `[$o, "RUN"]` reaches
/// `run`. A letter byte is compared after `| 0x20` against its lowercase form, which maps exactly
/// the two spellings of that letter (`0x41|0x20 == 0x61`) and nothing else; every other byte is
/// compared exactly.
///
/// The length test comes first and each byte test exits through `br_if`, so the loads are
/// short-circuited: a shorter runtime string is never read past its end.
fn append_name_guard(wat: &mut String, label: &str, ptr: &str, len: &str, name: &str) {
    wat.push_str(&format!(
        "    (br_if {} (i64.ne (local.get {}) (i64.const {})))\n",
        label,
        len,
        name.len()
    ));
    for (i, byte) in name.as_bytes().iter().enumerate() {
        if byte.is_ascii_alphabetic() {
            wat.push_str(&format!(
                "    (br_if {} (i32.ne (i32.or (i32.load8_u offset={} (local.get {})) (i32.const 32)) (i32.const {}))) ;; {:?} (any case)\n",
                label,
                i,
                ptr,
                byte.to_ascii_lowercase(),
                *byte as char
            ));
        } else {
            wat.push_str(&format!(
                "    (br_if {} (i32.ne (i32.load8_u offset={} (local.get {})) (i32.const {}))) ;; {:?}\n",
                label,
                i,
                ptr,
                byte,
                *byte as char
            ));
        }
    }
}

/// Builds `__rt_cbarr_invoke_mixed`: dispatches `[$object, "method"]`.
///
/// Element 0 must be an object cell and element 1 a string cell; the receiver's `class_id`
/// (`objects.rs`: `[ptr+0]`) and the method-name bytes select the arm.
fn build_instance_ladder(arms: &[(u64, String, String)]) -> String {
    let mut wat = String::new();
    wat.push_str(
        "(func $__rt_cbarr_invoke_mixed (param $arr i32) (param $args i32) (result i32)\n",
    );
    wat.push_str("  (local $arr_size i32) (local $c0 i32) (local $c1 i32)\n");
    wat.push_str("  (local $recv i32) (local $cid i32) (local $np i32) (local $nl i64)\n");
    append_callable_array_guard(&mut wat, VALUE_TYPE_MIXED_CELL, "mixed");
    wat.push_str(&format!(
        "  (local.set $c0 (i32.wrap_i64 (i64.load offset={} (local.get $arr))))\n",
        slot(0)
    ));
    wat.push_str(&format!(
        "  (local.set $c1 (i32.wrap_i64 (i64.load offset={} (local.get $arr))))\n",
        slot(1)
    ));
    wat.push_str(&format!(
        "  (if (i64.ne (i64.load offset={} (local.get $c0)) (i64.const {}))\n    (then (call $__rt_fail_callable_dispatch) unreachable)) ;; elephc-trap:post-noreturn:cbarr-mixed-receiver-tag\n",
        CELL_TAG, TAG_OBJECT
    ));
    wat.push_str(&format!(
        "  (if (i64.ne (i64.load offset={} (local.get $c1)) (i64.const {}))\n    (then (call $__rt_fail_callable_dispatch) unreachable)) ;; elephc-trap:post-noreturn:cbarr-mixed-method-tag\n",
        CELL_TAG, TAG_STRING
    ));
    wat.push_str(&format!(
        "  (local.set $recv (i32.wrap_i64 (i64.load offset={} (local.get $c0))))\n",
        CELL_LO
    ));
    wat.push_str(&format!(
        "  (local.set $np (i32.wrap_i64 (i64.load offset={} (local.get $c1))))\n",
        CELL_LO
    ));
    wat.push_str(&format!(
        "  (local.set $nl (i64.load offset={} (local.get $c1)))\n",
        CELL_HI
    ));
    wat.push_str("  (local.set $cid (i32.wrap_i64 (i64.load (local.get $recv)))) ;; class_id @ +0\n");
    for (index, (class_id, method_name, wrapper)) in arms.iter().enumerate() {
        let label = format!("$cbarr_m{}", index);
        wat.push_str(&format!("  (block {}\n", label));
        wat.push_str(&format!(
            "    (br_if {} (i32.ne (local.get $cid) (i32.const {})))\n",
            label, class_id
        ));
        append_name_guard(&mut wat, &label, "$np", "$nl", method_name);
        wat.push_str(&format!(
            "    (return (call ${} (local.get $recv) (local.get $args))))\n",
            wrapper
        ));
    }
    wat.push_str(
        "  (call $__rt_fail_callable_dispatch) unreachable) ;; elephc-trap:post-noreturn:cbarr-mixed-no-match\n",
    );
    wat
}

/// Builds `__rt_cbarr_invoke_str`: dispatches `["Class", "method"]`.
///
/// Both elements are inline `(ptr, len)` strings, so the class NAME is compared rather than a
/// class id — the callable never carries an object to read one from.
fn build_static_ladder(arms: &[(String, String, String)]) -> String {
    let mut wat = String::new();
    wat.push_str("(func $__rt_cbarr_invoke_str (param $arr i32) (param $args i32) (result i32)\n");
    wat.push_str("  (local $arr_size i32)\n");
    wat.push_str("  (local $cp i32) (local $cl i64) (local $np i32) (local $nl i64)\n");
    append_callable_array_guard(&mut wat, VALUE_TYPE_STRING, "str");
    wat.push_str(&format!(
        "  (local.set $cp (i32.wrap_i64 (i64.load offset={} (local.get $arr))))\n",
        slot(0)
    ));
    wat.push_str(&format!(
        "  (local.set $cl (i64.load offset={} (local.get $arr)))\n",
        slot(0) + 8
    ));
    wat.push_str(&format!(
        "  (local.set $np (i32.wrap_i64 (i64.load offset={} (local.get $arr))))\n",
        slot(1)
    ));
    wat.push_str(&format!(
        "  (local.set $nl (i64.load offset={} (local.get $arr)))\n",
        slot(1) + 8
    ));
    // php's class lookup ignores one leading backslash, so `["\\C", "m"]` reaches class `C`.
    wat.push_str(
        "  (if (i32.and (i64.ne (local.get $cl) (i64.const 0)) (i32.eq (i32.load8_u (local.get $cp)) (i32.const 92)))\n    (then (local.set $cp (i32.add (local.get $cp) (i32.const 1))) (local.set $cl (i64.sub (local.get $cl) (i64.const 1)))))\n",
    );
    for (index, (class_name, method_name, wrapper)) in arms.iter().enumerate() {
        let label = format!("$cbarr_s{}", index);
        wat.push_str(&format!("  (block {}\n", label));
        append_name_guard(
            &mut wat,
            &label,
            "$cp",
            "$cl",
            class_name.trim_start_matches('\\'),
        );
        append_name_guard(&mut wat, &label, "$np", "$nl", method_name);
        // The static wrapper IS a first-class-callable wrapper, so it takes `(desc, args)`.
        // A `["Class", "method"]` callable carries no descriptor — and the wrapper never reads
        // one, taking its called-class id as a baked constant — so the slot is passed as zero.
        wat.push_str(&format!(
            "    (return (call ${} (i32.const 0) (local.get $args))))\n",
            wrapper
        ));
    }
    wat.push_str(
        "  (call $__rt_fail_callable_dispatch) unreachable) ;; elephc-trap:post-noreturn:cbarr-str-no-match\n",
    );
    wat
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two array spellings must classify apart, and nothing else may classify at all.
    ///
    /// The forms differ only in their ELEMENT TYPE, and each ladder reads a different layout
    /// from the same header: `array<mixed>` slots hold a Mixed-cell POINTER, `array<string>`
    /// slots hold `(ptr, len)` INLINE. Mixing them up dispatches on garbage rather than
    /// failing, so the classification is the load-bearing step.
    #[test]
    fn array_callable_forms_classify_by_element_type() {
        assert_eq!(
            array_form(&PhpType::Array(Box::new(PhpType::Mixed))),
            Some(ArrayCallableForm::InstanceMixed),
        );
        assert_eq!(
            array_form(&PhpType::Array(Box::new(PhpType::Str))),
            Some(ArrayCallableForm::StaticString),
        );
        // An `array<int>` is a PHP array, never a callable: it has no method name to dispatch on.
        assert_eq!(array_form(&PhpType::Array(Box::new(PhpType::Int))), None);
        assert_eq!(array_form(&PhpType::Callable), None);
        assert_eq!(array_form(&PhpType::Str), None);
    }

    /// The two ladders answer on different symbols, so a call site cannot reach the other's
    /// element layout by naming the wrong entry point.
    #[test]
    fn each_form_dispatches_through_its_own_ladder() {
        assert_ne!(
            ladder_symbol(ArrayCallableForm::InstanceMixed),
            ladder_symbol(ArrayCallableForm::StaticString),
        );
    }

    /// The name guard tests the LENGTH before any byte, and every test exits the arm.
    ///
    /// `i32.and` would evaluate both sides, so a folded condition would run the byte loads even
    /// when the length already ruled the arm out — reading past the end of a shorter runtime
    /// string, which near the end of linear memory traps. `br_if` short-circuits, and the
    /// length guard coming first is what makes that matter.
    #[test]
    fn name_guard_checks_length_first_and_short_circuits() {
        let mut wat = String::new();
        append_name_guard(&mut wat, "$arm", "$np", "$nl", "hi");

        let lines: Vec<&str> = wat.lines().collect();
        assert_eq!(lines.len(), 3, "one length test then one test per byte: {wat}");
        assert!(wat.contains("i32.const 32"), "letters compare case-insensitively: {wat}");
        assert!(
            lines[0].contains("i64.ne (local.get $nl) (i64.const 2)"),
            "the length test must come first: {}",
            lines[0]
        );
        for line in &lines {
            assert!(line.contains("br_if $arm"), "every test must exit the arm: {line}");
        }
        assert!(lines[1].contains("i32.const 104"), "'h' folded to lower case: {}", lines[1]);
        assert!(lines[2].contains("offset=1"), "second byte: {}", lines[2]);
    }

    /// An empty module offers no targets, and the audit says so instead of emitting an empty
    /// ladder whose every call would fall through to the dispatch failure.
    #[test]
    fn a_module_without_methods_has_no_array_callable_targets() {
        let module = crate::ir::Module::new(crate::codegen_support::platform::Target::wasm());
        for form in [
            ArrayCallableForm::InstanceMixed,
            ArrayCallableForm::StaticString,
        ] {
            let issue = unsupported_target_issue(&module, form);
            assert!(issue.is_some_and(|issue| issue.contains("no public")), "{form:?}");
        }
    }
}
