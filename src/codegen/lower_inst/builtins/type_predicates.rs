//! Purpose:
//! Lowers static and runtime PHP type predicates, including iterable and countable checks.
//!
//! Called from:
//! - `super` EIR type-predicate and runtime-function dispatch.
//!
//! Key details:
//! - Uses boxed Mixed tags and interface metadata consistently across supported targets.

use super::*;

/// Lowers the reusable EIR PHP type predicate through target-aware value inspection.
pub(crate) fn lower_type_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(Immediate::TypePredicate(predicate)) = inst.immediate else {
        return Err(CodegenIrError::unsupported(
            "type_predicate requires a typed predicate immediate",
        ));
    };
    match predicate {
        PhpTypePredicate::Array => lower_is_array(ctx, inst),
        PhpTypePredicate::Bool => {
            lower_static_type_predicate(ctx, inst, "type_predicate", PhpType::Bool)
        }
        PhpTypePredicate::Countable => lower_is_countable(ctx, inst),
        PhpTypePredicate::Float => {
            lower_static_type_predicate(ctx, inst, "type_predicate", PhpType::Float)
        }
        PhpTypePredicate::Int => {
            lower_static_type_predicate(ctx, inst, "type_predicate", PhpType::Int)
        }
        PhpTypePredicate::Iterable => lower_is_iterable(ctx, inst),
        PhpTypePredicate::Object => lower_is_object(ctx, inst),
        PhpTypePredicate::Resource => types::lower_is_resource(ctx, inst),
        PhpTypePredicate::Scalar => lower_is_scalar(ctx, inst),
        PhpTypePredicate::String => {
            lower_static_type_predicate(ctx, inst, "type_predicate", PhpType::Str)
        }
    }
}

/// Lowers a static PHP type predicate for concrete non-Mixed values.
pub(crate) fn lower_static_type_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    expected: PhpType,
) -> Result<()> {
    ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    // The RAW php type. `value_php_type` answers with `codegen_repr()`, which maps a resource
    // onto the machine word it travels in — an integer — so `is_int(STDIN)` compared `Int` with
    // `Int` and said TRUE where php says false. The same value's `gettype()` said `resource`,
    // because that one asks the runtime rather than the type. A resource is the only shape whose
    // representation collides with another predicate's answer; every other mapping `codegen_repr`
    // performs is either identity or onto `Mixed`, which the branch below handles separately.
    let ty = ctx.raw_value_php_type(value)?;
    if matches!(ty, PhpType::Resource(_)) {
        emit_static_bool(ctx, false);
        return store_if_result(ctx, inst);
    }
    let ty = ty.codegen_repr();
    if ty == PhpType::TaggedScalar {
        if expected == PhpType::Int {
            emit_tagged_scalar_int_predicate(ctx, value)?;
        } else {
            emit_static_bool(ctx, false);
        }
        return store_if_result(ctx, inst);
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        if let Some(tag) = mixed_type_predicate_tag(&expected) {
            predicates::emit_mixed_tag_eq(ctx, value, tag)?;
        } else {
            emit_static_bool(ctx, false);
        }
        return store_if_result(ctx, inst);
    }
    emit_static_bool(ctx, ty == expected);
    store_if_result(ctx, inst)
}

/// Emits `is_int()` for a tagged scalar by checking that its tag is not null.
pub(in crate::codegen::lower_inst) fn emit_tagged_scalar_int_predicate(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let cmp_inst = format!(
                "cmp x1, #{}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // does the tagged scalar carry the runtime null tag?
            ctx.emitter.instruction("cset x0, ne");                             // materialize true when the tagged scalar holds an integer
        }
        Arch::X86_64 => {
            let cmp_inst = format!(
                "cmp rdx, {}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // does the tagged scalar carry the runtime null tag?
            ctx.emitter.instruction("setne al");                                // materialize true when the tagged scalar holds an integer
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
    Ok(())
}

/// Lowers `is_iterable()` for concrete values and boxed Mixed payloads.
pub(crate) fn lower_is_iterable(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_iterable", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?;
    let result = match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => true,
        PhpType::Object(name) => object_type_implements_iterable(ctx, &name),
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::False
        | PhpType::Void
        | PhpType::Never
        | PhpType::Callable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => false,
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_is_iterable(ctx, value)?;
            return store_if_result(ctx, inst);
        }
    };
    emit_static_bool(ctx, result);
    store_if_result(ctx, inst)
}

