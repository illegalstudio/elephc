//! Purpose:
//! Tests runtime class-string subjects against AOT and eval inheritance metadata.
//!
//! Called from:
//! - The typed is_a/is_subclass_of builtin lowerer.
//!
//! Key details:
//! - The source string and optional runtime flag survive every comparison call.
//! - Candidate names include classes and interfaces, with PHP case-insensitive matching.
//! - A versioned query follows dynamic inheritance through the compiled positive candidates.
//! - A runtime target name (`is_a($name, $name, true)`) dispatches the subject over every
//!   compiled class and interface, then compares the target with that subject's ancestors.
//!   An object subject dispatches on its runtime class id the same way; a callable descriptor
//!   (a callable, or a Mixed with runtime tag 10) is a `Closure`, with no other ancestor, only
//!   when it carries an object handle: an implicit string or array callable is no object.
//! - A target holding a NUL byte, literal or runtime, never reaches the eval bridge, which
//!   splits its candidate list at NUL bytes.

use super::*;

/// Emits a relation test for a dynamic string subject and a literal target.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, subject: ValueId,
    target: &str, exclude_self: bool,
) -> Result<()> {
    let flag = inst.operands.get(2).copied();
    let literal_flag = match flag {
        Some(value) => const_bool_operand(ctx, value)?,
        None => Some(exclude_self),
    };
    // No class name holds a NUL byte, so such a target names no class. The eval bridge below
    // reads its candidate list as NUL-separated names and would match a piece of it.
    if literal_flag == Some(false) || target.contains('\0') {
        emit_bool_result(ctx, false);
        return store_if_result(ctx, inst);
    }
    let target_key = php_symbol_key(target.trim_start_matches('\\'));
    let mut candidates = std::collections::BTreeSet::new();
    // `Closure` has no class-info entry; as a name it is a class whose only ancestor is itself.
    if !exclude_self && target_key == php_symbol_key("Closure") {
        candidates.insert("Closure".to_string());
    }
    for name in ctx.module.class_infos.keys() {
        let same = php_symbol_key(name.trim_start_matches('\\')) == target_key;
        if (!exclude_self && same) || parent_chain_contains(ctx, name, &target_key)
            || (class_interfaces_contain(ctx, name, &target_key)
                && interface_is_relation(ctx, name, target, exclude_self))
        { candidates.insert(name.trim_start_matches('\\').to_string()); }
    }
    for name in ctx.module.interface_infos.keys() {
        let same = php_symbol_key(name.trim_start_matches('\\')) == target_key;
        let mut parents = Vec::new();
        super::super::class_relations::collect_interface_parents(ctx, name, &mut parents);
        if (!exclude_self && same) || parents.iter().any(|name|
            php_symbol_key(name.trim_start_matches('\\')) == target_key)
        { candidates.insert(name.trim_start_matches('\\').to_string()); }
    }
    let matched = ctx.next_label("class_string_relation_match");
    let done = ctx.next_label("class_string_relation_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(subject, ptr, len)?;
    abi::emit_store_to_sp(ctx.emitter, ptr, 0);
    abi::emit_store_to_sp(ctx.emitter, len, 8);
    if literal_flag.is_none() {
        predicates::emit_value_truthiness(ctx, flag.expect("runtime flag exists"), "is_a")?;
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    }
    for candidate in &candidates {
        emit_match(ctx, 0, candidate, &matched);
        emit_match(ctx, 0, &format!("\\{candidate}"), &matched);
    }
    if ctx.module.required_runtime_features.eval_bridge {
        let mut related = vec![target.trim_start_matches('\\').to_string()];
        // Dynamic classes can inherit a compiled class or interface related to the target.
        // Include the target itself even for is_subclass_of; the ABI excludes only exact self.
        related.extend(candidates);
        let related = related.join("\0");
        let (label, len) = ctx.data.add_string(related.as_bytes());
        for index in 0..2 {
            abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index), index * 8);
        }
        abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), &label);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), len as i64);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), i64::from(exclude_self));
        let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_class_string_relation_v1");
        abi::emit_call_label(ctx.emitter, &symbol);
        if literal_flag.is_none() {
            abi::emit_branch_if_int_result_nonzero(ctx.emitter, &matched);
        }
    } else {
        emit_bool_result(ctx, false);
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&matched);
    if literal_flag.is_none() {
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    } else { emit_bool_result(ctx, true); }
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    store_if_result(ctx, inst)
}

