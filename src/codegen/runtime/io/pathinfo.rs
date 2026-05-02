use crate::codegen::{emit::Emitter, platform::Arch};

/// pathinfo (component-flag form): return one component of a path as a string.
/// Input:  x1/x2 = path, x3 = flag (1=DIRNAME, 2=BASENAME, 4=EXTENSION, 8=FILENAME)
/// Output: x1/x2 = component string (empty when the requested component is absent)
///
/// PHP accepts component bitmasks; when several component bits are present it
/// returns the first component in DIRNAME → BASENAME → EXTENSION → FILENAME
/// order. Exact PATHINFO_ALL is handled by the array helper before reaching
/// this routine, so dynamic exact-15 flags fail closed to an empty string.
///
/// EXTENSION / FILENAME are computed by first reducing the path to its
/// basename (via `__rt_basename`) and then locating the last `.` in the
/// resulting slice.
pub fn emit_pathinfo_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pathinfo_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: pathinfo (single-flag form) ---");
    emitter.label_global("__rt_pathinfo_str");

    // Reserve a frame because we tail-call helpers that establish their own.
    emitter.instruction("sub sp, sp, #16");                                     // allocate frame for the saved frame regs
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- dispatch on the flag value --
    emitter.instruction("cmp x3, #15");                                         // dynamic PATHINFO_ALL reaches the string helper only when not statically known
    emitter.instruction("b.eq __rt_pathinfo_empty");                            // fail closed instead of returning a misleading component string
    emitter.instruction("and x9, x3, #1");                                      // does the bitmask request PATHINFO_DIRNAME first?
    emitter.instruction("cbnz x9, __rt_pathinfo_dirname");                      // delegate to dirname runtime when dirname is present
    emitter.instruction("and x9, x3, #2");                                      // does the bitmask request PATHINFO_BASENAME next?
    emitter.instruction("cbnz x9, __rt_pathinfo_basename");                     // delegate to basename runtime when basename is present
    emitter.instruction("and x9, x3, #4");                                      // does the bitmask request PATHINFO_EXTENSION next?
    emitter.instruction("cbnz x9, __rt_pathinfo_extension");                    // compute extension from basename when requested
    emitter.instruction("and x9, x3, #8");                                      // does the bitmask request PATHINFO_FILENAME last?
    emitter.instruction("cbnz x9, __rt_pathinfo_filename");                     // compute filename when requested
    emitter.label("__rt_pathinfo_empty");
    emitter.instruction("mov x1, #0");                                          // return empty pointer
    emitter.instruction("mov x2, #0");                                          // return empty length
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_dirname");
    emitter.instruction("cbz x2, __rt_pathinfo_empty");                         // pathinfo("", PATHINFO_DIRNAME) returns "" rather than dirname("") = "."
    emitter.instruction("bl __rt_dirname");                                     // run dirname; result in x1/x2
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_basename");
    emitter.instruction("mov x3, #0");                                          // basename takes optional suffix; pass empty
    emitter.instruction("mov x4, #0");                                          // suffix length 0
    emitter.instruction("bl __rt_basename");                                    // run basename; result in x1/x2
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_extension");
    emitter.instruction("mov x3, #0");                                          // basename with empty suffix
    emitter.instruction("mov x4, #0");                                          // suffix length 0
    emitter.instruction("bl __rt_basename");                                    // x1/x2 now point at the basename slice
    // Find the last '.' in the basename slice.
    emitter.instruction("mov x5, x2");                                          // scan index = length
    emitter.label("__rt_pathinfo_ext_scan");
    emitter.instruction("cbz x5, __rt_pathinfo_ext_none");                      // no '.' encountered → empty extension
    emitter.instruction("sub x9, x5, #1");                                      // candidate index
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load candidate byte
    emitter.instruction("cmp w10, #0x2E");                                      // is it '.'?
    emitter.instruction("b.eq __rt_pathinfo_ext_found");                        // located the dot
    emitter.instruction("sub x5, x5, #1");                                      // step left
    emitter.instruction("b __rt_pathinfo_ext_scan");                            // continue scanning

    emitter.label("__rt_pathinfo_ext_found");
    // Dot at index x5-1. PHP treats leading-dot names as having an extension,
    // but trailing-dot names have an empty extension key in the array form.
    emitter.instruction("add x1, x1, x5");                                      // skip past the dot
    emitter.instruction("sub x2, x2, x5");                                      // remaining bytes form the extension
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_ext_none");
    emitter.instruction("mov x1, #0");                                          // empty extension
    emitter.instruction("mov x2, #0");                                          // empty length
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_filename");
    emitter.instruction("mov x3, #0");                                          // basename with empty suffix
    emitter.instruction("mov x4, #0");                                          // suffix length 0
    emitter.instruction("bl __rt_basename");                                    // x1/x2 = basename slice
    // Find the last '.'; PHP trims leading-dot names to an empty filename.
    emitter.instruction("mov x5, x2");                                          // scan index = length
    emitter.label("__rt_pathinfo_filename_scan");
    emitter.instruction("cbz x5, __rt_pathinfo_done");                          // no dot found → keep the full basename
    emitter.instruction("sub x9, x5, #1");                                      // candidate index
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load candidate byte
    emitter.instruction("cmp w10, #0x2E");                                      // is it '.'?
    emitter.instruction("b.eq __rt_pathinfo_filename_trim");                    // located the trimming dot
    emitter.instruction("sub x5, x5, #1");                                      // step left
    emitter.instruction("b __rt_pathinfo_filename_scan");                       // continue scanning

    emitter.label("__rt_pathinfo_filename_trim");
    emitter.instruction("sub x2, x5, #1");                                      // length becomes everything before the last dot
    // x1 unchanged: filename starts at the same position as the basename.

    emitter.label("__rt_pathinfo_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return component slice in x1/x2
}

