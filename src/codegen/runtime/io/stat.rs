use crate::codegen::emit::Emitter;

/// Stat-related helpers: file_exists, is_file, is_dir, is_readable, is_writable,
/// filesize, filemtime.
/// All take x1/x2=path string, return result in x0.
pub fn emit_stat(emitter: &mut Emitter) {
    // ================================================================
    // __rt_file_exists: check if a path exists
    // Input:  x1/x2=path
    // Output: x0=1 if exists, 0 if not
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: file_exists ---");
    emitter.label("__rt_file_exists");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #176");                                    // allocate 176 bytes (144 stat + frame)
    emitter.instruction("stp x29, x30, [sp, #160]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #160");                                   // establish new frame pointer

    // -- null-terminate path and call stat64 --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer on stack
    emitter.instruction("mov x16, #338");                                       // syscall 338 = stat64
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- check return value: 0=success (exists), -1=error (not found) --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if stat succeeded (file exists)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #176");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_is_file: check if path is a regular file
    // Input:  x1/x2=path
    // Output: x0=1 if regular file, 0 if not
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: is_file ---");
    emitter.label("__rt_is_file");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #176");                                    // allocate 176 bytes (144 stat + frame)
    emitter.instruction("stp x29, x30, [sp, #160]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #160");                                   // establish new frame pointer

    // -- null-terminate path and call stat64 --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer on stack
    emitter.instruction("mov x16, #338");                                       // syscall 338 = stat64
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- check if stat failed --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("b.ne __rt_is_file_no");                                // if stat failed, not a file

    // -- check st_mode & S_IFMT == S_IFREG --
    emitter.instruction("ldrh w9, [sp, #4]");                                   // load st_mode (uint16 at offset 4)
    emitter.instruction("and w9, w9, #0xF000");                                 // mask with S_IFMT
    emitter.instruction("mov w10, #0x8000");                                    // S_IFREG = 0x8000
    emitter.instruction("cmp w9, w10");                                         // compare with regular file type
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if regular file
    emitter.instruction("b __rt_is_file_ret");                                  // jump to return

    emitter.label("__rt_is_file_no");
    emitter.instruction("mov x0, #0");                                          // not a regular file

    emitter.label("__rt_is_file_ret");
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #176");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_is_dir: check if path is a directory
    // Input:  x1/x2=path
    // Output: x0=1 if directory, 0 if not
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: is_dir ---");
    emitter.label("__rt_is_dir");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #176");                                    // allocate 176 bytes (144 stat + frame)
    emitter.instruction("stp x29, x30, [sp, #160]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #160");                                   // establish new frame pointer

    // -- null-terminate path and call stat64 --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer on stack
    emitter.instruction("mov x16, #338");                                       // syscall 338 = stat64
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- check if stat failed --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("b.ne __rt_is_dir_no");                                 // if stat failed, not a directory

    // -- check st_mode & S_IFMT == S_IFDIR --
    emitter.instruction("ldrh w9, [sp, #4]");                                   // load st_mode (uint16 at offset 4)
    emitter.instruction("and w9, w9, #0xF000");                                 // mask with S_IFMT
    emitter.instruction("mov w10, #0x4000");                                    // S_IFDIR = 0x4000
    emitter.instruction("cmp w9, w10");                                         // compare with directory type
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if directory
    emitter.instruction("b __rt_is_dir_ret");                                   // jump to return

    emitter.label("__rt_is_dir_no");
    emitter.instruction("mov x0, #0");                                          // not a directory

    emitter.label("__rt_is_dir_ret");
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #176");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_is_readable: check if path is readable
    // Input:  x1/x2=path
    // Output: x0=1 if readable, 0 if not
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: is_readable ---");
    emitter.label("__rt_is_readable");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- null-terminate path --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr

    // -- call access(path, R_OK) --
    emitter.instruction("mov x1, #4");                                          // R_OK = 4 (read permission check)
    emitter.instruction("mov x16, #33");                                        // syscall 33 = access
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- return 1 if accessible, 0 if not --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if access succeeded

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_is_writable: check if path is writable
    // Input:  x1/x2=path
    // Output: x0=1 if writable, 0 if not
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: is_writable ---");
    emitter.label("__rt_is_writable");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- null-terminate path --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr

    // -- call access(path, W_OK) --
    emitter.instruction("mov x1, #2");                                          // W_OK = 2 (write permission check)
    emitter.instruction("mov x16, #33");                                        // syscall 33 = access
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- return 1 if accessible, 0 if not --
    emitter.instruction("cmp x0, #0");                                          // check syscall result
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if access succeeded

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_filesize: get file size
    // Input:  x1/x2=path
    // Output: x0=file size in bytes
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: filesize ---");
    emitter.label("__rt_filesize");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #176");                                    // allocate 176 bytes (144 stat + frame)
    emitter.instruction("stp x29, x30, [sp, #160]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #160");                                   // establish new frame pointer

    // -- null-terminate path and call stat64 --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer on stack
    emitter.instruction("mov x16, #338");                                       // syscall 338 = stat64
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- extract st_size from stat struct --
    emitter.instruction("ldr x0, [sp, #96]");                                   // load st_size (int64 at offset 96)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #176");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // ================================================================
    // __rt_filemtime: get file modification time
    // Input:  x1/x2=path
    // Output: x0=mtime as unix timestamp
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: filemtime ---");
    emitter.label("__rt_filemtime");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #176");                                    // allocate 176 bytes (144 stat + frame)
    emitter.instruction("stp x29, x30, [sp, #160]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #160");                                   // establish new frame pointer

    // -- null-terminate path and call stat64 --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
    emitter.instruction("add x1, sp, #0");                                      // pointer to stat buffer on stack
    emitter.instruction("mov x16, #338");                                       // syscall 338 = stat64
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- extract st_mtimespec.tv_sec from stat struct --
    emitter.instruction("ldr x0, [sp, #48]");                                   // load tv_sec (int64 at offset 48)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #176");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