/// Compares the class string saved at `slot` (pointer, then length) with `candidate`, using the
/// target's string-comparison ABI, and branches to `matched` when they are equal.
fn emit_match(ctx: &mut FunctionContext<'_>, slot: usize, candidate: &str, matched: &str) {
    let (label, len) = ctx.data.add_string(candidate.as_bytes());
    let regs = match ctx.emitter.target.arch {
        Arch::AArch64 => ["x1", "x2", "x3", "x4"],
        Arch::X86_64 => ["rdi", "rsi", "rdx", "rcx"],
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, regs[0], slot);
    abi::emit_load_temporary_stack_slot(ctx.emitter, regs[1], slot + 8);
    abi::emit_symbol_address(ctx.emitter, regs[2], &label);
    abi::emit_load_int_immediate(ctx.emitter, regs[3], len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test the PHP class-name comparison result
            ctx.emitter.instruction(&format!("b.eq {matched}"));                // accept this related class or interface
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test the PHP class-name comparison result
            ctx.emitter.instruction(&format!("je {matched}"));                  // accept this related class or interface
        }
    }
}

/// Emits a relation test whose target class name is only known at runtime
/// (`is_a($name, $name, true)`, `is_subclass_of($object, $name)`).
///
/// Every compiled class and interface the subject may be is known, and so is each one's set of
/// ancestors (itself unless `exclude_self`, its parents, and its interfaces), so the subject
/// selects one ancestor list and the target is compared against it. A string subject is matched
/// by name; an object subject by its runtime class id, not its static type, which may be a
/// parent. A Mixed subject is either, by its runtime tag. The `$allow_string` flag only governs a
/// string subject. A name no compiled class-like matches can only be an eval-declared class.
pub(super) fn lower_runtime_target(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, subject: ValueId,
    target: ValueId, exclude_self: bool,
) -> Result<()> {
    let subject_ty = ctx.value_php_type(subject)?;
    if ctx.module.required_runtime_features.eval_bridge {
        // An eval-declared object carries the class id of its nearest compiled ancestor, so the
        // class-id table below would miss the eval class itself. Objects go to the eval bridge.
        match subject_ty {
            PhpType::Object(_) => {
                return super::super::lower_eval_object_is_a_dynamic(ctx, inst, subject, target, exclude_self);
            }
            PhpType::Mixed | PhpType::Union(_) => {
                let object_label = ctx.next_label("class_relation_eval_object");
                let done = ctx.next_label("class_relation_eval_done");
                ctx.load_value_to_reg(subject, abi::int_result_reg(ctx.emitter))?;
                abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction("cmp x0, #6");                  // runtime tag 6 means the subject is an object
                        ctx.emitter.instruction(&format!("b.eq {object_label}")); // ask the eval bridge about an object subject
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction("cmp rax, 6");                  // runtime tag 6 means the subject is an object
                        ctx.emitter.instruction(&format!("je {object_label}")); // ask the eval bridge about an object subject
                    }
                }
                emit_runtime_target(ctx, inst, subject, &subject_ty, target, exclude_self)?;
                abi::emit_jump(ctx.emitter, &done);
                ctx.emitter.label(&object_label);
                // A literal target keeps the named relation query objects always used.
                match optional_const_string_operand(ctx, target)? {
                    Some(literal) => super::super::lower_eval_object_is_a(ctx, inst, subject, &literal, exclude_self)?,
                    None => super::super::lower_eval_object_is_a_dynamic(ctx, inst, subject, target, exclude_self)?,
                }
                ctx.emitter.label(&done);
                return Ok(());
            }
            _ => {}
        }
    }
    emit_runtime_target(ctx, inst, subject, &subject_ty, target, exclude_self)
}

