//! Purpose:
//! Emits the `__rt_localtime` runtime helper: builds PHP's localtime() array (raw struct tm fields)
//! from a timestamp, with numeric `[0..8]` keys or `tm_*` associative keys per the second argument.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - Values are the libc `struct tm` fields verbatim (so `tm_mon` is 0-based and `tm_year` is
//!   years-since-1900, unlike getdate). The hash is returned raw and boxed into a Mixed cell by the
//!   builtin emitter, like `getdate`/`stat`.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_localtime`, building the localtime() array.
///
/// ## Input registers (System V ABI)
/// - `x0`/`rax` = timestamp (or -1 for the current time), `x1`/`rsi` = associative-keys flag (0/1)
///
/// ## Output
/// - `x0`/`rax` = pointer to the array (numeric keys 0..8, or `tm_sec`..`tm_isdst` when the flag is set)
///
/// ## Behavior
/// - Fields are the raw `struct tm` values from libc `localtime`; the per-field key is chosen from the
///   associative flag. The hash pointer may move across inserts, so it is reloaded/saved each time.
pub fn emit_localtime(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: localtime ---");
    emitter.label_global("__rt_localtime");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #48");                             // frame: [sp]=ts, [sp+8]=tm ptr, [sp+16]=hash ptr, [sp+24]=assoc flag
            emitter.instruction("stp x29, x30, [sp, #32]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #32");                            // set the frame pointer
            emitter.instruction("str x1, [sp, #24]");                           // save the associative-keys flag across the libc/runtime calls
            emitter.instruction("cmn x0, #1");                                  // timestamp == -1 (current-time sentinel)?
            emitter.instruction("b.ne __rt_localtime_have");                    // explicit timestamp supplied → use it
            emitter.instruction("mov x0, #0");                                  // NULL argument to time()
            emitter.bl_c("time");                                               // time(NULL) → x0 = current Unix timestamp
            emitter.label("__rt_localtime_have");
            emitter.instruction("str x0, [sp, #0]");                            // save the resolved timestamp
            emitter.instruction("add x0, sp, #0");                              // x0 = &timestamp for localtime()
            emitter.bl_c("localtime");                                          // localtime(&ts) → x0 = struct tm
            emitter.instruction("str x0, [sp, #8]");                            // save the struct tm pointer
            emitter.instruction("mov x0, #16");                                 // capacity 16 (>= 9 entries, avoids a realloc)
            emitter.instruction("mov x1, #7");                                  // value type = mixed
            emitter.instruction("bl __rt_hash_new");                            // → x0 = new hash table
            emitter.instruction("str x0, [sp, #16]");                           // save the hash table pointer
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for tm_sec
            emitter.instruction("ldr w3, [x9, #0]");                            // load the raw tm_sec field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the associative-keys flag
            emitter.instruction("cbz x10, __rt_localtime_num_0");               // flag clear → use the numeric key
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_lt_k_tm_sec");
            emitter.instruction("mov x2, #6");                                  // length of "tm_sec"
            emitter.instruction("b __rt_localtime_set_0");                      // key ready → insert
            emitter.label("__rt_localtime_num_0");
            emitter.instruction("mov x1, #0");                                  // numeric key 0
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_0");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert the tm_sec entry
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for tm_min
            emitter.instruction("ldr w3, [x9, #4]");                            // load the raw tm_min field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the associative-keys flag
            emitter.instruction("cbz x10, __rt_localtime_num_1");               // flag clear → use the numeric key
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_lt_k_tm_min");
            emitter.instruction("mov x2, #6");                                  // length of "tm_min"
            emitter.instruction("b __rt_localtime_set_1");                      // key ready → insert
            emitter.label("__rt_localtime_num_1");
            emitter.instruction("mov x1, #1");                                  // numeric key 1
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_1");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert the tm_min entry
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for tm_hour
            emitter.instruction("ldr w3, [x9, #8]");                            // load the raw tm_hour field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the associative-keys flag
            emitter.instruction("cbz x10, __rt_localtime_num_2");               // flag clear → use the numeric key
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_lt_k_tm_hour");
            emitter.instruction("mov x2, #7");                                  // length of "tm_hour"
            emitter.instruction("b __rt_localtime_set_2");                      // key ready → insert
            emitter.label("__rt_localtime_num_2");
            emitter.instruction("mov x1, #2");                                  // numeric key 2
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_2");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert the tm_hour entry
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for tm_mday
            emitter.instruction("ldr w3, [x9, #12]");                           // load the raw tm_mday field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the associative-keys flag
            emitter.instruction("cbz x10, __rt_localtime_num_3");               // flag clear → use the numeric key
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_lt_k_tm_mday");
            emitter.instruction("mov x2, #7");                                  // length of "tm_mday"
            emitter.instruction("b __rt_localtime_set_3");                      // key ready → insert
            emitter.label("__rt_localtime_num_3");
            emitter.instruction("mov x1, #3");                                  // numeric key 3
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_3");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert the tm_mday entry
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for tm_mon
            emitter.instruction("ldr w3, [x9, #16]");                           // load the raw tm_mon field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the associative-keys flag
            emitter.instruction("cbz x10, __rt_localtime_num_4");               // flag clear → use the numeric key
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_lt_k_tm_mon");
            emitter.instruction("mov x2, #6");                                  // length of "tm_mon"
            emitter.instruction("b __rt_localtime_set_4");                      // key ready → insert
            emitter.label("__rt_localtime_num_4");
            emitter.instruction("mov x1, #4");                                  // numeric key 4
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_4");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert the tm_mon entry
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for tm_year
            emitter.instruction("ldr w3, [x9, #20]");                           // load the raw tm_year field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the associative-keys flag
            emitter.instruction("cbz x10, __rt_localtime_num_5");               // flag clear → use the numeric key
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_lt_k_tm_year");
            emitter.instruction("mov x2, #7");                                  // length of "tm_year"
            emitter.instruction("b __rt_localtime_set_5");                      // key ready → insert
            emitter.label("__rt_localtime_num_5");
            emitter.instruction("mov x1, #5");                                  // numeric key 5
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_5");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert the tm_year entry
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for tm_wday
            emitter.instruction("ldr w3, [x9, #24]");                           // load the raw tm_wday field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the associative-keys flag
            emitter.instruction("cbz x10, __rt_localtime_num_6");               // flag clear → use the numeric key
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_lt_k_tm_wday");
            emitter.instruction("mov x2, #7");                                  // length of "tm_wday"
            emitter.instruction("b __rt_localtime_set_6");                      // key ready → insert
            emitter.label("__rt_localtime_num_6");
            emitter.instruction("mov x1, #6");                                  // numeric key 6
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_6");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert the tm_wday entry
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for tm_yday
            emitter.instruction("ldr w3, [x9, #28]");                           // load the raw tm_yday field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the associative-keys flag
            emitter.instruction("cbz x10, __rt_localtime_num_7");               // flag clear → use the numeric key
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_lt_k_tm_yday");
            emitter.instruction("mov x2, #7");                                  // length of "tm_yday"
            emitter.instruction("b __rt_localtime_set_7");                      // key ready → insert
            emitter.label("__rt_localtime_num_7");
            emitter.instruction("mov x1, #7");                                  // numeric key 7
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_7");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert the tm_yday entry
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for tm_isdst
            emitter.instruction("ldr w3, [x9, #32]");                           // load the raw tm_isdst field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the associative-keys flag
            emitter.instruction("cbz x10, __rt_localtime_num_8");               // flag clear → use the numeric key
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_lt_k_tm_isdst");
            emitter.instruction("mov x2, #8");                                  // length of "tm_isdst"
            emitter.instruction("b __rt_localtime_set_8");                      // key ready → insert
            emitter.label("__rt_localtime_num_8");
            emitter.instruction("mov x1, #8");                                  // numeric key 8
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_8");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert the tm_isdst entry
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x0, [sp, #16]");                           // return the assoc array (hash pointer) in x0
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #48");                             // deallocate the frame
            emitter.instruction("ret");                                         // return to caller
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // frame: [rbp-8]=ts, [rbp-16]=tm ptr, [rbp-24]=hash ptr, [rbp-32]=flag
            emitter.instruction("mov rbp, rsp");                                // establish the frame pointer
            emitter.instruction("sub rsp, 48");                                 // reserve the local slots (16-aligned)
            emitter.instruction("mov QWORD PTR [rbp - 32], rsi");               // save the associative-keys flag (2nd argument)
            emitter.instruction("cmp rax, -1");                                 // timestamp == -1 (current-time sentinel)?
            emitter.instruction("jne __rt_localtime_have_x86");                 // explicit timestamp supplied → use it
            emitter.instruction("xor edi, edi");                                // NULL argument to time()
            emitter.instruction("call time");                                   // time(NULL) → rax = current Unix timestamp
            emitter.label("__rt_localtime_have_x86");
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // save the resolved timestamp
            emitter.instruction("lea rdi, [rbp - 8]");                          // rdi = &timestamp for localtime()
            emitter.instruction("call localtime");                              // localtime(&ts) → rax = struct tm
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // save the struct tm pointer
            emitter.instruction("mov rdi, 16");                                 // capacity 16 (>= 9 entries, avoids a realloc)
            emitter.instruction("mov rsi, 7");                                  // value type = mixed
            emitter.instruction("call __rt_hash_new");                          // → rax = new hash table
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the hash table pointer
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for tm_sec
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 0]");             // load and sign-extend the raw tm_sec field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the associative-keys flag
            emitter.instruction("test r11, r11");                               // flag clear → use the numeric key
            emitter.instruction("jz __rt_localtime_num_0_x86");                 // numeric key path
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_lt_k_tm_sec");
            emitter.instruction("mov rdx, 6");                                  // length of "tm_sec"
            emitter.instruction("jmp __rt_localtime_set_0_x86");                // key ready → insert
            emitter.label("__rt_localtime_num_0_x86");
            emitter.instruction("mov rsi, 0");                                  // numeric key 0
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_0_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert the tm_sec entry
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for tm_min
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 4]");             // load and sign-extend the raw tm_min field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the associative-keys flag
            emitter.instruction("test r11, r11");                               // flag clear → use the numeric key
            emitter.instruction("jz __rt_localtime_num_1_x86");                 // numeric key path
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_lt_k_tm_min");
            emitter.instruction("mov rdx, 6");                                  // length of "tm_min"
            emitter.instruction("jmp __rt_localtime_set_1_x86");                // key ready → insert
            emitter.label("__rt_localtime_num_1_x86");
            emitter.instruction("mov rsi, 1");                                  // numeric key 1
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_1_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert the tm_min entry
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for tm_hour
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 8]");             // load and sign-extend the raw tm_hour field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the associative-keys flag
            emitter.instruction("test r11, r11");                               // flag clear → use the numeric key
            emitter.instruction("jz __rt_localtime_num_2_x86");                 // numeric key path
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_lt_k_tm_hour");
            emitter.instruction("mov rdx, 7");                                  // length of "tm_hour"
            emitter.instruction("jmp __rt_localtime_set_2_x86");                // key ready → insert
            emitter.label("__rt_localtime_num_2_x86");
            emitter.instruction("mov rsi, 2");                                  // numeric key 2
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_2_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert the tm_hour entry
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for tm_mday
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 12]");            // load and sign-extend the raw tm_mday field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the associative-keys flag
            emitter.instruction("test r11, r11");                               // flag clear → use the numeric key
            emitter.instruction("jz __rt_localtime_num_3_x86");                 // numeric key path
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_lt_k_tm_mday");
            emitter.instruction("mov rdx, 7");                                  // length of "tm_mday"
            emitter.instruction("jmp __rt_localtime_set_3_x86");                // key ready → insert
            emitter.label("__rt_localtime_num_3_x86");
            emitter.instruction("mov rsi, 3");                                  // numeric key 3
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_3_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert the tm_mday entry
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for tm_mon
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 16]");            // load and sign-extend the raw tm_mon field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the associative-keys flag
            emitter.instruction("test r11, r11");                               // flag clear → use the numeric key
            emitter.instruction("jz __rt_localtime_num_4_x86");                 // numeric key path
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_lt_k_tm_mon");
            emitter.instruction("mov rdx, 6");                                  // length of "tm_mon"
            emitter.instruction("jmp __rt_localtime_set_4_x86");                // key ready → insert
            emitter.label("__rt_localtime_num_4_x86");
            emitter.instruction("mov rsi, 4");                                  // numeric key 4
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_4_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert the tm_mon entry
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for tm_year
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 20]");            // load and sign-extend the raw tm_year field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the associative-keys flag
            emitter.instruction("test r11, r11");                               // flag clear → use the numeric key
            emitter.instruction("jz __rt_localtime_num_5_x86");                 // numeric key path
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_lt_k_tm_year");
            emitter.instruction("mov rdx, 7");                                  // length of "tm_year"
            emitter.instruction("jmp __rt_localtime_set_5_x86");                // key ready → insert
            emitter.label("__rt_localtime_num_5_x86");
            emitter.instruction("mov rsi, 5");                                  // numeric key 5
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_5_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert the tm_year entry
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for tm_wday
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 24]");            // load and sign-extend the raw tm_wday field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the associative-keys flag
            emitter.instruction("test r11, r11");                               // flag clear → use the numeric key
            emitter.instruction("jz __rt_localtime_num_6_x86");                 // numeric key path
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_lt_k_tm_wday");
            emitter.instruction("mov rdx, 7");                                  // length of "tm_wday"
            emitter.instruction("jmp __rt_localtime_set_6_x86");                // key ready → insert
            emitter.label("__rt_localtime_num_6_x86");
            emitter.instruction("mov rsi, 6");                                  // numeric key 6
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_6_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert the tm_wday entry
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for tm_yday
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 28]");            // load and sign-extend the raw tm_yday field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the associative-keys flag
            emitter.instruction("test r11, r11");                               // flag clear → use the numeric key
            emitter.instruction("jz __rt_localtime_num_7_x86");                 // numeric key path
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_lt_k_tm_yday");
            emitter.instruction("mov rdx, 7");                                  // length of "tm_yday"
            emitter.instruction("jmp __rt_localtime_set_7_x86");                // key ready → insert
            emitter.label("__rt_localtime_num_7_x86");
            emitter.instruction("mov rsi, 7");                                  // numeric key 7
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_7_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert the tm_yday entry
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for tm_isdst
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 32]");            // load and sign-extend the raw tm_isdst field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov r11, QWORD PTR [rbp - 32]");               // reload the associative-keys flag
            emitter.instruction("test r11, r11");                               // flag clear → use the numeric key
            emitter.instruction("jz __rt_localtime_num_8_x86");                 // numeric key path
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_lt_k_tm_isdst");
            emitter.instruction("mov rdx, 8");                                  // length of "tm_isdst"
            emitter.instruction("jmp __rt_localtime_set_8_x86");                // key ready → insert
            emitter.label("__rt_localtime_num_8_x86");
            emitter.instruction("mov rsi, 8");                                  // numeric key 8
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.label("__rt_localtime_set_8_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert the tm_isdst entry
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // return the assoc array (hash pointer) in rax
            emitter.instruction("add rsp, 48");                                 // deallocate the local slots
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return to caller
        }
    }
}
