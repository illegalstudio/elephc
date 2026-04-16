use crate::codegen::{emit::Emitter, platform::Arch};

/// fgetcsv: read one line from fd and parse as CSV into an array of strings.
/// Input:  x0=fd
/// Output: x0=array pointer (array of field strings)
pub fn emit_fgetcsv(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fgetcsv_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fgetcsv ---");
    emitter.label_global("__rt_fgetcsv");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer

    // -- read one line from fd --
    emitter.instruction("bl __rt_fgets");                                       // read line, x1=ptr, x2=len
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save line ptr and len

    // -- create a new string array (capacity = 64 fields) --
    emitter.instruction("mov x0, #64");                                         // initial capacity of 64 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // create array, x0=array pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save array pointer

    // -- set up CSV parsing state --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload line ptr and len
    emitter.instruction("add x3, x1, x2");                                      // x3 = end pointer (ptr + len)
    emitter.instruction("mov x4, x1");                                          // x4 = current field start
    emitter.instruction("mov x5, #0");                                          // x5 = current field length
    emitter.instruction("mov x6, #0");                                          // x6 = in_quotes flag (0=no, 1=yes)

    // -- main parsing loop --
    emitter.label("__rt_fgetcsv_loop");
    emitter.instruction("cmp x1, x3");                                          // check if at end of line
    emitter.instruction("b.hs __rt_fgetcsv_push_last");                         // if at end, push final field

    // -- load current character --
    emitter.instruction("ldrb w7, [x1], #1");                                   // load byte and advance pointer

    // -- strip trailing \n or \r --
    emitter.instruction("cmp w7, #0x0A");                                       // check for newline
    emitter.instruction("b.eq __rt_fgetcsv_check_end");                         // handle newline
    emitter.instruction("cmp w7, #0x0D");                                       // check for carriage return
    emitter.instruction("b.eq __rt_fgetcsv_check_end");                         // handle carriage return

    // -- check if inside quotes --
    emitter.instruction("cbnz x6, __rt_fgetcsv_inquote");                       // if in quotes, handle quoted context

    // -- unquoted context: check for comma or opening quote --
    emitter.instruction("cmp w7, #0x2C");                                       // check for comma
    emitter.instruction("b.eq __rt_fgetcsv_comma");                             // if comma, push current field
    emitter.instruction("cmp w7, #0x22");                                       // check for double quote
    emitter.instruction("b.eq __rt_fgetcsv_open_q");                            // if quote, enter quoted mode

    // -- regular character in unquoted field --
    emitter.instruction("add x5, x5, #1");                                      // increment field length
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue parsing

    // -- opening quote: enter quoted mode --
    emitter.label("__rt_fgetcsv_open_q");
    emitter.instruction("mov x6, #1");                                          // set in_quotes flag
    emitter.instruction("mov x4, x1");                                          // field starts after opening quote
    emitter.instruction("mov x5, #0");                                          // reset field length
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue parsing

    // -- inside quotes: check for closing quote --
    emitter.label("__rt_fgetcsv_inquote");
    emitter.instruction("cmp w7, #0x22");                                       // check for double quote
    emitter.instruction("b.eq __rt_fgetcsv_maybe_close");                       // might be closing or escaped quote
    emitter.instruction("add x5, x5, #1");                                      // regular char, increment length
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue parsing

    // -- double quote inside quotes: check if escaped ("") or closing --
    emitter.label("__rt_fgetcsv_maybe_close");
    emitter.instruction("cmp x1, x3");                                          // check if at end of line
    emitter.instruction("b.hs __rt_fgetcsv_close_q");                           // if at end, it's a closing quote
    emitter.instruction("ldrb w8, [x1]");                                       // peek at next character
    emitter.instruction("cmp w8, #0x22");                                       // check if next is also quote (escaped)
    emitter.instruction("b.ne __rt_fgetcsv_close_q");                           // if not, it's a closing quote

    // -- escaped quote: skip the second quote, count one char --
    emitter.instruction("add x1, x1, #1");                                      // skip the second quote
    emitter.instruction("add x5, x5, #1");                                      // count one quote character
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue parsing

    // -- closing quote: exit quoted mode --
    emitter.label("__rt_fgetcsv_close_q");
    emitter.instruction("mov x6, #0");                                          // clear in_quotes flag
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue parsing

    // -- newline/carriage return at end: treat like end of line --
    emitter.label("__rt_fgetcsv_check_end");
    emitter.instruction("cmp x1, x3");                                          // check if this is the last char
    emitter.instruction("b.lo __rt_fgetcsv_loop");                              // if more data follows, skip and continue
    // fall through to push_last

    // -- push the last field --
    emitter.label("__rt_fgetcsv_push_last");
    // -- save parsing state --
    emitter.instruction("stp x1, x3, [sp, #24]");                               // save scan ptr and end ptr
    emitter.instruction("stp x4, x5, [sp, #40]");                               // save field start and length

    // -- copy field to concat_buf --
    emitter.instruction("mov x1, x4");                                          // field start pointer
    emitter.instruction("mov x2, x5");                                          // field length
    emitter.instruction("bl __rt_str_persist");                                 // copy field to heap

    // -- push field to array --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push field string to array
    emitter.instruction("str x0, [sp, #16]");                                   // update array pointer after possible realloc

    // -- return array --
    emitter.instruction("ldr x0, [sp, #16]");                                   // return array pointer

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- comma found: push current field and start new one --
    emitter.label("__rt_fgetcsv_comma");

    // -- save parsing state --
    emitter.instruction("stp x1, x3, [sp, #24]");                               // save scan ptr and end ptr
    emitter.instruction("stp x4, x5, [sp, #40]");                               // save field start and length
    emitter.instruction("str x6, [sp, #56]");                                   // save in_quotes flag

    // -- copy field to concat_buf --
    emitter.instruction("mov x1, x4");                                          // field start pointer
    emitter.instruction("mov x2, x5");                                          // field length
    emitter.instruction("bl __rt_str_persist");                                 // copy field to heap

    // -- push field to array --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push field string to array
    emitter.instruction("str x0, [sp, #16]");                                   // update array pointer after possible realloc

    // -- restore parsing state and reset for next field --
    emitter.instruction("ldp x1, x3, [sp, #24]");                               // restore scan ptr and end ptr
    emitter.instruction("ldr x6, [sp, #56]");                                   // restore in_quotes flag
    emitter.instruction("mov x4, x1");                                          // next field starts at current position
    emitter.instruction("mov x5, #0");                                          // reset field length
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue parsing
}

