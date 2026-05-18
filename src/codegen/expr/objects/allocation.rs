//! Purpose:
//! Lowers object allocation and constructor-ready initialization.
//! Produces object-related expression results while respecting runtime metadata and ownership rules.
//!
//! Called from:
//! - `crate::codegen::expr::objects`
//!
//! Key details:
//! - Object handles, property storage, and class ids must stay consistent with emitted class tables.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::calls::args as call_args;
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::names::method_symbol;
use crate::parser::ast::{Expr, ExprKind, TypeExpr};
use crate::types::PhpType;

use super::super::{
    coerce_result_to_type, emit_expr, restore_concat_offset_after_nested_call,
    save_concat_offset_before_nested_call,
};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

pub(super) fn emit_new_object(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if class_name == "Fiber" {
        return emit_new_fiber(args, emitter, ctx, data);
    }
    if super::reflection::is_reflection_owner_class(class_name) {
        return super::reflection::emit_new_reflection_owner(
            class_name, args, emitter, ctx, data,
        );
    }
    emit_new_object_core(class_name, args, true, emitter, ctx, data)
}

pub(super) fn emit_new_object_core(
    class_name: &str,
    args: &[Expr],
    run_constructor: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let class_info = match ctx.classes.get(class_name).cloned() {
        Some(c) => c,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return PhpType::Int;
        }
    };
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        emitter.comment("new stdClass()");
        // stdClass instances do not have static property slots; the
        // dedicated runtime helper allocates the 16-byte payload, stamps
        // the class_id, and seeds the dynamic-property hash. User-supplied
        // arguments (none allowed by PHP for stdClass) are ignored here.
        let _ = args;
        abi::emit_call_label(emitter, "__rt_stdclass_new");                     // allocate a fresh stdClass instance with an empty property hash
        return PhpType::Object(class_name.to_string());
    }
    let num_props = class_info.properties.len();
    // PHP 8.2 #[\AllowDynamicProperties] adds a single 8-byte slot after the
    // declared properties to hold a lazily-allocated hashtable pointer for
    // undeclared property storage. The slot is initialised to 0 (null) and
    // only allocated on the first dynamic property write.
    let dyn_props_slot = if class_info.allow_dynamic_properties {
        8
    } else {
        0
    };
    let obj_size = 8 + num_props * 16 + dyn_props_slot; // 8 for class_id + 16 per property + optional dyn_props ptr

    emitter.comment(&format!("new {}()", class_name));

    // -- allocate object on heap --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", obj_size));             // object size in bytes
            emitter.instruction("bl __rt_heap_alloc");                          // allocate object -> x0 = pointer
            emitter.instruction("mov x9, #4");                                  // heap kind 4 = object instance
            emitter.instruction("str x9, [x0, #-8]");                           // store object kind in the uniform heap header
            emitter.instruction(&format!("mov x10, #{}", class_info.class_id)); // load compile-time class id
            emitter.instruction("str x10, [x0]");                               // store class id at object header
            abi::emit_push_reg(emitter, "x0");                                  // save the allocated object pointer while property slots are initialized
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", obj_size));             // object size in bytes
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate object -> rax = pointer
            emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word with the uniform heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocation as an object instance in the x86_64 uniform heap header
            emitter.instruction(&format!("mov r10, {}", class_info.class_id));  // load the compile-time class id for the allocated object instance
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the class id in the first field of the object payload
            abi::emit_push_reg(emitter, "rax");                                 // save the allocated object pointer while property slots are initialized
        }
    }

    // -- zero-initialize all property slots --
    for i in 0..num_props {
        let offset = 8 + i * 16;
        let property_name = &class_info.properties[i].0;
        let starts_uninitialized = class_info.declared_properties.contains(property_name)
            && class_info.defaults.get(i).is_some_and(|default| default.is_none());
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x9, [sp]");                            // peek object pointer
                emitter.instruction(&format!("str xzr, [x9, #{}]", offset));    // zero-init property lo
                emitter.instruction(&format!("str xzr, [x9, #{}]", offset + 8)); // zero-init property hi
            }
            Arch::X86_64 => {
                emitter.instruction("mov r11, QWORD PTR [rsp]");                // peek the allocated object pointer from the temporary stack slot
                emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", offset)); // zero-initialize the low word of the property storage slot
                emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", offset + 8)); // zero-initialize the high word / runtime metadata slot
            }
        }
        if starts_uninitialized {
            let marker_reg = abi::temp_int_reg(emitter.target);
            abi::emit_load_int_immediate(emitter, marker_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x9, [sp]");                        // peek object pointer before marking this typed property uninitialized
                }
                Arch::X86_64 => {
                    emitter.instruction("mov r11, QWORD PTR [rsp]");            // peek object pointer before marking this typed property uninitialized
                }
            }
            let object_reg = match emitter.target.arch {
                Arch::AArch64 => "x9",
                Arch::X86_64 => "r11",
            };
            abi::emit_store_to_address(emitter, marker_reg, object_reg, offset + 8);
        }
    }

    // -- allocate the dyn_props hashtable if the class declares
    // #[\AllowDynamicProperties], and store the pointer in the slot --
    if dyn_props_slot != 0 {
        let offset = 8 + num_props * 16;
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #4");                              // initial hashtable capacity for dyn_props
                emitter.instruction("mov x1, #7");                              // value type tag = mixed (heterogeneous)
                emitter.instruction("bl __rt_hash_new");                        // allocate empty hashtable -> x0 = hashtable pointer
                emitter.instruction("ldr x9, [sp]");                            // peek object pointer for dyn_props slot store
                emitter.instruction(&format!("str x0, [x9, #{}]", offset));     // store the hashtable pointer in the dyn_props slot
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, 4");                              // initial hashtable capacity for dyn_props
                emitter.instruction("mov rsi, 7");                              // value type tag = mixed
                emitter.instruction("call __rt_hash_new");                      // allocate empty hashtable -> rax = hashtable pointer
                emitter.instruction("mov r11, QWORD PTR [rsp]");                // peek object pointer for dyn_props slot store
                emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", offset)); // store the hashtable pointer in the dyn_props slot
            }
        }
    }

    // -- set default property values --
    for i in 0..num_props {
        if let Some(default_expr) = &class_info.defaults[i] {
            let default_expr = default_expr.clone();
            let offset = 8 + i * 16;
            let actual_ty = emit_expr(&default_expr, emitter, ctx, data);
            let prop_name = &class_info.properties[i].0;
            let expected_ty = class_info.properties[i].1.clone();
            let prop_ty = if class_info.declared_properties.contains(prop_name) {
                coerce_result_to_type(emitter, ctx, data, &actual_ty, &expected_ty);
                expected_ty
            } else {
                actual_ty
            };
            let object_reg = abi::symbol_scratch_reg(emitter);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("ldr {}, [sp]", object_reg));  // peek object pointer from the temporary stack slot on AArch64
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", object_reg)); // peek object pointer from the temporary stack slot on x86_64
                }
            }
            match &prop_ty {
                PhpType::Int
                | PhpType::Bool
                | PhpType::Callable
                | PhpType::Pointer(_)
                | PhpType::Buffer(_)
                | PhpType::Packed(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
                }
                PhpType::Resource(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 9);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Mixed | PhpType::Iterable => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 7);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Union(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 7);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 4);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::AssocArray { .. } => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 5);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Object(_) => {
                    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
                    let tag_reg = abi::temp_int_reg(emitter.target);
                    abi::emit_load_int_immediate(emitter, tag_reg, 6);
                    abi::emit_store_to_address(emitter, tag_reg, object_reg, offset + 8);
                }
                PhpType::Float => {
                    abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), object_reg, offset);
                    abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
                }
                PhpType::Str => {
                    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                    abi::emit_store_to_address(emitter, ptr_reg, object_reg, offset);
                    abi::emit_store_to_address(emitter, len_reg, object_reg, offset + 8);
                }
                PhpType::Void | PhpType::Never => {}
            }
        }
    }

    // -- call __construct if it exists --
    if run_constructor && class_info.methods.contains_key("__construct") {
        let sig = class_info.methods.get("__construct").cloned();
        let regular_param_count = call_args::regular_param_count(sig.as_ref(), args.len());
        let emitted_args = call_args::emit_pushed_call_args(
            args,
            sig.as_ref(),
            regular_param_count,
            "constructor ref arg",
            false,
            true,
            emitter,
            ctx,
            data,
        );
        let arg_types = emitted_args.arg_types;

        let assignments = crate::codegen::abi::build_outgoing_arg_assignments_for_target(
            emitter.target,
            &arg_types,
            1,
        );
        let overflow_bytes =
            crate::codegen::abi::materialize_outgoing_args(emitter, &assignments);

        let receiver_offset = overflow_bytes + emitted_args.source_temp_bytes;
        if receiver_offset == 0 {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x0, [sp]");                        // load $this directly from the top of the stack when all args stayed in registers on AArch64
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, QWORD PTR [rsp]");            // load $this directly into the first SysV integer argument register when all args stayed in registers on x86_64
                }
            }
        } else {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("ldr x0, [sp, #{}]", receiver_offset)); // skip argument temporaries to reload the saved object pointer as $this on AArch64
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", receiver_offset)); // skip argument temporaries to reload the saved object pointer as $this in the first SysV integer argument register on x86_64
                }
            }
        }
        save_concat_offset_before_nested_call(emitter, ctx);
        let constructor_impl = class_info
            .method_impl_classes
            .get("__construct")
            .map(String::as_str)
            .unwrap_or(class_name);
        abi::emit_call_label(emitter, &method_symbol(constructor_impl, "__construct")); // call the resolved constructor implementation for the active target ABI
        restore_concat_offset_after_nested_call(emitter, ctx, &PhpType::Void);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);             // drop spilled constructor arguments after the nested call returns
        abi::emit_release_temporary_stack(emitter, emitted_args.source_temp_bytes); // drop source-order named-argument temporaries after constructor dispatch
    }

    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the allocated object pointer as the expression result for the active target ABI
    PhpType::Object(class_name.to_string())
}

