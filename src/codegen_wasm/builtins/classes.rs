//! Purpose:
//! Class-like queries answered from the module declarations: `class_exists` family, `is_a`, class relations, declared-name lists, `spl_object_id` and `Countable::count`.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.
//! - `crate::codegen_wasm::plan` reads the `*_answer_strings` lists to intern the answers.

use super::*;

/// Answers `class_exists` and its three siblings from the module's own declarations.
///
/// The checker already requires a STRING LITERAL here in AOT mode, so the question is closed:
/// this module is the whole program, and whether it declares a name is settled at compile time.
/// The answer folds to a constant and no runtime table is consulted.
///
/// The four namespaces are distinct, which is the part worth pinning: measured on php-src 8.5.6,
/// `class_exists("Shape")` is FALSE for an interface and `interface_exists("Circle")` is FALSE
/// for a class. An ENUM is the one crossover — `class_exists("Suit")` is TRUE — because an enum
/// IS a class in PHP, and it is registered in `class_infos` here for that reason.
///
/// Names are matched case-insensitively and with any leading `\` stripped, which is how PHP
/// resolves a class name written as a string.
pub(super) fn class_exists_answer(module: &Module, target: RuntimeFnId, name: &str) -> Option<bool> {
    // `function_exists` asks the same closed-world question of a different namespace, and it
    // has one more source of truth: a name the program never declares can still exist, because
    // the compiler provides it. Both are settled here.
    // Not a declaration question at all, but the same SHAPE of one: a literal name in, a bool
    // settled at compile time out. It rides this family so the fold, the gate and the diagnostic
    // are the ones already written rather than a second copy of each.
    if target == RuntimeFnId::ExtensionLoaded {
        return Some(wasm_extension_is_loaded(name));
    }
    if target == RuntimeFnId::FunctionExists {
        let declared: Vec<&String> = module.functions.iter().map(|f| &f.name).collect();
        if name_is_declared(&declared, name) {
            return Some(true);
        }
        let wanted = crate::names::php_symbol_key(name.trim_start_matches('\\'));
        return Some(
            crate::builtins::registry::names()
                .any(|name| crate::names::php_symbol_key(name) == wanted),
        );
    }
    let declared: Vec<&String> = match target {
        RuntimeFnId::ClassExists => module.class_infos.keys().collect(),
        RuntimeFnId::InterfaceExists => module.interface_infos.keys().collect(),
        RuntimeFnId::EnumExists => module.enum_infos.keys().collect(),
        RuntimeFnId::TraitExists => module.declared_trait_names.iter().collect(),
        _ => return None,
    };
    Some(name_is_declared(&declared, name))
}

/// The names `get_declared_classes()` answers with, in php's declaration order.
///
/// Three sources in the order the native backend consults them, so the two targets can never
/// report a different program. The module's own recorded order is authoritative; the pipeline's
/// declaration-order registry is what an EIR built without it falls back to; the class table is
/// the last resort, and it is the only one that needs the synthetic-name filter, because the
/// first two never held `__elephc*` helpers.
pub(in crate::codegen_wasm) fn declared_class_names(module: &Module) -> Vec<String> {
    if !module.declared_class_names.is_empty() {
        return module.declared_class_names.clone();
    }
    let recorded = crate::codegen_support::declared_class_names();
    if !recorded.is_empty() {
        return recorded;
    }
    module
        .class_table
        .names
        .iter()
        .filter(|name| !crate::names::php_symbol_key(name).starts_with("__elephc"))
        .cloned()
        .collect()
}