/// Emits the compiled-metadata half of [`lower_runtime_target`].
fn emit_runtime_target(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, subject: ValueId,
    subject_ty: &PhpType, target: ValueId, exclude_self: bool,
) -> Result<()> {
    let flag = inst.operands.get(2).copied();
    let literal_flag = match flag {
        Some(value) => const_bool_operand(ctx, value)?,
        None => Some(exclude_self),
    };
    let literal_subject = optional_const_string_operand(ctx, subject)?
        .map(|name| php_symbol_key(name.trim_start_matches('\\')));
    let mut table = ancestor_table(ctx, exclude_self);
    // A literal target keeps only the class-likes related to it, and each of those matches
    // outright: any other compiled subject is unrelated.
    let literal_target = optional_const_string_operand(ctx, target)?
        .map(|name| php_symbol_key(name.trim_start_matches('\\')));
    if let Some(key) = &literal_target {
        table.retain(|_, ancestors| ancestors.iter().any(|ancestor| php_symbol_key(ancestor) == *key));
    }
    let named = table.iter()
        .filter(|(name, _)| literal_subject.as_ref().is_none_or(|key| php_symbol_key(name) == *key))
        .map(|(name, ancestors)| (name.clone(), ancestors))
        .collect::<Vec<_>>();
    let class_ids = ctx.module.class_infos.iter()
        .filter_map(|(name, info)| table.get_key_value(name.trim_start_matches('\\')).map(|(name, _)| (info.class_id, name.clone())))
        .collect::<std::collections::BTreeMap<_, _>>();
    let blocks = table.keys()
        .map(|name| (name.clone(), ctx.next_label("class_relation_ancestors")))
        .collect::<std::collections::BTreeMap<_, _>>();
    let string_dispatch = ctx.next_label("class_relation_string_subject");
    let object_dispatch = ctx.next_label("class_relation_object_subject");
    let closure_subject = ctx.next_label("class_relation_closure_subject");
    let matched = ctx.next_label("class_relation_match");
    let unknown = ctx.next_label("class_relation_unknown_name");
    let unrelated = ctx.next_label("class_relation_unrelated");
    let done = ctx.next_label("class_relation_done");

    // Frame: [0) subject name ptr, [8) its length, [16) the answer when related (the runtime
    // `$allow_string` truthiness for a name, 1 for an object), [24) target ptr, [32) its length,
    // [40) the object subject's class id, [48) the eval bridge candidate list ptr, [56) its length.
    abi::emit_reserve_temporary_stack(ctx.emitter, 64);
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(target, ptr, len)?;
    abi::emit_store_to_sp(ctx.emitter, ptr, 24);
    abi::emit_store_to_sp(ctx.emitter, len, 32);
    if literal_flag.is_none() {
        predicates::emit_value_truthiness(ctx, flag.expect("runtime flag exists"), "is_a")?;
        abi::emit_store_to_sp(ctx.emitter, abi::int_result_reg(ctx.emitter), 16);
    }
    let result = abi::int_result_reg(ctx.emitter);
    match subject_ty {
        PhpType::Str => {
            ctx.load_string_value_to_regs(subject, ptr, len)?;
            abi::emit_store_to_sp(ctx.emitter, ptr, 0);
            abi::emit_store_to_sp(ctx.emitter, len, 8);
            abi::emit_jump(ctx.emitter, &string_dispatch);
        }
        PhpType::Object(_) => {
            ctx.load_value_to_reg(subject, result)?;
            emit_object_class_id(ctx, &unrelated);
            abi::emit_jump(ctx.emitter, &object_dispatch);
        }
        PhpType::Callable => {
            ctx.load_value_to_reg(subject, result)?;
            abi::emit_jump(ctx.emitter, &closure_subject);
        }
        _ => {
            ctx.load_value_to_reg(subject, result)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let object_label = ctx.next_label("class_relation_mixed_object");
            let mixed_closure = ctx.next_label("class_relation_mixed_closure");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #6");                      // runtime tag 6 means the subject is an object
                    ctx.emitter.instruction(&format!("b.eq {object_label}"));   // match an object subject by its class id
                    ctx.emitter.instruction("cmp x0, #10");                     // runtime tag 10 means the subject is a Closure
                    ctx.emitter.instruction(&format!("b.eq {mixed_closure}"));  // match a callable descriptor that is a Closure
                    ctx.emitter.instruction("cmp x0, #1");                      // runtime tag 1 means the subject is a class name
                    ctx.emitter.instruction(&format!("b.ne {unrelated}"));      // any other value names no class
                    ctx.emitter.instruction("str x1, [sp, #0]");                // save the unboxed subject name pointer
                    ctx.emitter.instruction("str x2, [sp, #8]");                // save the unboxed subject name length
                    ctx.emitter.instruction(&format!("b {string_dispatch}"));   // match the subject by name
                    ctx.emitter.label(&mixed_closure);
                    ctx.emitter.instruction("mov x0, x1");                      // move the unboxed descriptor into the result register
                    ctx.emitter.instruction(&format!("b {closure_subject}"));   // check the descriptor's Closure identity
                    ctx.emitter.label(&object_label);
                    ctx.emitter.instruction("mov x0, x1");                      // move the unboxed object pointer into the result register
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 6");                      // runtime tag 6 means the subject is an object
                    ctx.emitter.instruction(&format!("je {object_label}"));     // match an object subject by its class id
                    ctx.emitter.instruction("cmp rax, 10");                     // runtime tag 10 means the subject is a Closure
                    ctx.emitter.instruction(&format!("je {mixed_closure}"));    // match a callable descriptor that is a Closure
                    ctx.emitter.instruction("cmp rax, 1");                      // runtime tag 1 means the subject is a class name
                    ctx.emitter.instruction(&format!("jne {unrelated}"));       // any other value names no class
                    ctx.emitter.instruction("mov QWORD PTR [rsp], rdi");        // save the unboxed subject name pointer
                    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");    // save the unboxed subject name length
                    ctx.emitter.instruction(&format!("jmp {string_dispatch}")); // match the subject by name
                    ctx.emitter.label(&mixed_closure);
                    ctx.emitter.instruction("mov rax, rdi");                    // move the unboxed descriptor into the result register
                    ctx.emitter.instruction(&format!("jmp {closure_subject}")); // check the descriptor's Closure identity
                    ctx.emitter.label(&object_label);
                    ctx.emitter.instruction("mov rax, rdi");                    // move the unboxed object pointer into the result register
                }
            }
            emit_object_class_id(ctx, &unrelated);
            abi::emit_jump(ctx.emitter, &object_dispatch);
        }
    }

    // -- closure subject: the callable descriptor is in the result register. Only a descriptor
    // with an object handle (or an eval callback adapter wrapping an eval closure) is a Closure;
    // an implicit string or array callable (`'strlen'`, `[$obj, 'm']` passed as a callable) is
    // no object. A Closure is final and implements
    // nothing, so its only ancestor is Closure itself, which is_subclass_of() excludes --
    ctx.emitter.label(&closure_subject);
    if exclude_self {
        abi::emit_jump(ctx.emitter, &unrelated);
    } else {
        let closure = ctx.next_label("class_relation_closure_identity");
        crate::codegen::lower_inst::predicates::emit_descriptor_closure_branch(ctx, result, &closure, &unrelated);
        ctx.emitter.label(&closure);
        if literal_flag.is_none() {
            abi::emit_load_int_immediate(ctx.emitter, result, 1);
            abi::emit_store_to_sp(ctx.emitter, result, 16);
        }
        emit_match(ctx, 24, "Closure", &matched);
        emit_match(ctx, 24, "\\Closure", &matched);
        abi::emit_jump(ctx.emitter, &unrelated);
    }

    // -- object subject: the runtime class id selects the ancestor list --
    ctx.emitter.label(&object_dispatch);
    if literal_flag.is_none() {
        abi::emit_load_int_immediate(ctx.emitter, result, 1);
        abi::emit_store_to_sp(ctx.emitter, result, 16);
    }
    let (id_reg, candidate_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x9", "x10"),
        Arch::X86_64 => ("r10", "r11"),
    };
    abi::emit_load_temporary_stack_slot(ctx.emitter, id_reg, 40);
    for (class_id, name) in &class_ids {
        abi::emit_load_int_immediate(ctx.emitter, candidate_reg, *class_id as i64);
        let block = &blocks[name];
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x9, x10");                         // is the object an instance of exactly this class?
                ctx.emitter.instruction(&format!("b.eq {block}"));              // test the target against this class's ancestors
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp r10, r11");                        // is the object an instance of exactly this class?
                ctx.emitter.instruction(&format!("je {block}"));                // test the target against this class's ancestors
            }
        }
    }
    abi::emit_jump(ctx.emitter, &unrelated);

    // -- string subject: the class or interface name selects the ancestor list --
    ctx.emitter.label(&string_dispatch);
    if literal_flag == Some(false) {
        abi::emit_jump(ctx.emitter, &unrelated);
    } else {
        for (name, _) in &named {
            let block = blocks[name].clone();
            emit_match(ctx, 0, name, &block);
            emit_match(ctx, 0, &format!("\\{name}"), &block);
        }
        abi::emit_jump(ctx.emitter, &unknown);
    }

    for (name, ancestors) in &table {
        ctx.emitter.label(&blocks[name]);
        if literal_target.is_some() {
            abi::emit_jump(ctx.emitter, &matched);
            continue;
        }
        for ancestor in ancestors {
            emit_match(ctx, 24, ancestor, &matched);
            emit_match(ctx, 24, &format!("\\{ancestor}"), &matched);
        }
        abi::emit_jump(ctx.emitter, &unrelated);
    }

    ctx.emitter.label(&unknown);
    if ctx.module.required_runtime_features.eval_bridge {
        // The bridge splits its candidate list at NUL bytes, so a target holding one would be
        // read as several names. No class name holds a NUL byte: such a target names no class.
        emit_branch_if_slot_string_has_nul(ctx, 24, &unrelated);
        // Only an eval-declared class can still answer, and it may extend or implement a
        // compiled class-like related to the target. The bridge only walks eval classes, so like
        // the literal-target probe it gets the target first, then those compiled descendants.
        emit_eval_candidate_list(ctx, literal_target.as_deref())?;
        for index in 0..2 {
            abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, index), index * 8);
        }
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), 48);
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), 56);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), i64::from(exclude_self));
        let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_class_string_relation_v1");
        abi::emit_call_label(ctx.emitter, &symbol);
        if literal_flag.is_none() {
            abi::emit_branch_if_int_result_nonzero(ctx.emitter, &matched);
        }
        abi::emit_jump(ctx.emitter, &done);
    }
    ctx.emitter.label(&unrelated);
    emit_bool_result(ctx, false);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&matched);
    if literal_flag.is_none() {
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    } else { emit_bool_result(ctx, true); }
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    store_if_result(ctx, inst)
}

