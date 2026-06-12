//! Purpose:
//! Emits the `__rt_getdate` runtime helper: builds PHP's getdate() associative array from a
//! timestamp (or the current time when -1 is passed).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - Decomposes the timestamp with libc `localtime`, then fills an 11-entry hash (value type mixed)
//!   with the 8 integer fields, the day/month names (heap-persisted strings), and the integer key 0
//!   holding the timestamp. Returns the raw hash pointer (like `stat`), which builtin codegen treats
//!   as the Mixed array result.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_getdate`, building the getdate() associative array.
///
/// ## Input registers (System V ABI)
/// - `x0`/`rax` = timestamp, or -1 to use the current time
///
/// ## Output
/// - `x0`/`rax` = pointer to the assoc array (string keys plus the integer key 0)
///
/// ## Behavior
/// - Keys (PHP order): seconds, minutes, hours, mday, wday, mon, year, yday, weekday, month, 0.
/// - `weekday`/`month` values are heap-persisted copies of the `_day_names`/`_month_names` entries.
/// - The integer key 0 stores the timestamp. The hash pointer may move across inserts (reallocation),
///   so it is reloaded from the frame before, and saved after, every `__rt_hash_set`.
pub fn emit_getdate(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getdate ---");
    emitter.label_global("__rt_getdate");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #48");                             // frame: [sp]=ts, [sp+8]=tm ptr, [sp+16]=hash ptr
            emitter.instruction("stp x29, x30, [sp, #32]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #32");                            // set the frame pointer
            emitter.instruction("cmn x0, #1");                                  // timestamp == -1 (current-time sentinel)?
            emitter.instruction("b.ne __rt_getdate_have");                      // explicit timestamp supplied → use it
            emitter.instruction("mov x0, #0");                                  // NULL argument to time()
            emitter.bl_c("time");                                               // time(NULL) → x0 = current Unix timestamp
            emitter.label("__rt_getdate_have");
            emitter.instruction("str x0, [sp, #0]");                            // save the resolved timestamp (also the [0] entry value)
            emitter.instruction("add x0, sp, #0");                              // x0 = &timestamp for localtime()
            emitter.bl_c("localtime");                                          // localtime(&ts) → x0 = struct tm
            emitter.instruction("str x0, [sp, #8]");                            // save the struct tm pointer
            emitter.instruction("mov x0, #16");                                 // initial capacity 16 (>= 11 entries, avoids a mid-build realloc)
            emitter.instruction("mov x1, #7");                                  // value type = mixed (int and string values)
            emitter.instruction("bl __rt_hash_new");                            // → x0 = new hash table
            emitter.instruction("str x0, [sp, #16]");                           // save the hash table pointer
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for seconds
            emitter.instruction("ldr w3, [x9, #0]");                            // load the seconds field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_seconds");
            emitter.instruction("mov x2, #7");                                  // length of "seconds"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "seconds" → int
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for minutes
            emitter.instruction("ldr w3, [x9, #4]");                            // load the minutes field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_minutes");
            emitter.instruction("mov x2, #7");                                  // length of "minutes"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "minutes" → int
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for hours
            emitter.instruction("ldr w3, [x9, #8]");                            // load the hours field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_hours");
            emitter.instruction("mov x2, #5");                                  // length of "hours"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "hours" → int
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for mday
            emitter.instruction("ldr w3, [x9, #12]");                           // load the mday field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_mday");
            emitter.instruction("mov x2, #4");                                  // length of "mday"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "mday" → int
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for wday
            emitter.instruction("ldr w3, [x9, #24]");                           // load the wday field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_wday");
            emitter.instruction("mov x2, #4");                                  // length of "wday"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "wday" → int
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for mon
            emitter.instruction("ldr w3, [x9, #16]");                           // load the mon field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("add x3, x3, #1");                              // adjust to PHP's mon value
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_mon");
            emitter.instruction("mov x2, #3");                                  // length of "mon"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "mon" → int
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for year
            emitter.instruction("ldr w3, [x9, #20]");                           // load the year field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("add x3, x3, #1900");                           // adjust to PHP's year value
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_year");
            emitter.instruction("mov x2, #4");                                  // length of "year"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "year" → int
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for yday
            emitter.instruction("ldr w3, [x9, #28]");                           // load the yday field
            emitter.instruction("sxtw x3, w3");                                 // sign-extend the 32-bit field to 64 bits
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_yday");
            emitter.instruction("mov x2, #4");                                  // length of "yday"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "yday" → int
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for weekday
            emitter.instruction("ldr w10, [x9, #24]");                          // load the index field for weekday
            emitter.instruction("mov x11, #12");                                // name-table stride is 12 bytes
            emitter.instruction("mul x10, x10, x11");                           // index * 12 → byte offset into the name table
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_day_names");
            emitter.instruction("add x1, x1, x10");                             // x1 = pointer to the name string
            emitter.instruction("ldrb w2, [x1, #10]");                          // load the name length from table offset 10
            emitter.instruction("bl __rt_str_persist");                         // copy the name to the heap → x1=ptr, x2=len
            emitter.instruction("mov x3, x1");                                  // value_lo = heap string pointer
            emitter.instruction("mov x4, x2");                                  // value_hi = string length
            emitter.instruction("mov x5, #1");                                  // value tag = string
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_weekday");
            emitter.instruction("mov x2, #7");                                  // length of "weekday"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "weekday" → string
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x9, [sp, #8]");                            // reload struct tm pointer for month
            emitter.instruction("ldr w10, [x9, #16]");                          // load the index field for month
            emitter.instruction("mov x11, #12");                                // name-table stride is 12 bytes
            emitter.instruction("mul x10, x10, x11");                           // index * 12 → byte offset into the name table
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_month_names");
            emitter.instruction("add x1, x1, x10");                             // x1 = pointer to the name string
            emitter.instruction("ldrb w2, [x1, #10]");                          // load the name length from table offset 10
            emitter.instruction("bl __rt_str_persist");                         // copy the name to the heap → x1=ptr, x2=len
            emitter.instruction("mov x3, x1");                                  // value_lo = heap string pointer
            emitter.instruction("mov x4, x2");                                  // value_hi = string length
            emitter.instruction("mov x5, #1");                                  // value tag = string
            crate::codegen::abi::emit_symbol_address(emitter, "x1", "_gd_k_month");
            emitter.instruction("mov x2, #5");                                  // length of "month"
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert "month" → string
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("mov x1, #0");                                  // integer key is literally 0
            emitter.instruction("mov x2, #-1");                                 // key_hi = -1 marks an integer key
            emitter.instruction("ldr x3, [sp, #0]");                            // value_lo = the timestamp
            emitter.instruction("mov x4, #0");                                  // value_hi = 0
            emitter.instruction("mov x5, #0");                                  // value tag = int
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the hash table pointer
            emitter.instruction("bl __rt_hash_set");                            // insert 0 → timestamp
            emitter.instruction("str x0, [sp, #16]");                           // save the (possibly reallocated) hash table
            emitter.instruction("ldr x0, [sp, #16]");                           // return the assoc array (hash pointer) in x0
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #48");                             // deallocate the frame
            emitter.instruction("ret");                                         // return to caller
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // frame: [rbp-8]=ts, [rbp-16]=tm ptr, [rbp-24]=hash ptr
            emitter.instruction("mov rbp, rsp");                                // establish the frame pointer
            emitter.instruction("sub rsp, 32");                                 // reserve the local slots (16-aligned)
            emitter.instruction("cmp rax, -1");                                 // timestamp == -1 (current-time sentinel)?
            emitter.instruction("jne __rt_getdate_have_x86");                   // explicit timestamp supplied → use it
            emitter.instruction("xor edi, edi");                                // NULL argument to time()
            emitter.instruction("call time");                                   // time(NULL) → rax = current Unix timestamp
            emitter.label("__rt_getdate_have_x86");
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // save the resolved timestamp (also the [0] entry)
            emitter.instruction("lea rdi, [rbp - 8]");                          // rdi = &timestamp for localtime()
            emitter.instruction("call localtime");                              // localtime(&ts) → rax = struct tm
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // save the struct tm pointer
            emitter.instruction("mov rdi, 16");                                 // initial capacity 16 (>= 11 entries, avoids a mid-build realloc)
            emitter.instruction("mov rsi, 7");                                  // value type = mixed
            emitter.instruction("call __rt_hash_new");                          // → rax = new hash table
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the hash table pointer
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for seconds
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 0]");             // load and sign-extend the seconds field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_seconds");
            emitter.instruction("mov rdx, 7");                                  // length of "seconds"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "seconds" → int
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for minutes
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 4]");             // load and sign-extend the minutes field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_minutes");
            emitter.instruction("mov rdx, 7");                                  // length of "minutes"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "minutes" → int
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for hours
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 8]");             // load and sign-extend the hours field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_hours");
            emitter.instruction("mov rdx, 5");                                  // length of "hours"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "hours" → int
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for mday
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 12]");            // load and sign-extend the mday field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_mday");
            emitter.instruction("mov rdx, 4");                                  // length of "mday"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "mday" → int
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for wday
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 24]");            // load and sign-extend the wday field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_wday");
            emitter.instruction("mov rdx, 4");                                  // length of "wday"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "wday" → int
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for mon
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 16]");            // load and sign-extend the mon field
            emitter.instruction("add rcx, 1");                                  // adjust to PHP's mon value
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_mon");
            emitter.instruction("mov rdx, 3");                                  // length of "mon"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "mon" → int
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for year
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 20]");            // load and sign-extend the year field
            emitter.instruction("add rcx, 1900");                               // adjust to PHP's year value
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_year");
            emitter.instruction("mov rdx, 4");                                  // length of "year"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "year" → int
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for yday
            emitter.instruction("movsxd rcx, DWORD PTR [r10 + 28]");            // load and sign-extend the yday field
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_yday");
            emitter.instruction("mov rdx, 4");                                  // length of "yday"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "yday" → int
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for weekday
            emitter.instruction("movsxd r11, DWORD PTR [r10 + 24]");            // load the index field for weekday
            emitter.instruction("imul r11, r11, 12");                           // index * 12 → byte offset into the name table
            crate::codegen::abi::emit_symbol_address(emitter, "rax", "_day_names");
            emitter.instruction("add rax, r11");                                // rax = pointer to the name string
            emitter.instruction("movzx edx, BYTE PTR [rax + 10]");              // load the name length from table offset 10
            emitter.instruction("call __rt_str_persist");                       // copy the name to the heap → rax=ptr, rdx=len
            emitter.instruction("mov rcx, rax");                                // value_lo = heap string pointer
            emitter.instruction("mov r8, rdx");                                 // value_hi = string length
            emitter.instruction("mov r9, 1");                                   // value tag = string
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_weekday");
            emitter.instruction("mov rdx, 7");                                  // length of "weekday"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "weekday" → string
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload struct tm pointer for month
            emitter.instruction("movsxd r11, DWORD PTR [r10 + 16]");            // load the index field for month
            emitter.instruction("imul r11, r11, 12");                           // index * 12 → byte offset into the name table
            crate::codegen::abi::emit_symbol_address(emitter, "rax", "_month_names");
            emitter.instruction("add rax, r11");                                // rax = pointer to the name string
            emitter.instruction("movzx edx, BYTE PTR [rax + 10]");              // load the name length from table offset 10
            emitter.instruction("call __rt_str_persist");                       // copy the name to the heap → rax=ptr, rdx=len
            emitter.instruction("mov rcx, rax");                                // value_lo = heap string pointer
            emitter.instruction("mov r8, rdx");                                 // value_hi = string length
            emitter.instruction("mov r9, 1");                                   // value tag = string
            crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_gd_k_month");
            emitter.instruction("mov rdx, 5");                                  // length of "month"
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert "month" → string
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov rsi, 0");                                  // integer key is literally 0
            emitter.instruction("mov rdx, -1");                                 // key_hi = -1 marks an integer key
            emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                // value_lo = the timestamp
            emitter.instruction("mov r8, 0");                                   // value_hi = 0
            emitter.instruction("mov r9, 0");                                   // value tag = int
            emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");               // reload the hash table pointer
            emitter.instruction("call __rt_hash_set");                          // insert 0 → timestamp
            emitter.instruction("mov QWORD PTR [rbp - 24], rax");               // save the (possibly reallocated) hash table
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // return the assoc array (hash pointer) in rax
            emitter.instruction("add rsp, 32");                                 // deallocate the local slots
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return to caller
        }
    }
}
