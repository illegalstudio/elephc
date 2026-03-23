use crate::codegen::emit::Emitter;

/// fgetcsv: read one line from fd and parse as CSV into an array of strings.
/// Input:  x0=fd
/// Output: x0=array pointer (array of field strings)
pub fn emit_fgetcsv(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgetcsv ---");
    emitter.label("__rt_fgetcsv");

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
    emitter.instruction("bl __rt_strcopy");                                     // copy field to concat_buf

    // -- push field to array --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push field string to array

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
    emitter.instruction("bl __rt_strcopy");                                     // copy field to concat_buf

    // -- push field to array --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push field string to array

    // -- restore parsing state and reset for next field --
    emitter.instruction("ldp x1, x3, [sp, #24]");                               // restore scan ptr and end ptr
    emitter.instruction("ldr x6, [sp, #56]");                                   // restore in_quotes flag
    emitter.instruction("mov x4, x1");                                          // next field starts at current position
    emitter.instruction("mov x5, #0");                                          // reset field length
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue parsing
}
