//! Purpose:
//! Emits the `__rt_assoc_diff_intersect` runtime helper for array_diff_assoc / array_intersect_assoc.
//! Keeps entries of hash1 whose (key, value) pair is absent from (diff) or present in (intersect) hash2.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Values compare by PHP string cast: `(string)a === (string)b`. Temporary Mixed boxes are released to avoid leaks.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// assoc_diff_intersect: filter hash1 entries by (key, value) membership in hash2.
/// Input:  x0 = hash1, x1 = hash2, x2 = mode (0 = diff, 1 = intersect)
/// Output: x0 = new owned hash with the kept entries (keys/values retained for the result)
///
/// For each hash1 entry, looks up the key in hash2 and compares the values with PHP
/// string-cast equality (`__rt_mixed_from_value` -> `__rt_mixed_cast_string` -> `__rt_str_eq`),
/// releasing both temporary boxes afterward. diff keeps entries whose pair is NOT in hash2;
/// intersect keeps entries whose pair IS in hash2.
pub fn emit_assoc_diff_intersect(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_assoc_diff_intersect_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: assoc_diff_intersect ---");
    emitter.label_global("__rt_assoc_diff_intersect");
    emitter.instruction("sub sp, sp, #208");                                    // allocate the diff/intersect stack frame
    emitter.instruction("stp x29, x30, [sp, #192]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #192");                                   // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save hash2 pointer
    emitter.instruction("str x2, [sp, #24]");                                   // save mode (0 = diff, 1 = intersect)
    emitter.instruction("str x3, [sp, #168]");                                  // remember which converted argument hashes belong to this call
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload hash1 pointer
    emitter.instruction("ldr x0, [x9, #8]");                                    // x0 = hash1 capacity for the result hash
    emitter.instruction("ldr x1, [x9, #16]");                                   // x1 = hash1 value_type summary
    emitter.instruction("bl __rt_hash_new");                                    // create the result hash table, x0 = result
    emitter.instruction("str x0, [sp, #16]");                                   // save the result hash pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // iterator cursor = 0 (start from hash1 head)
    emitter.label("__rt_assoc_diff_intersect_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = hash1 pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // x1 = current iterator cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // next hash1 entry: x0=cursor,x1=kptr,x2=klen,x3=vlo,x4=vhi,x5=vtag
    emitter.instruction("cmn x0, #1");                                          // has iteration reached the end (cursor == -1)?
    emitter.instruction("b.eq __rt_assoc_diff_intersect_done");                 // stop once every hash1 entry has been visited
    emitter.instruction("str x0, [sp, #32]");                                   // save the next iterator cursor
    emitter.instruction("str x1, [sp, #40]");                                   // save key pointer
    emitter.instruction("str x2, [sp, #48]");                                   // save key length
    emitter.instruction("str x3, [sp, #56]");                                   // save hash1 value low word
    emitter.instruction("str x4, [sp, #64]");                                   // save hash1 value high word
    emitter.instruction("str x5, [sp, #72]");                                   // save hash1 value runtime tag
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = hash2 pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // x1 = key low word (key pointer or integer key)
    emitter.instruction("ldr x2, [sp, #48]");                                   // x2 = key high word (-1 marks an integer key)
    emitter.instruction("bl __rt_hash_get");                                    // look up the key in hash2: x0=found,x1=vlo,x2=vhi,x3=vtag
    emitter.instruction("str x1, [sp, #80]");                                   // save hash2 value low word
    emitter.instruction("str x2, [sp, #88]");                                   // save hash2 value high word
    emitter.instruction("str x3, [sp, #96]");                                   // save hash2 value runtime tag
    emitter.instruction("cbz x0, __rt_assoc_diff_intersect_nomatch");           // absent key cannot form a matching pair
    emitter.comment("-- compare the two values by PHP string cast --");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_concat_off", 0);
    emitter.instruction("str x9, [sp, #160]");                                  // save the concat cursor before the comparison casts
    emitter.instruction("ldr x9, [sp, #72]");                                   // inspect the left value before allocating comparison temporaries
    emitter.instruction("cmp x9, #7");                                          // is the hash slot itself a boxed Mixed value?
    let instruction = "b.ne __rt_assoc_diff_intersect_callable_left_tag_ready";
    emitter.instruction(instruction);                                           // direct payload tags are already authoritative
    emitter.instruction("ldr x0, [sp, #56]");                                   // borrow the boxed value to inspect its effective tag
    emitter.instruction("bl __rt_mixed_unbox");                                 // read the PHP value kind without allocating a temporary
    emitter.instruction("mov x9, x0");                                          // use the unboxed PHP type for Closure rejection
    emitter.label("__rt_assoc_diff_intersect_callable_left_tag_ready");
    emitter.instruction("cmp x9, #10");                                         // does PHP need to stringify a Closure?
    emitter.instruction("b.eq __rt_assoc_diff_intersect_callable_error");       // reject callable-to-string conversion with a catchable Error
    emitter.instruction("ldr x0, [sp, #72]");                                   // hash1 value runtime tag
    emitter.instruction("ldr x1, [sp, #56]");                                   // hash1 value low word
    emitter.instruction("ldr x2, [sp, #64]");                                   // hash1 value high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the hash1 value, x0 = box1
    emitter.instruction("str x0, [sp, #104]");                                  // save box1 for later release
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast box1 to string: x1=ptr, x2=len
    emitter.instruction("str x1, [sp, #120]");                                  // save the hash1 value string pointer
    emitter.instruction("str x2, [sp, #128]");                                  // save the hash1 value string length
    emitter.instruction("ldr x9, [sp, #96]");                                   // inspect the right value before allocating comparison temporaries
    emitter.instruction("cmp x9, #7");                                          // is the hash slot itself a boxed Mixed value?
    let instruction = "b.ne __rt_assoc_diff_intersect_callable_right_tag_ready";
    emitter.instruction(instruction);                                           // direct payload tags are already authoritative
    emitter.instruction("ldr x0, [sp, #80]");                                   // borrow the boxed value to inspect its effective tag
    emitter.instruction("bl __rt_mixed_unbox");                                 // read the PHP value kind without allocating a temporary
    emitter.instruction("mov x9, x0");                                          // use the unboxed PHP type for Closure rejection
    emitter.label("__rt_assoc_diff_intersect_callable_right_tag_ready");
    emitter.instruction("cmp x9, #10");                                         // does PHP need to stringify a Closure?
    emitter.instruction("b.eq __rt_assoc_diff_intersect_callable_right_error"); // reject callable-to-string conversion with a catchable Error
    emitter.instruction("ldr x0, [sp, #96]");                                   // hash2 value runtime tag
    emitter.instruction("ldr x1, [sp, #80]");                                   // hash2 value low word
    emitter.instruction("ldr x2, [sp, #88]");                                   // hash2 value high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the hash2 value, x0 = box2
    emitter.instruction("str x0, [sp, #112]");                                  // save box2 for later release
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast box2 to string: x1=ptr, x2=len
    emitter.instruction("str x1, [sp, #144]");                                  // keep the right owned string across equality and box cleanup
    emitter.instruction("str x2, [sp, #152]");                                  // keep its length for complete result bookkeeping
    emitter.instruction("mov x3, x1");                                          // move the hash2 string pointer into the str_eq right operand
    emitter.instruction("mov x4, x2");                                          // move the hash2 string length into the str_eq right operand
    emitter.instruction("ldr x1, [sp, #120]");                                  // reload the hash1 string pointer as the str_eq left operand
    emitter.instruction("ldr x2, [sp, #128]");                                  // reload the hash1 string length as the str_eq left operand
    emitter.instruction("bl __rt_str_eq");                                      // compare the two cast strings, x0 = equal
    emitter.instruction("str x0, [sp, #136]");                                  // save the value-equality result across the box releases
    emitter.instruction("ldr x0, [sp, #104]");                                  // reload box1 for release
    emitter.instruction("bl __rt_decref_mixed");                                // release the temporary hash1 value box
    emitter.instruction("ldr x0, [sp, #112]");                                  // reload box2 for release
    emitter.instruction("bl __rt_decref_mixed");                                // release the temporary hash2 value box
    emitter.instruction("ldr x0, [sp, #120]");                                  // load an independently owned comparison string
    emitter.instruction("bl __rt_heap_free");                                   // retire owned cast storage; scratch and null are ignored
    emitter.instruction("ldr x0, [sp, #144]");                                  // load an independently owned comparison string
    emitter.instruction("bl __rt_heap_free");                                   // retire owned cast storage; scratch and null are ignored
    emitter.instruction("ldr x10, [sp, #160]");                                 // recover the pre-comparison concat cursor
    abi::emit_store_reg_to_symbol(emitter, "x10", "_concat_off", 0);
    emitter.instruction("ldr x0, [sp, #136]");                                  // x0 = pair matches (found and string-equal values)
    emitter.instruction("b __rt_assoc_diff_intersect_decide");                  // decide whether to keep this entry
    emitter.label("__rt_assoc_diff_intersect_nomatch");
    emitter.instruction("mov x0, #0");                                          // the pair does not match (key absent from hash2)
    emitter.label("__rt_assoc_diff_intersect_decide");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the mode selector
    emitter.instruction("cbz x9, __rt_assoc_diff_intersect_diff");              // mode 0 selects difference semantics
    emitter.instruction("cbz x0, __rt_assoc_diff_intersect_skip");              // intersect drops entries whose pair is not in hash2
    emitter.instruction("b __rt_assoc_diff_intersect_keep");                    // intersect keeps matching pairs
    emitter.label("__rt_assoc_diff_intersect_diff");
    emitter.instruction("cbnz x0, __rt_assoc_diff_intersect_skip");             // diff drops entries whose pair is in hash2
    emitter.label("__rt_assoc_diff_intersect_keep");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the hash1 value runtime tag
    emitter.instruction("cmp x9, #1");                                          // is the kept value a string?
    emitter.instruction("b.eq __rt_assoc_diff_intersect_persist");              // strings are persisted as an independent copy
    emitter.instruction("cmp x9, #10");                                         // is this an owned callable descriptor?
    emitter.instruction("b.eq __rt_assoc_diff_intersect_retain");               // callable payloads need an independent result reference
    emitter.instruction("cmp x9, #4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("b.lt __rt_assoc_diff_intersect_insert");               // scalar values need no retain
    emitter.instruction("cmp x9, #7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("b.gt __rt_assoc_diff_intersect_insert");               // non-heap tags need no retain
    emitter.label("__rt_assoc_diff_intersect_retain");
    emitter.instruction("ldr x0, [sp, #56]");                                   // load the kept heap-backed value low word
    emitter.instruction("bl __rt_incref");                                      // retain the kept heap-backed value for the result owner
    emitter.instruction("b __rt_assoc_diff_intersect_insert");                  // continue to the insertion
    emitter.label("__rt_assoc_diff_intersect_persist");
    emitter.instruction("ldr x1, [sp, #56]");                                   // string pointer to persist
    emitter.instruction("ldr x2, [sp, #64]");                                   // string length to persist
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #56]");                                   // store the persisted string pointer
    emitter.instruction("str x2, [sp, #64]");                                   // store the persisted string length
    emitter.label("__rt_assoc_diff_intersect_insert");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload key pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // reload key length
    emitter.instruction("ldr x3, [sp, #56]");                                   // reload value low word
    emitter.instruction("ldr x4, [sp, #64]");                                   // reload value high word
    emitter.instruction("ldr x5, [sp, #72]");                                   // reload value runtime tag
    emitter.instruction("bl __rt_hash_set");                                    // insert the kept entry into the result hash
    emitter.instruction("str x0, [sp, #16]");                                   // update the result pointer after possible reallocation
    emitter.label("__rt_assoc_diff_intersect_skip");
    emitter.instruction("b __rt_assoc_diff_intersect_loop");                    // continue with the next hash1 entry
    emitter.label("__rt_assoc_diff_intersect_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #192]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #208");                                    // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result hash in x0
    super::hash_value_diff_intersect::emit_callable_right_error(emitter, "__rt_assoc_diff_intersect_callable_right_error", "__rt_assoc_diff_intersect_callable_error",
        ("[sp, #104]", "[sp, #120]", "[sp, #160]"));
    super::hash_value_diff_intersect::emit_callable_comparison_error(emitter, "__rt_assoc_diff_intersect_callable_error", 208, "[sp, #168]");

}

/// x86_64 Linux implementation of `__rt_assoc_diff_intersect`.
/// Input:  rdi = hash1, rsi = hash2, rdx = mode (0 = diff, 1 = intersect)
/// Output: rax = new owned hash with the kept entries
fn emit_assoc_diff_intersect_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: assoc_diff_intersect ---");
    emitter.label_global("__rt_assoc_diff_intersect");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 208");                                        // reserve local spill slots for the filter loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save hash1 pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save hash2 pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save mode (0 = diff, 1 = intersect)
    emitter.instruction("mov QWORD PTR [rbp - 184], rcx");                      // remember which converted argument hashes belong to this call
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload hash1 pointer
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // rdi = hash1 capacity for the result hash
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // rsi = hash1 value_type summary
    emitter.instruction("call __rt_hash_new");                                  // create the result hash table, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // iterator cursor = 0 (start from hash1 head)
    emitter.label("__rt_assoc_diff_intersect_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // rdi = hash1 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // rsi = current iterator cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // next hash1 entry: rax=cursor,rdi=kptr,rdx=klen,rcx=vlo,r8=vhi,r9=vtag
    emitter.instruction("cmp rax, -1");                                         // has iteration reached the end?
    emitter.instruction("je __rt_assoc_diff_intersect_done");                   // stop once every hash1 entry has been visited
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the next iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save key pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save key length
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // save hash1 value low word
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save hash1 value high word
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // save hash1 value runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // rdi = hash2 pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // rsi = key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // rdx = key high word (-1 marks an integer key)
    emitter.instruction("call __rt_hash_get");                                  // look up the key in hash2: rax=found,rdi=vlo,rsi=vhi,rcx=vtag
    emitter.instruction("mov QWORD PTR [rbp - 88], rdi");                       // save hash2 value low word
    emitter.instruction("mov QWORD PTR [rbp - 96], rsi");                       // save hash2 value high word
    emitter.instruction("mov QWORD PTR [rbp - 104], rcx");                      // save hash2 value runtime tag
    emitter.instruction("test rax, rax");                                       // was the key present in hash2?
    emitter.instruction("je __rt_assoc_diff_intersect_nomatch");                // absent key cannot form a matching pair
    emitter.comment("-- compare the two values by PHP string cast --");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);
    emitter.instruction("mov QWORD PTR [rbp - 176], r10");                      // save the concat cursor before the comparison casts
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // inspect the left value before allocating comparison temporaries
    emitter.instruction("cmp r10, 7");                                          // is the hash slot itself a boxed Mixed value?
    let instruction = "jne __rt_assoc_diff_intersect_callable_left_tag_ready";
    emitter.instruction(instruction);                                           // direct payload tags are already authoritative
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // borrow the boxed value to inspect its effective tag
    emitter.instruction("call __rt_mixed_unbox");                               // read the PHP value kind without allocating a temporary
    emitter.instruction("mov r10, rax");                                        // use the unboxed PHP type for Closure rejection
    emitter.label("__rt_assoc_diff_intersect_callable_left_tag_ready");
    emitter.instruction("cmp r10, 10");                                         // does PHP need to stringify a Closure?
    emitter.instruction("je __rt_assoc_diff_intersect_callable_error");         // reject callable-to-string conversion with a catchable Error
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // hash1 value runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // hash1 value low word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // hash1 value high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the hash1 value, rax = box1
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                      // save box1 for later release
    emitter.instruction("mov rdi, rax");                                        // pass box1 to the string cast helper
    emitter.instruction("call __rt_mixed_cast_string");                         // cast box1 to string: rax=ptr, rdx=len
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // save the hash1 value string pointer
    emitter.instruction("mov QWORD PTR [rbp - 136], rdx");                      // save the hash1 value string length
    emitter.instruction("mov r10, QWORD PTR [rbp - 104]");                      // inspect the right value before allocating comparison temporaries
    emitter.instruction("cmp r10, 7");                                          // is the hash slot itself a boxed Mixed value?
    let instruction = "jne __rt_assoc_diff_intersect_callable_right_tag_ready";
    emitter.instruction(instruction);                                           // direct payload tags are already authoritative
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // borrow the boxed value to inspect its effective tag
    emitter.instruction("call __rt_mixed_unbox");                               // read the PHP value kind without allocating a temporary
    emitter.instruction("mov r10, rax");                                        // use the unboxed PHP type for Closure rejection
    emitter.label("__rt_assoc_diff_intersect_callable_right_tag_ready");
    emitter.instruction("cmp r10, 10");                                         // does PHP need to stringify a Closure?
    emitter.instruction("je __rt_assoc_diff_intersect_callable_right_error");   // reject callable-to-string conversion with a catchable Error
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // hash2 value runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // hash2 value low word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 96]");                       // hash2 value high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the hash2 value, rax = box2
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                      // save box2 for later release
    emitter.instruction("mov rdi, rax");                                        // pass box2 to the string cast helper
    emitter.instruction("call __rt_mixed_cast_string");                         // cast box2 to string: rax=ptr, rdx=len
    emitter.instruction("mov QWORD PTR [rbp - 160], rax");                      // keep the right owned string across equality and box cleanup
    emitter.instruction("mov QWORD PTR [rbp - 168], rdx");                      // keep its length for complete result bookkeeping
    emitter.instruction("mov rcx, rdx");                                        // move the hash2 string length into the str_eq right length
    emitter.instruction("mov rdx, rax");                                        // move the hash2 string pointer into the str_eq right pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 128]");                      // reload the hash1 string pointer as the str_eq left pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 136]");                      // reload the hash1 string length as the str_eq left length
    emitter.instruction("call __rt_str_eq");                                    // compare the two cast strings, rax = equal
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                      // save the value-equality result across the box releases
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                      // reload box1 for release
    emitter.instruction("call __rt_decref_mixed");                              // release the temporary hash1 value box
    emitter.instruction("mov rax, QWORD PTR [rbp - 120]");                      // reload box2 for release
    emitter.instruction("call __rt_decref_mixed");                              // release the temporary hash2 value box
    emitter.instruction("mov rax, QWORD PTR [rbp - 128]");                      // load an independently owned comparison string
    emitter.instruction("call __rt_heap_free");                                 // retire owned cast storage; scratch and null are ignored
    emitter.instruction("mov rax, QWORD PTR [rbp - 160]");                      // load an independently owned comparison string
    emitter.instruction("call __rt_heap_free");                                 // retire owned cast storage; scratch and null are ignored
    emitter.instruction("mov r10, QWORD PTR [rbp - 176]");                      // recover the pre-comparison concat cursor
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 144]");                      // rax = pair matches (found and string-equal values)
    emitter.instruction("jmp __rt_assoc_diff_intersect_decide");                // decide whether to keep this entry
    emitter.label("__rt_assoc_diff_intersect_nomatch");
    emitter.instruction("xor eax, eax");                                        // the pair does not match (key absent from hash2)
    emitter.label("__rt_assoc_diff_intersect_decide");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the mode selector
    emitter.instruction("test r10, r10");                                       // is the mode difference (0)?
    emitter.instruction("je __rt_assoc_diff_intersect_diff");                   // mode 0 selects difference semantics
    emitter.instruction("test rax, rax");                                       // did the pair match for intersect?
    emitter.instruction("je __rt_assoc_diff_intersect_skip");                   // intersect drops entries whose pair is not in hash2
    emitter.instruction("jmp __rt_assoc_diff_intersect_keep");                  // intersect keeps matching pairs
    emitter.label("__rt_assoc_diff_intersect_diff");
    emitter.instruction("test rax, rax");                                       // did the pair match for diff?
    emitter.instruction("jne __rt_assoc_diff_intersect_skip");                  // diff drops entries whose pair is in hash2
    emitter.label("__rt_assoc_diff_intersect_keep");
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the hash1 value runtime tag
    emitter.instruction("cmp r10, 1");                                          // is the kept value a string?
    emitter.instruction("je __rt_assoc_diff_intersect_persist");                // strings are persisted as an independent copy
    emitter.instruction("cmp r10, 10");                                         // is this an owned callable descriptor?
    emitter.instruction("je __rt_assoc_diff_intersect_retain");                 // callable payloads need an independent result reference
    emitter.instruction("cmp r10, 4");                                          // is the value below the heap-backed tag range?
    emitter.instruction("jl __rt_assoc_diff_intersect_insert");                 // scalar values need no retain
    emitter.instruction("cmp r10, 7");                                          // is the value above the heap-backed tag range?
    emitter.instruction("jg __rt_assoc_diff_intersect_insert");                 // non-heap tags need no retain
    emitter.label("__rt_assoc_diff_intersect_retain");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the kept heap-backed value low word
    emitter.instruction("call __rt_incref");                                    // retain the kept heap-backed value for the result owner
    emitter.instruction("jmp __rt_assoc_diff_intersect_insert");                // continue to the insertion
    emitter.label("__rt_assoc_diff_intersect_persist");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // string pointer to persist
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // string length to persist
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // store the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // store the persisted string length
    emitter.label("__rt_assoc_diff_intersect_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // rdi = result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload key length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 72]");                        // reload value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // reload value runtime tag
    emitter.instruction("call __rt_hash_set");                                  // insert the kept entry into the result hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // update the result pointer after possible reallocation
    emitter.label("__rt_assoc_diff_intersect_skip");
    emitter.instruction("jmp __rt_assoc_diff_intersect_loop");                  // continue with the next hash1 entry
    emitter.label("__rt_assoc_diff_intersect_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 208");                                        // release the local spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax
    super::hash_value_diff_intersect::emit_callable_right_error(emitter, "__rt_assoc_diff_intersect_callable_right_error", "__rt_assoc_diff_intersect_callable_error",
        ("[rbp - 112]", "[rbp - 128]", "[rbp - 176]"));
    super::hash_value_diff_intersect::emit_callable_comparison_error(emitter, "__rt_assoc_diff_intersect_callable_error", 208, "[rbp - 184]");

}

