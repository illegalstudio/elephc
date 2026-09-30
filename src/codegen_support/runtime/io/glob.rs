//! Purpose:
//! Emits the `__rt_glob`, `__rt_cstr` runtime helper assembly for glob.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::data::GLOB_INVALID_FLAGS_WARNING;

/// Returns the value php gives a `GLOB_*` constant, read from the table that declares it.
///
/// Repeating the number here would let the runtime and the declared constant drift apart in
/// silence, which is the whole failure mode this translation exists to avoid.
fn php_glob_flag(name: &str) -> i64 {
    crate::types::stream_constants::STREAM_INT_CONSTANTS
        .iter()
        .find(|(declared, _)| *declared == name)
        .unwrap_or_else(|| panic!("{name} must be a declared php constant"))
        .1
}

/// Returns the php flag / libc flag pairs `__rt_glob` translates between, in declaration order.
///
/// `GLOB_ONLYDIR` is absent: no libc bit means what php means by it, so the match loop filters.
fn glob_flag_translation(emitter: &Emitter) -> Vec<(i64, i64, &'static str)> {
    let libc = emitter.platform.glob_libc_flags();
    vec![
        (php_glob_flag("GLOB_ERR"), libc.err, "GLOB_ERR"),
        (php_glob_flag("GLOB_MARK"), libc.mark, "GLOB_MARK"),
        (php_glob_flag("GLOB_NOCHECK"), libc.nocheck, "GLOB_NOCHECK"),
        (php_glob_flag("GLOB_NOSORT"), libc.nosort, "GLOB_NOSORT"),
        (php_glob_flag("GLOB_BRACE"), libc.brace, "GLOB_BRACE"),
        (php_glob_flag("GLOB_NOESCAPE"), libc.noescape, "GLOB_NOESCAPE"),
    ]
}

