//! Purpose:
//! Emits PHP `var_dump` diagnostic output for scalar, array, and mixed values.
//! Owns recursive/runtime-aware formatting needed for PHP-visible stdout text.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Output is a side effect, and refcounted values must be inspected without consuming ownership.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a `write(fd=1, buf=literal, len=sizeof(literal))` syscall to stdout.
///
/// Writes a compile-time-known byte string directly to stdout without any
/// runtime buffering or length computation. The string is stored in the data
/// section and referenced by address.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `data` - Data section where the literal string is placed
/// * `bytes` - The literal byte content to write
fn emit_write_literal(emitter: &mut Emitter, data: &mut DataSection, bytes: &[u8]) {
    let (lbl, len) = data.add_string(bytes);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x1", &lbl);                                         // load the page that contains the literal string bytes
            emitter.add_lo12("x1", "x1", &lbl);                               // resolve the literal string address within that page
            emitter.instruction(&format!("mov x2, #{}", len));                  // pass the literal string length to write()
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea rsi, [rip + {}]", lbl));          // point the Linux write() buffer register at the literal string bytes
            emitter.instruction(&format!("mov edx, {}", len));                  // pass the literal string length to write()
            emitter.instruction("mov edi, 1");                                  // fd = stdout
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // write the literal bytes directly to stdout
        }
    }
}

/// Emits a branch instruction when the integer payload is non-zero.
///
/// Used to test whether a value is truthy or non-null without consuming
/// ownership. Dispatches to `b.ne` on ARM64 or `jne` on x86_64.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `label` - The target label for the branch when the condition is true
fn emit_branch_if_nonzero(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b.ne {}", label));                    // branch when the compared integer payload is non-zero
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jne {}", label));                     // branch when the compared integer payload is non-zero
        }
    }
}

/// Emits a branch instruction when two compared values are equal.
///
/// Dispatches to `b.eq` on ARM64 or `je` on x86_64.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `label` - The target label for the branch when the condition is true
fn emit_branch_if_eq(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b.eq {}", label));                    // branch when the compared values are equal
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("je {}", label));                      // branch when the compared values are equal
        }
    }
}

/// Emits a branch instruction when two compared values are different.
///
/// Dispatches to `b.ne` on ARM64 or `jne` on x86_64.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `label` - The target label for the branch when the condition is true
fn emit_branch_if_ne(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b.ne {}", label));                    // branch when the compared values are different
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jne {}", label));                     // branch when the compared values are different
        }
    }
}

/// Writes the current string result register to stdout.
///
/// Uses the target ABI to emit the string pointer and length from the
/// string result registers through `__rt_write`.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
fn emit_write_current_string(emitter: &mut Emitter) {
    abi::emit_write_stdout(emitter, &PhpType::Str);                            // write the current string result through the active target ABI
}

/// Emits var_dump output for an integer payload.
///
/// Checks the integer against the shared null sentinel (0x7fff_ffff_ffff_fffe).
/// If the payload is null, prints `NULL\n`. Otherwise prints `int(N)\n`
/// where N is the decimal conversion via `__rt_itoa`.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `ctx` - Codegen context (used for label allocation)
/// * `data` - Data section for literal strings
fn emit_var_dump_int(emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    let not_null = ctx.next_label("vd_not_null");
    let done = ctx.next_label("vd_done");
    let result_reg = abi::int_result_reg(emitter);
    let scratch_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch_reg, 0x7fff_ffff_ffff_fffe_u64 as i64); // materialize the shared null sentinel used by int-valued locals
    emitter.instruction(&format!("cmp {}, {}", result_reg, scratch_reg));       // compare the incoming integer payload against the null sentinel
    emit_branch_if_ne(emitter, &not_null);                                      // branch to the ordinary int path when the payload is not null
    emit_write_literal(emitter, data, b"NULL\n");
    abi::emit_jump(emitter, &done);                                             // skip the int formatter after printing NULL
    emitter.label(&not_null);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the integer payload before prefix writes clobber the integer result register
    emit_write_literal(emitter, data, b"int(");
    abi::emit_pop_reg(emitter, result_reg);                                     // restore the integer payload after the prefix write
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the integer payload to decimal text through the target-aware runtime helper
    emit_write_current_string(emitter);                                         // write the converted decimal text to stdout
    emit_write_literal(emitter, data, b")\n");
    emitter.label(&done);
}