/// Emits runtime `is_iterable()` checks for a boxed Mixed or Union value.
pub(in crate::codegen::lower_inst) fn emit_mixed_is_iterable(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let true_case = ctx.next_label("is_iterable_mixed_true");
    let object_case = ctx.next_label("is_iterable_mixed_object");
    let done = ctx.next_label("is_iterable_mixed_done");
    let ty = ctx.load_value_to_result(value)?;
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "is_iterable Mixed check for PHP type {:?}",
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // check for a boxed indexed-array payload
            ctx.emitter.instruction(&format!("b.eq {}", true_case));            // indexed arrays satisfy is_iterable
            ctx.emitter.instruction("cmp x0, #5");                              // check for a boxed associative-array payload
            ctx.emitter.instruction(&format!("b.eq {}", true_case));            // associative arrays satisfy is_iterable
            ctx.emitter.instruction("cmp x0, #6");                              // check for a boxed object payload
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // objects need a Traversable interface check
            ctx.emitter.instruction("mov x0, #0");                              // all other Mixed payloads are not iterable
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the truthy result path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // check for a boxed indexed-array payload
            ctx.emitter.instruction(&format!("je {}", true_case));              // indexed arrays satisfy is_iterable
            ctx.emitter.instruction("cmp rax, 5");                              // check for a boxed associative-array payload
            ctx.emitter.instruction(&format!("je {}", true_case));              // associative arrays satisfy is_iterable
            ctx.emitter.instruction("cmp rax, 6");                              // check for a boxed object payload
            ctx.emitter.instruction(&format!("je {}", object_case));            // objects need a Traversable interface check
            ctx.emitter.instruction("mov rax, 0");                              // all other Mixed payloads are not iterable
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the truthy result path
        }
    }
    ctx.emitter.label(&object_case);
    emit_runtime_object_iterable_check(ctx, &true_case, &done);
    ctx.emitter.label(&true_case);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits the object half of runtime `is_iterable()` by checking Traversable interfaces.
pub(in crate::codegen::lower_inst) fn emit_runtime_object_iterable_check(
    ctx: &mut FunctionContext<'_>,
    true_case: &str,
    done: &str,
) {
    let object_true = ctx.next_label("is_iterable_object_true");
    let interface_ids = traversable_interface_ids(ctx);
    if interface_ids.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_jump(ctx.emitter, done);
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #-16]!");                     // preserve the unboxed object pointer across Traversable checks
            for interface_id in interface_ids {
                emit_saved_object_interface_check(ctx, interface_id, &object_true);
            }
            ctx.emitter.instruction("add sp, sp, #16");                         // discard the saved object pointer after failed checks
            ctx.emitter.instruction("mov x0, #0");                              // non-Traversable objects are not iterable
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the truthy result path
            ctx.emitter.label(&object_true);
            ctx.emitter.instruction("add sp, sp, #16");                         // discard the saved object pointer before returning true
            ctx.emitter.instruction(&format!("b {}", true_case));               // continue through the shared truthy result path
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rdi");
            for interface_id in interface_ids {
                emit_saved_object_interface_check(ctx, interface_id, &object_true);
            }
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction("xor eax, eax");                            // non-Traversable objects are not iterable
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the truthy result path
            ctx.emitter.label(&object_true);
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction(&format!("jmp {}", true_case));             // continue through the shared truthy result path
        }
    }
}

/// Emits one interface matcher call for a saved object pointer.
pub(in crate::codegen::lower_inst) fn emit_saved_object_interface_check(
    ctx: &mut FunctionContext<'_>,
    interface_id: u64,
    true_case: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the object pointer as matcher argument 1
            abi::emit_load_int_immediate(ctx.emitter, "x1", interface_id as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches"); // check whether the object implements the Traversable interface
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the runtime matcher succeeded
            ctx.emitter.instruction(&format!("b.ne {}", true_case));            // a matching interface makes the object iterable
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the object pointer as matcher argument 1
            abi::emit_load_int_immediate(ctx.emitter, "rsi", interface_id as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches"); // check whether the object implements the Traversable interface
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime matcher succeeded
            ctx.emitter.instruction(&format!("jne {}", true_case));             // a matching interface makes the object iterable
        }
    }
}

/// Returns runtime interface IDs for the interfaces that make an object iterable.
pub(in crate::codegen::lower_inst) fn traversable_interface_ids(ctx: &FunctionContext<'_>) -> Vec<u64> {
    ["Iterator", "IteratorAggregate"]
        .into_iter()
        .filter_map(|name| {
            ctx.module
                .interface_infos
                .get(name)
                .map(|info| info.interface_id)
        })
        .collect()
}

