use crate::codegen::{emit::Emitter, platform::Arch};

/// fputcsv: write array elements as a CSV line to a file descriptor.
/// Input:  x0=fd, x1=array_ptr (array of strings)
/// Output: x0=total bytes written
pub fn emit_fputcsv(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fputcsv_linux_x86_64(emitter);
        return;
    }

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
    emitter.adrp("x1", "__rt_fputcsv_comma_lit");                // load comma literal address
    emitter.add_lo12("x1", "x1", "__rt_fputcsv_comma_lit");          // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (comma)
    emitter.syscall(4);
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
    emitter.adrp("x1", "__rt_fputcsv_quote_lit");                // load quote literal address
    emitter.add_lo12("x1", "x1", "__rt_fputcsv_quote_lit");          // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (quote)
    emitter.syscall(4);
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
    emitter.adrp("x1", "__rt_fputcsv_quote_lit");                // load quote literal address
    emitter.add_lo12("x1", "x1", "__rt_fputcsv_quote_lit");          // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (escape quote)
    emitter.syscall(4);
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
    emitter.syscall(4);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload total bytes
    emitter.instruction("add x9, x9, x0");                                      // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                   // save updated total
    emitter.instruction("ldr x6, [sp, #56]");                                   // reload byte index
    emitter.instruction("ldp x3, x4, [sp, #40]");                               // reload field ptr and len
    emitter.instruction("b __rt_fputcsv_qloop");                                // continue writing

    // -- write closing quote --
    emitter.label("__rt_fputcsv_close_q");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.adrp("x1", "__rt_fputcsv_quote_lit");                // load quote literal address
    emitter.add_lo12("x1", "x1", "__rt_fputcsv_quote_lit");          // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (quote)
    emitter.syscall(4);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload total bytes
    emitter.instruction("add x9, x9, x0");                                      // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                   // save updated total
    emitter.instruction("b __rt_fputcsv_next");                                 // proceed to next field

    // -- write plain field (no quoting needed) --
    emitter.label("__rt_fputcsv_plain");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("mov x1, x3");                                          // field pointer
    emitter.instruction("mov x2, x4");                                          // field length
    emitter.syscall(4);
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
    emitter.adrp("x1", "__rt_fputcsv_nl_lit");                   // load newline literal address
    emitter.add_lo12("x1", "x1", "__rt_fputcsv_nl_lit");             // resolve exact address
    emitter.instruction("mov x2, #1");                                          // write 1 byte (newline)
    emitter.syscall(4);
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