/// Emits var_dump output for a float payload.
///
/// Prints `float(N)\n` where N is the decimal conversion of the float
/// in the floating-point result register via `__rt_ftoa`.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `data` - Data section for literal strings
fn emit_var_dump_float(emitter: &mut Emitter, data: &mut DataSection) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_call_label(emitter, "__rt_ftoa");                                 // convert the float payload to decimal text through the target-aware runtime helper
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the converted float string across literal writes
    emit_write_literal(emitter, data, b"float(");
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);                          // restore the converted float string after the prefix write
    emit_write_current_string(emitter);                                         // write the converted float text to stdout
    emit_write_literal(emitter, data, b")\n");
}

/// Emits var_dump output for a string payload.
///
/// Prints `string(LEN) "VALUE"\n` where LEN is the decimal string length
/// via `__rt_itoa` and VALUE is the raw string content in quotes.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `data` - Data section for literal strings
fn emit_var_dump_string(emitter: &mut Emitter, data: &mut DataSection) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the original string payload while printing the type prefix and quoted payload
    emit_write_literal(emitter, data, b"string(");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp, #8]");                            // load the preserved string length without consuming the saved payload pair
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                // load the preserved string length without consuming the saved payload pair
        }
    }
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the string length to decimal text through the target-aware runtime helper
    emit_write_current_string(emitter);                                         // write the decimal string length to stdout
    emit_write_literal(emitter, data, b") \"");
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);                          // restore the original string payload after the prefix writes finish
    emit_write_current_string(emitter);                                         // write the original quoted string payload to stdout
    emit_write_literal(emitter, data, b"\"\n");
}

/// Emits var_dump output for a boolean payload.
///
/// Prints `bool(false)\n` or `bool(true)\n` based on the integer result register.
/// The payload is expected in the standard integer result register.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `ctx` - Codegen context (used for label allocation)
/// * `data` - Data section for literal strings
fn emit_var_dump_bool(emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    let true_label = ctx.next_label("vd_true");
    let done = ctx.next_label("vd_done");
    let result_reg = abi::int_result_reg(emitter);
    emitter.instruction(&format!("cmp {}, 0", result_reg));                     // test whether the boolean payload is false or true
    emit_branch_if_nonzero(emitter, &true_label);                               // branch when the boolean payload is true
    emit_write_literal(emitter, data, b"bool(false)\n");
    abi::emit_jump(emitter, &done);                                             // skip the true branch after printing false
    emitter.label(&true_label);
    emit_write_literal(emitter, data, b"bool(true)\n");
    emitter.label(&done);
}

/// Emits var_dump output for a resource payload.
///
/// Prints `resource(N) of type (stream)\n` where N is the 1-based display id
/// (native payload + 1) via `__rt_itoa`. The native payload is preserved and
/// incremented before conversion.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `data` - Data section for literal strings
fn emit_var_dump_resource(emitter: &mut Emitter, data: &mut DataSection) {
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the native resource payload before prefix writes clobber the result register
    emit_write_literal(emitter, data, b"resource(");
    abi::emit_pop_reg(emitter, result_reg);                                     // restore the native resource payload for display-id formatting
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("add x0, x0, #1");                              // convert the native resource payload into the 1-based display id
        }
        Arch::X86_64 => {
            emitter.instruction("add rax, 1");                                  // convert the native resource payload into the 1-based display id
        }
    }
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the resource display id to decimal text
    emit_write_current_string(emitter);                                         // write the converted resource id to stdout
    emit_write_literal(emitter, data, b") of type (stream)\n");
}

/// Emits var_dump output for a null/void/never payload.
///
/// Prints `NULL\n`. Used as a fallback for types with no specific formatter.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `data` - Data section for literal strings
fn emit_var_dump_null(emitter: &mut Emitter, data: &mut DataSection) {
    emit_write_literal(emitter, data, b"NULL\n");
}

/// Emits var_dump output for an array payload.
///
/// Prints `array(N) {\n}\n` where N is the element count loaded from
/// the array/hash header via `__rt_itoa`. Does not recursively dump elements.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `data` - Data section for literal strings
fn emit_var_dump_array(emitter: &mut Emitter, data: &mut DataSection) {
    emit_var_dump_array_with_elem(emitter, data, &PhpType::Mixed);
}