/// Codegen interception for `new Fiber($callable)`.
///
/// The standard `emit_new_object` path would size the object as `8 + num_props * 16`,
/// which for Fiber (zero declared properties) yields only the object header and
/// not enough room for the runtime-managed Fiber payload. We instead delegate the
/// entire allocation, stack setup, and field initialisation to `__rt_fiber_construct`,
/// passing the captured closure plus the runtime class id so `instanceof Fiber` keeps
/// working.
fn emit_new_fiber(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let class_id = ctx
        .classes
        .get("Fiber")
        .map(|info| info.class_id)
        .unwrap_or(0);

    emitter.comment("new Fiber() — runtime construction");

    let wrapper_label = if let Some(callable_expr) = args.first() {
        emit_expr(callable_expr, emitter, ctx, data);
        super::fiber_wrapper::prepare_fiber_wrapper(callable_expr, ctx)
    } else {
        emitter.comment("WARNING: Fiber constructor missing $callback argument");
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        None
    };

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the closure pointer across constructor-argument setup
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        class_id as i64,
    );                                                                          // load the runtime class id of Fiber into the second integer argument register
    if let Some(label) = wrapper_label {
        abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 2), &label);
    } else {
        abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 2), 0);
    }
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));       // pop the closure pointer into the first integer argument register for the active target ABI
    abi::emit_call_label(emitter, "__rt_fiber_construct");                      // delegate allocation, stack setup, and field initialisation to the runtime helper

    // -- closure capture pre-load: when `new Fiber(function(...) use(...))` is
    //    used, evaluate each captured variable in the surrounding scope NOW
    //    (it is no longer reachable from inside the fiber's stack) and stash
    //    the boxed Mixed value in the trailing start_args slots. The
    //    user_arg_max field is lowered to the closure's user-param count so a
    //    later $f->start(...) only writes to the leading slots and leaves the
    //    captured values intact.
    let capture_info = args.first().and_then(|callable_expr| match &callable_expr.kind {
        ExprKind::Closure {
            params,
            captures,
            capture_refs,
            ..
        } if !captures.is_empty() => {
            // Compute the closure's user-param register footprint so capture
            // cursors start past whatever the closure's own parameters consume.
            // Floats land in d-regs, strings consume two int regs, everything
            // else consumes one int reg.
            let mut user_int_regs = 0usize;
            let mut user_float_regs = 0usize;
            for (_, type_expr, _, _) in params {
                let mut ty = type_expr.as_ref();
                while let Some(TypeExpr::Nullable(inner)) = ty {
                    ty = Some(inner.as_ref());
                }
                match ty {
                    Some(TypeExpr::Float) => user_float_regs += 1,
                    Some(TypeExpr::Str) => user_int_regs += 2,
                    _ => user_int_regs += 1,
                }
            }
            // user_arg_max for the start() spill path is the *count of user
            // params* (not their reg footprint). String params take 2 int
            // regs but are still one user param from the user's perspective,
            // and start() is type-erased Mixed-only so the spill cap mirrors
            // the param count, not the reg footprint.
            let user_param_count = params.len();
            Some((
                callable_expr.span,
                user_param_count,
                user_int_regs,
                user_float_regs,
                captures.clone(),
                capture_refs.clone(),
            ))
        }
        _ => None,
    });

    if let Some((
        span,
        user_param_count,
        user_int_regs,
        user_float_regs,
        captures,
        capture_refs,
    )) = capture_info
    {
        emit_fiber_capture_preload(
            emitter,
            ctx,
            data,
            user_param_count,
            user_int_regs,
            user_float_regs,
            &captures,
            &capture_refs,
            span,
        );
    }

    PhpType::Object("Fiber".to_string())
}

