use crate::codegen::{emit::Emitter, platform::Arch};

/// glob: find pathnames matching a pattern.
/// Input:  x1/x2=pattern string
/// Output: x0=array pointer (array of matching path strings)
pub fn emit_glob(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_glob_linux_x86_64(emitter);
        return;
    }

    let pathv_off = emitter.platform.glob_pathv_offset();

    emitter.blank();
    emitter.comment("--- runtime: glob ---");
    emitter.label_global("__rt_glob");

    // -- set up stack frame (128 bytes for glob_t + locals + frame) --
    emitter.instruction("sub sp, sp, #176");                                    // allocate 176 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #160]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #160");                                   // establish new frame pointer

    // -- null-terminate pattern --
    emitter.instruction("bl __rt_cstr");                                        // convert pattern to C string, x0=cstr

    // -- call glob(pattern, 0, NULL, &glob_result) --
    // Stack layout: sp+0=cstr, sp+8=retcode, sp+16=glob_t, sp+104=array, sp+112=count, sp+120=index
    // `gl_pathc` stays at offset 0 on both supported libcs; `gl_pathv` is platform-specific.
    emitter.instruction("add x3, sp, #16");                                     // pointer to glob_t struct on stack
    emitter.instruction("mov x1, #0");                                          // flags = 0
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
    emitter.instruction("bl __rt_str_persist");                                 // copy to heap for persistence
    emitter.instruction("ldr x0, [sp, #104]");                                  // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push path to array
    emitter.instruction("str x0, [sp, #104]");                                  // update array pointer after possible realloc

    // -- advance to next entry --
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

fn emit_glob_linux_x86_64(emitter: &mut Emitter) {
    let pathv_off = emitter.platform.glob_pathv_offset();
    let frame_size = 160usize;

    emitter.blank();
    emitter.comment("--- runtime: glob ---");
    emitter.label_global("__rt_glob");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while glob() uses a stack glob_t and array bookkeeping slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the result array, glob() status, and iteration index locals
    emitter.instruction(&format!("sub rsp, {}", frame_size));                   // reserve an aligned stack frame large enough for the Linux glob_t plus local bookkeeping
    emitter.instruction("call __rt_cstr");                                      // convert the elephc glob pattern in rax/rdx into a null-terminated C pattern in rax
    emitter.instruction("mov rdi, rax");                                        // pass the C pattern pointer as the first libc glob() argument
    emitter.instruction("xor esi, esi");                                        // use glob() flags = 0 for the current minimal runtime slice
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
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the destination string array pointer into the x86_64 append-helper receiver register
    emitter.instruction("call __rt_array_push_str");                            // persist and append the current matched path into the destination string array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the possibly-grown destination string array pointer after appending one match
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