/// Saves the eval bridge's NUL-separated candidate list to frame slots 48 and 56: the target,
/// then every compiled class or interface that is the target or descends from it, which an
/// eval-declared class may extend or implement. A literal target (already folded to its symbol
/// key) selects its list at compile time; a runtime target in slots 24 and 32 is matched against
/// every compiled class-like name, and a name none matches is the whole list on its own.
fn emit_eval_candidate_list(ctx: &mut FunctionContext<'_>, literal_target: Option<&str>) -> Result<()> {
    let mut descendants: std::collections::BTreeMap<String, (String, std::collections::BTreeSet<String>)> =
        std::collections::BTreeMap::new();
    for (name, ancestors) in ancestor_table(ctx, false) {
        for ancestor in ancestors {
            descendants.entry(php_symbol_key(&ancestor))
                .or_insert_with(|| (ancestor.clone(), std::collections::BTreeSet::new()))
                .1.insert(name.clone());
        }
    }
    let list = |target: &str, related: &std::collections::BTreeSet<String>| {
        std::iter::once(target.to_string()).chain(related.iter().cloned()).collect::<Vec<_>>().join("\0")
    };
    let (ptr, len) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x9", "x10"),
        Arch::X86_64 => ("r10", "r11"),
    };
    let done = ctx.next_label("class_relation_eval_candidates_done");
    if let Some(key) = literal_target {
        if let Some((target, related)) = descendants.get(key) {
            let (label, length) = ctx.data.add_string(list(target, related).as_bytes());
            abi::emit_symbol_address(ctx.emitter, ptr, &label);
            abi::emit_load_int_immediate(ctx.emitter, len, length as i64);
            abi::emit_store_to_sp(ctx.emitter, ptr, 48);
            abi::emit_store_to_sp(ctx.emitter, len, 56);
            return Ok(());
        }
    } else {
        for (target, related) in descendants.values() {
            let selected = ctx.next_label("class_relation_eval_candidates");
            let next = ctx.next_label("class_relation_eval_candidates_next");
            emit_match(ctx, 24, target, &selected);
            emit_match(ctx, 24, &format!("\\{target}"), &selected);
            abi::emit_jump(ctx.emitter, &next);
            ctx.emitter.label(&selected);
            let (label, length) = ctx.data.add_string(list(target, related).as_bytes());
            abi::emit_symbol_address(ctx.emitter, ptr, &label);
            abi::emit_load_int_immediate(ctx.emitter, len, length as i64);
            abi::emit_store_to_sp(ctx.emitter, ptr, 48);
            abi::emit_store_to_sp(ctx.emitter, len, 56);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&next);
        }
    }
    // -- a target no compiled class-like names: only an eval-declared class can match it --
    abi::emit_load_temporary_stack_slot(ctx.emitter, ptr, 24);
    abi::emit_load_temporary_stack_slot(ctx.emitter, len, 32);
    abi::emit_store_to_sp(ctx.emitter, ptr, 48);
    abi::emit_store_to_sp(ctx.emitter, len, 56);
    ctx.emitter.label(&done);
    Ok(())
}