fn emit_pathinfo_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pathinfo (single-flag form) ---");
    emitter.label_global("__rt_pathinfo_str");

    // ABI: rax=path_ptr, rdx=path_len, rdi=flag → rax/rdx=result

    emitter.instruction("push rbp");                                            // preserve caller frame pointer while pathinfo dispatches
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base

    emitter.instruction("cmp rdi, 15");                                         // dynamic PATHINFO_ALL cannot be returned by the string helper
    emitter.instruction("je __rt_pathinfo_empty_x86");                          // fail closed instead of returning a misleading component string
    emitter.instruction("test rdi, 1");                                         // does the bitmask request PATHINFO_DIRNAME first?
    emitter.instruction("jnz __rt_pathinfo_dirname_x86");                       // delegate to dirname when dirname is present
    emitter.instruction("test rdi, 2");                                         // does the bitmask request PATHINFO_BASENAME next?
    emitter.instruction("jnz __rt_pathinfo_basename_x86");                      // delegate to basename when basename is present
    emitter.instruction("test rdi, 4");                                         // does the bitmask request PATHINFO_EXTENSION next?
    emitter.instruction("jnz __rt_pathinfo_extension_x86");                     // compute extension when requested
    emitter.instruction("test rdi, 8");                                         // does the bitmask request PATHINFO_FILENAME last?
    emitter.instruction("jnz __rt_pathinfo_filename_x86");                      // compute filename when requested
    emitter.label("__rt_pathinfo_empty_x86");
    emitter.instruction("xor eax, eax");                                        // unsupported flag → empty pointer
    emitter.instruction("xor edx, edx");                                        // unsupported flag → empty length
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return empty string

    emitter.label("__rt_pathinfo_dirname_x86");
    emitter.instruction("test rdx, rdx");                                       // pathinfo("", PATHINFO_DIRNAME) returns "" rather than dirname("") = "."
    emitter.instruction("jz __rt_pathinfo_empty_x86");                          // preserve PHP's pathinfo-specific empty-path rule
    emitter.instruction("call __rt_dirname");                                   // dirname; result in rax/rdx
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return result

    emitter.label("__rt_pathinfo_basename_x86");
    emitter.instruction("xor edi, edi");                                        // basename suffix pointer = 0
    emitter.instruction("xor esi, esi");                                        // basename suffix length = 0
    emitter.instruction("call __rt_basename");                                  // basename; result in rax/rdx
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return result

    emitter.label("__rt_pathinfo_extension_x86");
    emitter.instruction("xor edi, edi");                                        // basename suffix pointer = 0
    emitter.instruction("xor esi, esi");                                        // basename suffix length = 0
    emitter.instruction("call __rt_basename");                                  // basename; result in rax/rdx
    emitter.instruction("mov r8, rdx");                                         // r8 = scan index = basename length
    emitter.label("__rt_pathinfo_ext_scan_x86");
    emitter.instruction("test r8, r8");                                         // exhausted basename without finding '.'?
    emitter.instruction("jz __rt_pathinfo_ext_none_x86");                       // → empty extension
    emitter.instruction("mov r9, r8");                                          // candidate index = r8 - 1
    emitter.instruction("sub r9, 1");                                           // step left
    emitter.instruction("movzx ecx, BYTE PTR [rax + r9]");                      // load candidate byte
    emitter.instruction("cmp cl, 0x2E");                                        // is it '.'?
    emitter.instruction("je __rt_pathinfo_ext_found_x86");                      // located the dot
    emitter.instruction("sub r8, 1");                                           // step left
    emitter.instruction("jmp __rt_pathinfo_ext_scan_x86");                      // continue scanning
    emitter.label("__rt_pathinfo_ext_found_x86");
    emitter.instruction("add rax, r8");                                         // skip past the dot
    emitter.instruction("sub rdx, r8");                                         // remaining bytes form the extension
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return extension
    emitter.label("__rt_pathinfo_ext_none_x86");
    emitter.instruction("xor eax, eax");                                        // empty extension pointer
    emitter.instruction("xor edx, edx");                                        // empty extension length
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return empty string

    emitter.label("__rt_pathinfo_filename_x86");
    emitter.instruction("xor edi, edi");                                        // basename suffix pointer = 0
    emitter.instruction("xor esi, esi");                                        // basename suffix length = 0
    emitter.instruction("call __rt_basename");                                  // basename; result in rax/rdx
    emitter.instruction("mov r8, rdx");                                         // r8 = scan index = basename length
    emitter.label("__rt_pathinfo_filename_scan_x86");
    emitter.instruction("test r8, r8");                                         // exhausted the basename without finding a dot?
    emitter.instruction("jz __rt_pathinfo_filename_done_x86");                  // no dot found → keep full basename
    emitter.instruction("mov r9, r8");                                          // candidate index = r8 - 1
    emitter.instruction("sub r9, 1");                                           // step left
    emitter.instruction("movzx ecx, BYTE PTR [rax + r9]");                      // load candidate byte
    emitter.instruction("cmp cl, 0x2E");                                        // is it '.'?
    emitter.instruction("je __rt_pathinfo_filename_trim_x86");                  // trim at this position
    emitter.instruction("sub r8, 1");                                           // step left
    emitter.instruction("jmp __rt_pathinfo_filename_scan_x86");                 // continue scanning
    emitter.label("__rt_pathinfo_filename_trim_x86");
    emitter.instruction("mov rdx, r8");                                         // length becomes everything before the dot
    emitter.instruction("sub rdx, 1");                                          // drop the dot itself
    emitter.label("__rt_pathinfo_filename_done_x86");
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return filename slice
}

