use crate::codegen::emit::Emitter;

/// fopen: open a file and return its file descriptor.
/// Input:  x1/x2=filename string, x3/x4=mode string
/// Output: x0=file descriptor (or negative on error)
pub fn emit_fopen(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fopen ---");
    emitter.label_global("__rt_fopen");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save mode string for later parsing --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save mode ptr and len on stack

    // -- null-terminate the filename via __rt_cstr --
    emitter.instruction("bl __rt_cstr");                                        // convert filename to C string, x0=cstr path
    emitter.instruction("str x0, [sp, #0]");                                    // save null-terminated path pointer

    // -- parse mode string to derive open() flags --
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload mode ptr and len
    emitter.instruction("ldrb w9, [x3]");                                       // load first character of mode string

    // -- check for 'r' mode --
    emitter.instruction("cmp w9, #0x72");                                       // compare with 'r'
    emitter.instruction("b.ne __rt_fopen_check_w");                             // if not 'r', check for 'w'
    emitter.instruction("mov x1, #0");                                          // O_RDONLY = 0
    emitter.instruction("b __rt_fopen_check_plus");                             // proceed to check for '+' modifier

    // -- check for 'w' mode --
    emitter.label("__rt_fopen_check_w");
    emitter.instruction("cmp w9, #0x77");                                       // compare with 'w'
    emitter.instruction("b.ne __rt_fopen_check_a");                             // if not 'w', check for 'a'
    emitter.instruction("mov x1, #0x601");                                      // O_WRONLY|O_CREAT|O_TRUNC
    emitter.instruction("b __rt_fopen_check_plus");                             // proceed to check for '+' modifier

    // -- check for 'a' mode (append) --
    emitter.label("__rt_fopen_check_a");
    emitter.instruction("mov x1, #0x209");                                      // O_WRONLY|O_CREAT|O_APPEND
    // fall through to check_plus

    // -- check if second char is '+' to enable read+write --
    emitter.label("__rt_fopen_check_plus");
    emitter.instruction("cmp x4, #1");                                          // check if mode string has more than 1 char
    emitter.instruction("b.le __rt_fopen_do_open");                             // if only 1 char, skip '+' check
    emitter.instruction("ldrb w10, [x3, #1]");                                  // load second character of mode string
    emitter.instruction("cmp w10, #0x2B");                                      // compare with '+'
    emitter.instruction("b.ne __rt_fopen_do_open");                             // if not '+', keep original flags
    // -- upgrade to O_RDWR: clear O_RDONLY/O_WRONLY bits, set O_RDWR --
    emitter.instruction("and x1, x1, #0xFFFFFFFFFFFFFFFC");                     // clear lowest 2 bits (O_RDONLY/O_WRONLY)
    emitter.instruction("orr x1, x1, #0x2");                                    // set O_RDWR flag

    // -- perform the open syscall --
    emitter.label("__rt_fopen_do_open");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload null-terminated path
    emitter.instruction("mov x2, #0x1A4");                                      // file mode 0644 (octal)
    emitter.instruction("mov x16, #5");                                         // syscall 5 = open
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- check if open failed (carry flag set on macOS syscall error) --
    emitter.instruction("b.cc __rt_fopen_ok");                                  // if carry clear, syscall succeeded
    emitter.instruction("mov x0, #-1");                                         // return -1 to indicate failure

    // -- restore frame and return fd in x0 --
    emitter.label("__rt_fopen_ok");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller with fd in x0
}
