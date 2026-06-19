//! Purpose:
//! Emits the `__rt_print_r_*` runtime walkers that render PHP `print_r`
//! output for indexed arrays and associative arrays (hashes), matching
//! PHP's recursive `Array\n(\n    [key] => value\n)\n` layout with
//! 4-space-per-level indentation and unquoted keys.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - The `print_r` builtin emitter (`codegen_ir::lower_inst::builtins::debug`)
//!   when the value's static type is an array/hash, or a boxed Mixed cell.
//!
//! Key details:
//! - Indexed arrays reuse the 24-byte header (count at offset 0) + typed slots
//!   layout. `__rt_print_r_indexed` self-dispatches on the array value_type
//!   stamp (`[arr-8]` byte 1: 0=int, 1=str, 2=float, 3=bool, 4=array, 5=hash,
//!   6=object, 7=mixed-cell), so it handles every element type and nested
//!   arrays without a per-type walker.
//! - Hashes iterate with `__rt_hash_count` / `__rt_hash_iter_next` (same
//!   primitives as the JSON and var_dump hash walkers).
//! - `__rt_print_r_value` renders one value; tags 4/5 recurse into the array
//!   walkers (mutual recursion gives arbitrary nesting depth), tag 7 unboxes a
//!   Mixed cell then redispatches. Its 4th argument is the *paren base indent*
//!   to use when the value is itself an array (entry_indent + 4 for nested
//!   entries, 0 for a top-level Mixed value).
//! - Scalars render PHP-style with no type wrapper: int/float as decimals,
//!   strings raw, bool true as `1` and bool false / null as the empty string.
//! - Nested objects (tag 6) are rendered as the bare `Array` header only; full
//!   `ClassName Object` dumps need class metadata the runtime walker lacks.
//! - The AArch64 path is shared by macOS and Linux ARM64 (`emitter.syscall(4)`
//!   maps to the platform write number); the `_linux_x86_64` paths are SysV.

use crate::codegen::abi;
use crate::codegen::{emit::Emitter, platform::Arch};