/// pathinfo (no-flag form): build an associative array with the path components.
/// Input:  x1/x2 = path
/// Output: x0 = pointer to a freshly allocated hash table containing
///         "dirname" (except for empty paths), "basename", "extension" (only
///         when the basename contains a dot), and "filename".
///
/// Insertion order matches PHP: dirname → basename → extension (if present) → filename.
/// String values are persisted into owned heap storage via `__rt_str_persist`
/// before being inserted, so the hash remains valid after the path argument
/// is dropped.
pub fn emit_pathinfo_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pathinfo_array_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: pathinfo (no-flag, array form) ---");
    emitter.label_global("__rt_pathinfo_array");

    // Frame layout (80 bytes, 16-byte aligned):
    //   sp+ 0  : path_ptr
    //   sp+ 8  : path_len
    //   sp+16  : hash_ptr (saved across hash_set calls)
    //   sp+24  : value_ptr (component string after persist)
    //   sp+32  : value_len
    //   sp+40  : (padding)
    //   sp+48  : (padding)
    //   sp+56  : (padding)
    //   sp+64  : x29
    //   sp+72  : x30
    emitter.instruction("sub sp, sp, #80");                                     // allocate the pathinfo-array frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer

    // -- save the input path --
    emitter.instruction("str x1, [sp, #0]");                                    // preserve path pointer across helper calls
    emitter.instruction("str x2, [sp, #8]");                                    // preserve path length across helper calls

    // -- create the hash table (capacity 16, value_tag = 1 = Str) --
    emitter.instruction("mov x0, #16");                                         // initial capacity
    emitter.instruction("mov x1, #1");                                          // value type = Str
    emitter.instruction("bl __rt_hash_new");                                    // allocate empty hash; x0 = hash pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the hash pointer

    // -- insert "dirname" (flag = 1) --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the original path length before deciding whether dirname exists
    emitter.instruction("cbz x9, __rt_pathinfo_array_skip_dirname");            // pathinfo("") omits the dirname key
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload path pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload path length
    emitter.instruction("mov x3, #1");                                          // PATHINFO_DIRNAME
    emitter.instruction("bl __rt_pathinfo_str");                                // x1/x2 = dirname slice
    emitter.instruction("bl __rt_str_persist");                                 // copy slice into owned heap storage
    emitter.instruction("str x1, [sp, #24]");                                   // save persisted value pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save persisted value length
    emitter.adrp("x1", "_pathinfo_key_dirname");                     // load page of "dirname" literal
    emitter.add_lo12("x1", "x1", "_pathinfo_key_dirname");           // resolve full address of "dirname"
    emitter.instruction("mov x2, #7");                                          // length of "dirname"
    emitter.instruction("ldr x3, [sp, #24]");                                   // value_lo = persisted string pointer
    emitter.instruction("ldr x4, [sp, #32]");                                   // value_hi = persisted string length
    emitter.instruction("mov x5, #1");                                          // value tag = Str
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert dirname; x0 = updated hash pointer
    emitter.instruction("str x0, [sp, #16]");                                   // persist any post-grow hash pointer

    emitter.label("__rt_pathinfo_array_skip_dirname");

    // -- insert "basename" (flag = 2) --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload path pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload path length
    emitter.instruction("mov x3, #2");                                          // PATHINFO_BASENAME
    emitter.instruction("bl __rt_pathinfo_str");                                // x1/x2 = basename slice
    emitter.instruction("bl __rt_str_persist");                                 // persist basename into owned heap storage
    emitter.instruction("str x1, [sp, #24]");                                   // save persisted value pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save persisted value length
    emitter.adrp("x1", "_pathinfo_key_basename");                    // load page of "basename" literal
    emitter.add_lo12("x1", "x1", "_pathinfo_key_basename");          // resolve full address of "basename"
    emitter.instruction("mov x2, #8");                                          // length of "basename"
    emitter.instruction("ldr x3, [sp, #24]");                                   // value_lo
    emitter.instruction("ldr x4, [sp, #32]");                                   // value_hi
    emitter.instruction("mov x5, #1");                                          // value tag = Str
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert basename
    emitter.instruction("str x0, [sp, #16]");                                   // persist updated hash pointer

    // -- insert "extension" (flag = 4) only when non-empty --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload path pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload path length
    emitter.instruction("mov x3, #4");                                          // PATHINFO_EXTENSION
    emitter.instruction("bl __rt_pathinfo_str");                                // x1/x2 = extension slice (or 0/0 when absent)
    emitter.instruction("cbz x1, __rt_pathinfo_array_skip_ext");                // no dot in basename → skip the extension key
    emitter.instruction("bl __rt_str_persist");                                 // persist extension into owned heap storage
    emitter.instruction("str x1, [sp, #24]");                                   // save persisted value pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save persisted value length
    emitter.adrp("x1", "_pathinfo_key_extension");                   // load page of "extension" literal
    emitter.add_lo12("x1", "x1", "_pathinfo_key_extension");         // resolve full address of "extension"
    emitter.instruction("mov x2, #9");                                          // length of "extension"
    emitter.instruction("ldr x3, [sp, #24]");                                   // value_lo
    emitter.instruction("ldr x4, [sp, #32]");                                   // value_hi
    emitter.instruction("mov x5, #1");                                          // value tag = Str
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert extension
    emitter.instruction("str x0, [sp, #16]");                                   // persist updated hash pointer

    emitter.label("__rt_pathinfo_array_skip_ext");

    // -- insert "filename" (flag = 8) --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload path pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload path length
    emitter.instruction("mov x3, #8");                                          // PATHINFO_FILENAME
    emitter.instruction("bl __rt_pathinfo_str");                                // x1/x2 = filename slice
    emitter.instruction("bl __rt_str_persist");                                 // persist filename into owned heap storage
    emitter.instruction("str x1, [sp, #24]");                                   // save persisted value pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save persisted value length
    emitter.adrp("x1", "_pathinfo_key_filename");                    // load page of "filename" literal
    emitter.add_lo12("x1", "x1", "_pathinfo_key_filename");          // resolve full address of "filename"
    emitter.instruction("mov x2, #8");                                          // length of "filename"
    emitter.instruction("ldr x3, [sp, #24]");                                   // value_lo
    emitter.instruction("ldr x4, [sp, #32]");                                   // value_hi
    emitter.instruction("mov x5, #1");                                          // value tag = Str
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert filename
    emitter.instruction("str x0, [sp, #16]");                                   // persist updated hash pointer

    // -- return the completed hash --
    emitter.instruction("ldr x0, [sp, #16]");                                   // load final hash pointer into the result register
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the pathinfo-array frame
    emitter.instruction("ret");                                                 // return hash pointer in x0
}