/// Emit the var_dump body for an array/hash. The element type drives which
/// runtime walker is invoked: int arrays get \`__rt_var_dump_array_int\`,
/// string arrays \`__rt_var_dump_array_str\`. For other element shapes
/// (Hash, Mixed values) v1 prints just the header — the contents fall back
/// to the empty-body output. v2 will add a Mixed-aware walker that
/// dispatches per element tag.
fn emit_var_dump_array_with_elem(
    emitter: &mut Emitter,
    data: &mut DataSection,
    elem_ty: &PhpType,
) {
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result_reg);                                    // preserve the array pointer across the header write
    emit_write_literal(emitter, data, b"array(");
    abi::emit_pop_reg(emitter, result_reg);                                     // restore the array pointer after the prefix write
    abi::emit_push_reg(emitter, result_reg);                                    // preserve it again for the per-element walker below
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [x0]");                                // load the container element count from the array header
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rax]");                    // load the container element count from the array header
        }
    }
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the count to decimal text
    emit_write_current_string(emitter);                                         // write the count
    emit_write_literal(emitter, data, b") {\n");
    abi::emit_pop_reg(emitter, result_reg);                                     // restore the array pointer for the per-element walker
    // Dispatch to a specialised walker when the element type is known to
    // be homogeneous and one of the v1-supported scalar shapes.
    let walker = match elem_ty {
        PhpType::Int => Some("__rt_var_dump_array_int"),
        PhpType::Str => Some("__rt_var_dump_array_str"),
        PhpType::Bool => Some("__rt_var_dump_array_bool"),
        PhpType::Float => Some("__rt_var_dump_array_float"),
        // Mixed-element arrays need a per-element tag dispatch at runtime,
        // but the static type `Array(Mixed)` reaches here both for arrays
        // that were actually boxed as Mixed cells AND for arrays whose
        // concrete element type was simply erased at the call site. The
        // distinction is only visible through the array's value_type
        // stamp at runtime, and conflating the two paths in the static
        // dispatcher would corrupt the latter. Falling back to the
        // header-only fallback keeps both cases printable, at the cost
        // of empty bodies for genuine mixed-cell literals.
        _ => None,
    };
    if let Some(label) = walker {
        if matches!(emitter.target.arch, Arch::X86_64) {
            emitter.instruction("mov rdi, rax");                                // move the array pointer into the SysV first-arg register
        }
        abi::emit_call_label(emitter, label);                                   // walk the elements and emit per-element var_dump output
    }
    emit_write_literal(emitter, data, b"}\n");
}

/// Emits var_dump output for a callable payload.
///
/// Prints `callable\n`. Used when a value's type is exactly `Callable`
/// (not a closure or invokable object).
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `data` - Data section for literal strings
fn emit_var_dump_callable(emitter: &mut Emitter, data: &mut DataSection) {
    emit_write_literal(emitter, data, b"callable\n");
}

/// Emits var_dump output for a statically-known object class name.
///
/// Prints `object(ClassName)\n` where ClassName is the known class.
/// Used for types that carry a resolved class name at codegen time.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `data` - Data section for literal strings
/// * `class_name` - The resolved class name to display
fn emit_var_dump_object_name(emitter: &mut Emitter, data: &mut DataSection, class_name: &str) {
    let obj_str = format!("object({})\n", class_name);
    emit_write_literal(emitter, data, obj_str.as_bytes());
}

