//! Purpose:
//! Emits the x86_64 public entry boundary and recursive serialized-value decoder.
//!
//! Called from:
//! - `super::emit_unserialize()` around the x86_64 preflight validator.
//!
//! Key details:
//! - The entry owns throw-safe cleanup; the decoder assumes the allocation-free preflight succeeded.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

/// Emits the x86_64 public unserialize entry and its exception-cleanup boundary.
pub(super) fn emit_entry(emitter: &mut Emitter) {
    let boundary_bytes = TRY_HANDLER_SLOT_SIZE + 32;
    let previous_handler_offset = boundary_bytes;
    let survivor_offset = previous_handler_offset - 8;

    // -- entry wrapper: protect begin/end cleanup across hydration-hook throws --
    emitter.blank();
    emitter.comment("--- runtime: unserialize_mixed (serialize() wire -> boxed Mixed) ---");
    emitter.label_global("__rt_unserialize_mixed");
    emitter.instruction("push rbp");                                            // preserve the caller frame across the exception boundary
    emitter.instruction("mov rbp, rsp");                                        // establish a stable base for the complete handler record
    emitter.instruction(&format!("sub rsp, {}", boundary_bytes));               // reserve the handler record plus source/result spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve source pointer across setjmp
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve source length across setjmp
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", previous_handler_offset)); // handler.next = previous exception-handler head
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", survivor_offset)); // preserve the activation frame that survives this boundary
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", boundary_bytes - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // snapshot diagnostic suppression across longjmp
    emitter.instruction(&format!("lea r10, [rbp - {}]", previous_handler_offset)); // compute this wrapper's exception-handler record address
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("lea rdi, [rbp - {}]", boundary_bytes - TRY_HANDLER_JMP_BUF_OFFSET)); // pass this boundary's opaque jmp_buf to setjmp
    emitter.bl_c("setjmp"); // catch Throwable control flow escaping hydration hooks
    emitter.instruction("test eax, eax");                                       // did control return through longjmp?
    emitter.instruction("jnz __rt_unserialize_mixed_throw_x");                  // clean runtime state before propagating the Throwable
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base = preserved source string pointer
    emitter.instruction("xor esi, esi");                                        // start parsing at position 0
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // end = preserved source string length
    emitter.instruction("xor ecx, ecx");                                        // preflight starts at recursive depth zero
    emitter.instruction("call __rt_unser_validate_at");                         // reject truncated/overflowing grammar before allocating or running hooks
    emitter.instruction("test rax, rax");                                       // did the complete wire value validate?
    emitter.instruction("jz __rt_unserialize_mixed_invalid_x");                 // malformed input returns PHP false through the normal end path
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload base after caller-clobbered validator registers
    emitter.instruction("xor esi, esi");                                        // parse the already validated value from the beginning
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the validated source extent
    emitter.instruction("call __rt_unser_at");                                  // parse while the cleanup boundary is active
    emitter.instruction("jmp __rt_unserialize_mixed_parsed_x");                 // share exception-boundary teardown with validation failures
    emitter.label("__rt_unserialize_mixed_invalid_x");
    emitter.instruction("xor eax, eax");                                        // null result signals a bounded parse failure
    emitter.label("__rt_unserialize_mixed_parsed_x");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the parsed box while popping the boundary
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", previous_handler_offset)); // reload the previous exception-handler head
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", boundary_bytes - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // reload diagnostic suppression after the protected parse
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // recover the parsed Mixed result
    emitter.instruction("leave");                                               // release the exception boundary and restore the caller frame
    emitter.instruction("ret");                                                 // return the parsed box to the lowering's normal end path
    emitter.label("__rt_unserialize_mixed_throw_x");
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", previous_handler_offset)); // reload the handler preceding this internal boundary
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", boundary_bytes - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression skipped by longjmp
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction("xor eax, eax");                                        // end cleanup ignores the placeholder parse result on throw
    emitter.instruction("call __rt_unserialize_end");                           // release policy/context state before propagating the Throwable
    emitter.instruction("leave");                                               // discard the protected parser stack through its boundary
    emitter.instruction("jmp __rt_throw_current");                              // resume propagation at the caller's exception handler
}