/// Branches to `found` when the string saved at `slot` (pointer, then length) holds a NUL byte.
fn emit_branch_if_slot_string_has_nul(ctx: &mut FunctionContext<'_>, slot: usize, found: &str) {
    let scan = ctx.next_label("class_relation_nul_scan");
    let clean = ctx.next_label("class_relation_nul_free");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x9, [sp, #{slot}]"));         // load the string pointer to scan
            ctx.emitter.instruction(&format!("ldr x10, [sp, #{}]", slot + 8));  // load the count of bytes left to scan
            ctx.emitter.label(&scan);
            ctx.emitter.instruction(&format!("cbz x10, {clean}"));              // every byte scanned: the string holds no NUL
            ctx.emitter.instruction("ldrb w11, [x9], #1");                      // read the next byte and advance the pointer
            ctx.emitter.instruction(&format!("cbz w11, {found}"));              // a NUL byte: report it
            ctx.emitter.instruction("sub x10, x10, #1");                        // one byte fewer left to scan
            ctx.emitter.instruction(&format!("b {scan}"));                      // scan the next byte
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {slot}]")); // load the string pointer to scan
            ctx.emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", slot + 8)); // load the count of bytes left to scan
            ctx.emitter.label(&scan);
            ctx.emitter.instruction("test r11, r11");                           // are any bytes left to scan?
            ctx.emitter.instruction(&format!("jz {clean}"));                    // every byte scanned: the string holds no NUL
            ctx.emitter.instruction("cmp BYTE PTR [r10], 0");                   // is the next byte NUL?
            ctx.emitter.instruction(&format!("je {found}"));                    // a NUL byte: report it
            ctx.emitter.instruction("inc r10");                                 // advance to the next byte
            ctx.emitter.instruction("dec r11");                                 // one byte fewer left to scan
            ctx.emitter.instruction(&format!("jmp {scan}"));                    // scan the next byte
        }
    }
    ctx.emitter.label(&clean);
}

