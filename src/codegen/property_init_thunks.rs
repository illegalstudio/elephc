//! Purpose:
//! Emits a per-class property-default initialization thunk
//! (`_class_propinit_<class_id>`) so objects created through the by-name path
//! (`new $variable()`, registered stream wrappers, registered stream filters)
//! receive their declared property default values.
//!
//! Called from:
//! - `crate::codegen::generate_user_asm()` right after the `emit_class_methods`
//!   loop, over the same filtered/sorted class set.
//! - The emitted thunk is invoked indirectly by `__rt_new_by_name` through the
//!   `_class_propinit_ptrs` table (entry 0 = no defaults, no thunk).
//!
//! Key details:
//! - The thunk is emitted as a synthetic single-receiver instance method whose
//!   body is `$this->prop = <default>;` for every property that has a default.
//!   Routing it through `functions::emit_method` reuses the full method frame
//!   and codegen Context, so default-value expressions evaluate exactly as they
//!   do in the normal `new ClassName()` path (correct refcounting, type
//!   coercion, and constant/enum/self:: resolution) — no hand-rolled frame.
//! - `__rt_new_by_name` already zeroes the property region; the dyn_props
//!   hashtable (`#[AllowDynamicProperties]`) is lazily allocated on first write,
//!   so the zeroed slot is correct without a thunk. NOT handled here: running
//!   `__construct` (out of scope, matching `new $var()`'s documented limit) and
//!   stamping the typed-uninitialized sentinel for defaultless typed properties.
//! - The predicate [`class_needs_property_init`] MUST stay in lockstep with the
//!   `_class_propinit_ptrs` table emission in `runtime::data::user` so every
//!   referenced `_class_propinit_<id>` symbol is actually emitted.

use std::collections::{HashMap, HashSet};

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{
    ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo,
    PackedClassInfo, PhpType,
};

/// Returns true when the class has at least one property default value that
/// must run on by-name-instantiated objects. Kept identical to the predicate
/// that emits the `_class_propinit_ptrs` table entry, so the table never points
/// at a thunk that was not emitted (and never emits a thunk with no table slot).
pub(crate) fn class_needs_property_init(class_info: &ClassInfo) -> bool {
    class_info.defaults.iter().any(|default| default.is_some())
}

/// Emits the `_class_propinit_<class_id>` thunk for one class, when it has any
/// property defaults. Synthesizes a single-receiver instance method whose body
/// assigns each default and emits it through the normal method machinery.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_property_init_thunk(
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    class_info: &ClassInfo,
    functions: &HashMap<String, FunctionSig>,
    callable_param_sigs: &HashMap<(String, String), FunctionSig>,
    callable_return_sigs: &HashMap<String, FunctionSig>,
    function_variant_groups: &HashSet<String>,
    global_constants: &HashMap<String, (ExprKind, PhpType)>,
    interfaces: &HashMap<String, InterfaceInfo>,
    traits: &HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
    enums: &HashMap<String, EnumInfo>,
    packed_classes: &HashMap<String, PackedClassInfo>,
    extern_functions: &HashMap<String, ExternFunctionSig>,
    extern_classes: &HashMap<String, ExternClassInfo>,
    extern_globals: &HashMap<String, PhpType>,
) {
    if !class_needs_property_init(class_info) {
        return;
    }
    let span = Span::dummy();

    // Synthesize `$this->prop = <default>;` for each property with a default.
    let mut body: Vec<Stmt> = Vec::new();
    for (i, default) in class_info.defaults.iter().enumerate() {
        let Some(default_expr) = default else { continue };
        let property = class_info.properties[i].0.clone();
        let assignment = Expr::new(
            ExprKind::Assignment {
                target: Box::new(Expr::new(
                    ExprKind::PropertyAccess {
                        object: Box::new(Expr::new(ExprKind::This, span)),
                        property,
                    },
                    span,
                )),
                value: Box::new(default_expr.clone()),
                result_target: None,
                prelude: Vec::new(),
                conditional_value_temp: None,
            },
            span,
        );
        body.push(Stmt::new(StmtKind::ExprStmt(assignment), span));
    }

    let label = format!("_class_propinit_{}", class_info.class_id);
    let epilogue_label = format!("{}_epilogue", label);
    // A single `this` receiver, no declared params, falling off the end (the
    // Int/undeclared-return shape matches build_instance_method_codegen_sig's
    // fallback for a method with no resolved signature).
    let sig = FunctionSig {
        params: vec![("this".to_string(), PhpType::Object(class_name.to_string()))],
        defaults: vec![None],
        return_type: PhpType::Int,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false],
        declared_params: vec![false],
        variadic: None,
        deprecation: None,
    };

    // Property default expressions are compile-time constants and never invoke a
    // callable-array-returning function, so no real array-return signatures are
    // reachable from this synthetic thunk body; pass an empty map.
    let callable_array_return_sigs: HashMap<String, FunctionSig> = HashMap::new();
    // Property default-value initializers are constant expressions; no fiber
    // calls are reachable from this synthetic thunk, so pass an empty map.
    let fiber_return_sigs: HashMap<String, FunctionSig> = HashMap::new();

    functions::emit_method(
        emitter,
        data,
        &label,
        &epilogue_label,
        &sig,
        &body,
        functions,
        callable_param_sigs,
        callable_return_sigs,
        &callable_array_return_sigs,
        &fiber_return_sigs,
        function_variant_groups,
        global_constants,
        interfaces,
        traits,
        classes,
        enums,
        packed_classes,
        class_name,
        extern_functions,
        extern_classes,
        extern_globals,
    );
}