/// Emits var_dump output for an object with runtime-determined class.
///
/// Probes the heap kind via `__rt_heap_kind`, then performs a switch on
/// the runtime class id (loaded from the object header) to dispatch to
/// the matching `object(ClassName)` formatter. Falls back to `object\n`
/// for unknown class ids, and to `NULL\n` for null object pointers.
///
/// # Arguments
/// * `emitter` - Target-aware instruction emitter
/// * `ctx` - Codegen context (used for label allocation and class metadata)
/// * `data` - Data section for literal strings
fn emit_var_dump_dynamic_object(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let mut classes: Vec<_> = ctx
        .classes
        .iter()
        .map(|(class_name, class_info)| (class_name.clone(), class_info.class_id))
        .collect();
    classes.sort_by_key(|(_, class_id)| *class_id);
    let mut cases = Vec::with_capacity(classes.len());
    let null_label = ctx.next_label("vd_object_null");
    let fallback = ctx.next_label("vd_object_fallback");
    let done = ctx.next_label("vd_object_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {}", null_label));            // null object pointers print as NULL
            emitter.instruction("ldr x9, [x0]");                                // load the runtime class id from the object header
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // null object pointers print as NULL
            emitter.instruction(&format!("je {}", null_label));                 // branch to the null formatter for null object pointers
            emitter.instruction("mov r11, QWORD PTR [rax]");                    // load the runtime class id from the object header
        }
    }
    for (class_name, class_id) in classes {
        let case = ctx.next_label("vd_object_case");
        cases.push((case.clone(), class_name.clone()));
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp x9, #{}", class_id));         // compare the runtime class id against a known class id
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp r11, {}", class_id));         // compare the runtime class id against a known class id
            }
        }
        emit_branch_if_eq(emitter, &case);                                      // branch when the class id matches this known class
    }
    abi::emit_jump(emitter, &fallback);                                         // unknown runtime class ids fall back to a generic object marker
    for (case, class_name) in cases {
        emitter.label(&case);
        emit_var_dump_object_name(emitter, data, &class_name);
        abi::emit_jump(emitter, &done);                                         // finish after printing the matching object class
    }
    emitter.label(&null_label);
    emit_var_dump_null(emitter, data);
    abi::emit_jump(emitter, &done);                                             // finish after printing NULL for a null object pointer
    emitter.label(&fallback);
    emit_write_literal(emitter, data, b"object\n");
    emitter.label(&done);
}