/// Push the freshly-built fiber pointer, evaluate each capture in the
/// surrounding scope, box it into a Mixed cell, and store it into the trailing
/// `start_args` slot the closure expects to find its capture in. Finally,
/// lower `user_arg_max` to the closure's user-param count so a future
/// `$f->start(...)` does not overwrite the pre-loaded captures.
fn emit_fiber_capture_preload(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    user_param_count: usize,
    user_int_regs: usize,
    user_float_regs: usize,
    captures: &[String],
    capture_refs: &[String],
    span: crate::span::Span,
) {
    let max_int_slots = crate::codegen::runtime::FIBER_START_ARGS_MAX as usize;
    let max_float_slots = crate::codegen::runtime::FIBER_FLOAT_ARGS_MAX as usize;
    let start_args_offset = crate::codegen::runtime::FIBER_START_ARGS_OFFSET;
    let float_args_offset = crate::codegen::runtime::FIBER_FLOAT_ARGS_OFFSET;
    let umax_off = crate::codegen::runtime::FIBER_USER_ARG_MAX_OFFSET;

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // park the freshly built Fiber pointer on the stack while we evaluate captures
    let mut int_slot = user_int_regs;
    let mut float_slot = user_float_regs;
    for capture_name in captures {
        let by_ref = capture_refs.iter().any(|name| name == capture_name);
        // The closure's body reads each capture through a local variable slot
        // whose declared type matches whatever PhpType the variable held in the
        // surrounding scope. Passing the raw register footprint (no Mixed
        // boxing) lines up with what the closure expects:
        //   * floats land in float_args[float_slot] and are loaded into d-regs
        //   * single-register int-like types (int, bool, object, callable,
        //     mixed) ride in start_args[int_slot]
        //   * strings ride as ptr+len across start_args[int_slot..int_slot+2]
        let actual_ty = if by_ref {
            if !crate::codegen::expr::calls::args::emit_ref_arg_variable_address(
                capture_name,
                "fiber capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: Fiber capture {} dropped — variable not found",
                    capture_name
                ));
                continue;
            }
            PhpType::Int
        } else {
            let var_expr = Expr::new(ExprKind::Variable(capture_name.clone()), span);
            emit_expr(&var_expr, emitter, ctx, data)
        };
        if matches!(actual_ty, PhpType::Float) {
            if float_slot >= max_float_slots {
                emitter.comment(&format!(
                    "WARNING: Fiber float capture {} dropped — float_args slot budget exhausted",
                    capture_name
                ));
                continue;
            }
            let off = float_args_offset + (float_slot as i32) * 8;
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x9, [sp]");                        // peek the saved Fiber pointer from the parking slot
                    emitter.instruction(&format!("str d0, [x9, #{}]", off));    // float_args[float_slot] = float bits (loaded back into d-reg by the trampoline)
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rcx, QWORD PTR [rsp]");            // peek the saved Fiber pointer from the parking slot
                    emitter.instruction(&format!("movsd QWORD PTR [rcx + {}], xmm0", off)); // float_args[float_slot] = float bits
                }
            }
            float_slot += 1;
            continue;                                                            // floats are scalar — no incref needed
        }
        let consumes_two = matches!(actual_ty, PhpType::Str);
        if int_slot >= max_int_slots {
            emitter.comment(&format!(
                "WARNING: Fiber capture {} dropped — start_args slot budget exhausted",
                capture_name
            ));
            break;
        }
        if consumes_two && int_slot + 1 >= max_int_slots {
            emitter.comment(&format!(
                "WARNING: Fiber string capture {} dropped — needs 2 register slots but only 1 remains",
                capture_name
            ));
            break;
        }
        let off_lo = start_args_offset + (int_slot as i32) * 8;
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x9, [sp]");                            // peek the saved Fiber pointer from the parking slot
                if consumes_two {
                    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                    let off_hi = start_args_offset + ((int_slot + 1) as i32) * 8;
                    emitter.instruction(&format!("str {}, [x9, #{}]", ptr_reg, off_lo)); // start_args[int_slot] = string ptr
                    emitter.instruction(&format!("str {}, [x9, #{}]", len_reg, off_hi)); // start_args[int_slot + 1] = string length
                    emitter.instruction(&format!("mov x0, {}", ptr_reg));       // x0 = string ptr for the incref helper (only the ptr half is heap-backed)
                    emitter.instruction("bl __rt_incref");                      // retain the captured string so it survives until the Fiber is freed
                } else {
                    emitter.instruction(&format!("str x0, [x9, #{}]", off_lo)); // start_args[int_slot] = single-register capture value
                    emitter.instruction("bl __rt_incref");                      // retain the captured heap pointer if any (no-op when x0 is not in the heap range)
                }
            }
            Arch::X86_64 => {
                emitter.instruction("mov rcx, QWORD PTR [rsp]");                // peek the saved Fiber pointer from the parking slot
                if consumes_two {
                    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                    let off_hi = start_args_offset + ((int_slot + 1) as i32) * 8;
                    emitter.instruction(&format!("mov QWORD PTR [rcx + {}], {}", off_lo, ptr_reg)); // start_args[int_slot] = string ptr
                    emitter.instruction(&format!("mov QWORD PTR [rcx + {}], {}", off_hi, len_reg)); // start_args[int_slot + 1] = string length
                    // x86_64 incref takes the pointer in rax; ptr_reg is already rax for the string case.
                    emitter.instruction("call __rt_incref");                    // retain the captured string so it survives until the Fiber is freed
                } else {
                    emitter.instruction(&format!("mov QWORD PTR [rcx + {}], rax", off_lo)); // start_args[int_slot] = single-register capture value
                    emitter.instruction("call __rt_incref");                    // retain the captured heap pointer if any (no-op when rax is not in the heap range)
                }
            }
        }
        int_slot += if consumes_two { 2 } else { 1 };
        // Silence dead-code warnings about user_param_count when there are no
        // remaining users below — kept around for future asserts.
        let _ = user_param_count;
    }
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));                   // restore the Fiber pointer as the expression result for the active target ABI

    // user_arg_max = user_param_count so the next $f->start() leaves the
    // pre-loaded captures alone. (We use user_param_count rather than `slot`
    // because user args always go into the first user_param_count slots, and
    // captures occupy the trailing range we just populated.)
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x9, #{}", user_param_count));     // load the closure's user-parameter count
            emitter.instruction(&format!("str x9, [x0, #{}]", umax_off));       // user_arg_max = user_param_count
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rcx, {}", user_param_count));     // load the closure's user-parameter count
            emitter.instruction(&format!("mov QWORD PTR [rax + {}], rcx", umax_off)); // user_arg_max = user_param_count
        }
    }
}
