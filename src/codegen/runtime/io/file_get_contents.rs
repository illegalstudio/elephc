use crate::codegen::{emit::Emitter, platform::Arch};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// file_get_contents: read an entire file into a string.
/// Input:  x1/x2=filename string
/// Output: x1=buffer pointer, x2=bytes read
pub fn emit_file_get_contents(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_file_get_contents_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let stat_buf = plat.stat_buf_size();
    // Layout: [0..8) path ptr, [8..16) file size, [16..16+stat_buf) stat buffer,
    //         then fd, heap ptr, bytes read, + saved frame regs
    let stat_base = 16;
    let st_size_abs = stat_base + plat.stat_size_offset();
    let locals_start = stat_base + stat_buf;
    let fd_off = locals_start;                // +0: fd
    let heap_off = locals_start + 8;          // +8: heap buffer ptr
    let bread_off = locals_start + 16;        // +16: bytes read
    let frame_size = (locals_start + 24 + 16 + 15) & !15; // +24 locals + 16 saved regs, aligned
    let save_offset = frame_size - 16;

    emitter.blank();
    emitter.comment("--- runtime: file_get_contents ---");
    emitter.label_global("__rt_file_get_contents");

    // -- set up stack frame --
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack for stat buf + locals + frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", save_offset));      // save frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{}", save_offset));             // establish new frame pointer

    // -- null-terminate the filename --
    emitter.instruction("bl __rt_cstr");                                        // convert filename to C string, x0=cstr path
    emitter.instruction("str x0, [sp, #0]");                                    // save null-terminated path pointer

    // -- call stat64 to get file size --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload path for stat64
    emitter.instruction(&format!("add x1, sp, #{}", stat_base));                // pointer to stat buffer on stack
    emitter.syscall(338);

    // -- extract file size from stat struct --
    emitter.instruction(&format!("ldr x9, [sp, #{}]", st_size_abs));            // load st_size from stat struct
    emitter.instruction("str x9, [sp, #8]");                                    // save file size on stack

    // -- open the file for reading --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload null-terminated path
    emitter.instruction("mov x1, #0");                                          // O_RDONLY = 0
    emitter.instruction("mov x2, #0");                                          // mode not needed for O_RDONLY
    emitter.syscall(5);
    emitter.instruction(&format!("str x0, [sp, #{}]", fd_off));                 // save fd on stack

    // -- allocate heap buffer for file contents --
    emitter.instruction("ldr x0, [sp, #8]");                                    // load file size as allocation request
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate buffer, x0=pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // store string kind in the uniform heap header
    emitter.instruction(&format!("str x0, [sp, #{}]", heap_off));               // save heap buffer pointer

    // -- read entire file into buffer --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd
    emitter.instruction(&format!("ldr x1, [sp, #{}]", heap_off));               // buffer pointer for read
    emitter.instruction("ldr x2, [sp, #8]");                                    // file size = bytes to read
    emitter.syscall(3);
    emitter.instruction(&format!("str x0, [sp, #{}]", bread_off));              // save actual bytes read

    // -- close the file --
    emitter.instruction(&format!("ldr x0, [sp, #{}]", fd_off));                 // reload fd
    emitter.syscall(6);

    // -- return buffer pointer and bytes read --
    emitter.instruction(&format!("ldr x1, [sp, #{}]", heap_off));               // return heap buffer pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", bread_off));              // return actual bytes read

    // -- restore frame and return --
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", save_offset));      // restore frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_file_get_contents_linux_x86_64(emitter: &mut Emitter) {
    let stat_buf = emitter.platform.stat_buf_size();
    let size_off = emitter.platform.stat_size_offset();
    let frame_size = ((stat_buf + 48) + 15) & !15;
    let path_off = 8usize;
    let size_slot_off = 16usize;
    let fd_off = 24usize;
    let heap_off = 32usize;
    let bread_off = 40usize;

    emitter.blank();
    emitter.comment("--- runtime: file_get_contents ---");
    emitter.label_global("__rt_file_get_contents");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while file_get_contents uses stat and I/O spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary path, size, fd, and heap pointer slots
    emitter.instruction(&format!("sub rsp, {}", frame_size));                   // reserve an aligned Linux stat buffer plus local spill slots for the read-path helper

    emitter.instruction("call __rt_cstr");                                      // convert the elephc filename in rax/rdx into a null-terminated C path in rax
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", path_off));   // preserve the C path pointer across the libc stat(), open(), read(), and close() calls

    emitter.instruction("mov rdi, rax");                                        // pass the C path pointer as the first libc stat() argument
    emitter.instruction("lea rsi, [rsp]");                                      // pass the temporary stack stat buffer as the second libc stat() argument
    emitter.instruction("call stat");                                           // populate the temporary Linux stat buffer so the file size can be read safely
    emitter.instruction("cmp rax, 0");                                          // test whether libc stat() succeeded before reading the file metadata
    emitter.instruction("jne __rt_file_get_contents_fail");                     // return the empty string when the input path cannot be stated

    emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", size_off));   // load st_size from the temporary Linux stat buffer after libc stat() succeeds
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", size_slot_off)); // preserve the file byte size across the later open(), heap_alloc(), and read() calls

    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", path_off));   // reload the C path pointer before opening the input file for reading
    emitter.instruction("xor esi, esi");                                        // pass O_RDONLY as the libc open() flags for the file_get_contents() read path
    emitter.instruction("call open");                                           // open the input file for reading through libc open()
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", fd_off));     // preserve the opened file descriptor across the later heap allocation and read() call
    emitter.instruction("cmp rax, 0");                                          // test whether libc open() succeeded before attempting to allocate or read
    emitter.instruction("jl __rt_file_get_contents_fail");                      // return the empty string when the file could not be opened for reading

    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", size_slot_off)); // reload the requested file size before allocating the owned destination buffer
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned heap storage for the file payload through the shared x86_64 heap wrapper
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated buffer as a persisted elephc string in the uniform heap header
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", heap_off));   // preserve the owned destination buffer pointer across the libc read() and close() calls

    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", fd_off));     // pass the opened file descriptor as the first libc read() argument
    emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {}]", heap_off));   // pass the owned destination buffer as the second libc read() argument
    emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {}]", size_slot_off)); // pass the requested byte count from the stat-derived file size to libc read()
    emitter.instruction("call read");                                           // read the entire file payload into the owned elephc string buffer through libc read()
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", bread_off));  // preserve the actual read byte count for the final elephc string result pair

    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", fd_off));     // reload the file descriptor before closing the successfully opened source file
    emitter.instruction("call close");                                          // close the opened source file after the read() call completes

    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", heap_off));   // return the owned file payload pointer in the x86_64 string result register
    emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {}]", bread_off));  // return the actual read byte count in the x86_64 string length register
    emitter.instruction(&format!("add rsp, {}", frame_size));                   // release the temporary Linux stat buffer and local spill slots used by file_get_contents()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the owned file payload
    emitter.instruction("ret");                                                 // return the owned file contents as an elephc string

    emitter.label("__rt_file_get_contents_fail");
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when the file could not be stated or opened
    emitter.instruction("xor edx, edx");                                        // return an empty string length when the file could not be stated or opened
    emitter.instruction(&format!("add rsp, {}", frame_size));                   // release the temporary Linux stat buffer and local spill slots on the failure path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty string
    emitter.instruction("ret");                                                 // return the empty string result for the failed read-path helper
}