/// Saves the class id of the object in the result register to the frame's slot 40, branching
/// to `unrelated` for a null object: a zero pointer, or the in-band null sentinel a missed
/// array read (`@$objects[$missing]`) leaves in an object slot.
fn emit_object_class_id(ctx: &mut FunctionContext<'_>, unrelated: &str) {
    let (result, scratch) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x0", "x9"),
        Arch::X86_64 => ("rax", "r10"),
    };
    crate::codegen_support::sentinels::emit_branch_if_null_container(ctx.emitter, result, scratch, unrelated);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            elephc_memoir::values::object_storage::emit_load_class_id(ctx.emitter, "x0", "x0");
            ctx.emitter.instruction("str x0, [sp, #40]");                       // save the subject's class id for the dispatch
        }
        Arch::X86_64 => {
            elephc_memoir::values::object_storage::emit_load_class_id(ctx.emitter, "rax", "rax");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 40], rax");           // save the subject's class id for the dispatch
        }
    }
}

/// Whether `class` implementing `interface` counts for the relation. A generic class template
/// (`Box`) is a hidden marker interface of every instantiation, and php-src's `Box` is the one
/// class those instantiations stand for: `is_a()` holds for an instantiation, `is_subclass_of()`
/// only for a class extending one, so with `exclude_self` the marker counts only when the
/// class's parent carries it. The literal-target path applies the same rule.
fn interface_is_relation(ctx: &FunctionContext<'_>, class: &str, interface: &str, exclude_self: bool) -> bool {
    let interface = interface.trim_start_matches('\\');
    if !exclude_self || !super::super::class_relations::reflection_hidden_interface(ctx, interface) {
        return true;
    }
    lookup_class(ctx, class.trim_start_matches('\\'))
        .and_then(|info| info.parent.clone())
        .is_some_and(|parent| {
            class_interfaces_contain(ctx, parent.trim_start_matches('\\'), &php_symbol_key(interface))
        })
}