/// Returns whether a statically known class or interface satisfies `is_iterable()`.
pub(in crate::codegen::lower_inst) fn object_type_implements_iterable(ctx: &FunctionContext<'_>, type_name: &str) -> bool {
    let normalized = normalized_type_name(type_name);
    if let Some(class_info) = ctx.module.class_infos.get(normalized) {
        return class_info.interfaces.iter().any(|interface_name| {
            is_traversable_interface_name(interface_name)
                || interface_extends_traversable(ctx, interface_name)
        });
    }
    if ctx.module.interface_infos.contains_key(normalized) {
        return is_traversable_interface_name(normalized)
            || interface_extends_traversable(ctx, normalized);
    }
    false
}

/// Returns whether an interface name is one of PHP's Traversable contracts.
pub(in crate::codegen::lower_inst) fn is_traversable_interface_name(interface_name: &str) -> bool {
    let key = php_symbol_key(normalized_type_name(interface_name));
    key == php_symbol_key("Iterator") || key == php_symbol_key("IteratorAggregate")
}

/// Returns whether an interface extends Iterator or IteratorAggregate.
pub(in crate::codegen::lower_inst) fn interface_extends_traversable(ctx: &FunctionContext<'_>, interface_name: &str) -> bool {
    let mut stack = vec![normalized_type_name(interface_name).to_string()];
    while let Some(current) = stack.pop() {
        if is_traversable_interface_name(&current) {
            return true;
        }
        if let Some(interface_info) = ctx.module.interface_infos.get(&current) {
            stack.extend(
                interface_info
                    .parents
                    .iter()
                    .map(|parent| normalized_type_name(parent).to_string()),
            );
        }
    }
    false
}

/// Normalizes a PHP class or interface name for metadata lookups.
pub(in crate::codegen::lower_inst) fn normalized_type_name(type_name: &str) -> &str {
    type_name.trim_start_matches('\\')
}

/// Lowers `is_countable()`: true for arrays and for objects whose class implements `Countable`.
///
/// A statically typed object answers `true` at compile time only when its declared class or
/// interface already implements `Countable`; otherwise the runtime class is checked, because a
/// subclass (or a class behind an interface type) may still implement it. Boxed values test
/// their runtime tag, and `iterable` values their heap kind, before the same object check.
pub(crate) fn lower_is_countable(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_countable", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Array(_) | PhpType::AssocArray { .. } => emit_static_bool(ctx, true),
        PhpType::Object(name) if object_name_satisfies_interface(ctx, &name, "Countable") => {
            emit_static_bool(ctx, true)
        }
        PhpType::Object(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_object_countable_check(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => emit_mixed_is_countable(ctx, value)?,
        PhpType::Iterable => emit_iterable_is_countable(ctx, value)?,
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Emits the runtime `Countable` check for an object pointer in the first integer argument
/// register, leaving PHP's boolean answer in the integer result register.
///
/// A module that knows no `Countable` interface has no class that implements it, so the answer
/// is a constant `false` there.
fn emit_object_countable_check(ctx: &mut FunctionContext<'_>) {
    let Some(countable_id) = ctx
        .module
        .interface_infos
        .get("Countable")
        .map(|info| info.interface_id)
    else {
        emit_static_bool(ctx, false);
        return;
    };
    let target = ctx.emitter.target;
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(target, 1),
        countable_id as i64,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 2), 1);
    abi::emit_call_label(ctx.emitter, "__rt_exception_matches"); // test the object's runtime interface list for Countable
}

/// Emits runtime `is_countable()` for a boxed Mixed or Union value.
///
/// Boxed indexed and associative arrays (tags 4 and 5) are countable; a boxed object (tag 6)
/// is countable when its class implements `Countable`; every other payload is not.
fn emit_mixed_is_countable(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let array_case = ctx.next_label("is_countable_mixed_array");
    let object_case = ctx.next_label("is_countable_mixed_object");
    let done = ctx.next_label("is_countable_mixed_done");
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // check for a boxed indexed-array payload
            ctx.emitter.instruction(&format!("b.eq {}", array_case));           // indexed arrays are countable
            ctx.emitter.instruction("cmp x0, #5");                              // check for a boxed associative-array payload
            ctx.emitter.instruction(&format!("b.eq {}", array_case));           // associative arrays are countable
            ctx.emitter.instruction("cmp x0, #6");                              // check for a boxed object payload
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // objects need a Countable interface check
            ctx.emitter.instruction("mov x0, #0");                              // every other boxed payload is not countable
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the array and object result paths
            ctx.emitter.label(&object_case);
            ctx.emitter.instruction("mov x0, x1");                              // pass the unboxed object pointer to the interface matcher
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // check for a boxed indexed-array payload
            ctx.emitter.instruction(&format!("je {}", array_case));             // indexed arrays are countable
            ctx.emitter.instruction("cmp rax, 5");                              // check for a boxed associative-array payload
            ctx.emitter.instruction(&format!("je {}", array_case));             // associative arrays are countable
            ctx.emitter.instruction("cmp rax, 6");                              // check for a boxed object payload
            ctx.emitter.instruction(&format!("je {}", object_case));            // objects need a Countable interface check
            ctx.emitter.instruction("xor eax, eax");                            // every other boxed payload is not countable
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the array and object result paths
            ctx.emitter.label(&object_case);
        }
    }
    emit_object_countable_check(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&array_case);
    emit_static_bool(ctx, true);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits runtime `is_countable()` for an `iterable` value, whose raw heap pointer is either an