/// Emits PHP `var_dump` output for the first argument expression.
///
/// Dispatches to a type-specific formatter based on the resolved type of
/// `args[0]`. Handles all PHP types: int, float, string, bool, resource,
/// null, array, object, callable, pointer, buffer, packed, and mixed/union
/// (which unboxes via `__rt_mixed_unbox` and re-dispatches).
///
/// Does not consume ownership of the argument; values are inspected in place.
/// Returns `PhpType::Void` to indicate the call has side effects and yields no
/// value.
///
/// # Arguments
/// * `_name` - The builtin name (unused; dispatch is by resolved type)
/// * `args` - Call arguments; only `args[0]` is formatted
/// * `emitter` - Target-aware instruction emitter
/// * `ctx` - Codegen context (label allocation, class metadata)
/// * `data` - Data section for literal strings
///
/// # Returns
/// `Some(PhpType::Void)` because var_dump always produces output and returns null
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("var_dump()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match &ty {
        PhpType::Int => emit_var_dump_int(emitter, ctx, data),
        PhpType::Float => emit_var_dump_float(emitter, data),
        PhpType::Str => emit_var_dump_string(emitter, data),
        PhpType::Bool => emit_var_dump_bool(emitter, ctx, data),
        PhpType::Resource(_) => emit_var_dump_resource(emitter, data),
        PhpType::Void | PhpType::Never => emit_var_dump_null(emitter, data),
        PhpType::Iterable => {
            // Iterable values are raw heap pointers. Probe the heap kind and reuse
            // the array/object var_dump helpers directly, instead of routing through
            // __rt_mixed_unbox which expects a Mixed cell layout.
            let array_case = ctx.next_label("vd_iter_array");
            let object_case = ctx.next_label("vd_iter_object");
            let null_case = ctx.next_label("vd_iter_null");
            let done = ctx.next_label("vd_iter_done");

            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve iterable pointer across heap-kind probe
            abi::emit_call_label(emitter, "__rt_heap_kind");                    // x0/rax = heap kind tag for the iterable payload
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #2");                          // iterable backed by indexed array?
                    emit_branch_if_eq(emitter, &array_case);                    // dispatch the array var_dump path
                    emitter.instruction("cmp x0, #3");                          // iterable backed by hash table?
                    emit_branch_if_eq(emitter, &array_case);                    // hash tables also use the array var_dump path
                    emitter.instruction("cmp x0, #4");                          // iterable backed by an object?
                    emit_branch_if_eq(emitter, &object_case);                   // dispatch the object var_dump path
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 2");                          // iterable backed by indexed array?
                    emit_branch_if_eq(emitter, &array_case);                    // dispatch the array var_dump path
                    emitter.instruction("cmp rax, 3");                          // iterable backed by hash table?
                    emit_branch_if_eq(emitter, &array_case);                    // hash tables also use the array var_dump path
                    emitter.instruction("cmp rax, 4");                          // iterable backed by an object?
                    emit_branch_if_eq(emitter, &object_case);                   // dispatch the object var_dump path
                }
            }
            abi::emit_jump(emitter, &null_case);                                // null pointers and unknown kinds print as NULL

            emitter.label(&array_case);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // restore the iterable container pointer for the array var_dump prologue
            emit_var_dump_array(emitter, data);
            abi::emit_jump(emitter, &done);                                     // finish after printing the array shell

            emitter.label(&object_case);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // restore the iterable object pointer for the object var_dump prologue
            emit_var_dump_dynamic_object(emitter, ctx, data);
            abi::emit_jump(emitter, &done);                                     // finish after printing the object marker

            emitter.label(&null_case);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));           // discard the saved iterable pointer on the null/fallback path
            emit_var_dump_null(emitter, data);                                  // print NULL for null/unknown iterable payloads

            emitter.label(&done);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let int_case = ctx.next_label("vd_mixed_int");
            let string_case = ctx.next_label("vd_mixed_string");
            let float_case = ctx.next_label("vd_mixed_float");
            let bool_case = ctx.next_label("vd_mixed_bool");
            let resource_case = ctx.next_label("vd_mixed_resource");
            let array_case = ctx.next_label("vd_mixed_array");
            let object_case = ctx.next_label("vd_mixed_object");
            let null_case = ctx.next_label("vd_mixed_null");
            let done = ctx.next_label("vd_mixed_done");

            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // unwrap the boxed mixed payload before formatting it
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #0");                          // does the mixed payload hold an int?
                    emit_branch_if_eq(emitter, &int_case);                      // ints reuse the ordinary int var_dump formatter
                    emitter.instruction("cmp x0, #1");                          // does the mixed payload hold a string?
                    emit_branch_if_eq(emitter, &string_case);                   // strings reuse the ordinary string var_dump formatter
                    emitter.instruction("cmp x0, #2");                          // does the mixed payload hold a float?
                    emit_branch_if_eq(emitter, &float_case);                    // floats reuse the ordinary float var_dump formatter
                    emitter.instruction("cmp x0, #3");                          // does the mixed payload hold a bool?
                    emit_branch_if_eq(emitter, &bool_case);                     // bools reuse the ordinary bool var_dump formatter
                    emitter.instruction("cmp x0, #9");                          // does the mixed payload hold a resource?
                    emit_branch_if_eq(emitter, &resource_case);                 // resources reuse the ordinary resource var_dump formatter
                    emitter.instruction("cmp x0, #4");                          // does the mixed payload hold an indexed array?
                    emit_branch_if_eq(emitter, &array_case);                    // arrays reuse the ordinary array var_dump formatter
                    emitter.instruction("cmp x0, #5");                          // does the mixed payload hold an associative array?
                    emit_branch_if_eq(emitter, &array_case);                    // associative arrays reuse the ordinary array var_dump formatter
                    emitter.instruction("cmp x0, #6");                          // does the mixed payload hold an object/callable heap value?
                    emit_branch_if_eq(emitter, &object_case);                   // objects use runtime class-id dispatch for their name
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 0");                          // does the mixed payload hold an int?
                    emit_branch_if_eq(emitter, &int_case);                      // ints reuse the ordinary int var_dump formatter
                    emitter.instruction("cmp rax, 1");                          // does the mixed payload hold a string?
                    emit_branch_if_eq(emitter, &string_case);                   // strings reuse the ordinary string var_dump formatter
                    emitter.instruction("cmp rax, 2");                          // does the mixed payload hold a float?
                    emit_branch_if_eq(emitter, &float_case);                    // floats reuse the ordinary float var_dump formatter
                    emitter.instruction("cmp rax, 3");                          // does the mixed payload hold a bool?
                    emit_branch_if_eq(emitter, &bool_case);                     // bools reuse the ordinary bool var_dump formatter
                    emitter.instruction("cmp rax, 9");                          // does the mixed payload hold a resource?
                    emit_branch_if_eq(emitter, &resource_case);                 // resources reuse the ordinary resource var_dump formatter
                    emitter.instruction("cmp rax, 4");                          // does the mixed payload hold an indexed array?
                    emit_branch_if_eq(emitter, &array_case);                    // arrays reuse the ordinary array var_dump formatter
                    emitter.instruction("cmp rax, 5");                          // does the mixed payload hold an associative array?
                    emit_branch_if_eq(emitter, &array_case);                    // associative arrays reuse the ordinary array var_dump formatter
                    emitter.instruction("cmp rax, 6");                          // does the mixed payload hold an object/callable heap value?
                    emit_branch_if_eq(emitter, &object_case);                   // objects use runtime class-id dispatch for their name
                }
            }
            abi::emit_jump(emitter, &null_case);                                // null and unknown tags print as NULL

            emitter.label(&int_case);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, x1");                          // move the unboxed int payload into the standard integer result register
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rax, rdi");                        // move the unboxed int payload into the standard integer result register
                }
            }
            emit_var_dump_int(emitter, ctx, data);
            abi::emit_jump(emitter, &done);                                     // finish after printing the mixed int payload

            emitter.label(&string_case);
            match emitter.target.arch {
                Arch::AArch64 => {}
                Arch::X86_64 => {
                    emitter.instruction("mov rax, rdi");                        // move the unboxed string pointer into the standard string result register
                }
            }
            emit_var_dump_string(emitter, data);                                // reuse the ordinary string var_dump formatter for mixed strings
            abi::emit_jump(emitter, &done);                                     // finish after printing the mixed string payload

            emitter.label(&float_case);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("fmov d0, x1");                         // move the unboxed float bits into the floating-point result register
                }
                Arch::X86_64 => {
                    emitter.instruction("movq xmm0, rdi");                      // move the unboxed float bits into the floating-point result register
                }
            }
            emit_var_dump_float(emitter, data);
            abi::emit_jump(emitter, &done);                                     // finish after printing the mixed float payload

            emitter.label(&bool_case);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, x1");                          // move the unboxed bool payload into the standard integer result register
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rax, rdi");                        // move the unboxed bool payload into the standard integer result register
                }
            }
            emit_var_dump_bool(emitter, ctx, data);
            abi::emit_jump(emitter, &done);                                     // finish after printing the mixed bool payload

            emitter.label(&resource_case);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, x1");                          // move the unboxed resource payload into the standard integer result register
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rax, rdi");                        // move the unboxed resource payload into the standard integer result register
                }
            }
            emit_var_dump_resource(emitter, data);
            abi::emit_jump(emitter, &done);                                     // finish after printing the mixed resource payload

            emitter.label(&array_case);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, x1");                          // move the unboxed container pointer into the standard integer result register
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rax, rdi");                        // move the unboxed container pointer into the standard integer result register
                }
            }
            emit_var_dump_array(emitter, data);
            abi::emit_jump(emitter, &done);                                     // finish after printing the mixed array payload

            emitter.label(&object_case);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, x1");                          // move the unboxed object pointer into the standard integer result register
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rax, rdi");                        // move the unboxed object pointer into the standard integer result register
                }
            }
            emit_var_dump_dynamic_object(emitter, ctx, data);
            abi::emit_jump(emitter, &done);                                     // finish after printing the mixed object payload

            emitter.label(&null_case);
            emit_var_dump_null(emitter, data);                                  // print NULL for null/unknown mixed payloads
            emitter.label(&done);
        }
        PhpType::Array(elem_ty) => {
            emit_var_dump_array_with_elem(emitter, data, elem_ty);
        }
        PhpType::AssocArray { .. } => {
            // Assoc-array layout differs (hash table, not contiguous
            // 8-byte slots) — the v1 indexed-element walkers do not
            // apply. Print just the `array(N) {\n}\n` shell.
            emit_var_dump_array(emitter, data);
        }
        PhpType::Callable => emit_var_dump_callable(emitter, data),
        PhpType::Object(class_name) => emit_var_dump_object_name(emitter, data, class_name),
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            // -- print pointer as hex address followed by newline --
            abi::emit_call_label(emitter, "__rt_ptoa");                         // convert the pointer payload into the active target string result registers
            emit_write_current_string(emitter);                                 // write the converted pointer text to stdout
            emit_write_literal(emitter, data, b"\n");                           // terminate the pointer dump with a trailing newline
        }
    }
    Some(PhpType::Void)
}
