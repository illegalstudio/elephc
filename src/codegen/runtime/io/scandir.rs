use crate::codegen::{emit::Emitter, platform::Arch};

/// scandir: list directory entries as an array of strings.
/// Input:  x1/x2=path string
/// Output: x0=array pointer (array of filename strings)
pub fn emit_scandir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_scandir_linux_x86_64(emitter);
        return;
    }

    let name_off = emitter.platform.dirent_name_offset();

    emitter.blank();
    emitter.comment("--- runtime: scandir ---");
    emitter.label_global("__rt_scandir");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- null-terminate path --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr

    // -- open directory --
    emitter.bl_c("opendir");                                         // opendir(cstr), x0=DIR* or NULL
    emitter.instruction("str x0, [sp, #0]");                                    // save DIR pointer on stack

    // -- create a new string array (capacity = 128 entries) --
    emitter.instruction("mov x0, #128");                                        // initial capacity of 128 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // create array, x0=array pointer
    emitter.instruction("str x0, [sp, #8]");                                    // save array pointer on stack

    // -- read directory entries in a loop --
    emitter.label("__rt_scandir_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload DIR pointer
    emitter.bl_c("readdir");                                         // readdir(DIR*), x0=dirent* or NULL
    emitter.instruction("cbz x0, __rt_scandir_close");                          // if NULL, no more entries

    // -- point at d_name and measure it until the terminating NUL --
    emitter.instruction(&format!("add x1, x0, #{}", name_off));                 // x1 = pointer to dirent.d_name for this platform
    emitter.instruction("mov x2, #0");                                          // x2 = filename length
    emitter.label("__rt_scandir_strlen");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load the next byte from d_name
    emitter.instruction("cbz w9, __rt_scandir_name_ready");                     // stop at the terminating NUL byte
    emitter.instruction("add x2, x2, #1");                                      // count one more filename byte
    emitter.instruction("b __rt_scandir_strlen");                               // continue scanning the filename
    emitter.label("__rt_scandir_name_ready");

    // -- copy name to concat_buf so it persists after next readdir call --
    emitter.instruction("str x0, [sp, #16]");                                   // save dirent pointer (will be clobbered)
    emitter.instruction("bl __rt_str_persist");                                 // copy string to heap, x1=new ptr, x2=len

    // -- push name string to array --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push name to array
    emitter.instruction("str x0, [sp, #8]");                                    // update array pointer after possible realloc
    emitter.instruction("b __rt_scandir_loop");                                 // continue reading entries

    // -- close directory and return --
    emitter.label("__rt_scandir_close");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload DIR pointer
    emitter.bl_c("closedir");                                        // closedir(DIR*)

    // -- return array pointer --
    emitter.instruction("ldr x0, [sp, #8]");                                    // return array pointer

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_scandir_linux_x86_64(emitter: &mut Emitter) {
    let name_off = emitter.platform.dirent_name_offset();

    emitter.blank();
    emitter.comment("--- runtime: scandir ---");
    emitter.label_global("__rt_scandir");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while scandir() uses directory and result-array spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the C path, result array, and DIR* locals
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the C path pointer, result array pointer, DIR* handle, and loop scratch
    emitter.instruction("call __rt_cstr");                                      // convert the elephc directory string in rax/rdx into a null-terminated C path in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the C directory path pointer across the result-array allocation and opendir() call
    emitter.instruction("mov rdi, 128");                                        // request an initial result-array capacity of 128 directory entry names
    emitter.instruction("mov rsi, 16");                                         // request 16-byte payload slots because scandir() returns string ptr/len pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the destination string array that will collect the directory entry names
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination string array pointer across the directory iteration loop
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the C directory path pointer before opening the directory stream
    emitter.instruction("call opendir");                                        // open the directory stream through libc opendir()
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the DIR* handle across the readdir() loop and the final closedir() call
    emitter.instruction("test rax, rax");                                       // detect opendir() failure before entering the directory iteration loop
    emitter.instruction("jz __rt_scandir_ret");                                 // return the empty result array when the directory stream cannot be opened

    emitter.label("__rt_scandir_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the DIR* handle before asking libc for the next directory entry
    emitter.instruction("call readdir");                                        // fetch the next directory entry through libc readdir()
    emitter.instruction("test rax, rax");                                       // detect the end-of-directory marker before measuring a filename or appending it
    emitter.instruction("jz __rt_scandir_close");                               // stop iterating once libc readdir() reports that no more directory entries remain
    emitter.instruction(&format!("lea rsi, [rax + {}]", name_off));             // compute the pointer to dirent.d_name for the current Linux directory entry layout
    emitter.instruction("xor edx, edx");                                        // start the filename length counter at zero before scanning for the trailing null byte
    emitter.label("__rt_scandir_strlen");
    emitter.instruction("mov r8b, BYTE PTR [rsi + rdx]");                       // load the next filename byte from dirent.d_name while measuring its elephc string length
    emitter.instruction("test r8b, r8b");                                       // stop scanning once the trailing C null terminator is reached
    emitter.instruction("jz __rt_scandir_push");                                // continue into the append path once the current filename length is known
    emitter.instruction("add rdx, 1");                                          // advance the measured filename length after consuming one non-null byte
    emitter.instruction("jmp __rt_scandir_strlen");                             // continue scanning until the current directory entry name is fully measured

    emitter.label("__rt_scandir_push");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the destination string array pointer into the x86_64 append-helper receiver register
    emitter.instruction("call __rt_array_push_str");                            // persist and append the current directory entry name into the destination string array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the possibly-grown destination string array pointer after appending one directory entry
    emitter.instruction("jmp __rt_scandir_loop");                               // continue iterating until libc readdir() reports end-of-directory

    emitter.label("__rt_scandir_close");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the DIR* handle before closing the directory stream
    emitter.instruction("call closedir");                                       // close the directory stream through libc closedir()

    emitter.label("__rt_scandir_ret");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the destination string array pointer in the canonical x86_64 integer result register
    emitter.instruction("add rsp, 32");                                         // release the temporary scandir() spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the directory entry array
    emitter.instruction("ret");                                                 // return the array of directory entry names to the caller
}