/// Emits the x86_64 recursive decoder for one prevalidated serialized value.
pub(super) fn emit_parser(emitter: &mut Emitter) {
    // -- __rt_unser_at(base=rdi, pos=rsi, end=rdx) -> rax=box (0 fail), rdx=newpos --
    emitter.blank();
    emitter.comment("--- runtime: unser_at (recursive serialize() value parser) ---");
    emitter.label_global("__rt_unser_at");
    // [rbp-8]=base [16]=pos [24]=end [32]=container [40]=count [48]=index [56]=key_lo [64]=key_hi
    // [rbp-72]=scratch/hook [80]=policy/data hash [88]=registry index [96]=object box
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 96");                                         // recursive parser frame (with a reference-index slot)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the base pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the current position
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the end position
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r8", "_unser_depth", 0); // load current recursive unserialize depth
    emitter.instruction("add r8, 1");                                           // account for this parser frame
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r8", "_unser_depth", 0); // publish parser depth before consuming wire bytes
    emitter.instruction("cmp r8, 512");                                         // bound recursive frames before native-stack exhaustion
    emitter.instruction("jg __rt_unser_depth_fatal_x");                         // terminate hostile deeply nested serialized input
    emitter.instruction("cmp rsi, rdx");                                        // is the cursor already at/past the end?
    emitter.instruction("jge __rt_unser_at_fail");                              // nothing left to parse
    emitter.instruction("movzx r9d, BYTE PTR [rdi + rsi]");                     // load the leading type byte
    // Every value consumes the next pre-order index, including r:/R: aliases.
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_unser_count");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // current value index
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // reserve this value's index
    emitter.instruction("add r11, 1");                                          // advance the registry counter
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the advanced counter
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // recover the reserved zero-based registry slot
    emitter.instruction("cmp r10, 65536");                                      // is the reserved slot inside the fixed registry?
    emitter.instruction("jae __rt_unser_at_registry_slot_ready_x");             // out-of-capacity values remain deliberately unregistered
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_unser_values");
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8], 0");                    // erase any stale object pointer before parsing this value
    emitter.label("__rt_unser_at_registry_slot_ready_x");
    emitter.instruction("cmp r9d, 114");                                        // ASCII 'r'?
    emitter.instruction("je __rt_unser_at_ref");                                // resolve an object back-reference
    emitter.instruction("cmp r9d, 82");                                         // ASCII 'R'?
    emitter.instruction("je __rt_unser_at_ref");                                // resolve a PHP reference
    emitter.instruction("cmp r9d, 78");                                         // ASCII 'N' (null)?
    emitter.instruction("je __rt_unser_at_null");                               // parse null
    emitter.instruction("cmp r9d, 98");                                         // ASCII 'b' (bool)?
    emitter.instruction("je __rt_unser_at_bool");                               // parse bool
    emitter.instruction("cmp r9d, 105");                                        // ASCII 'i' (int)?
    emitter.instruction("je __rt_unser_at_int");                                // parse int
    emitter.instruction("cmp r9d, 100");                                        // ASCII 'd' (float)?
    emitter.instruction("je __rt_unser_at_float");                              // parse float
    emitter.instruction("cmp r9d, 115");                                        // ASCII 's' (string)?
    emitter.instruction("je __rt_unser_at_str");                                // parse string
    emitter.instruction("cmp r9d, 97");                                         // ASCII 'a' (array)?
    emitter.instruction("je __rt_unser_at_array");                              // parse array
    emitter.instruction("cmp r9d, 79");                                         // ASCII 'O' (object)?
    emitter.instruction("je __rt_unser_at_object");                             // parse object
    emitter.instruction("jmp __rt_unser_at_fail");                              // unsupported wire form

    // -- null: "N;" --
    emitter.label("__rt_unser_at_null");
    emitter.instruction("mov rax, 8");                                          // value tag = null
    emitter.instruction("mov rdi, 0");                                          // null payload low word
    emitter.instruction("mov rsi, 0");                                          // null payload high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the null value
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload position
    emitter.instruction("add rdx, 2");                                          // newpos skips "N;"
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- bool: "b:0;" / "b:1;" --
    emitter.label("__rt_unser_at_bool");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("movzx r9d, BYTE PTR [r10 + 2]");                       // load the bool digit at offset 2
    emitter.instruction("sub r9d, 48");                                         // ASCII '0'/'1' -> 0/1
    emitter.instruction("and r9, 1");                                           // clamp to a single bool bit
    emitter.instruction("mov rdi, r9");                                         // value payload = bool bit
    emitter.instruction("mov rax, 3");                                          // value tag = bool
    emitter.instruction("mov rsi, 0");                                          // bool high payload unused
    emitter.instruction("call __rt_mixed_from_value");                          // box the bool value
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload position
    emitter.instruction("add rdx, 4");                                          // newpos skips "b:X;"
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- int: "i:" + optional '-' + digits + ";" --
    emitter.label("__rt_unser_at_int");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "i:" to the first digit
    emitter.instruction("xor r11, r11");                                        // digit accumulator
    emitter.instruction("xor r8, r8");                                          // negative-sign flag
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // first numeric byte
    emitter.instruction("cmp r9d, 45");                                         // leading '-'?
    emitter.instruction("jne __rt_unser_at_int_loop");                          // no sign
    emitter.instruction("mov r8, 1");                                           // record negative sign
    emitter.instruction("add r10, 1");                                          // skip '-'
    emitter.label("__rt_unser_at_int_loop");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next numeric byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_int_done");                           // terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_int_done");                           // terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_int_loop");                          // continue
    emitter.label("__rt_unser_at_int_done");
    emitter.instruction("test r8, r8");                                         // signed?
    emitter.instruction("jz __rt_unser_at_int_box");                            // not signed
    emitter.instruction("neg r11");                                             // apply sign
    emitter.label("__rt_unser_at_int_box");
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save the cursor (at ';') across the box call
    emitter.instruction("mov rdi, r11");                                        // value payload = parsed int
    emitter.instruction("mov rax, 0");                                          // value tag = int
    emitter.instruction("mov rsi, 0");                                          // int high payload unused
    emitter.instruction("call __rt_mixed_from_value");                          // box the int value
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the cursor
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // newpos = cursor - base
    emitter.instruction("add r10, 1");                                          // skip the ';'
    emitter.instruction("mov rdx, r10");                                        // newpos
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- float: "d:" + (INF/-INF/NAN | digits) + ";" --
    emitter.label("__rt_unser_at_float");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add rdi, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add rdi, 2");                                          // strtod source = first byte after "d:"
    emitter.instruction("lea rsi, [rbp - 72]");                                 // strtod endptr = &scratch
    emitter.instruction("call strtod");                                         // parse the float (stops at ';') -> xmm0, scratch=endptr
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // bounded conversion end pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // source base
    emitter.instruction("add r11, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add r11, 2");                                          // first float payload byte
    emitter.instruction("cmp r10, r11");                                        // did strtod consume at least one byte?
    emitter.instruction("je __rt_unser_at_fail");                               // invalid numeric payload
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // source base
    emitter.instruction("add r11, QWORD PTR [rbp - 24]");                       // absolute source end
    emitter.instruction("cmp r10, r11");                                        // end pointer must still address a delimiter
    emitter.instruction("jae __rt_unser_at_fail");                              // reject a conversion escaping the source extent
    emitter.instruction("cmp BYTE PTR [r10], 59");                              // exact semicolon delimiter
    emitter.instruction("jne __rt_unser_at_fail");                              // reject partial conversions such as `1x;`
    emitter.instruction("movq r9, xmm0");                                       // move the parsed double into a GPR
    emitter.instruction("mov rdi, r9");                                         // value payload = float bits
    emitter.instruction("mov rax, 2");                                          // value tag = float
    emitter.instruction("mov rsi, 0");                                          // float high payload unused
    emitter.instruction("call __rt_mixed_from_value");                          // box the float value
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the strtod endptr
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // newpos = endptr - base
    emitter.instruction("add r10, 1");                                          // skip the ';'
    emitter.instruction("mov rdx, r10");                                        // newpos
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- string: "s:" + bytelen + ":\"" + raw + "\";" --
    emitter.label("__rt_unser_at_str");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "s:" to the length digits
    emitter.instruction("xor r11, r11");                                        // length accumulator
    emitter.label("__rt_unser_at_strlen");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next length byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_strlen_done");                        // ':' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_strlen_done");                        // ':' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_strlen");                            // continue
    emitter.label("__rt_unser_at_strlen_done");
    emitter.instruction("add r10, 2");                                          // skip ':' and opening '\"' to the raw bytes
    emitter.instruction("mov r8, r10");                                         // raw end accumulator = raw start
    emitter.instruction("add r8, r11");                                         // raw end = raw + len
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save raw end across the box call
    emitter.instruction("mov rdi, r10");                                        // string payload pointer = raw bytes
    emitter.instruction("mov rsi, r11");                                        // string payload length
    emitter.instruction("mov rax, 1");                                          // value tag = string (mixed_from_value persists it)
    emitter.instruction("call __rt_mixed_from_value");                          // box an owned copy of the string
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload raw end
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // newpos = raw end - base
    emitter.instruction("add r10, 2");                                          // skip closing '\"' and ';'
    emitter.instruction("mov rdx, r10");                                        // newpos
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- array: "a:" + count + ":{" + count*(key value) + "}" --
    emitter.label("__rt_unser_at_array");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "a:" to the count digits
    emitter.instruction("xor r11, r11");                                        // count accumulator
    emitter.label("__rt_unser_at_count");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next count byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_count_done");                         // ':' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_count_done");                         // ':' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_count");                             // continue
    emitter.label("__rt_unser_at_count_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the entry count
    emitter.instruction("add r10, 2");                                          // skip ':' and '{' to the body
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // body position offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // advance the cursor to the body
    emitter.instruction("mov rdi, r11");                                        // hash capacity = entry count
    emitter.instruction("mov rsi, 7");                                          // hash value_type = boxed Mixed
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the entry index
    emitter.label("__rt_unser_at_array_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the entry index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 40]");                       // all entries parsed?
    emitter.instruction("jge __rt_unser_at_array_close");                       // box the hash when done
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // current position
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_key");                                 // parse the key -> rax=key_lo, rdx=key_hi, rcx=newpos
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // key parser must not escape the validated source
    emitter.instruction("ja __rt_unser_at_array_fail");                         // release the partially built hash on failure
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save key_lo
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save key_hi
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // advance past the key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // position after the key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_at");                                  // recursively parse the value -> rax=box, rdx=newpos
    emitter.instruction("test rax, rax");                                       // did the child parse succeed?
    emitter.instruction("jz __rt_unser_at_array_fail");                         // child failure invalidates the whole array
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // advance past the value
    emitter.instruction("mov rcx, rax");                                        // value_lo = parsed value box
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // key_hi (-1 for int keys)
    emitter.instruction("mov r8, 0");                                           // value_hi unused
    emitter.instruction("mov r9, 7");                                           // value tag = boxed Mixed (transfer the box)
    emitter.instruction("call __rt_hash_set");                                  // insert the entry -> rax = (possibly new) hash
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the updated hash pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the entry index
    emitter.instruction("add rcx, 1");                                          // advance the entry index
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the entry index
    emitter.instruction("jmp __rt_unser_at_array_loop");                        // continue with the next entry
    emitter.label("__rt_unser_at_array_close");
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // closing-brace position
    emitter.instruction("cmp r8, QWORD PTR [rbp - 24]");                        // require the closing delimiter byte
    emitter.instruction("jae __rt_unser_at_array_fail");                        // input ended before the closing '}'
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // source base
    emitter.instruction("cmp BYTE PTR [r9 + r8], 125");                         // exact `}`
    emitter.instruction("jne __rt_unser_at_array_fail");                        // anything but '}' fails the array
    emitter.instruction("mov rax, 24");                                         // box the hash: Mixed cell = tag + two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the boxed Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the hash pointer
    emitter.instruction(&format!("mov r11, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the x86_64 boxed-Mixed heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // stamp the Mixed box without discarding the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax], 5");                              // value tag 5 = associative array (hash)
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the hash pointer (ownership transferred)
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the high payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload position (at the closing '}')
    emitter.instruction("add rdx, 1");                                          // newpos skips the '}'
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position
    emitter.label("__rt_unser_at_array_fail");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // partially built hash pointer
    emitter.instruction("call __rt_hash_free_deep");                            // release keys and transferred boxed values locally
    emitter.instruction("xor eax, eax");                                        // report parse failure
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // preserve current cursor for the caller
    emitter.instruction("jmp __rt_unser_at_ret");                               // only the shared return decrements parser depth

    // -- object: "O:" + namelen + ":\"" + class + "\":" + count + ":{" + count*(key value) + "}" --
    emitter.label("__rt_unser_at_object");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "O:" to the class-name length digits
    emitter.instruction("xor r11, r11");                                        // class-name length accumulator
    emitter.label("__rt_unser_at_obj_namelen");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next length byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_obj_namelen_done");                   // ':' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_obj_namelen_done");                   // ':' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_obj_namelen");                       // continue
    emitter.label("__rt_unser_at_obj_namelen_done");
    emitter.instruction("add r10, 2");                                          // skip ':' and opening '\"' to the class name bytes
    emitter.instruction("mov r8, r10");                                         // class-name end accumulator = name start
    emitter.instruction("add r8, r11");                                         // class-name end = name + len
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save the class-name end across the call
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save class-name start across policy helper
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save class-name length across policy helper
    emitter.instruction("mov rax, r10");                                        // class name pointer for allowed_classes policy
    emitter.instruction("mov rdx, r11");                                        // class name length for allowed_classes policy
    emitter.instruction("call __rt_unserialize_class_allowed");                 // decide whether hydration is permitted
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // retain policy result until hook/property dispatch
    emitter.instruction("test rax, rax");                                       // blocked classes become incomplete objects
    emitter.instruction("jz __rt_unser_obj_incomplete_x");                      // build the incomplete object instead
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload class-name start after helper call
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload class-name length after helper call
    emitter.instruction("mov rax, r10");                                        // class-name pointer (new_by_name arg)
    emitter.instruction("mov rdx, r11");                                        // class-name length (new_by_name arg)
    emitter.instruction("call __rt_new_by_name");                               // instantiate the class by name (0 on unknown class)
    emitter.instruction("test rax, rax");                                       // unknown class?
    emitter.instruction("jnz __rt_unser_obj_allocated_x");                      // known classes use their declared layout
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // unknown classes suppress hooks and use opaque properties
    emitter.instruction("jmp __rt_unser_obj_incomplete_x");                     // match PHP's __PHP_Incomplete_Class fallback
    emitter.label("__rt_unser_obj_incomplete_x");
    emitter.instruction("mov rax, 32");                                         // class id, original class name, and opaque property hash
    emitter.instruction("call __rt_heap_alloc");                                // allocate the incomplete-object payload
    let object_heap_marker = format!(
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(4)
    );
    emitter.instruction(&object_heap_marker);                                   // materialize the full-width object heap marker before storing it
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the object header without an unencodable imm64 memory move
    emitter.instruction("mov rdi, rax");                                        // object handle allocator input
    emitter.instruction("call __rt_object_handle_acquire");                     // give the incomplete object a normal PHP handle
    emitter.instruction("mov QWORD PTR [rax], -2");                             // reserved class id for __PHP_Incomplete_Class
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve incomplete object across string persistence
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // original serialized class-name bytes
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // original serialized class-name length
    emitter.instruction("call __rt_str_persist");                               // own the class name independently of the source wire
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload incomplete object payload
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // persisted original class-name pointer
    emitter.instruction("mov QWORD PTR [r10 + 16], rdx");                       // persisted original class-name length
    emitter.instruction("mov QWORD PTR [r10 + 24], 0");                         // property hash is created after its count is parsed
    emitter.instruction("mov rax, r10");                                        // restore object pointer for the shared allocated path
    emitter.label("__rt_unser_obj_allocated_x");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the new object pointer
    emitter.instruction("mov rax, 24");                                         // allocate the object's stable boxed Mixed cell
    emitter.instruction("call __rt_heap_alloc");                                // create the box before decoding any property values
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the boxed-Mixed heap kind
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the Mixed box with the target heap marker
    emitter.instruction("mov QWORD PTR [rax], 6");                              // value tag 6 = object
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the object pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // transfer object ownership into the box
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the high payload word
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // retain the stable result box through hooks and parsing
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // reserved value index for this object
    emitter.instruction("cmp r10, 65536");                                      // is the reserved slot inside the registry?
    emitter.instruction("jae __rt_unser_obj_registered_x");                     // overflow values cannot participate in back-references
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_unser_values");
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rax");                    // publish before parsing so r: can resolve self-references
    emitter.label("__rt_unser_obj_registered_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the class-name end
    emitter.instruction("add r10, 2");                                          // skip closing '\"' and ':' to the property count
    emitter.instruction("xor r11, r11");                                        // property-count accumulator
    emitter.label("__rt_unser_at_obj_count");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next count byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_obj_count_done");                     // ':' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_obj_count_done");                     // ':' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_obj_count");                         // continue
    emitter.label("__rt_unser_at_obj_count_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the property count
    emitter.instruction("add r10, 2");                                          // skip ':' and '{' to the body
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // body position offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // advance the cursor to the body
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // blocked objects cannot inspect class hook tables
    emitter.instruction("je __rt_unser_obj_default");                           // parse properties without hydration
    // -- __unserialize magic: parse the body into an assoc array, then call
    //    __unserialize($this, $data) instead of injecting properties by name --
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // object pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // class id from the object header
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_class_unserialize_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r11 + rax*8]");                    // __unserialize method symbol (0 if none)
    emitter.instruction("test r10, r10");                                       // does the class define __unserialize?
    emitter.instruction("jz __rt_unser_obj_default");                           // no → inject properties by name
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // park the __unserialize target
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // entry count = hash capacity hint
    emitter.instruction("mov rsi, 7");                                          // hash value_type = boxed Mixed
    emitter.instruction("call __rt_hash_new");                                  // allocate the $data hash
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the $data hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // entry index = 0
    emitter.label("__rt_unser_obj_data_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the entry index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 40]");                       // all entries parsed?
    emitter.instruction("jge __rt_unser_obj_data_done");                        // call __unserialize when done
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // current position
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_key");                                 // parse the key -> rax=key_lo, rdx=key_hi, rcx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save key_lo
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save key_hi
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // advance past the key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // position after the key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_at");                                  // recursively parse the value -> rax=box, rdx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // advance past the value
    emitter.instruction("test rax, rax");                                       // did the child decoder reject the value?
    emitter.instruction("jz __rt_unser_obj_data_fail_x");                       // fail without passing a null box to the hash
    emitter.instruction("mov rcx, rax");                                        // value_lo = parsed value box
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // $data hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // key_hi (-1 for int keys)
    emitter.instruction("mov r8, 0");                                           // value_hi unused
    emitter.instruction("mov r9, 7");                                           // value tag = boxed Mixed (transfer the box)
    emitter.instruction("call __rt_hash_set");                                  // insert the entry -> rax = (possibly new) hash
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the updated $data hash pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the entry index
    emitter.instruction("add rcx, 1");                                          // advance the entry index
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the entry index
    emitter.instruction("jmp __rt_unser_obj_data_loop");                        // continue with the next entry
    emitter.label("__rt_unser_obj_data_done");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // cursor where the closing brace must appear
    emitter.instruction("mov r12, QWORD PTR [rbp - 24]");                       // serialized input length
    emitter.instruction("cmp r11, r12");                                        // is the closing brace truncated?
    emitter.instruction("jge __rt_unser_obj_magic_missing_close");              // end-of-input is php-src's failure offset
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload serialized input base
    emitter.instruction("movzx r10d, BYTE PTR [r9 + r11]");                     // inspect the expected closing byte
    emitter.instruction("cmp r10d, 125");                                       // ASCII '}'?
    emitter.instruction("je __rt_unser_obj_magic_close_valid");                 // complete objects invoke the hook directly
    emitter.label("__rt_unser_obj_magic_missing_close");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_unser_warning_callback");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // call-site warning callback, if one was installed
    emitter.instruction("test r10, r10");                                       // is a callback available?
    emitter.instruction("jz __rt_unser_obj_magic_missing_reported");            // runtime-only callers may have no callback
    emitter.instruction("mov rdi, r11");                                        // callback arg 1 = failure offset
    emitter.instruction("mov rsi, r12");                                        // callback arg 2 = total input length
    emitter.instruction("call r10");                                            // warn before invoking __unserialize()
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_warning_emitted");
    emitter.instruction("mov QWORD PTR [r9], 1");                               // lowering must not duplicate the warning
    emitter.label("__rt_unser_obj_magic_missing_reported");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_force_failure");
    emitter.instruction("mov QWORD PTR [r9], 1");                               // hook still runs, but the public result is false
    emitter.label("__rt_unser_obj_magic_close_valid");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the concrete receiver for native date-hook trace metadata
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // class id indexes the generated trace-owner table
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "r10",
        "_class_date_unserialize_trace_entries",
    );
    emitter.instruction("shl r9, 4");                                           // each owner row contains two eight-byte words
    emitter.instruction("add r10, r9");                                         // select this class's (owner ptr, owner len) row
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // native implementation class-name pointer
    emitter.instruction("test r11, r11");                                       // zero identifies an ordinary user hook
    emitter.instruction("jz __rt_unser_obj_trace_ready");                       // keep the legacy uncaught path for non-date hooks
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_trace_owner_ptr");
    emitter.instruction("mov QWORD PTR [r9], r11");                             // publish the native implementation class name
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // native implementation class-name length
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_trace_owner_len");
    emitter.instruction("mov QWORD PTR [r9], r11");                             // publish its byte length
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // serialized call argument base
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_trace_input_ptr");
    emitter.instruction("mov QWORD PTR [r9], r11");                             // preserve the original wire string for the trace preview
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // serialized call argument length
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_trace_input_len");
    emitter.instruction("mov QWORD PTR [r9], r11");                             // preserve the wire length for the 15-byte preview bound
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_trace_exception_ptr");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // the thrown Error identity is captured at the first unwind boundary
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_trace_active");
    emitter.instruction("mov QWORD PTR [r9], 1");                               // activate the specialized uncaught trace
    emitter.label("__rt_unser_obj_trace_ready");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // $this receiver = first argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // $data assoc array (bare hash) = second argument
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the __unserialize target
    emitter.instruction("call r10");                                            // call __unserialize($this, $data)
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_unser_trace_active");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // a successful native hook no longer owns uncaught-trace state
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_unser_trace_exception_ptr");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // discard the unused exception identity slot after success
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the concrete receiver for DateInterval-handler detection
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load its runtime class id
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "r10",
        "_class_dateinterval_unserialize_flags",
    );
    emitter.instruction("mov r9, QWORD PTR [r10 + r9*8]");                      // does this class inherit DateInterval's native restoration hook?
    emitter.instruction("test r9, r9");                                         // inspect the generated handler marker
    emitter.instruction("jz __rt_unser_dateinterval_dynamic_done");             // other date objects have no DateInterval dynamic fields
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // parsed DateInterval data hash
    emitter.instruction("xor esi, esi");                                        // empty dynamic-property key pointer
    emitter.instruction("xor edx, edx");                                        // empty dynamic-property key length
    emitter.instruction("call __rt_hash_get");                                  // did the malformed payload contain the empty key?
    emitter.instruction("test rax, rax");                                       // was the empty custom key present?
    emitter.instruction("jz __rt_unser_dateinterval_dynamic_done");             // no empty custom property means no deprecation
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "r10",
        "_unser_dateinterval_dynamic_callback",
    );
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // call-site deprecation callback
    emitter.instruction("test r10, r10");                                       // is a callback available?
    emitter.instruction("jz __rt_unser_dateinterval_dynamic_done");             // runtime-only callers may have no callback
    emitter.instruction("call r10");                                            // emit the dynamic-property deprecation at the call site
    emitter.label("__rt_unser_dateinterval_dynamic_done");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the concrete object receiver
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // reload the parsed magic data hash
    emitter.instruction("call __rt_date_magic_restore_props");                  // restore user-declared date-subclass properties
    emitter.instruction("jmp __rt_unser_at_obj_box");                           // box the object (position is at the closing '}')
    emitter.label("__rt_unser_obj_default");
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // blocked objects own an opaque Mixed property hash
    emitter.instruction("jne __rt_unser_obj_default_props_x");                  // hydrated objects use their declared property slots
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // property count is the hash capacity hint
    emitter.instruction("mov rsi, 7");                                          // values are boxed Mixed cells
    emitter.instruction("call __rt_hash_new");                                  // allocate property hash before parsing values
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // incomplete-object payload
    emitter.instruction("mov QWORD PTR [r10 + 24], rax");                       // transfer hash ownership into incomplete object
    emitter.label("__rt_unser_obj_default_props_x");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the property index
    emitter.label("__rt_unser_at_obj_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the property index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 40]");                       // all properties parsed?
    emitter.instruction("jge __rt_unser_at_obj_close");                         // box the object when done
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // current position
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_key");                                 // parse the mangled key -> rax=key_ptr, rdx=key_len, rcx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the key pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the key length
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // advance past the key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // position after the key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_at");                                  // recursively parse the value -> rax=box, rdx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // advance past the value
    emitter.instruction("test rax, rax");                                       // did the child decoder reject the value?
    emitter.instruction("jz __rt_unser_obj_fail_x");                            // never pass a failed child box to property storage
    emitter.instruction("mov rcx, rax");                                        // value box
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // object pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // key length
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // blocked objects keep their wire properties opaque
    emitter.instruction("je __rt_unser_obj_store_opaque_prop_x");               // blocked objects retain the parsed property semantically
    emitter.instruction("call __rt_obj_store_prop");                            // store the value into the matching property slot
    emitter.instruction("jmp __rt_unser_obj_skip_prop_store_x");                // transferred value now belongs to the hydrated object
    emitter.label("__rt_unser_obj_store_opaque_prop_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // incomplete-object payload
    emitter.instruction("mov rdi, QWORD PTR [rdi + 24]");                       // opaque property hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // serialized property key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // serialized property key length
    emitter.instruction("mov r8, 0");                                           // boxed Mixed values have no high payload word
    emitter.instruction("mov r9, 7");                                           // transfer the parsed box as a Mixed hash value
    emitter.instruction("call __rt_hash_set");                                  // insert property, preserving key/value ownership and order
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // incomplete-object payload after a possible hash grow
    emitter.instruction("mov QWORD PTR [r10 + 24], rax");                       // retain updated property hash pointer
    emitter.label("__rt_unser_obj_skip_prop_store_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the property index
    emitter.instruction("add rcx, 1");                                          // advance the property index
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the property index
    emitter.instruction("jmp __rt_unser_at_obj_loop");                          // continue with the next property
    emitter.label("__rt_unser_at_obj_close");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // closing-brace position after the final property
    emitter.instruction("cmp r11, QWORD PTR [rbp - 24]");                       // is a closing byte available?
    emitter.instruction("jae __rt_unser_obj_fail_x");                           // truncated default objects are parse failures
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload serialized input base
    emitter.instruction("cmp BYTE PTR [r9 + r11], 125");                        // exact bounded `}`?
    emitter.instruction("jne __rt_unser_obj_fail_x");                           // malformed default object is a parse failure
    // -- __wakeup magic: after default property injection, call __wakeup($this) --
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // blocked classes cannot run __wakeup
    emitter.instruction("je __rt_unser_at_obj_box");                            // incomplete objects never run class hooks
    emitter.label("__rt_unser_obj_wakeup_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // object pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // class id from the object header
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_class_wakeup_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r11 + rax*8]");                    // __wakeup method symbol (0 if none)
    emitter.instruction("test r10, r10");                                       // does the class define __wakeup?
    emitter.instruction("jz __rt_unser_at_obj_box");                            // no → box the object directly
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // $this receiver
    emitter.instruction("call r10");                                            // call __wakeup($this)
    emitter.label("__rt_unser_at_obj_box");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_force_failure");
    emitter.instruction("cmp QWORD PTR [r9], 0");                               // did a malformed magic object force failure?
    emitter.instruction("jne __rt_unser_at_force_fail");                        // honor a failure raised by a required magic hook
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // return the stable box published before body parsing
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload position (at the closing '}')
    emitter.instruction("add rdx, 1");                                          // newpos skips the '}'
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    emitter.label("__rt_unser_at_force_fail");
    emitter.instruction("xor eax, eax");                                        // required hook effects ran, but result is false
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // preserve php-src's failure offset
    emitter.instruction("jmp __rt_unser_at_ret");                               // return the preserved failure position

    emitter.label("__rt_unser_obj_data_fail_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // partially built __unserialize data hash
    emitter.instruction("call __rt_hash_free_deep");                            // release keys and transferred child boxes
    emitter.instruction("jmp __rt_unser_obj_fail_x");                           // release the partially hydrated object too
    emitter.label("__rt_unser_obj_fail_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // reserved registry slot for the failed object
    emitter.instruction("cmp r10, 65536");                                      // was the object published in the fixed registry?
    emitter.instruction("jae __rt_unser_obj_fail_noreg_x");                     // skip clearing an out-of-capacity slot
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_unser_values");
    emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                      // remove the failed object from future reference lookup
    emitter.label("__rt_unser_obj_fail_noreg_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // partially hydrated object's owning Mixed box
    emitter.instruction("call __rt_decref_mixed");                              // release the box and its object ownership
    emitter.instruction("xor eax, eax");                                        // report parse failure
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // preserve the child decoder's current cursor
    emitter.instruction("jmp __rt_unser_at_ret");                               // only the shared return decrements parser depth

    // -- failure: null box, position unchanged --
    emitter.label("__rt_unser_at_fail");
    emitter.instruction("xor eax, eax");                                        // null result signals parse failure
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // newpos = unchanged position

    emitter.label("__rt_unser_at_ret");
    emitter.instruction("test rax, rax");                                       // did the parser produce a referenceable boxed value?
    emitter.instruction("jz __rt_unser_at_ret_done");                           // failed values have nothing to register
    emitter.instruction("mov r9, QWORD PTR [rbp - 88]");                        // reserved value index
    emitter.instruction("cmp r9, 65536");                                       // is the reserved result slot in bounds?
    emitter.instruction("jge __rt_unser_at_ret_done");                          // skip publishing results beyond registry capacity
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_unser_values");
    emitter.instruction("mov QWORD PTR [r10 + r9*8], rax");                     // make every parsed value referenceable
    emitter.label("__rt_unser_at_ret_done");
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "r8", "_unser_depth", 0); // load parser depth before returning to the caller
    emitter.instruction("sub r8, 1");                                           // release this completed recursive parser frame
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "r8", "_unser_depth", 0); // keep sibling parses independent
    emitter.instruction("add rsp, 96");                                         // deallocate the parser frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return rax=box, rdx=newpos

    emitter.label("__rt_unser_depth_fatal_x");
    emitter.instruction("mov edi, 2");                                          // stderr file descriptor
    crate::codegen_support::abi::emit_symbol_address(emitter, "rsi", "_unser_depth_msg");
    emitter.instruction("mov edx, 48");                                         // complete unserialize-depth fatal diagnostic length
    emitter.instruction("mov eax, 1");                                          // Linux write syscall number
    emitter.instruction("syscall");                                             // report the recursive parser limit
    emitter.instruction("mov edi, 1");                                          // non-zero failure status
    emitter.instruction("mov eax, 60");                                         // Linux exit syscall number
    emitter.instruction("syscall");                                             // terminate without returning to the overflowing caller

    // -- back-reference: r:N; / R:N; -> a fresh box aliasing the Nth parsed value
    //    (1-based); objects are retained. Out-of-range/unregistered index -> null. --
    emitter.label("__rt_unser_at_ref");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // position
    emitter.instruction("add r10, r11");                                        // pointer to the leading 'r'/'R'
    emitter.instruction("add r10, 2");                                          // skip the marker and ':'
    emitter.instruction("xor r11, r11");                                        // index accumulator
    emitter.label("__rt_unser_at_ref_loop");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_ref_done");                           // terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_ref_done");                           // terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift the accumulator
    emitter.instruction("add r11, r9");                                         // add the digit
    emitter.instruction("add r10, 1");                                          // advance the cursor
    emitter.instruction("jmp __rt_unser_at_ref_loop");                          // continue
    emitter.label("__rt_unser_at_ref_done");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload base
    emitter.instruction("sub r10, r9");                                         // offset of the ';'
    emitter.instruction("add r10, 1");                                          // newpos skips the ';'
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the new position
    emitter.instruction("test r11, r11");                                       // index 0 is invalid
    emitter.instruction("jz __rt_unser_at_ref_fail");                           // bail to null
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_count");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // number of registered values
    emitter.instruction("cmp r11, r9");                                         // index beyond what was parsed?
    emitter.instruction("jg __rt_unser_at_ref_fail");                           // out of range → null
    emitter.instruction("sub r11, 1");                                          // 0-based registry slot
    emitter.instruction("cmp r11, 65536");                                      // would this logical reference index exceed the registry?
    emitter.instruction("jae __rt_unser_at_ref_fail");                          // fail closed instead of reading beyond the fixed registry
    crate::codegen_support::abi::emit_symbol_address(emitter, "r9", "_unser_values");
    emitter.instruction("mov r9, QWORD PTR [r9 + r11*8]");                      // the registered value box (0 if none)
    emitter.instruction("test r9, r9");                                         // was this reserved registry slot left unpublished?
    emitter.instruction("jz __rt_unser_at_ref_fail");                           // → null
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save the source box across the alloc
    emitter.instruction("mov rax, 24");                                         // a fresh boxed Mixed cell
    emitter.instruction("call __rt_heap_alloc");                                // allocate it
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the source box
    emitter.instruction("mov r10, QWORD PTR [r9 - 8]");                         // source heap header
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // copy the heap header
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // source value tag
    emitter.instruction("mov QWORD PTR [rax], r10");                            // copy the value tag
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // source low payload (object pointer)
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // copy the low payload
    emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                        // source high payload
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // copy the high payload
    emitter.instruction("cmp QWORD PTR [rax], 6");                              // does the alias point at an object?
    emitter.instruction("jne __rt_unser_at_ref_boxed");                         // non-objects need no retain
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the fresh box across the retain
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // move the object pointer into incref's x86_64 input register
    emitter.instruction("call __rt_incref");                                    // retain the shared object before the source box releases it
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the fresh box
    emitter.label("__rt_unser_at_ref_boxed");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // newpos past the ';'
    emitter.instruction("jmp __rt_unser_at_ret");                               // return the aliasing box
    emitter.label("__rt_unser_at_ref_fail");
    emitter.instruction("xor eax, eax");                                        // unresolved reference → null
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // newpos past the ';'
    emitter.instruction("jmp __rt_unser_at_ret");                               // return the null result
}
