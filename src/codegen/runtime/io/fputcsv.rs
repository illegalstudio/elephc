use crate::codegen::emit::Emitter;

/// fputcsv: write array elements as a CSV line to a file descriptor.
/// Input:  x0=fd, x1=array_ptr (array of strings)
/// Output: x0=total bytes written
pub fn emit_fputcsv(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fputcsv ---");
    emitter.label_global("__rt_fputcsv");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #96");                                     // allocate 96 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish new frame pointer

    // -- save inputs --
    emitter.instruction("str x0, [sp, #0]");                                    // save fd
    emitter.instruction("str x1, [sp, #8]");                                    // save array pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // total bytes written = 0
    emitter.instruction("str xzr, [sp, #24]");                                  // current element index = 0

    // -- get array length --
    emitter.instruction("ldr x9, [x1]");                                        // load array length from header
    emitter.instruction("str x9, [sp, #32]");                                   // save array length

    // -- main loop: iterate over array elements --
    emitter.label("__rt_fputcsv_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // load current index
    emitter.instruction("ldr x10, [sp, #32]");                                  // load array length
    emitter.instruction("cmp x9, x10");                                         // check if we've processed all elements
    emitter.instruction("b.hs __rt_fputcsv_newline");                           // if done, write trailing newline

    // -- write comma separator before 2nd+ fields --
    emitter.instruction("cbz x9, __rt_fputcsv_field");                          // skip comma for first field
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("adrp x1, __rt_fputcsv_comma_lit@PAGE");                // load comma literal address
    emitter.instruction("add x1, x1, __rt_fputcsv_comma_lit@PAGEOFF");          // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (comma)
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload total bytes
    emitter.instruction("add x9, x9, x0");                                      // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                   // save updated total

    // -- load current field from array --
    emitter.label("__rt_fputcsv_field");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload current index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload array pointer
    emitter.instruction("lsl x11, x9, #4");                                     // byte offset = index * 16
    emitter.instruction("add x11, x10, x11");                                   // element address = array + offset
    emitter.instruction("ldr x3, [x11, #24]");                                  // load string pointer (skip 24-byte header)
    emitter.instruction("ldr x4, [x11, #32]");                                  // load string length

    // -- check if field needs quoting (contains comma, quote, or newline) --
    emitter.instruction("stp x3, x4, [sp, #40]");                               // save field ptr and len
    emitter.instruction("mov x5, #0");                                          // needs_quote flag = 0
    emitter.instruction("mov x6, #0");                                          // scan index = 0
    emitter.label("__rt_fputcsv_scan");
    emitter.instruction("cmp x6, x4");                                          // check if scan complete
    emitter.instruction("b.hs __rt_fputcsv_write");                             // if done scanning, proceed to write
    emitter.instruction("ldrb w7, [x3, x6]");                                   // load byte at current position
    emitter.instruction("cmp w7, #0x2C");                                       // check for comma
    emitter.instruction("b.eq __rt_fputcsv_need_q");                            // comma found, needs quoting
    emitter.instruction("cmp w7, #0x22");                                       // check for double quote
    emitter.instruction("b.eq __rt_fputcsv_need_q");                            // quote found, needs quoting
    emitter.instruction("cmp w7, #0x0A");                                       // check for newline
    emitter.instruction("b.eq __rt_fputcsv_need_q");                            // newline found, needs quoting
    emitter.instruction("add x6, x6, #1");                                      // increment scan index
    emitter.instruction("b __rt_fputcsv_scan");                                 // continue scanning

    emitter.label("__rt_fputcsv_need_q");
    emitter.instruction("mov x5, #1");                                          // set needs_quote flag

    // -- write the field (quoted or unquoted) --
    emitter.label("__rt_fputcsv_write");
    emitter.instruction("ldp x3, x4, [sp, #40]");                               // reload field ptr and len
    emitter.instruction("cbz x5, __rt_fputcsv_plain");                          // if no quoting needed, write directly

    // -- write opening quote --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("adrp x1, __rt_fputcsv_quote_lit@PAGE");                // load quote literal address
    emitter.instruction("add x1, x1, __rt_fputcsv_quote_lit@PAGEOFF");          // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (quote)
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload total bytes
    emitter.instruction("add x9, x9, x0");                                      // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                   // save updated total

    // -- write field contents, escaping internal quotes --
    emitter.instruction("ldp x3, x4, [sp, #40]");                               // reload field ptr and len
    emitter.instruction("mov x6, #0");                                          // byte index = 0
    emitter.label("__rt_fputcsv_qloop");
    emitter.instruction("cmp x6, x4");                                          // check if all bytes written
    emitter.instruction("b.hs __rt_fputcsv_close_q");                           // if done, write closing quote
    emitter.instruction("ldrb w7, [x3, x6]");                                   // load current byte
    emitter.instruction("add x6, x6, #1");                                      // advance index
    emitter.instruction("str x6, [sp, #56]");                                   // save current index
    emitter.instruction("cmp w7, #0x22");                                       // check if byte is a quote
    emitter.instruction("b.ne __rt_fputcsv_qchar");                             // if not quote, write normally

    // -- escape quote by writing two quotes --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("adrp x1, __rt_fputcsv_quote_lit@PAGE");                // load quote literal address
    emitter.instruction("add x1, x1, __rt_fputcsv_quote_lit@PAGEOFF");          // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (escape quote)
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload total bytes
    emitter.instruction("add x9, x9, x0");                                      // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                   // save updated total

    // -- write the actual character --
    emitter.label("__rt_fputcsv_qchar");
    emitter.instruction("ldp x3, x4, [sp, #40]");                               // reload field ptr and len
    emitter.instruction("ldr x6, [sp, #56]");                                   // reload byte index
    emitter.instruction("sub x9, x6, #1");                                      // index of byte to write
    emitter.instruction("add x1, x3, x9");                                      // pointer to the byte
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("mov x2, #1");                                          // write 1 byte
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload total bytes
    emitter.instruction("add x9, x9, x0");                                      // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                   // save updated total
    emitter.instruction("ldr x6, [sp, #56]");                                   // reload byte index
    emitter.instruction("ldp x3, x4, [sp, #40]");                               // reload field ptr and len
    emitter.instruction("b __rt_fputcsv_qloop");                                // continue writing

    // -- write closing quote --
    emitter.label("__rt_fputcsv_close_q");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("adrp x1, __rt_fputcsv_quote_lit@PAGE");                // load quote literal address
    emitter.instruction("add x1, x1, __rt_fputcsv_quote_lit@PAGEOFF");          // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (quote)
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload total bytes
    emitter.instruction("add x9, x9, x0");                                      // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                   // save updated total
    emitter.instruction("b __rt_fputcsv_next");                                 // proceed to next field

    // -- write plain field (no quoting needed) --
    emitter.label("__rt_fputcsv_plain");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("mov x1, x3");                                          // field pointer
    emitter.instruction("mov x2, x4");                                          // field length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload total bytes
    emitter.instruction("add x9, x9, x0");                                      // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                   // save updated total

    // -- advance to next element --
    emitter.label("__rt_fputcsv_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload current index
    emitter.instruction("add x9, x9, #1");                                      // increment index
    emitter.instruction("str x9, [sp, #24]");                                   // save updated index
    emitter.instruction("b __rt_fputcsv_loop");                                 // continue loop

    // -- write trailing newline --
    emitter.label("__rt_fputcsv_newline");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("adrp x1, __rt_fputcsv_nl_lit@PAGE");                   // load newline literal address
    emitter.instruction("add x1, x1, __rt_fputcsv_nl_lit@PAGEOFF");             // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (newline)
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload total bytes
    emitter.instruction("add x9, x9, x0");                                      // add final bytes written
    emitter.instruction("str x9, [sp, #16]");                                   // save final total

    // -- return total bytes written --
    emitter.instruction("ldr x0, [sp, #16]");                                   // return total bytes written

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- literal data for comma, quote, and newline characters --
    emitter.label("__rt_fputcsv_comma_lit");
    emitter.instruction(".ascii \",\"");                                        // comma character literal
    emitter.label("__rt_fputcsv_quote_lit");
    emitter.instruction(".ascii \"\\\"\"");                                     // double quote character literal
    emitter.label("__rt_fputcsv_nl_lit");
    emitter.instruction(".ascii \"\\n\"");                                      // newline character literal
}