/// indexed array, an associative array, or a `Traversable` object.
fn emit_iterable_is_countable(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let array_case = ctx.next_label("is_countable_iterable_array");
    let object_case = ctx.next_label("is_countable_iterable_object");
    let done = ctx.next_label("is_countable_iterable_done");
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #2");                              // heap kind 2 identifies indexed arrays
            ctx.emitter.instruction(&format!("b.eq {}", array_case));           // indexed arrays are countable
            ctx.emitter.instruction("cmp x0, #3");                              // heap kind 3 identifies associative arrays
            ctx.emitter.instruction(&format!("b.eq {}", array_case));           // associative arrays are countable
            ctx.emitter.instruction("cmp x0, #4");                              // heap kind 4 identifies object payloads
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // objects need a Countable interface check
            ctx.emitter.instruction("mov x0, #0");                              // any other heap kind is not countable
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the array and object result paths
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 2");                              // heap kind 2 identifies indexed arrays
            ctx.emitter.instruction(&format!("je {}", array_case));             // indexed arrays are countable
            ctx.emitter.instruction("cmp rax, 3");                              // heap kind 3 identifies associative arrays
            ctx.emitter.instruction(&format!("je {}", array_case));             // associative arrays are countable
            ctx.emitter.instruction("cmp rax, 4");                              // heap kind 4 identifies object payloads
            ctx.emitter.instruction(&format!("je {}", object_case));            // objects need a Countable interface check
            ctx.emitter.instruction("xor eax, eax");                            // any other heap kind is not countable
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the array and object result paths
        }
    }
    ctx.emitter.label(&object_case);
    ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
    emit_object_countable_check(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&array_case);
    emit_static_bool(ctx, true);
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers `is_array()`: true for statically-known arrays/hashes, or a boxed Mixed/Union value
/// whose runtime tag is an indexed (4) or associative (5) array. An `iterable`-typed value is
/// not treated as a definite array here (it may hold a Traversable); use `is_iterable` for that.
pub(crate) fn lower_is_array(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_array", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Array(_) | PhpType::AssocArray { .. } => emit_static_bool(ctx, true),
        PhpType::Mixed | PhpType::Union(_) => {
            predicates::emit_mixed_tag_membership(ctx, value, &[4, 5])?;
        }
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_object()`: true for objects and Closures, including boxed runtime tags 6 and 10.
pub(crate) fn lower_is_object(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_object", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Object(_) | PhpType::Callable => emit_static_bool(ctx, true),
        PhpType::Mixed | PhpType::Union(_) => {
            predicates::emit_mixed_tag_membership(ctx, value, &[6, 10])?;
        }
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_scalar()`: true for int/float/string/bool, a non-null tagged scalar, or a boxed
/// Mixed/Union value whose runtime tag is int (0), string (1), float (2), or bool (3). Null,
/// arrays, objects, and resources are not scalars, matching PHP.
pub(crate) fn lower_is_scalar(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_scalar", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool | PhpType::False => {
            emit_static_bool(ctx, true)
        }
        PhpType::TaggedScalar => emit_tagged_scalar_int_predicate(ctx, value)?,
        PhpType::Mixed | PhpType::Union(_) => {
            predicates::emit_mixed_tag_membership(ctx, value, &[0, 1, 2, 3])?;
        }
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Returns the runtime Mixed tag used by a supported type predicate.
pub(in crate::codegen::lower_inst) fn mixed_type_predicate_tag(expected: &PhpType) -> Option<u8> {
    match expected {
        PhpType::Int => Some(0),
        PhpType::Str => Some(1),
        PhpType::Float => Some(2),
        PhpType::Bool | PhpType::False => Some(3),
        _ => None,
    }
}