/// `__rt_print_r_spaces`: write `n` ASCII spaces to stdout in <=64-byte chunks.
/// Input: AArch64 x0 / x86_64 rdi = space count.
pub fn emit_print_r_spaces(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_spaces_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: print_r_spaces ---");
    emitter.label_global("__rt_print_r_spaces");

    emitter.instruction("sub sp, sp, #32");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // remaining space count

    emitter.label("__rt_pr_spaces_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the remaining count
    emitter.instruction("cmp x0, #0");                                          // any spaces left to write?
    emitter.instruction("b.le __rt_pr_spaces_done");                            // none → finish
    emitter.instruction("mov x9, #64");                                         // the pad buffer is 64 bytes wide
    emitter.instruction("cmp x0, x9");                                          // remaining vs the chunk cap
    emitter.instruction("csel x2, x0, x9, lt");                                 // chunk len = min(remaining, 64)
    emitter.instruction("sub x0, x0, x2");                                      // remaining -= chunk
    emitter.instruction("str x0, [sp, #0]");                                    // save the decremented count
    abi::emit_symbol_address(emitter, "x1", "_pr_spaces");                      // buffer = the 64-space pad
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the space chunk
    emitter.instruction("b __rt_pr_spaces_loop");                               // continue padding

    emitter.label("__rt_pr_spaces_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 helper that writes `n` spaces in <=64-byte chunks.
fn emit_print_r_spaces_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_spaces ---");
    emitter.label_global("__rt_print_r_spaces");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // remaining space count

    emitter.label("__rt_pr_spaces_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the remaining count
    emitter.instruction("cmp rax, 0");                                          // any spaces left to write?
    emitter.instruction("jle __rt_pr_spaces_done_x86");                         // none → finish
    emitter.instruction("mov rdx, 64");                                         // the pad buffer is 64 bytes wide
    emitter.instruction("cmp rax, 64");                                         // remaining vs the chunk cap
    emitter.instruction("cmovl rdx, rax");                                      // chunk len = min(remaining, 64)
    emitter.instruction("sub rax, rdx");                                        // remaining -= chunk
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the decremented count
    abi::emit_symbol_address(emitter, "rsi", "_pr_spaces");                     // buffer = the 64-space pad
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the space chunk
    emitter.instruction("jmp __rt_pr_spaces_loop_x86");                         // continue padding

    emitter.label("__rt_pr_spaces_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_print_r_open`: write `<base spaces>(\n`. Input: AArch64 x0 / x86_64 rdi = base indent.
pub fn emit_print_r_open(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_paren_linux_x86_64(emitter, "__rt_print_r_open", "_pr_open");
        return;
    }
    emit_print_r_paren_aarch64(emitter, "__rt_print_r_open", "_pr_open");
}

/// `__rt_print_r_close`: write `<base spaces>)\n`. Input: AArch64 x0 / x86_64 rdi = base indent.
pub fn emit_print_r_close(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_paren_linux_x86_64(emitter, "__rt_print_r_close", "_pr_close");
        return;
    }
    emit_print_r_paren_aarch64(emitter, "__rt_print_r_close", "_pr_close");
}

/// Emits an AArch64 paren-line helper: indent `base` spaces then write the 2-byte `paren_sym`.
fn emit_print_r_paren_aarch64(emitter: &mut Emitter, label: &str, paren_sym: &str) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", &label[5..]));
    emitter.label_global(label);

    emitter.instruction("sub sp, sp, #16");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("bl __rt_print_r_spaces");                              // x0 holds the base indent → pad it
    abi::emit_symbol_address(emitter, "x1", paren_sym);                         // load the `(\n` or `)\n` literal
    emitter.instruction("mov x2, #2");                                          // both paren literals are 2 bytes
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the paren line
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits a Linux x86_64 paren-line helper: indent `base` spaces then write the 2-byte `paren_sym`.
fn emit_print_r_paren_linux_x86_64(emitter: &mut Emitter, label: &str, paren_sym: &str) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", &label[5..]));
    emitter.label_global(label);

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("call __rt_print_r_spaces");                            // rdi holds the base indent → pad it
    abi::emit_symbol_address(emitter, "rsi", paren_sym);                        // load the `(\n` or `)\n` literal
    emitter.instruction("mov edx, 2");                                          // both paren literals are 2 bytes
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the paren line
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_print_r_int_key`: write `<indent spaces>[IDX] => ` for an integer key.
/// Input: AArch64 x0=idx x1=indent / x86_64 rdi=idx rsi=indent.
pub fn emit_print_r_int_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_int_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: print_r_int_key ---");
    emitter.label_global("__rt_print_r_int_key");

    emitter.instruction("sub sp, sp, #32");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the integer key

    emitter.instruction("mov x0, x1");                                          // indent → spaces helper argument
    emitter.instruction("bl __rt_print_r_spaces");                              // pad the entry indent
    abi::emit_symbol_address(emitter, "x1", "_pr_lbrack");                      // load the `[` delimiter
    emitter.instruction("mov x2, #1");                                          // len("[") = 1
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `[`
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the integer key
    emitter.instruction("bl __rt_itoa");                                        // x1=digits ptr, x2=digits len
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the key digits
    abi::emit_symbol_address(emitter, "x1", "_pr_arrow");                       // load the `] => ` separator
    emitter.instruction("mov x2, #5");                                          // len("] => ") = 5
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `] => `
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 helper that writes `<indent>[IDX] => ` for an integer key.
fn emit_print_r_int_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_int_key ---");
    emitter.label_global("__rt_print_r_int_key");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the integer key

    emitter.instruction("mov rdi, rsi");                                        // indent → spaces helper argument
    emitter.instruction("call __rt_print_r_spaces");                            // pad the entry indent
    abi::emit_symbol_address(emitter, "rsi", "_pr_lbrack");                     // load the `[` delimiter
    emitter.instruction("mov edx, 1");                                          // len("[") = 1
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `[`
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the integer key
    emitter.instruction("call __rt_itoa");                                      // rax=digits ptr, rdx=digits len
    emitter.instruction("mov rsi, rax");                                        // digits ptr → write buffer
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the key digits
    abi::emit_symbol_address(emitter, "rsi", "_pr_arrow");                      // load the `] => ` separator
    emitter.instruction("mov edx, 5");                                          // len("] => ") = 5
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `] => `
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_print_r_str_key`: write `<indent spaces>[KEY] => ` for an unquoted string key.
/// Input: AArch64 x0=ptr x1=len x2=indent / x86_64 rdi=ptr rsi=len rdx=indent.
pub fn emit_print_r_str_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_str_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: print_r_str_key ---");
    emitter.label_global("__rt_print_r_str_key");

    emitter.instruction("sub sp, sp, #32");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the key ptr/len

    emitter.instruction("mov x0, x2");                                          // indent → spaces helper argument
    emitter.instruction("bl __rt_print_r_spaces");                              // pad the entry indent
    abi::emit_symbol_address(emitter, "x1", "_pr_lbrack");                      // load the `[` delimiter
    emitter.instruction("mov x2, #1");                                          // len("[") = 1
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `[`
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the key ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the key len
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the raw (unquoted) key bytes
    abi::emit_symbol_address(emitter, "x1", "_pr_arrow");                       // load the `] => ` separator
    emitter.instruction("mov x2, #5");                                          // len("] => ") = 5
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `] => `
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 helper that writes `<indent>[KEY] => ` for an unquoted string key.
fn emit_print_r_str_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_str_key ---");
    emitter.label_global("__rt_print_r_str_key");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // allocate the helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the key ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the key len

    emitter.instruction("mov rdi, rdx");                                        // indent → spaces helper argument
    emitter.instruction("call __rt_print_r_spaces");                            // pad the entry indent
    abi::emit_symbol_address(emitter, "rsi", "_pr_lbrack");                     // load the `[` delimiter
    emitter.instruction("mov edx, 1");                                          // len("[") = 1
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `[`
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the key ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the key len
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the raw (unquoted) key bytes
    abi::emit_symbol_address(emitter, "rsi", "_pr_arrow");                      // load the `] => ` separator
    emitter.instruction("mov edx, 5");                                          // len("] => ") = 5
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `] => `
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_print_r_value`: render one PHP value (no type wrapper). Tags 4/5 recurse
/// into the array walkers, tag 7 unboxes a Mixed cell and redispatches, scalars
/// render directly, null/object render nothing/Array-header only.
/// Input: AArch64 x0=tag x1=lo x2=hi x3=nested_base / x86_64 rdi=tag rsi=lo rdx=hi rcx=nested_base.
pub fn emit_print_r_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: print_r_value ---");
    emitter.label_global("__rt_print_r_value");

    emitter.instruction("sub sp, sp, #48");                                     // allocate the value frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the value frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the value low word
    emitter.instruction("str x2, [sp, #8]");                                    // save the value high word
    emitter.instruction("str x3, [sp, #16]");                                   // save the nested paren base indent

    emitter.instruction("cmp x0, #7");                                          // boxed Mixed cell?
    emitter.instruction("b.eq __rt_pr_val_mixed");                              // unbox then redispatch
    emitter.instruction("cmp x0, #0");                                          // tag 0 = int
    emitter.instruction("b.eq __rt_pr_val_int");                                // render the integer
    emitter.instruction("cmp x0, #1");                                          // tag 1 = string
    emitter.instruction("b.eq __rt_pr_val_str");                                // render the string
    emitter.instruction("cmp x0, #2");                                          // tag 2 = float
    emitter.instruction("b.eq __rt_pr_val_flt");                                // render the float
    emitter.instruction("cmp x0, #3");                                          // tag 3 = bool
    emitter.instruction("b.eq __rt_pr_val_bool");                               // render the bool
    emitter.instruction("cmp x0, #4");                                          // tag 4 = indexed array
    emitter.instruction("b.eq __rt_pr_val_arr");                                // recurse into the indexed walker
    emitter.instruction("cmp x0, #5");                                          // tag 5 = hash
    emitter.instruction("b.eq __rt_pr_val_hash");                               // recurse into the hash walker
    emitter.instruction("b __rt_pr_val_done");                                  // tag 6 object / 8 null → render nothing

    emitter.label("__rt_pr_val_int");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the integer payload
    emitter.instruction("bl __rt_itoa");                                        // x1=digits ptr, x2=digits len
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the decimal digits
    emitter.instruction("b __rt_pr_val_done");                                  // value rendered

    emitter.label("__rt_pr_val_str");
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the string ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the string len
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the raw string bytes
    emitter.instruction("b __rt_pr_val_done");                                  // value rendered

    emitter.label("__rt_pr_val_flt");
    emitter.instruction("ldr d0, [sp, #0]");                                    // reload the float bit pattern
    emitter.instruction("bl __rt_ftoa");                                        // x1=text ptr, x2=text len
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write the float text
    emitter.instruction("b __rt_pr_val_done");                                  // value rendered

    emitter.label("__rt_pr_val_bool");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the bool payload
    emitter.instruction("cbz x9, __rt_pr_val_done");                            // false → render the empty string
    abi::emit_symbol_address(emitter, "x1", "_pr_one");                         // true → load the `1` literal
    emitter.instruction("mov x2, #1");                                          // len("1") = 1
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `1`
    emitter.instruction("b __rt_pr_val_done");                                  // value rendered

    emitter.label("__rt_pr_val_arr");
    abi::emit_symbol_address(emitter, "x1", "_pr_array_hdr");                   // load the `Array\n` header
    emitter.instruction("mov x2, #6");                                          // len("Array\n") = 6
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `Array\n`
    emitter.instruction("ldr x0, [sp, #0]");                                    // nested indexed-array pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // base = the nested paren indent
    emitter.instruction("bl __rt_print_r_indexed");                             // recurse into the indexed walker
    emitter.instruction("b __rt_pr_val_done");                                  // value rendered

    emitter.label("__rt_pr_val_hash");
    abi::emit_symbol_address(emitter, "x1", "_pr_array_hdr");                   // load the `Array\n` header
    emitter.instruction("mov x2, #6");                                          // len("Array\n") = 6
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // write `Array\n`
    emitter.instruction("ldr x0, [sp, #0]");                                    // nested hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // base = the nested paren indent
    emitter.instruction("bl __rt_print_r_hash");                                // recurse into the hash walker
    emitter.instruction("b __rt_pr_val_done");                                  // value rendered

    emitter.label("__rt_pr_val_mixed");
    emitter.instruction("ldr x0, [sp, #0]");                                    // boxed Mixed cell pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=inner tag, x1=lo, x2=hi
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the nested paren base indent
    emitter.instruction("bl __rt_print_r_value");                               // redispatch the unboxed scalar/array

    emitter.label("__rt_pr_val_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the value frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 single-value renderer for print_r.
fn emit_print_r_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_value ---");
    emitter.label_global("__rt_print_r_value");

    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the value frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate the value frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the value low word
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the value high word
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the nested paren base indent
    emitter.instruction("mov rax, rdi");                                        // tag → dispatch register

    emitter.instruction("cmp rax, 7");                                          // boxed Mixed cell?
    emitter.instruction("je __rt_pr_val_mixed_x86");                            // unbox then redispatch
    emitter.instruction("cmp rax, 0");                                          // tag 0 = int
    emitter.instruction("je __rt_pr_val_int_x86");                              // render the integer
    emitter.instruction("cmp rax, 1");                                          // tag 1 = string
    emitter.instruction("je __rt_pr_val_str_x86");                              // render the string
    emitter.instruction("cmp rax, 2");                                          // tag 2 = float
    emitter.instruction("je __rt_pr_val_flt_x86");                              // render the float
    emitter.instruction("cmp rax, 3");                                          // tag 3 = bool
    emitter.instruction("je __rt_pr_val_bool_x86");                             // render the bool
    emitter.instruction("cmp rax, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __rt_pr_val_arr_x86");                              // recurse into the indexed walker
    emitter.instruction("cmp rax, 5");                                          // tag 5 = hash
    emitter.instruction("je __rt_pr_val_hash_x86");                             // recurse into the hash walker
    emitter.instruction("jmp __rt_pr_val_done_x86");                            // tag 6 object / 8 null → render nothing

    emitter.label("__rt_pr_val_int_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the integer payload
    emitter.instruction("call __rt_itoa");                                      // rax=digits ptr, rdx=digits len
    emitter.instruction("mov rsi, rax");                                        // digits ptr → write buffer
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the decimal digits
    emitter.instruction("jmp __rt_pr_val_done_x86");                            // value rendered

    emitter.label("__rt_pr_val_str_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the string ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the string len
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the raw string bytes
    emitter.instruction("jmp __rt_pr_val_done_x86");                            // value rendered

    emitter.label("__rt_pr_val_flt_x86");
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload the float bit pattern
    emitter.instruction("call __rt_ftoa");                                      // rax=text ptr, rdx=text len
    emitter.instruction("mov rsi, rax");                                        // text ptr → write buffer
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write the float text
    emitter.instruction("jmp __rt_pr_val_done_x86");                            // value rendered

    emitter.label("__rt_pr_val_bool_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the bool payload
    emitter.instruction("test rax, rax");                                       // false?
    emitter.instruction("jz __rt_pr_val_done_x86");                             // false → render the empty string
    abi::emit_symbol_address(emitter, "rsi", "_pr_one");                        // true → load the `1` literal
    emitter.instruction("mov edx, 1");                                          // len("1") = 1
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `1`
    emitter.instruction("jmp __rt_pr_val_done_x86");                            // value rendered

    emitter.label("__rt_pr_val_arr_x86");
    abi::emit_symbol_address(emitter, "rsi", "_pr_array_hdr");                  // load the `Array\n` header
    emitter.instruction("mov edx, 6");                                          // len("Array\n") = 6
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `Array\n`
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // nested indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // base = the nested paren indent
    emitter.instruction("call __rt_print_r_indexed");                           // recurse into the indexed walker
    emitter.instruction("jmp __rt_pr_val_done_x86");                            // value rendered

    emitter.label("__rt_pr_val_hash_x86");
    abi::emit_symbol_address(emitter, "rsi", "_pr_array_hdr");                  // load the `Array\n` header
    emitter.instruction("mov edx, 6");                                          // len("Array\n") = 6
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // write `Array\n`
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // nested hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // base = the nested paren indent
    emitter.instruction("call __rt_print_r_hash");                              // recurse into the hash walker
    emitter.instruction("jmp __rt_pr_val_done_x86");                            // value rendered

    emitter.label("__rt_pr_val_mixed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // boxed Mixed cell pointer → RAX
    emitter.instruction("call __rt_mixed_unbox");                               // rax=inner tag, rdi=lo, rdx=hi
    emitter.instruction("mov rsi, rdi");                                        // unboxed lo → value low argument
    emitter.instruction("mov rdi, rax");                                        // unboxed tag → value tag argument
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the nested paren base indent
    emitter.instruction("call __rt_print_r_value");                             // redispatch the unboxed scalar/array

    emitter.label("__rt_pr_val_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the value frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_print_r_indexed`: render an indexed array body `<base>(\n ... <base>)\n`,
/// self-dispatching each element on the array value_type stamp. Input:
/// AArch64 x0=arr x1=base / x86_64 rdi=arr rsi=base.
pub fn emit_print_r_indexed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_indexed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: print_r_indexed ---");
    emitter.label_global("__rt_print_r_indexed");

    // Frame (64 bytes): [0]arr [8]base [16]entry_indent [24]count [32]index
    //   [40]stamp [48]x29 [56]x30.
    emitter.instruction("sub sp, sp, #64");                                     // allocate the indexed-walk frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the walk frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the paren base indent
    emitter.instruction("add x9, x1, #4");                                      // entry indent = base + 4
    emitter.instruction("str x9, [sp, #16]");                                   // save the entry indent
    emitter.instruction("ldr x10, [x0]");                                       // load the element count from the header
    emitter.instruction("str x10, [sp, #24]");                                  // save the element count
    emitter.instruction("str xzr, [sp, #32]");                                  // index = 0
    emitter.instruction("ldr x11, [x0, #-8]");                                  // load the packed array kind word
    emitter.instruction("lsr x11, x11, #8");                                    // shift the value_type stamp into the low byte
    emitter.instruction("and x11, x11, #0x0f");                                 // isolate the value_type field (low nibble), dropping the COW bit
    emitter.instruction("str x11, [sp, #40]");                                  // save the element value_type stamp

    emitter.instruction("ldr x0, [sp, #8]");                                    // base → open helper argument
    emitter.instruction("bl __rt_print_r_open");                                // write `<base>(\n`

    emitter.label("__rt_pr_idx_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the current index
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the element count
    emitter.instruction("cmp x9, x10");                                         // processed every element?
    emitter.instruction("b.ge __rt_pr_idx_done");                               // walk complete

    emitter.instruction("ldr x0, [sp, #32]");                                   // index → integer key
    emitter.instruction("ldr x1, [sp, #16]");                                   // entry indent → integer key
    emitter.instruction("bl __rt_print_r_int_key");                             // write `<indent>[N] => `

    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the element stamp
    emitter.instruction("ldr x13, [sp, #0]");                                   // reload the array pointer
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the current index
    emitter.instruction("cmp x12, #1");                                         // string elements use a 16-byte stride
    emitter.instruction("b.eq __rt_pr_idx_str");                                // handle string elements
    emitter.instruction("cmp x12, #7");                                         // mixed elements are boxed cells
    emitter.instruction("b.eq __rt_pr_idx_mixed");                              // handle mixed cells

    // 8-byte-stride elements: int(0) / float(2) / bool(3) / array(4) / hash(5) / object(6).
    emitter.instruction("add x15, x14, #3");                                    // skip the 24-byte (3-quad) header
    emitter.instruction("ldr x1, [x13, x15, lsl #3]");                          // load the raw element word → value low
    emitter.instruction("mov x0, x12");                                         // tag = the array stamp
    emitter.instruction("mov x2, #0");                                          // high word unused for 8-byte elements
    emitter.instruction("ldr x3, [sp, #16]");                                   // entry indent
    emitter.instruction("add x3, x3, #4");                                      // nested base = entry indent + 4
    emitter.instruction("bl __rt_print_r_value");                               // render the element
    emitter.instruction("b __rt_pr_idx_after");                                 // advance to the line terminator

    emitter.label("__rt_pr_idx_str");
    emitter.instruction("lsl x15, x14, #4");                                    // index * 16
    emitter.instruction("add x15, x15, #24");                                   // element base offset = 24 + index*16
    emitter.instruction("add x15, x13, x15");                                   // element address
    emitter.instruction("ldr x1, [x15]");                                       // string ptr → value low
    emitter.instruction("ldr x2, [x15, #8]");                                   // string len → value high
    emitter.instruction("mov x0, #1");                                          // tag = string
    emitter.instruction("ldr x3, [sp, #16]");                                   // entry indent
    emitter.instruction("add x3, x3, #4");                                      // nested base = entry indent + 4
    emitter.instruction("bl __rt_print_r_value");                               // render the element
    emitter.instruction("b __rt_pr_idx_after");                                 // advance to the line terminator

    emitter.label("__rt_pr_idx_mixed");
    emitter.instruction("add x15, x14, #3");                                    // skip the 24-byte (3-quad) header
    emitter.instruction("ldr x15, [x13, x15, lsl #3]");                         // load the Mixed cell pointer
    emitter.instruction("ldr x0, [x15]");                                       // cell tag → value tag
    emitter.instruction("ldr x1, [x15, #8]");                                   // cell low word → value low
    emitter.instruction("ldr x2, [x15, #16]");                                  // cell high word → value high
    emitter.instruction("ldr x3, [sp, #16]");                                   // entry indent
    emitter.instruction("add x3, x3, #4");                                      // nested base = entry indent + 4
    emitter.instruction("bl __rt_print_r_value");                               // render the element

    emitter.label("__rt_pr_idx_after");
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");                          // load the line terminator
    emitter.instruction("mov x2, #1");                                          // len("\n") = 1
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // terminate the entry line
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the index
    emitter.instruction("add x9, x9, #1");                                      // advance the index
    emitter.instruction("str x9, [sp, #32]");                                   // save the updated index
    emitter.instruction("b __rt_pr_idx_loop");                                  // continue scanning

    emitter.label("__rt_pr_idx_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // base → close helper argument
    emitter.instruction("bl __rt_print_r_close");                               // write `<base>)\n`
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the indexed-walk frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 indexed-array print_r walker.
fn emit_print_r_indexed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_indexed ---");
    emitter.label_global("__rt_print_r_indexed");

    // rbp-relative frame: [-8]arr [-16]base [-24]entry_indent [-32]count
    //   [-40]index [-48]stamp.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the walk frame pointer
    emitter.instruction("sub rsp, 64");                                         // allocate the indexed-walk frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the paren base indent
    emitter.instruction("mov rax, rsi");                                        // copy the base indent
    emitter.instruction("add rax, 4");                                          // entry indent = base + 4
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the entry indent
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the element count from the header
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the element count
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // index = 0
    emitter.instruction("mov rax, QWORD PTR [rdi - 8]");                        // load the packed array kind word
    emitter.instruction("shr rax, 8");                                          // shift the value_type stamp into the low byte
    emitter.instruction("and rax, 0x0f");                                       // isolate the value_type field (low nibble), dropping the COW bit
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the element value_type stamp

    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // base → open helper argument
    emitter.instruction("call __rt_print_r_open");                              // write `<base>(\n`

    emitter.label("__rt_pr_idx_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the current index
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the element count
    emitter.instruction("cmp rax, rcx");                                        // processed every element?
    emitter.instruction("jge __rt_pr_idx_done_x86");                            // walk complete

    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // index → integer key
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // entry indent → integer key
    emitter.instruction("call __rt_print_r_int_key");                           // write `<indent>[N] => `

    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the element stamp
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the current index
    emitter.instruction("cmp r10, 1");                                          // string elements use a 16-byte stride
    emitter.instruction("je __rt_pr_idx_str_x86");                              // handle string elements
    emitter.instruction("cmp r10, 7");                                          // mixed elements are boxed cells
    emitter.instruction("je __rt_pr_idx_mixed_x86");                            // handle mixed cells

    // 8-byte-stride elements: int(0) / float(2) / bool(3) / array(4) / hash(5) / object(6).
    emitter.instruction("mov rax, r11");                                        // copy the index
    emitter.instruction("add rax, 3");                                          // skip the 24-byte (3-quad) header
    emitter.instruction("mov rsi, QWORD PTR [r9 + rax * 8]");                   // load the raw element word → value low
    emitter.instruction("mov rdi, r10");                                        // tag = the array stamp
    emitter.instruction("mov rdx, 0");                                          // high word unused for 8-byte elements
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // entry indent
    emitter.instruction("add rcx, 4");                                          // nested base = entry indent + 4
    emitter.instruction("call __rt_print_r_value");                             // render the element
    emitter.instruction("jmp __rt_pr_idx_after_x86");                           // advance to the line terminator

    emitter.label("__rt_pr_idx_str_x86");
    emitter.instruction("mov rax, r11");                                        // copy the index
    emitter.instruction("shl rax, 4");                                          // index * 16
    emitter.instruction("add rax, 24");                                         // element base offset = 24 + index*16
    emitter.instruction("add rax, r9");                                         // element address
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // string ptr → value low
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // string len → value high
    emitter.instruction("mov rdi, 1");                                          // tag = string
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // entry indent
    emitter.instruction("add rcx, 4");                                          // nested base = entry indent + 4
    emitter.instruction("call __rt_print_r_value");                             // render the element
    emitter.instruction("jmp __rt_pr_idx_after_x86");                           // advance to the line terminator

    emitter.label("__rt_pr_idx_mixed_x86");
    emitter.instruction("mov rax, r11");                                        // copy the index
    emitter.instruction("add rax, 3");                                          // skip the 24-byte (3-quad) header
    emitter.instruction("mov rax, QWORD PTR [r9 + rax * 8]");                   // load the Mixed cell pointer
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // cell tag → value tag
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // cell low word → value low
    emitter.instruction("mov rdx, QWORD PTR [rax + 16]");                       // cell high word → value high
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // entry indent
    emitter.instruction("add rcx, 4");                                          // nested base = entry indent + 4
    emitter.instruction("call __rt_print_r_value");                             // render the element

    emitter.label("__rt_pr_idx_after_x86");
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");                         // load the line terminator
    emitter.instruction("mov edx, 1");                                          // len("\n") = 1
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // terminate the entry line
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the index
    emitter.instruction("add rax, 1");                                          // advance the index
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the updated index
    emitter.instruction("jmp __rt_pr_idx_loop_x86");                            // continue scanning

    emitter.label("__rt_pr_idx_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // base → close helper argument
    emitter.instruction("call __rt_print_r_close");                             // write `<base>)\n`
    emitter.instruction("add rsp, 64");                                         // release the indexed-walk frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// `__rt_print_r_hash`: render an associative-array body `<base>(\n ... <base>)\n`,
/// iterating entries and rendering unquoted keys (int as `[N]`, string as `[KEY]`).
/// Input: AArch64 x0=hash x1=base / x86_64 rdi=hash rsi=base.
pub fn emit_print_r_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: print_r_hash ---");
    emitter.label_global("__rt_print_r_hash");

    // Frame (112 bytes): [0]hash [8]base [16]entry_indent [24]count [32]cursor
    //   [40]items [48]key_ptr [56]key_len [64]val_lo [72]val_hi [80]val_tag
    //   [96]x29 [104]x30.
    emitter.instruction("sub sp, sp, #112");                                    // allocate the hash-walk frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the walk frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the paren base indent
    emitter.instruction("add x9, x1, #4");                                      // entry indent = base + 4
    emitter.instruction("str x9, [sp, #16]");                                   // save the entry indent
    emitter.instruction("ldr x0, [sp, #0]");                                    // hash → count helper argument
    emitter.instruction("bl __rt_hash_count");                                  // x0 = number of entries
    emitter.instruction("str x0, [sp, #24]");                                   // save the entry count
    emitter.instruction("str xzr, [sp, #32]");                                  // iterator cursor = 0
    emitter.instruction("str xzr, [sp, #40]");                                  // items emitted = 0

    emitter.instruction("ldr x0, [sp, #8]");                                    // base → open helper argument
    emitter.instruction("bl __rt_print_r_open");                                // write `<base>(\n`

    emitter.label("__rt_pr_hash_loop");
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload items emitted
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the entry count
    emitter.instruction("cmp x9, x10");                                         // processed every entry?
    emitter.instruction("b.ge __rt_pr_hash_done");                              // walk complete

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // x0=cursor, x1=key ptr, x2=key len, x3=val_lo, x4=val_hi, x5=val_tag
    emitter.instruction("str x0, [sp, #32]");                                   // save the next iterator cursor
    emitter.instruction("str x1, [sp, #48]");                                   // save the key ptr (or integer payload)
    emitter.instruction("str x2, [sp, #56]");                                   // save the key len (-1 for integer keys)
    emitter.instruction("str x3, [sp, #64]");                                   // save the value low word
    emitter.instruction("str x4, [sp, #72]");                                   // save the value high word
    emitter.instruction("str x5, [sp, #80]");                                   // save the value runtime tag

    emitter.instruction("ldr x2, [sp, #56]");                                   // reload the key len
    emitter.instruction("cmn x2, #1");                                          // integer key? (len == -1)
    emitter.instruction("b.eq __rt_pr_hash_int_key");                           // format integer keys as [N]
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the key ptr
    emitter.instruction("ldr x1, [sp, #56]");                                   // reload the key len
    emitter.instruction("ldr x2, [sp, #16]");                                   // entry indent
    emitter.instruction("bl __rt_print_r_str_key");                             // write `<indent>[KEY] => `
    emitter.instruction("b __rt_pr_hash_after_key");                            // continue to the value
    emitter.label("__rt_pr_hash_int_key");
    emitter.instruction("ldr x0, [sp, #48]");                                   // integer key payload → integer key
    emitter.instruction("ldr x1, [sp, #16]");                                   // entry indent → integer key
    emitter.instruction("bl __rt_print_r_int_key");                             // write `<indent>[N] => `

    emitter.label("__rt_pr_hash_after_key");
    emitter.instruction("ldr x0, [sp, #80]");                                   // value tag → value renderer
    emitter.instruction("ldr x1, [sp, #64]");                                   // value low → value renderer
    emitter.instruction("ldr x2, [sp, #72]");                                   // value high → value renderer
    emitter.instruction("ldr x3, [sp, #16]");                                   // entry indent
    emitter.instruction("add x3, x3, #4");                                      // nested base = entry indent + 4
    emitter.instruction("bl __rt_print_r_value");                               // render the entry value

    abi::emit_symbol_address(emitter, "x1", "_pr_nl");                          // load the line terminator
    emitter.instruction("mov x2, #1");                                          // len("\n") = 1
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);                                                         // terminate the entry line
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload items emitted
    emitter.instruction("add x9, x9, #1");                                      // count this entry
    emitter.instruction("str x9, [sp, #40]");                                   // save the updated item count
    emitter.instruction("b __rt_pr_hash_loop");                                 // continue with the next entry

    emitter.label("__rt_pr_hash_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // base → close helper argument
    emitter.instruction("bl __rt_print_r_close");                               // write `<base>)\n`
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the hash-walk frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 associative-array print_r walker.
fn emit_print_r_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r_hash ---");
    emitter.label_global("__rt_print_r_hash");

    // rbp-relative frame: [-8]hash [-16]base [-24]entry_indent [-32]count
    //   [-40]cursor [-48]items [-56]key_ptr [-64]key_len [-72]val_lo
    //   [-80]val_hi [-88]val_tag.
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the walk frame pointer
    emitter.instruction("sub rsp, 112");                                        // allocate the hash-walk frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the paren base indent
    emitter.instruction("mov rax, rsi");                                        // copy the base indent
    emitter.instruction("add rax, 4");                                          // entry indent = base + 4
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the entry indent
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // hash → count helper argument
    emitter.instruction("call __rt_hash_count");                                // rax = number of entries
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the entry count
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // iterator cursor = 0
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // items emitted = 0

    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // base → open helper argument
    emitter.instruction("call __rt_print_r_open");                              // write `<base>(\n`

    emitter.label("__rt_pr_hash_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload items emitted
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the entry count
    emitter.instruction("cmp rax, rcx");                                        // processed every entry?
    emitter.instruction("jge __rt_pr_hash_done_x86");                           // walk complete

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // rax=cursor, rdi=key ptr, rdx=key len, rcx=val_lo, r8=val_hi, r9=val_tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the next iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save the key ptr (or integer payload)
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the key len (-1 for integer keys)
    emitter.instruction("mov QWORD PTR [rbp - 72], rcx");                       // save the value low word
    emitter.instruction("mov QWORD PTR [rbp - 80], r8");                        // save the value high word
    emitter.instruction("mov QWORD PTR [rbp - 88], r9");                        // save the value runtime tag

    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // reload the key len
    emitter.instruction("cmp rdx, -1");                                         // integer key?
    emitter.instruction("je __rt_pr_hash_int_key_x86");                         // format integer keys as [N]
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the key ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // reload the key len
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // entry indent
    emitter.instruction("call __rt_print_r_str_key");                           // write `<indent>[KEY] => `
    emitter.instruction("jmp __rt_pr_hash_after_key_x86");                      // continue to the value
    emitter.label("__rt_pr_hash_int_key_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // integer key payload → integer key
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // entry indent → integer key
    emitter.instruction("call __rt_print_r_int_key");                           // write `<indent>[N] => `

    emitter.label("__rt_pr_hash_after_key_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // value tag → value renderer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // value low → value renderer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // value high → value renderer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // entry indent
    emitter.instruction("add rcx, 4");                                          // nested base = entry indent + 4
    emitter.instruction("call __rt_print_r_value");                             // render the entry value

    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");                         // load the line terminator
    emitter.instruction("mov edx, 1");                                          // len("\n") = 1
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // sys_write
    emitter.instruction("syscall");                                             // terminate the entry line
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload items emitted
    emitter.instruction("add rax, 1");                                          // count this entry
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the updated item count
    emitter.instruction("jmp __rt_pr_hash_loop_x86");                           // continue with the next entry

    emitter.label("__rt_pr_hash_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // base → close helper argument
    emitter.instruction("call __rt_print_r_close");                             // write `<base>)\n`
    emitter.instruction("add rsp, 112");                                        // release the hash-walk frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
