//! Purpose:
//! Emits PHP `gettype` type conversion or type-name builtin calls.
//! Applies PHP scalar conversion rules or materializes runtime type names for values.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Conversion results must stay aligned with type-checker signatures and boxed Mixed handling.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a PHP type-name string into the string-result registers and records it in
/// the data section.
///
/// Adds `type_name` as a null-terminated string to the data section, then loads its
/// address into `ptr_reg` and its byte length into `len_reg` per the target ABI.
/// Always returns `Some(PhpType::Str)`.
fn emit_type_name_result(
    emitter: &mut Emitter,
    data: &mut DataSection,
    type_name: &[u8],
) -> Option<PhpType> {
    let (label, len) = data.add_string(type_name);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &label);                         // materialize the selected PHP type-name literal in the active string-pointer result register
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, #{}", len_reg, len));         // load the PHP type-name byte length into the active AArch64 string-length result register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, {}", len_reg, len));          // load the PHP type-name byte length into the active x86_64 string-length result register
        }
    }
    Some(PhpType::Str)
}

/// Emits code for the `gettype()` builtin, which returns the PHP type name of a
/// value as a string.
///
/// Handles three cases:
/// - `PhpType::Iterable`: probes the runtime heap kind tag to distinguish array,
///   object, and unknown heap representations and emits the corresponding PHP type name.
/// - `PhpType::Mixed` or `PhpType::Union`: unboxes the mixed payload and dispatches on
///   its runtime tag to emit one of: integer, double, string, boolean, NULL, array,
///   object, or resource.
/// - All other types: directly emits the known PHP type name string.
///
/// Returns `Some(PhpType::Str)` with the type-name bytes materialized in the ABI
/// string-result registers. The caller is responsible for releasing any temporary
/// Mixed box owned by the expression before entry.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("gettype()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if matches!(ty, PhpType::Iterable) {
        let (array_label, array_len) = data.add_string(b"array");
        let (object_label, object_len) = data.add_string(b"object");
        let (unknown_label, unknown_len) = data.add_string(b"unknown type");
        let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
        let array_case = ctx.next_label("builtin_gettype_iter_array");
        let object_case = ctx.next_label("builtin_gettype_iter_object");
        let unknown_case = ctx.next_label("builtin_gettype_iter_unknown");
        let done = ctx.next_label("builtin_gettype_iter_done");

        // -- iterable values are raw heap pointers; resolve their PHP type by reading
        //    the runtime heap kind and mapping array/hash kinds to "array" --
        abi::emit_call_label(emitter, "__rt_heap_kind");                        // probe the runtime heap kind tag for the iterable operand
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #2");                              // is the iterable backed by an indexed array?
                emitter.instruction(&format!("b.eq {}", array_case));           // indexed arrays report PHP type \"array\"
                emitter.instruction("cmp x0, #3");                              // is the iterable backed by a hash table?
                emitter.instruction(&format!("b.eq {}", array_case));           // hash tables also report PHP type \"array\"
                emitter.instruction("cmp x0, #4");                              // is the iterable backed by an object instance?
                emitter.instruction(&format!("b.eq {}", object_case));          // object instances report PHP type \"object\"
                emitter.instruction(&format!("b {}", unknown_case));            // any other heap kind falls back to \"unknown type\"
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 2");                              // is the iterable backed by an indexed array?
                emitter.instruction(&format!("je {}", array_case));             // indexed arrays report PHP type \"array\"
                emitter.instruction("cmp rax, 3");                              // is the iterable backed by a hash table?
                emitter.instruction(&format!("je {}", array_case));             // hash tables also report PHP type \"array\"
                emitter.instruction("cmp rax, 4");                              // is the iterable backed by an object instance?
                emitter.instruction(&format!("je {}", object_case));            // object instances report PHP type \"object\"
                emitter.instruction(&format!("jmp {}", unknown_case));          // any other heap kind falls back to \"unknown type\"
            }
        }

        emitter.label(&array_case);
        abi::emit_symbol_address(emitter, ptr_reg, &array_label);               // materialize the array type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, array_len)); //load the array type-name byte length into the active AArch64 string-length result register
                emitter.instruction(&format!("b {}", done));                    // finish after selecting the array type string on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, array_len)); //load the array type-name byte length into the active x86_64 string-length result register
                emitter.instruction(&format!("jmp {}", done));                  // finish after selecting the array type string on x86_64
            }
        }

        emitter.label(&object_case);
        abi::emit_symbol_address(emitter, ptr_reg, &object_label);              // materialize the object type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, object_len)); //load the object type-name byte length into the active AArch64 string-length result register
                emitter.instruction(&format!("b {}", done));                    // finish after selecting the object type string on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, object_len)); //load the object type-name byte length into the active x86_64 string-length result register
                emitter.instruction(&format!("jmp {}", done));                  // finish after selecting the object type string on x86_64
            }
        }

        emitter.label(&unknown_case);
        abi::emit_symbol_address(emitter, ptr_reg, &unknown_label);             // materialize the unknown type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, unknown_len)); //load the unknown type-name byte length into the active AArch64 string-length result register
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, unknown_len)); //load the unknown type-name byte length into the active x86_64 string-length result register
            }
        }
        emitter.label(&done);
        return Some(PhpType::Str);
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        let (integer_label, integer_len) = data.add_string(b"integer");
        let (double_label, double_len) = data.add_string(b"double");
        let (string_label, string_len) = data.add_string(b"string");
        let (boolean_label, boolean_len) = data.add_string(b"boolean");
        let (null_label, null_len) = data.add_string(b"NULL");
        let (array_label, array_len) = data.add_string(b"array");
        let (object_label, object_len) = data.add_string(b"object");
        let (resource_label, resource_len) = data.add_string(b"resource");
        let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
        let integer_case = ctx.next_label("builtin_gettype_mixed_integer");
        let double_case = ctx.next_label("builtin_gettype_mixed_double");
        let string_case = ctx.next_label("builtin_gettype_mixed_string");
        let boolean_case = ctx.next_label("builtin_gettype_mixed_boolean");
        let null_case = ctx.next_label("builtin_gettype_mixed_null");
        let array_case = ctx.next_label("builtin_gettype_mixed_array");
        let object_case = ctx.next_label("builtin_gettype_mixed_object");
        let resource_case = ctx.next_label("builtin_gettype_mixed_resource");
        let done = ctx.next_label("builtin_gettype_mixed_done");

        // -- mixed gettype() unwraps the payload and dispatches on its concrete runtime tag --
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // resolve the boxed payload tag before selecting the PHP type string
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // check whether the unboxed mixed tag denotes an integer payload
                emitter.instruction(&format!("b.eq {}", integer_case));         // integers map to PHP's integer type name
                emitter.instruction("cmp x0, #1");                              // check whether the unboxed mixed tag denotes a string payload
                emitter.instruction(&format!("b.eq {}", string_case));          // strings map to PHP's string type name
                emitter.instruction("cmp x0, #2");                              // check whether the unboxed mixed tag denotes a float payload
                emitter.instruction(&format!("b.eq {}", double_case));          // floats map to PHP's double type name
                emitter.instruction("cmp x0, #3");                              // check whether the unboxed mixed tag denotes a boolean payload
                emitter.instruction(&format!("b.eq {}", boolean_case));         // booleans map to PHP's boolean type name
                emitter.instruction("cmp x0, #4");                              // check whether the unboxed mixed tag denotes an indexed-array payload
                emitter.instruction(&format!("b.eq {}", array_case));           // indexed arrays map to PHP's array type name
                emitter.instruction("cmp x0, #5");                              // check whether the unboxed mixed tag denotes an associative-array payload
                emitter.instruction(&format!("b.eq {}", array_case));           // associative arrays also map to PHP's array type name
                emitter.instruction("cmp x0, #6");                              // check whether the unboxed mixed tag denotes an object payload
                emitter.instruction(&format!("b.eq {}", object_case));          // objects map to PHP's object type name
                emitter.instruction("cmp x0, #9");                              // check whether the unboxed mixed tag denotes a resource payload
                emitter.instruction(&format!("b.eq {}", resource_case));        // resources map to PHP's resource type name
                emitter.instruction(&format!("b {}", null_case));               // null and unknown tags fall back to PHP's NULL type name
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 0");                              // check whether the unboxed mixed tag denotes an integer payload
                emitter.instruction(&format!("je {}", integer_case));           // integers map to PHP's integer type name
                emitter.instruction("cmp rax, 1");                              // check whether the unboxed mixed tag denotes a string payload
                emitter.instruction(&format!("je {}", string_case));            // strings map to PHP's string type name
                emitter.instruction("cmp rax, 2");                              // check whether the unboxed mixed tag denotes a float payload
                emitter.instruction(&format!("je {}", double_case));            // floats map to PHP's double type name
                emitter.instruction("cmp rax, 3");                              // check whether the unboxed mixed tag denotes a boolean payload
                emitter.instruction(&format!("je {}", boolean_case));           // booleans map to PHP's boolean type name
                emitter.instruction("cmp rax, 4");                              // check whether the unboxed mixed tag denotes an indexed-array payload
                emitter.instruction(&format!("je {}", array_case));             // indexed arrays map to PHP's array type name
                emitter.instruction("cmp rax, 5");                              // check whether the unboxed mixed tag denotes an associative-array payload
                emitter.instruction(&format!("je {}", array_case));             // associative arrays also map to PHP's array type name
                emitter.instruction("cmp rax, 6");                              // check whether the unboxed mixed tag denotes an object payload
                emitter.instruction(&format!("je {}", object_case));            // objects map to PHP's object type name
                emitter.instruction("cmp rax, 9");                              // check whether the unboxed mixed tag denotes a resource payload
                emitter.instruction(&format!("je {}", resource_case));          // resources map to PHP's resource type name
                emitter.instruction(&format!("jmp {}", null_case));             // null and unknown tags fall back to PHP's NULL type name
            }
        }

        emitter.label(&integer_case);
        abi::emit_symbol_address(emitter, ptr_reg, &integer_label);             // materialize the integer type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, integer_len)); //load the integer type-name byte length into the active AArch64 string-length result register
                emitter.instruction(&format!("b {}", done));                    // finish after selecting the integer type string on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, integer_len)); //load the integer type-name byte length into the active x86_64 string-length result register
                emitter.instruction(&format!("jmp {}", done));                  // finish after selecting the integer type string on x86_64
            }
        }

        emitter.label(&double_case);
        abi::emit_symbol_address(emitter, ptr_reg, &double_label);              // materialize the double type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, double_len)); //load the double type-name byte length into the active AArch64 string-length result register
                emitter.instruction(&format!("b {}", done));                    // finish after selecting the double type string on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, double_len)); //load the double type-name byte length into the active x86_64 string-length result register
                emitter.instruction(&format!("jmp {}", done));                  // finish after selecting the double type string on x86_64
            }
        }

        emitter.label(&string_case);
        abi::emit_symbol_address(emitter, ptr_reg, &string_label);              // materialize the string type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, string_len)); //load the string type-name byte length into the active AArch64 string-length result register
                emitter.instruction(&format!("b {}", done));                    // finish after selecting the string type string on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, string_len)); //load the string type-name byte length into the active x86_64 string-length result register
                emitter.instruction(&format!("jmp {}", done));                  // finish after selecting the string type string on x86_64
            }
        }

        emitter.label(&boolean_case);
        abi::emit_symbol_address(emitter, ptr_reg, &boolean_label);             // materialize the boolean type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, boolean_len)); //load the boolean type-name byte length into the active AArch64 string-length result register
                emitter.instruction(&format!("b {}", done));                    // finish after selecting the boolean type string on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, boolean_len)); //load the boolean type-name byte length into the active x86_64 string-length result register
                emitter.instruction(&format!("jmp {}", done));                  // finish after selecting the boolean type string on x86_64
            }
        }

        emitter.label(&null_case);
        abi::emit_symbol_address(emitter, ptr_reg, &null_label);                // materialize the NULL type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, null_len)); //load the NULL type-name byte length into the active AArch64 string-length result register
                emitter.instruction(&format!("b {}", done));                    // finish after selecting the NULL type string on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, null_len)); // load the NULL type-name byte length into the active x86_64 string-length result register
                emitter.instruction(&format!("jmp {}", done));                  // finish after selecting the NULL type string on x86_64
            }
        }

        emitter.label(&array_case);
        abi::emit_symbol_address(emitter, ptr_reg, &array_label);               // materialize the array type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, array_len)); //load the array type-name byte length into the active AArch64 string-length result register
                emitter.instruction(&format!("b {}", done));                    // finish after selecting the array type string on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, array_len)); //load the array type-name byte length into the active x86_64 string-length result register
                emitter.instruction(&format!("jmp {}", done));                  // finish after selecting the array type string on x86_64
            }
        }

        emitter.label(&object_case);
        abi::emit_symbol_address(emitter, ptr_reg, &object_label);              // materialize the object type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, object_len)); //load the object type-name byte length into the active AArch64 string-length result register
                emitter.instruction(&format!("b {}", done));                    // finish after selecting the object type string on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, object_len)); //load the object type-name byte length into the active x86_64 string-length result register
                emitter.instruction(&format!("jmp {}", done));                  // finish after selecting the object type string on x86_64
            }
        }

        emitter.label(&resource_case);
        abi::emit_symbol_address(emitter, ptr_reg, &resource_label);            // materialize the resource type-name literal in the active string-pointer result register
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov {}, #{}", len_reg, resource_len)); //load the resource type-name byte length into the active AArch64 string-length result register
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", len_reg, resource_len)); //load the resource type-name byte length into the active x86_64 string-length result register
            }
        }
        emitter.label(&done);
        return Some(PhpType::Str);
    }

    if matches!(&ty, PhpType::TaggedScalar) {
        let null_case = ctx.next_label("gettype_tagged_null");
        let done = ctx.next_label("gettype_tagged_done");
        crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(emitter, &null_case);
        emit_type_name_result(emitter, data, b"integer");
        abi::emit_jump(emitter, &done);                                         // skip the NULL literal after selecting the integer type name
        emitter.label(&null_case);
        emit_type_name_result(emitter, data, b"NULL");
        emitter.label(&done);
        return Some(PhpType::Str);
    }

    let type_str = match &ty {
        PhpType::Int => b"integer".as_slice(),
        PhpType::Float => b"double".as_slice(),
        PhpType::Str => b"string".as_slice(),
        PhpType::Bool => b"boolean".as_slice(),
        PhpType::Void | PhpType::Never => b"NULL".as_slice(),
        PhpType::Array(_) | PhpType::AssocArray { .. } => b"array".as_slice(),
        PhpType::Callable => b"callable".as_slice(),
        PhpType::Object(_) => b"object".as_slice(),
        PhpType::Pointer(_) => b"pointer".as_slice(),
        PhpType::Buffer(_) => b"buffer".as_slice(),
        PhpType::Packed(_) => b"packed".as_slice(),
        PhpType::Resource(_) => b"resource".as_slice(),
        PhpType::Iterable => unreachable!("iterable handled above via runtime heap-kind dispatch"),
        PhpType::Mixed | PhpType::Union(_) => unreachable!("mixed handled above"),
        PhpType::TaggedScalar => unreachable!("tagged scalar handled above via runtime tag dispatch"),
    };
    emit_type_name_result(emitter, data, type_str)
}