/// The names `spl_classes()` answers with.
///
/// The catalogue the shared contract attributes to `ext/spl`, which is the same list the native
/// backend answers and is not a claim about what this target can INSTANTIATE. php's own
/// `spl_classes()` is a directory of the extension, and `extension_loaded('SPL')` already
/// answers true here; a program that goes on to `new` one of these gets the audit's refusal at
/// compile time, which is a better answer than a catalogue that lies about the extension.
pub(in crate::codegen_wasm) fn spl_class_names() -> Vec<String> {
    crate::types::builtin_classes::class_names_in_module(elephc_builtin_contract::PhpModule::Spl)
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

/// The names a declared-name builtin answers with in this module, for the data layout.
///
/// The planner needs every one of them addressable before the lowering runs, exactly as the
/// extension list does, because they are computed from the program rather than written in it.
pub(in crate::codegen_wasm) fn declared_name_answer_strings(module: &Module) -> Vec<String> {
    let mut names = Vec::new();
    if module_calls(module, RuntimeFnId::GetDeclaredClasses) {
        names.extend(declared_class_names(module));
    }
    if module_calls(module, RuntimeFnId::SplClasses) {
        for name in spl_class_names() {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Whether any instruction in the module calls `target`.
pub(super) fn module_calls(module: &Module, target: RuntimeFnId) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .any(|function| {
            function.instructions.iter().any(|inst| {
                matches!(
                    inst.immediate.as_ref(),
                    Some(Immediate::RuntimeCall(
                        RuntimeCallTarget::Function(id)
                            | RuntimeCallTarget::ProfiledFunction { target: id, .. }
                    )) if *id == target
                )
            })
        })
}

/// Validates `get_declared_classes()` / `spl_classes()`: no operands in, `array<string>` out.
pub(super) fn declared_names_shape_issue(call: &Instruction, name: &str) -> Option<String> {
    if !call.operands.is_empty() {
        return Some(format!(
            "{name} takes no arguments, got {}",
            call.operands.len()
        ));
    }
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "{name} answers array<string>, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Lowers `get_declared_classes()` / `spl_classes()` to the list settled when the module is built.
pub(super) fn lower_declared_names(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    let names = match target {
        RuntimeFnId::GetDeclaredClasses => declared_class_names(ctx.module),
        RuntimeFnId::SplClasses => spl_class_names(),
        other => {
            return Err(WasmError::Unsupported(format!(
                "{other:?} is not a declared-name builtin"
            )))
        }
    };
    let array = emit_name_array(ctx, &names, "the declared-name list, one string slot per entry")?;
    ctx.fb.ins(&format!("local.get {}", array), "the finished list");
    store_result(ctx, inst)
}

/// Validates `spl_object_id($object)`: one object in, an int out.
///
/// The receiver must be a settled `Object`. A boxed one would have to be unboxed and checked for
/// its tag first, and php raises a TypeError for a non-object rather than answering — neither is
/// written here, so the audit refuses the call instead of guessing which.
pub(super) fn spl_object_id_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if call.operands.len() != 1 {
        return Some(format!(
            "spl_object_id takes one object, got {} operands",
            call.operands.len()
        ));
    }
    let php_type = function
        .values
        .get(call.operands[0].as_raw() as usize)
        .map(|value| value.php_type.codegen_repr());
    if !matches!(php_type, Some(PhpType::Object(_))) {
        return Some(format!(
            "spl_object_id takes a settled object, not {php_type:?}"
        ));
    }
    if call.result.is_none() || call.result_type != IrType::I64 {
        return Some(format!(
            "spl_object_id answers an int, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Lowers `spl_object_id($object)` to the handle the pool gave it at allocation.
pub(super) fn lower_spl_object_id(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let operand = inst
        .operands
        .first()
        .copied()
        .ok_or_else(|| WasmError::Unsupported("spl_object_id has no receiver".into()))?;
    ctx.emit_load_value(operand)?;
    ctx.fb.ins(
        "call $__rt_object_handle_of",
        "the handle this object was registered with",
    );
    store_result(ctx, inst)
}

/// Validates `is_a($object, $class)`: an object and a string name in, a bool out.
///
/// php's third argument is `$allow_string`, which lets the FIRST argument be a class name
/// rather than an instance. That is a different question — it asks the class table about a name
/// nothing instantiated — and this lowering asks an object about itself, so a call that passes
/// it is refused rather than answered with the wrong question's answer.
pub(super) fn is_a_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if call.operands.len() != 2 {
        return Some(format!(
            "is_a takes an object and a class name, got {} operands",
            call.operands.len()
        ));
    }
    let php_type = |index: usize| {
        function
            .values
            .get(call.operands[index].as_raw() as usize)
            .map(|value| value.php_type.codegen_repr())
    };
    if !matches!(php_type(0), Some(PhpType::Object(_))) {
        return Some(format!(
            "is_a takes a settled object, not {:?} — $allow_string is not served here",
            php_type(0)
        ));
    }
    if !matches!(php_type(1), Some(PhpType::Str)) {
        return Some(format!("is_a takes a class NAME, not {:?}", php_type(1)));
    }
    if call.result.is_none() || call.result_type != IrType::I64 {
        return Some(format!("is_a answers a bool, not {:?}", call.result_type));
    }
    None
}

/// Lowers `is_a($object, $class)` onto the runtime instanceof check.
pub(super) fn lower_is_a(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    crate::codegen_wasm::classes::lower_object_is_a(ctx, inst)
}

/// Whether `name` matches one of `declared`, the way PHP resolves a class name written as a
/// string: case-insensitively, and with any leading `\` stripped from either side.
pub(super) fn name_is_declared(declared: &[&String], name: &str) -> bool {
    let wanted = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    declared.iter().any(|candidate| {
        crate::names::php_symbol_key(candidate.trim_start_matches('\\')) == wanted
    })
}

/// Whether this `*_exists` call is one the closed-world answer covers.
pub(super) fn class_exists_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    // PHP's second argument is `$autoload`, which cannot change a closed-world answer: a name
    // this module never declared has nothing to load. It is accepted and ignored.
    if call.operands.is_empty() || call.operands.len() > 2 {
        return Some(format!(
            "expected one or two operands, got {}",
            call.operands.len()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Bool
    {
        return Some(format!(
            "{target:?} answers a bool, got {:?}/{:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    let Some(name) = literal_string_operand(function, module, call.operands[0]) else {
        return Some(format!("{target:?} needs a literal name to answer statically"));
    };
    if class_exists_answer(module, target, &name).is_none() {
        return Some(format!("{target:?} has no closed-world answer"));
    }
    None
}

/// Lowers a `*_exists` call to the constant its declarations imply.
///
/// The name operand is still evaluated and dropped: it is a literal here, so nothing observable
/// happens, but discarding the expression rather than the value is the wrong shape to establish.
pub(super) fn lower_class_exists(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    let name = literal_string_operand(ctx.function, ctx.module, operand(inst, 0)?)
        .ok_or_else(|| WasmError::Unsupported(format!("{target:?} name is not a literal")))?;
    // A name whose bodies arrive from an include is the ONE case the closed world cannot fold:
    // whether it exists depends on whether that include has run, which is runtime state. Read
    // the group's active-variant slot instead, exactly as the native backend does — a fold here
    // would answer `true` before the `require` that defines it.
    if target == RuntimeFnId::FunctionExists {
        if let Some(group) = crate::codegen_wasm::includes::dispatch_group_for(ctx.module, &name) {
            ctx.fb.ins(
                &format!(
                    "global.get ${}",
                    crate::codegen_wasm::symbols::function_variant_active_symbol(&group.name)
                ),
                "which include-loaded body is active, if any",
            );
            ctx.fb.ins("i32.const 0", "zero means no include has defined it");
            ctx.fb.ins("i32.ne", "exists exactly when one is active");
            ctx.fb.ins("i64.extend_i32_u", "PHP bool travels as i64");
            return store_result(ctx, inst);
        }
    }
    let answer = class_exists_answer(ctx.module, target, &name)
        .ok_or_else(|| WasmError::Unsupported(format!("{target:?} has no answer")))?;
    ctx.fb.ins(
        &format!("i64.const {}", i64::from(answer)),
        "this module is the whole program, so the answer is settled",
    );
    store_result(ctx, inst)
}

/// Maps the three class-relation builtins onto the shared relation they ask for.
pub(super) fn class_relation_of(target: RuntimeFnId) -> Option<crate::ir::class_relations::ClassRelation> {
    use crate::ir::class_relations::ClassRelation;
    match target {
        RuntimeFnId::ClassImplements => Some(ClassRelation::Implements),
        RuntimeFnId::ClassParents => Some(ClassRelation::Parents),
        RuntimeFnId::ClassUses => Some(ClassRelation::Uses),
        _ => None,
    }
}

/// Resolves the class-like target of one class-relation call, or `None` when it cannot be
/// settled at compile time.
///
/// Two spellings are settled: a STATICALLY TYPED object (the class is in its EIR type) and a
/// literal name. Anything else — a `mixed` receiver, a computed name — is runtime state this
/// closed-world fold cannot answer, and is refused rather than guessed.
pub(super) fn class_relation_target(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<crate::ir::class_relations::ClassLikeTarget> {
    use crate::ir::class_relations;
    let operand = *call.operands.first()?;
    let value = function.value(operand)?;
    match value.php_type.codegen_repr() {
        PhpType::Object(class_name) => {
            Some(class_relations::resolve_object_target(module, &class_name))
        }
        PhpType::Str => {
            let name = literal_string_operand(function, module, operand)?;
            Some(class_relations::resolve_named_target(module, &name))
        }
        _ => None,
    }
}

/// Every name any class-relation call in this module can answer with.
///
/// Read by `plan_module` so those bytes get a data segment: they are computed from the module's
/// declarations, never written as PHP literals, so they have no `DataId` of their own.
pub(in crate::codegen_wasm) fn class_relation_answer_strings(module: &Module) -> Vec<String> {
    use crate::ir::class_relations;
    let mut names = Vec::new();
    for function in crate::codegen_wasm::function::all_functions(module) {
        for inst in &function.instructions {
            let Some(Immediate::RuntimeCall(target)) = inst.immediate.as_ref() else {
                continue;
            };
            let target = match target {
                RuntimeCallTarget::Function(id) => *id,
                RuntimeCallTarget::ProfiledFunction { target, .. } => *target,
                _ => continue,
            };
            let Some(relation) = class_relation_of(target) else {
                continue;
            };
            let Some(resolved) = class_relation_target(module, function, inst) else {
                continue;
            };
            for name in class_relations::relation_names(module, relation, &resolved) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names
}

/// Whether this class-relation call is one the closed-world answer covers.
pub(super) fn class_relation_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    use crate::ir::class_relations::ClassLikeTarget;
    // PHP's second argument is `$autoload`, which cannot change a closed-world answer.
    if call.operands.is_empty() || call.operands.len() > 2 {
        return Some(format!(
            "expected one or two operands, got {}",
            call.operands.len()
        ));
    }
    // PHP returns `array<string,string>|false`, which reaches codegen boxed.
    if call.result.is_some()
        && (call.result_type != IrType::Heap(IrHeapKind::Mixed)
            || call.result_php_type.codegen_repr() != PhpType::Mixed)
    {
        return Some(format!(
            "{target:?} answers a boxed array|false, got {:?}/{:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    let Some(resolved) = class_relation_target(module, function, call) else {
        return Some(format!(
            "{target:?} needs a literal name or a statically typed object to answer statically"
        ));
    };
    // `Unknown` is not a failure: PHP answers `false` for a name it cannot resolve, and so does
    // the fold. It is listed here only to show the case was considered.
    let _ = matches!(resolved, ClassLikeTarget::Unknown);
    None
}

/// Lowers a class-relation call to the hash its declarations imply.
///
/// An unresolvable target answers PHP's `false`, boxed; anything else builds the `name => name`
/// hash entry by entry, in the order `ir::class_relations` produced — `foreach` walks a hash in
/// insertion order, so that order is observable output.
pub(super) fn lower_class_relation(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    use crate::ir::class_relations::{self, ClassLikeTarget};

    let relation = class_relation_of(target)
        .ok_or_else(|| WasmError::Unsupported(format!("{target:?} is not a class relation")))?;
    let resolved = class_relation_target(ctx.module, ctx.function, inst).ok_or_else(|| {
        WasmError::Unsupported(format!("{target:?} target is not statically known"))
    })?;
    if matches!(resolved, ClassLikeTarget::Unknown) {
        ctx.fb.ins("i64.const 3", "mixed tag = bool");
        ctx.fb.ins("i64.const 0", "false: this module declares no such name");
        ctx.fb.ins("i64.const 0", "no high payload");
        ctx.fb.ins(
            "call $__rt_mixed_from_value",
            "PHP answers false for an unresolvable class-like name",
        );
        return store_result(ctx, inst);
    }

    let names = class_relations::relation_names(ctx.module, relation, &resolved);
    let capacity = (names.len() * 2).max(16);
    let string_tag = crate::codegen::runtime_value_tag(&PhpType::Str) as i64;
    let hash = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    ctx.fb.ins(
        &format!(
            "(local.set {} (call $__rt_hash_new (i64.const {}) (i64.const {})))",
            hash, capacity, string_tag
        ),
        "the class-relation hash, string-keyed and string-valued",
    );
    for name in &names {
        let (offset, len) = ctx.default_str_literal(name)?;
        // Key and value are the SAME bytes: PHP answers `name => name`.
        ctx.fb.ins(
            &format!(
                "(local.set {} (call $__rt_hash_set (local.get {})                  (i64.const {}) (i64.const {}) (i64.const {}) (i64.const {}) (i64.const {})))",
                hash, hash, offset, len, offset, len, string_tag
            ),
            &format!("{:?} entry", name),
        );
    }
    ctx.fb.ins("i64.const 5", "mixed tag = hash");
    ctx.fb.ins(
        &format!("(i64.extend_i32_u (local.get {}))", hash),
        "the hash pointer as the cell payload",
    );
    ctx.fb.ins("i64.const 0", "no high payload");
    ctx.fb.ins(
        "call $__rt_mixed_from_value",
        "box the relation hash, which PHP types array|false",
    );
    store_result(ctx, inst)
}

/// Whether `count($obj)` on this class is one the Countable path can serve.
pub(super) fn countable_count_shape_issue(
    module: &Module,
    class_name: &str,
    call: &Instruction,
) -> Option<String> {
    use crate::ir::class_relations;
    if !class_relations::class_implements_interface(module, class_name, "Countable") {
        return Some(format!(
            "count() of {} which does not implement Countable",
            class_name
        ));
    }
    // The body has to exist HERE: `Countable::count` is a declaration, not an implementation.
    let key = crate::names::php_symbol_key("count");
    let Some(info) = class_relations::lookup_class(module, class_name) else {
        return Some(format!("count() of undeclared class {}", class_name));
    };
    let impl_class = info
        .method_impl_classes
        .get(&key)
        .cloned()
        .unwrap_or_else(|| class_name.to_string());
    let wanted = crate::names::php_symbol_key(&format!("{}::count", impl_class));
    if !module
        .class_methods
        .iter()
        .any(|body| crate::names::php_symbol_key(&body.name) == wanted)
    {
        return Some(format!("count() of {} has no compiled body", class_name));
    }
    // php-src ignores `$mode` entirely for a Countable, so a second operand is accepted.
    if call.operands.is_empty() || call.operands.len() > 2 {
        return Some(format!(
            "count() expects one or two operands, got {}",
            call.operands.len()
        ));
    }
    if call.result.is_some() && call.result_type != IrType::I64 {
        return Some(format!(
            "count() answers an int, got {:?}",
            call.result_type
        ));
    }
    None
}

/// Lowers `count($obj)` on a Countable to the object's own `count()`.
pub(super) fn lower_countable_count(
    ctx: &mut FnCtx,
    inst: &Instruction,
    receiver: crate::ir::ValueId,
    class_name: &str,
) -> Result<()> {
    let key = crate::names::php_symbol_key("count");
    // The method name is needed as bytes for the null-receiver diagnostic, and "count" is a
    // backend constant here rather than a PHP literal the program wrote.
    let (ptr, len) = ctx.default_str_literal("count")?;
    crate::codegen_wasm::methods::emit_no_arg_method_call(ctx, receiver, class_name, &key, "count", ptr, len)?;
    store_result(ctx, inst)
}