fn emit_fgetcsv_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgetcsv ---");
    emitter.label_global("__rt_fgetcsv");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fgetcsv() keeps parsing state in stack slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved line metadata and CSV parser state
    emitter.instruction("sub rsp, 80");                                         // reserve aligned stack space for the line slice, result array pointer, and CSV parser bookkeeping

    emitter.instruction("call __rt_fgets");                                     // read one line from the stream helper and return it as a borrowed string slice in rax/rdx
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the borrowed line pointer across array allocation and field-persist helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the borrowed line length across array allocation and field-persist helper calls

    emitter.instruction("mov edi, 64");                                         // request an initial indexed-array capacity of 64 CSV fields
    emitter.instruction("mov esi, 16");                                         // request 16-byte payload slots because fgetcsv() stores string ptr/len pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the result string array that will receive each parsed CSV field
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the result array pointer across the parser loop and helper calls

    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // current scan pointer starts at the beginning of the borrowed line slice
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the borrowed line length so the end pointer can be computed once
    emitter.instruction("add r8, rcx");                                         // compute the one-past-end pointer of the borrowed line slice for parser bounds checks
    emitter.instruction("mov r9, rcx");                                         // the first CSV field initially starts at the beginning of the borrowed line slice
    emitter.instruction("xor r10d, r10d");                                      // current field length starts at zero bytes before scanning any CSV payload
    emitter.instruction("xor r11d, r11d");                                      // in_quotes flag starts false before parsing the first CSV field

    emitter.label("__rt_fgetcsv_loop_x86");
    emitter.instruction("cmp rcx, r8");                                         // has the CSV scan cursor reached the end of the borrowed line slice?
    emitter.instruction("jae __rt_fgetcsv_push_last_x86");                      // push the final field once the scan cursor reaches the line end
    emitter.instruction("movzx eax, BYTE PTR [rcx]");                           // load the current CSV byte before dispatching on commas, quotes, and line terminators
    emitter.instruction("add rcx, 1");                                          // advance the CSV scan cursor after consuming the current byte
    emitter.instruction("cmp al, 0x0A");                                        // did we just consume a newline byte from the borrowed line slice?
    emitter.instruction("je __rt_fgetcsv_check_end_x86");                       // treat a terminal newline as end-of-line instead of field payload
    emitter.instruction("cmp al, 0x0D");                                        // did we just consume a carriage return byte from the borrowed line slice?
    emitter.instruction("je __rt_fgetcsv_check_end_x86");                       // treat a terminal carriage return as end-of-line instead of field payload
    emitter.instruction("test r11, r11");                                       // are we currently inside a quoted CSV field?
    emitter.instruction("jne __rt_fgetcsv_inquote_x86");                        // dispatch to the quoted-field parser when the in_quotes flag is set
    emitter.instruction("cmp al, 0x2C");                                        // does the current byte terminate the current unquoted CSV field with a comma?
    emitter.instruction("je __rt_fgetcsv_comma_x86");                           // push the current field and start the next one when a comma is found
    emitter.instruction("cmp al, 0x22");                                        // does the current byte open a quoted CSV field?
    emitter.instruction("je __rt_fgetcsv_open_q_x86");                          // switch to quoted-field parsing when an opening quote is found
    emitter.instruction("add r10, 1");                                          // count a regular unquoted CSV payload byte toward the current field length
    emitter.instruction("jmp __rt_fgetcsv_loop_x86");                           // continue parsing the remaining bytes of the borrowed line slice

    emitter.label("__rt_fgetcsv_open_q_x86");
    emitter.instruction("mov r11, 1");                                          // enter quoted-field parsing after consuming the opening quote
    emitter.instruction("mov r9, rcx");                                         // start the quoted field immediately after the opening quote byte
    emitter.instruction("xor r10d, r10d");                                      // reset the current field length so only quoted payload bytes are counted
    emitter.instruction("jmp __rt_fgetcsv_loop_x86");                           // continue parsing inside the quoted field

    emitter.label("__rt_fgetcsv_inquote_x86");
    emitter.instruction("cmp al, 0x22");                                        // did we just consume a double quote while parsing a quoted field?
    emitter.instruction("je __rt_fgetcsv_maybe_close_x86");                     // distinguish escaped quotes from closing quotes before continuing
    emitter.instruction("add r10, 1");                                          // count a regular quoted CSV payload byte toward the current field length
    emitter.instruction("jmp __rt_fgetcsv_loop_x86");                           // continue parsing the quoted field payload

    emitter.label("__rt_fgetcsv_maybe_close_x86");
    emitter.instruction("cmp rcx, r8");                                         // are we at the end of the borrowed line slice right after the consumed quote?
    emitter.instruction("jae __rt_fgetcsv_close_q_x86");                        // treat the quote as closing when there is no following byte to inspect
    emitter.instruction("movzx edx, BYTE PTR [rcx]");                           // peek at the next byte to detect the escaped-quote CSV sequence \"\"
    emitter.instruction("cmp dl, 0x22");                                        // is the next byte another quote, meaning the current quote was escaped?
    emitter.instruction("jne __rt_fgetcsv_close_q_x86");                        // treat the current quote as closing when the next byte is not another quote
    emitter.instruction("add rcx, 1");                                          // skip the second quote of the escaped-quote CSV sequence
    emitter.instruction("add r10, 1");                                          // count the escaped quote as one logical payload byte in the current field length
    emitter.instruction("jmp __rt_fgetcsv_loop_x86");                           // continue parsing after the escaped-quote sequence

    emitter.label("__rt_fgetcsv_close_q_x86");
    emitter.instruction("xor r11d, r11d");                                      // leave quoted-field parsing after consuming the closing quote
    emitter.instruction("jmp __rt_fgetcsv_loop_x86");                           // continue parsing the remainder of the borrowed line slice

    emitter.label("__rt_fgetcsv_check_end_x86");
    emitter.instruction("cmp rcx, r8");                                         // did the newline or carriage return terminate the borrowed line slice?
    emitter.instruction("jb __rt_fgetcsv_loop_x86");                            // ignore embedded line terminators when more bytes still follow in the borrowed line slice

    emitter.label("__rt_fgetcsv_push_last_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the current scan cursor across the field-persist and array-push helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // preserve the borrowed line end pointer across the field-persist and array-push helper calls
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve the current field start pointer across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // preserve the current field length across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // preserve the quoted-field flag across the helper calls
    emitter.instruction("mov rax, r9");                                         // move the current field pointer into the x86_64 string-persist input register
    emitter.instruction("mov rdx, r10");                                        // move the current field length into the x86_64 string-persist length register
    emitter.instruction("call __rt_str_persist");                               // duplicate the current CSV field into owned heap storage before storing it in the result array
    emitter.instruction("mov rsi, rax");                                        // move the owned CSV field pointer into the x86_64 array-push string payload register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result array pointer before appending the owned CSV field string
    emitter.instruction("call __rt_array_push_str");                            // append the current owned CSV field into the result string array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the possibly grown result array pointer after appending the current field
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the result string array once the final field has been appended
    emitter.instruction("add rsp, 80");                                         // release the CSV parser spill slots before returning the result array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the x86_64 CSV parser completes
    emitter.instruction("ret");                                                 // return the parsed CSV row as an indexed array of owned strings

    emitter.label("__rt_fgetcsv_comma_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the current scan cursor across the field-persist and array-push helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // preserve the borrowed line end pointer across the field-persist and array-push helper calls
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve the current field start pointer across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // preserve the current field length across the helper calls
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // preserve the quoted-field flag across the helper calls
    emitter.instruction("mov rax, r9");                                         // move the current field pointer into the x86_64 string-persist input register
    emitter.instruction("mov rdx, r10");                                        // move the current field length into the x86_64 string-persist length register
    emitter.instruction("call __rt_str_persist");                               // duplicate the current CSV field into owned heap storage before storing it in the result array
    emitter.instruction("mov rsi, rax");                                        // move the owned CSV field pointer into the x86_64 array-push string payload register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result array pointer before appending the owned CSV field string
    emitter.instruction("call __rt_array_push_str");                            // append the current owned CSV field into the result string array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the possibly grown result array pointer after appending the current field
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // restore the scan cursor so CSV parsing can continue after the comma separator
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // restore the borrowed line end pointer after the helper calls
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // restore the quoted-field flag after the helper calls
    emitter.instruction("mov r9, rcx");                                         // start the next CSV field at the current scan cursor right after the comma
    emitter.instruction("xor r10d, r10d");                                      // reset the current field length to zero for the next CSV field
    emitter.instruction("jmp __rt_fgetcsv_loop_x86");                           // continue parsing the remaining CSV payload after the comma separator
}