fn emit_fputcsv_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fputcsv ---");
    emitter.label_global("__rt_fputcsv");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fputcsv() keeps stream and field state in stack slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the file descriptor, array pointer, and CSV writer bookkeeping
    emitter.instruction("sub rsp, 80");                                         // reserve aligned stack space for the CSV writer state across repeated libc write() calls
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the destination file descriptor across all field-scan and write helper steps
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the source string-array pointer across repeated field loads
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // total written bytes start at zero before any CSV separator or field bytes are emitted
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // current field index starts at zero before iterating the source array
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source string-array logical length before entering the CSV writer loop
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the source string-array length for the loop termination check

    emitter.label("__rt_fputcsv_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current field index before checking loop completion
    emitter.instruction("cmp r10, QWORD PTR [rbp - 40]");                       // have we already emitted every field from the source string array?
    emitter.instruction("jae __rt_fputcsv_newline_x86");                        // write the trailing newline once every field has been emitted
    emitter.instruction("test r10, r10");                                       // is the current field index zero, meaning this is the first CSV field?
    emitter.instruction("jz __rt_fputcsv_field_x86");                           // skip the comma separator before the first field
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the destination file descriptor as the first libc write() argument for the comma separator
    emitter.instruction("lea rsi, [rip + __rt_fputcsv_comma_lit]");             // pass the comma literal address as the second libc write() argument
    emitter.instruction("mov edx, 1");                                          // write exactly one comma byte between consecutive CSV fields
    emitter.instruction("call write");                                          // emit the comma separator through libc write()
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // accumulate the comma byte count into the running CSV write total

    emitter.label("__rt_fputcsv_field_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current field index before loading the next string slot from the array
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the source string-array pointer before computing the current field slot address
    emitter.instruction("mov rcx, r10");                                        // copy the field index before scaling it into the 16-byte string-slot offset
    emitter.instruction("shl rcx, 4");                                          // convert the field index into the byte offset of the current 16-byte string slot
    emitter.instruction("lea rcx, [r11 + rcx + 24]");                           // compute the current string-slot address inside the source array payload region
    emitter.instruction("mov r8, QWORD PTR [rcx]");                             // load the current field string pointer from the source array slot
    emitter.instruction("mov r9, QWORD PTR [rcx + 8]");                         // load the current field string length from the source array slot
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // preserve the current field string pointer across the quote scan and repeated write() calls
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // preserve the current field string length across the quote scan and repeated write() calls
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // needs_quote starts false before scanning the current field payload
    emitter.instruction("xor ecx, ecx");                                        // start scanning the current field payload from byte index zero

    emitter.label("__rt_fputcsv_scan_x86");
    emitter.instruction("cmp rcx, r9");                                         // have we scanned every byte of the current field payload?
    emitter.instruction("jae __rt_fputcsv_write_x86");                          // proceed to field emission once the quote scan reaches the end of the payload
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load the current field byte while deciding whether CSV quoting is required
    emitter.instruction("cmp dl, 0x2C");                                        // does the current field byte contain a comma separator?
    emitter.instruction("je __rt_fputcsv_need_q_x86");                          // quote the field when it contains a comma byte
    emitter.instruction("cmp dl, 0x22");                                        // does the current field byte contain a double quote?
    emitter.instruction("je __rt_fputcsv_need_q_x86");                          // quote the field when it contains a literal quote byte
    emitter.instruction("cmp dl, 0x0A");                                        // does the current field byte contain a newline?
    emitter.instruction("je __rt_fputcsv_need_q_x86");                          // quote the field when it contains a newline byte
    emitter.instruction("add rcx, 1");                                          // advance to the next field byte while scanning for CSV quote triggers
    emitter.instruction("jmp __rt_fputcsv_scan_x86");                           // continue scanning the field payload for quote-triggering bytes

    emitter.label("__rt_fputcsv_need_q_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], 1");                         // remember that the current field must be emitted inside CSV quotes

    emitter.label("__rt_fputcsv_write_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // does the current field require CSV quoting based on the scan result?
    emitter.instruction("je __rt_fputcsv_plain_x86");                           // write the field directly when no quotes or separators were found
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the destination file descriptor as the first libc write() argument for the opening quote
    emitter.instruction("lea rsi, [rip + __rt_fputcsv_quote_lit]");             // pass the quote literal address as the second libc write() argument
    emitter.instruction("mov edx, 1");                                          // write exactly one opening quote byte before the field payload
    emitter.instruction("call write");                                          // emit the opening quote through libc write()
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // accumulate the opening-quote byte count into the running CSV write total
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // current byte index inside the quoted field starts at zero before the per-byte writer loop

    emitter.label("__rt_fputcsv_qloop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // reload the current byte index before checking whether the quoted field payload is finished
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 56]");                       // have we emitted every byte from the quoted field payload?
    emitter.instruction("jae __rt_fputcsv_close_q_x86");                        // write the closing quote once all quoted field bytes have been emitted
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // reload the current field string pointer before fetching the next quoted field byte
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load the current field byte while deciding whether it must be escaped as \"\"
    emitter.instruction("cmp dl, 0x22");                                        // is the current field byte itself a double quote that must be escaped in CSV output?
    emitter.instruction("jne __rt_fputcsv_qchar_x86");                          // skip the escape-prefix write when the current byte is not a quote
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the destination file descriptor as the first libc write() argument for the escaped quote prefix
    emitter.instruction("lea rsi, [rip + __rt_fputcsv_quote_lit]");             // pass the quote literal address as the second libc write() argument for the escaped quote prefix
    emitter.instruction("mov edx, 1");                                          // write the extra quote byte that escapes a literal quote in CSV output
    emitter.instruction("call write");                                          // emit the escape-prefix quote through libc write()
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // accumulate the escape-prefix byte count into the running CSV write total

    emitter.label("__rt_fputcsv_qchar_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the destination file descriptor as the first libc write() argument for the current field byte
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // reload the current field string pointer before writing the current field byte
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                       // reload the current byte index before computing the source pointer of the current field byte
    emitter.instruction("lea rsi, [r8 + rcx]");                                 // point libc write() at the current field byte inside the source string payload
    emitter.instruction("mov edx, 1");                                          // write exactly one payload byte from the quoted field
    emitter.instruction("call write");                                          // emit the current field byte through libc write()
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // accumulate the current field-byte count into the running CSV write total
    emitter.instruction("add QWORD PTR [rbp - 72], 1");                         // advance to the next byte inside the quoted field payload
    emitter.instruction("jmp __rt_fputcsv_qloop_x86");                          // continue emitting the quoted field payload byte-by-byte

    emitter.label("__rt_fputcsv_close_q_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the destination file descriptor as the first libc write() argument for the closing quote
    emitter.instruction("lea rsi, [rip + __rt_fputcsv_quote_lit]");             // pass the quote literal address as the second libc write() argument for the closing quote
    emitter.instruction("mov edx, 1");                                          // write exactly one closing quote byte after the quoted field payload
    emitter.instruction("call write");                                          // emit the closing quote through libc write()
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // accumulate the closing-quote byte count into the running CSV write total
    emitter.instruction("jmp __rt_fputcsv_next_x86");                           // advance to the next field after finishing the quoted field emission

    emitter.label("__rt_fputcsv_plain_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the destination file descriptor as the first libc write() argument for the plain field path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // pass the current field string pointer as the second libc write() argument for the plain field path
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // pass the current field string length as the third libc write() argument for the plain field path
    emitter.instruction("call write");                                          // emit the entire unquoted field payload through one libc write() call
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // accumulate the plain field byte count into the running CSV write total

    emitter.label("__rt_fputcsv_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next field index before looping back to the CSV field iterator
    emitter.instruction("jmp __rt_fputcsv_loop_x86");                           // continue emitting the remaining CSV fields from the source string array

    emitter.label("__rt_fputcsv_newline_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the destination file descriptor as the first libc write() argument for the trailing newline
    emitter.instruction("lea rsi, [rip + __rt_fputcsv_nl_lit]");                // pass the newline literal address as the second libc write() argument
    emitter.instruction("mov edx, 1");                                          // write exactly one trailing newline byte after the last CSV field
    emitter.instruction("call write");                                          // emit the trailing newline through libc write()
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // accumulate the trailing newline byte count into the running CSV write total
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the total number of bytes that fputcsv() emitted through libc write()
    emitter.instruction("add rsp, 80");                                         // release the CSV writer spill slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the x86_64 CSV writer completes
    emitter.instruction("ret");                                                 // return the total written byte count in the x86_64 integer result register

    emitter.label("__rt_fputcsv_comma_lit");
    emitter.instruction(".ascii \",\"");                                        // comma character literal used as the CSV field separator
    emitter.label("__rt_fputcsv_quote_lit");
    emitter.instruction(".ascii \"\\\"\"");                                     // double quote character literal used for CSV quoting and escaping
    emitter.label("__rt_fputcsv_nl_lit");
    emitter.instruction(".ascii \"\\n\"");                                      // trailing newline character literal written after the final CSV field
}