/// Emits the `__rt_glob` runtime helper for ARM64 targets.
/// Receives a pattern string pointer/length in x1/x2, calls libc `glob()` to find matching
/// filesystem paths, and returns a runtime array of PHP strings (each with ptr/length).
/// On success the array contains one entry per match; on failure (no matches, error) returns
/// an empty array. Calls `globfree()` before returning to release libc resources.
/// Preserves all callee-saved registers and restores the stack frame before returning.
/// Input:  x1=pattern string pointer, x2=pattern string length
/// Output: x0=array pointer (PhpArray of matching path strings as PhpString entries)
pub fn emit_glob(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_glob_linux_x86_64(emitter);
        return;
    }

    let pathv_off = emitter.platform.glob_pathv_offset();
    let available = php_glob_flag("GLOB_AVAILABLE_FLAGS");
    let onlydir = php_glob_flag("GLOB_ONLYDIR");

    emitter.blank();
    emitter.comment("--- runtime: glob ---");
    emitter.label_global("__rt_glob");

    // -- set up stack frame (128 bytes for glob_t + locals + frame) --
    emitter.instruction("sub sp, sp, #176");                                    // allocate 176 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #160]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #160");                                   // establish new frame pointer

    // -- refuse any bit php does not expose, the way php does: a warning and false --
    // Stack layout: sp+0=cstr, sp+8=retcode, sp+16=glob_t, sp+104=array, sp+112=count,
    // sp+120=index, sp+128=ONLYDIR requested, sp+136/144=the path across the directory test,
    // sp+152=the translated libc flags.
    emitter.instruction(&format!("mov w9, #{}", available & 0xFFFF));           // low half of GLOB_AVAILABLE_FLAGS
    emitter.instruction(&format!("movk w9, #{}, lsl #16", available >> 16));    // high half of GLOB_AVAILABLE_FLAGS
    emitter.instruction("bic x10, x3, x9");                                     // keep whatever php does not expose
    emitter.instruction("cbz x10, __rt_glob_flags_ok");                         // every bit is a php flag, carry on
    emitter.instruction("str xzr, [sp, #104]");                                 // php answers false, not an empty array
    abi::emit_symbol_address(emitter, "x1", "_diag_glob_flags");
    emitter.instruction(&format!("mov x2, #{}", GLOB_INVALID_FLAGS_WARNING.len()));
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    // Straight to the RETURN label, deliberately: `globfree()` lives at `__rt_glob_free`, which
    // merely falls through into `__rt_glob_ret`. Branching to the tail therefore never passes
    // through that call, which matters because libc `glob()` has not run and the stack `glob_t`
    // at [sp,#16] holds whatever the frame held before.
    emitter.instruction("b __rt_glob_ret");

    emitter.label("__rt_glob_flags_ok");
    // -- split off GLOB_ONLYDIR, which is php's own bit and which the match loop applies --
    emitter.instruction(&format!("and x10, x3, #{onlydir}"));                   // php's private 1 << 30, no libc has it
    emitter.instruction("str x10, [sp, #128]");                                 // the match loop filters on it

    // -- translate the rest to this platform's libc bits --
    emitter.instruction("mov x11, #0");                                         // start from no libc flags
    for (php_bit, libc_bit, name) in glob_flag_translation(emitter) {
        emitter.comment(&format!("php {name} -> this platform's libc bit"));
        emitter.instruction(&format!("orr x12, x11, #{libc_bit}"));             // the flags this one would add
        emitter.instruction(&format!("tst x3, #{php_bit}"));                    // did php's caller ask for it?
        emitter.instruction("csel x11, x12, x11, ne");                          // keep it only when php's caller set the bit
    }
    emitter.instruction("str x11, [sp, #152]");                                 // hold them across __rt_cstr

    // -- null-terminate pattern --
    emitter.instruction("bl __rt_cstr");                                        // convert pattern to C string, x0=cstr

    // -- call glob(pattern, flags, NULL, &glob_result) --
    // `gl_pathc` stays at offset 0 on both supported libcs; `gl_pathv` is platform-specific.
    emitter.instruction("add x3, sp, #16");                                     // pointer to glob_t struct on stack
    emitter.instruction("ldr x1, [sp, #152]");                                  // this platform's libc flags
    emitter.instruction("mov x2, #0");                                          // errfunc = NULL
    emitter.bl_c("glob");                                            // call glob(pattern=x0, flags, errfunc, glob_t)
    emitter.instruction("str x0, [sp, #8]");                                    // save return code

    // -- create result array --
    emitter.instruction("mov x0, #128");                                        // initial capacity of 128 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // create array, x0=array pointer
    emitter.instruction("str x0, [sp, #104]");                                  // save array pointer on stack

    // -- check if glob succeeded --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload return code
    emitter.instruction("cbnz x9, __rt_glob_ret");                              // if non-zero, return empty array

    // -- loop through matched paths --
    emitter.instruction("ldr x9, [sp, #16]");                                   // load gl_pathc (offset 0 in glob_t)
    emitter.instruction("str x9, [sp, #112]");                                  // save match count
    emitter.instruction("mov x11, #0");                                         // initialize loop index

    emitter.label("__rt_glob_loop");
    emitter.instruction("ldr x9, [sp, #112]");                                  // reload match count
    emitter.instruction("cmp x11, x9");                                         // check if we've processed all matches
    emitter.instruction("b.hs __rt_glob_free");                                 // if done, free and return
    emitter.instruction("str x11, [sp, #120]");                                 // save current index

    // -- load path pointer from pathv[i] --
    emitter.instruction(&format!("ldr x10, [sp, #{}]", 16 + pathv_off));        // load gl_pathv from this platform's glob_t layout
    emitter.instruction("lsl x12, x11, #3");                                    // byte offset = index * 8
    emitter.instruction("ldr x1, [x10, x12]");                                  // load pathv[i] = char* to path

    // -- calculate string length by scanning for null --
    emitter.instruction("mov x2, #0");                                          // initialize length counter
    emitter.label("__rt_glob_strlen");
    emitter.instruction("ldrb w13, [x1, x2]");                                  // load byte at current position
    emitter.instruction("cbz w13, __rt_glob_push");                             // if null terminator, done counting
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_glob_strlen");                                  // continue scanning

    // -- copy string and push to array --
    emitter.label("__rt_glob_push");
    // php's GLOB_ONLYDIR filters what libc already produced, and it filters everything: measured
    // on `php -n` 8.5.6, `GLOB_NOCHECK|GLOB_ONLYDIR` with no match answers `[]`, so even the
    // pattern NOCHECK invents is tested. `__rt_is_dir_core` is the guard-free entry, because
    // `glob()` keeps working when the plain-files wrapper is unregistered.
    emitter.instruction("ldr x9, [sp, #128]");                                  // was GLOB_ONLYDIR requested?
    emitter.instruction("cbz x9, __rt_glob_push_do");                           // no filter, keep every match
    emitter.instruction("str x1, [sp, #136]");                                  // the directory test clobbers the pair
    emitter.instruction("str x2, [sp, #144]");
    emitter.instruction("bl __rt_is_dir_core");                                 // stat, so a symlink to a directory counts
    emitter.instruction("cbz x0, __rt_glob_next");                              // drop everything that is not a directory
    emitter.instruction("ldr x1, [sp, #136]");                                  // restore the matched path
    emitter.instruction("ldr x2, [sp, #144]");

    emitter.label("__rt_glob_push_do");
    emitter.instruction("bl __rt_str_persist");                                 // copy to heap for persistence
    emitter.instruction("ldr x0, [sp, #104]");                                  // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push path to array
    emitter.instruction("str x0, [sp, #104]");                                  // update array pointer after possible realloc

    // -- advance to next entry --
    emitter.label("__rt_glob_next");
    emitter.instruction("ldr x11, [sp, #120]");                                 // reload current index
    emitter.instruction("add x11, x11, #1");                                    // increment index
    emitter.instruction("b __rt_glob_loop");                                    // continue loop

    // -- free glob resources --
    emitter.label("__rt_glob_free");
    emitter.instruction("add x0, sp, #16");                                     // pointer to glob_t struct
    emitter.bl_c("globfree");                                        // free glob results

    // -- return array pointer --
    emitter.label("__rt_glob_ret");
    emitter.instruction("ldr x0, [sp, #104]");                                  // return array pointer

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #176");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the `__rt_glob` runtime helper for the x86_64 Linux ABI.
/// Receives a pattern string pointer/length in rax/rdx (converted to null-terminated by `__rt_cstr`),
/// calls libc `glob()` to expand the pattern, and returns a runtime array of PHP strings (ptr/length).
/// On success the array holds one entry per matched path; on failure returns an empty array.
/// The stack frame holds the Linux glob_t at [rsp] and bookkeeping slots at rbp-8/16/24/32.
/// Cleans up with `globfree()` before returning the result array.
/// Input:  rax/rdx=pattern string pointer/length (cstr converted by `__rt_cstr`)
/// Output: rax=array pointer (PhpArray of matching path strings as PhpString entries)
fn emit_glob_linux_x86_64(emitter: &mut Emitter) {
    let pathv_off = emitter.platform.glob_pathv_offset();
    let frame_size = 160usize;
    let available = php_glob_flag("GLOB_AVAILABLE_FLAGS");
    let onlydir = php_glob_flag("GLOB_ONLYDIR");

    emitter.blank();
    emitter.comment("--- runtime: glob ---");
    emitter.label_global("__rt_glob");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while glob() uses a stack glob_t and array bookkeeping slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the result array, glob() status, and iteration index locals
    emitter.instruction(&format!("sub rsp, {}", frame_size));                   // reserve an aligned stack frame large enough for the Linux glob_t plus local bookkeeping
    // -- refuse any bit php does not expose, the way php does: a warning and false --
    // Bookkeeping: rbp-8=status, rbp-16=array, rbp-24=count, rbp-32=index, rbp-40=ONLYDIR
    // requested, rbp-48/56=the path across the directory test, rbp-64=the translated libc flags.
    emitter.instruction("mov r10, rcx");                                        // php's flags, as the caller wrote them
    emitter.instruction(&format!("mov r11, {}", !available));                   // everything GLOB_AVAILABLE_FLAGS omits
    emitter.instruction("and r10, r11");                                        // keep whatever php does not expose
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_glob_flags_ok");                               // every bit is a php flag, carry on
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // php answers false, not an empty array
    abi::emit_symbol_address(emitter, "rdi", "_diag_glob_flags");
    emitter.instruction(&format!("mov rsi, {}", GLOB_INVALID_FLAGS_WARNING.len()));
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    emitter.instruction("jmp __rt_glob_ret");

    emitter.label("__rt_glob_flags_ok");
    // -- split off GLOB_ONLYDIR, which is php's own bit and which the match loop applies --
    emitter.instruction(&format!("mov r10, {onlydir}"));                        // php's private 1 << 30, no libc has it
    emitter.instruction("and r10, rcx");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // the match loop filters on it

    // -- translate the rest to this platform's libc bits --
    emitter.instruction("xor r8d, r8d");                                        // start from no libc flags
    for (php_bit, libc_bit, name) in glob_flag_translation(emitter) {
        emitter.comment(&format!("php {name} -> this platform's libc bit"));
        emitter.instruction("mov r9, r8");                                      // the candidate, computed before the test
        emitter.instruction(&format!("or r9, {libc_bit}"));                     // because `or` writes the flags too
        emitter.instruction(&format!("test rcx, {php_bit}"));                   // did php's caller ask for it?
        emitter.instruction("cmovnz r8, r9");                                   // keep it only when php's caller set the bit
    }
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // hold them across __rt_cstr

    emitter.instruction("call __rt_cstr");                                      // convert the elephc glob pattern in rax/rdx into a null-terminated C pattern in rax
    emitter.instruction("mov rdi, rax");                                        // pass the C pattern pointer as the first libc glob() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // this platform's libc flags
    emitter.instruction("xor edx, edx");                                        // pass errfunc = NULL to the libc glob() helper
    emitter.instruction("lea rcx, [rsp]");                                      // pass the stack-resident glob_t storage as the final libc glob() argument
    emitter.instruction("call glob");                                           // expand the pattern through libc glob() into the temporary stack glob_t
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the libc glob() status code across the result-array allocation and match iteration loop
    emitter.instruction("mov rdi, 128");                                        // request an initial result-array capacity of 128 path strings
    emitter.instruction("mov rsi, 16");                                         // request 16-byte payload slots because glob() returns string ptr/len pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the destination string array that will collect the matched paths
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination string array pointer across the match iteration loop
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // detect glob() failure before trying to iterate gl_pathc/gl_pathv
    emitter.instruction("jne __rt_glob_ret");                                   // return the empty result array when libc glob() reports no matches or another error
    emitter.instruction("mov r10, QWORD PTR [rsp]");                            // load gl_pathc from the first field of the stack-resident Linux glob_t
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the matched-path count across append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the glob() match iteration index to the first path entry

    emitter.label("__rt_glob_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current match index before checking whether every matched path has been consumed
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // compare the current match index against gl_pathc
    emitter.instruction("jae __rt_glob_free");                                  // stop iterating once every matched path in gl_pathv has been appended
    emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {}]", pathv_off));  // load gl_pathv from the Linux glob_t layout before selecting the current match pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + r10 * 8]");                  // load the current matched-path C string pointer from gl_pathv[index]
    emitter.instruction("xor edx, edx");                                        // start the matched-path length counter at zero before scanning for the trailing null byte
    emitter.label("__rt_glob_strlen");
    emitter.instruction("mov r8b, BYTE PTR [rsi + rdx]");                       // load the next matched-path byte while measuring its elephc string length
    emitter.instruction("test r8b, r8b");                                       // stop scanning once the trailing C null terminator is reached
    emitter.instruction("jz __rt_glob_push");                                   // continue into the append path once the current matched-path length is known
    emitter.instruction("add rdx, 1");                                          // advance the measured matched-path length after consuming one non-null byte
    emitter.instruction("jmp __rt_glob_strlen");                                // continue scanning until the current matched path is fully measured

    emitter.label("__rt_glob_push");
    // php's GLOB_ONLYDIR filters what libc already produced, and it filters everything: measured
    // on `php -n` 8.5.6, `GLOB_NOCHECK|GLOB_ONLYDIR` with no match answers `[]`, so even the
    // pattern NOCHECK invents is tested. `__rt_is_dir_core` is the guard-free entry, because
    // `glob()` keeps working when the plain-files wrapper is unregistered.
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // was GLOB_ONLYDIR requested?
    emitter.instruction("je __rt_glob_push_do");                                // no filter, keep every match
    emitter.instruction("mov QWORD PTR [rbp - 48], rsi");                       // the directory test clobbers the pair
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");
    emitter.instruction("mov rax, rsi");                                        // the stat helper reads the path from rax/rdx
    emitter.instruction("call __rt_is_dir_core");                               // stat, so a symlink to a directory counts
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_glob_next");                                   // drop everything that is not a directory
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // restore the matched path
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");

    emitter.label("__rt_glob_push_do");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the destination string array pointer into the x86_64 append-helper receiver register
    emitter.instruction("call __rt_array_push_str");                            // persist and append the current matched path into the destination string array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the possibly-grown destination string array pointer after appending one match

    emitter.label("__rt_glob_next");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance the glob() match iteration index after consuming one matched path entry
    emitter.instruction("jmp __rt_glob_loop");                                  // continue iterating until every matched path has been appended

    emitter.label("__rt_glob_free");
    emitter.instruction("lea rdi, [rsp]");                                      // pass the stack-resident Linux glob_t back to libc globfree() for cleanup
    emitter.instruction("call globfree");                                       // release the libc glob() match storage before returning the result array

    emitter.label("__rt_glob_ret");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the destination string array pointer in the canonical x86_64 integer result register
    emitter.instruction(&format!("add rsp, {}", frame_size));                   // release the temporary glob_t frame and local bookkeeping slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the matched-path array
    emitter.instruction("ret");                                                 // return the array of matched paths to the caller
}