fn emit_pathinfo_array_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pathinfo (no-flag, array form) ---");
    emitter.label_global("__rt_pathinfo_array");

    // ABI: rax=path_ptr, rdx=path_len → rax=hash_ptr
    //
    // Frame layout (rbp-relative):
    //   [rbp - 8]  : path_ptr
    //   [rbp - 16] : path_len
    //   [rbp - 24] : hash_ptr
    //   [rbp - 32] : value_ptr (persisted component pointer)
    //   [rbp - 40] : value_len (persisted component length)

    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the path/hash/component fields

    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save path pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save path length

    emitter.instruction("mov rdi, 16");                                         // initial capacity
    emitter.instruction("mov rsi, 1");                                          // value type = Str
    emitter.instruction("call __rt_hash_new");                                  // allocate empty hash; rax = hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save hash pointer

    // -- "dirname" (flag = 1) --
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the original path length before deciding whether dirname exists
    emitter.instruction("test r8, r8");                                         // is the original path empty?
    emitter.instruction("jz __rt_pathinfo_array_skip_dirname_x86");             // pathinfo("") omits the dirname key
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload path pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload path length
    emitter.instruction("mov rdi, 1");                                          // PATHINFO_DIRNAME
    emitter.instruction("call __rt_pathinfo_str");                              // rax/rdx = dirname slice
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = persisted dirname
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save persisted value pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save persisted value length
    emitter.instruction("lea rsi, [rip + _pathinfo_key_dirname]");              // key pointer = "dirname"
    emitter.instruction("mov rdx, 7");                                          // key length = 7
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // value_lo
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // value_hi
    emitter.instruction("mov r9, 1");                                           // value tag = Str
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // hash pointer (first __rt_hash_set arg)
    emitter.instruction("call __rt_hash_set");                                  // insert dirname; rax = updated hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist updated hash pointer

    emitter.label("__rt_pathinfo_array_skip_dirname_x86");

    // -- "basename" (flag = 2) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload path pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload path length
    emitter.instruction("mov rdi, 2");                                          // PATHINFO_BASENAME
    emitter.instruction("call __rt_pathinfo_str");                              // rax/rdx = basename slice
    emitter.instruction("call __rt_str_persist");                               // rax/rdx = persisted basename
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save persisted value pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save persisted value length
    emitter.instruction("lea rsi, [rip + _pathinfo_key_basename]");             // key = "basename"
    emitter.instruction("mov rdx, 8");                                          // key length = 8
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // value_lo
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // value_hi
    emitter.instruction("mov r9, 1");                                           // value tag = Str
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert basename
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist updated hash pointer

    // -- "extension" (flag = 4), only when non-empty --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload path pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload path length
    emitter.instruction("mov rdi, 4");                                          // PATHINFO_EXTENSION
    emitter.instruction("call __rt_pathinfo_str");                              // rax/rdx = extension slice (or 0/0)
    emitter.instruction("test rax, rax");                                       // no dot in basename?
    emitter.instruction("jz __rt_pathinfo_array_skip_ext_x86");                 // → skip the extension key
    emitter.instruction("call __rt_str_persist");                               // persist extension into owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save persisted value pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save persisted value length
    emitter.instruction("lea rsi, [rip + _pathinfo_key_extension]");            // key = "extension"
    emitter.instruction("mov rdx, 9");                                          // key length = 9
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // value_lo
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // value_hi
    emitter.instruction("mov r9, 1");                                           // value tag = Str
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert extension
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist updated hash pointer

    emitter.label("__rt_pathinfo_array_skip_ext_x86");

    // -- "filename" (flag = 8) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload path pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload path length
    emitter.instruction("mov rdi, 8");                                          // PATHINFO_FILENAME
    emitter.instruction("call __rt_pathinfo_str");                              // rax/rdx = filename slice
    emitter.instruction("call __rt_str_persist");                               // persist filename into owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save persisted value pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save persisted value length
    emitter.instruction("lea rsi, [rip + _pathinfo_key_filename]");             // key = "filename"
    emitter.instruction("mov rdx, 8");                                          // key length = 8
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // value_lo
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // value_hi
    emitter.instruction("mov r9, 1");                                           // value tag = Str
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert filename
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist updated hash pointer

    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return final hash pointer
    emitter.instruction("add rsp, 64");                                         // release the spill frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return hash pointer in rax
}