/// Lists every compiled class and interface with the names it is related to: itself unless
/// `exclude_self`, its parent classes and its interfaces (an interface's own parents). Sorted, so
/// the emitted comparisons do not depend on hash order.
fn ancestor_table(
    ctx: &FunctionContext<'_>, exclude_self: bool,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut table = std::collections::BTreeMap::new();
    for (name, info) in &ctx.module.class_infos {
        let name = name.trim_start_matches('\\').to_string();
        let mut ancestors = std::collections::BTreeSet::new();
        if !exclude_self { ancestors.insert(name.clone()); }
        let mut current = info.parent.clone();
        while let Some(parent) = current {
            let parent = parent.trim_start_matches('\\').to_string();
            current = lookup_class(ctx, &parent).and_then(|info| info.parent.clone());
            if !ancestors.insert(parent) { break; }
        }
        ancestors.extend(info.interfaces.iter()
            .filter(|interface| interface_is_relation(ctx, &name, interface, exclude_self))
            .map(|name| name.trim_start_matches('\\').to_string()));
        if !ancestors.is_empty() { table.insert(name, ancestors); }
    }
    for name in ctx.module.interface_infos.keys() {
        let mut parents = Vec::new();
        super::super::class_relations::collect_interface_parents(ctx, name, &mut parents);
        let name = name.trim_start_matches('\\').to_string();
        let mut ancestors: std::collections::BTreeSet<String> =
            parents.iter().map(|parent| parent.trim_start_matches('\\').to_string()).collect();
        if !exclude_self { ancestors.insert(name.clone()); }
        if !ancestors.is_empty() { table.insert(name, ancestors); }
    }
    // `Closure` has no class-info entry; as a name it is a class whose only ancestor is itself.
    if !exclude_self && !table.keys().any(|name| php_symbol_key(name) == php_symbol_key("Closure")) {
        table.insert("Closure".to_string(), std::collections::BTreeSet::from(["Closure".to_string()]));
    }
    table
}
