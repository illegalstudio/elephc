//! Purpose:
//! Emits Windows runtime marshalling helpers for computed `proc_open` arguments.
//!
//! Called from:
//! - `crate::codegen_support::runtime::io::emit_proc_open_marshalling()`.
//!
//! Key details:
//! - Array commands follow the `CommandLineToArgvW` backslash/quote contract,
//!   plus php-src's `cmd.exe`/batch-file caret escaping for later arguments.
//! - Command-processor detection deliberately uses the bounded argv[0] spelling
//!   (basename and ASCII-insensitive `cmd`/`.bat`/`.cmd` checks), rather than
//!   php-src's allocation-heavy full/long-path canonicalization.
//! - Returned buffers are raw runtime-heap allocations owned by the caller.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform};

/// Emits the dynamic Windows `proc_open` argument marshallers when targeting PE.
pub(crate) fn emit_proc_open_marshalling(emitter: &mut Emitter) {
    if emitter.target.arch != Arch::X86_64 || emitter.target.platform != Platform::Windows {
        return;
    }
    emit_command_array(emitter);
    emit_command_uses_cmd(emitter);
    emit_options(emitter);
    emit_scalar_string(emitter);
    emit_key_equal(emitter);
    emit_environment(emitter);
}

/// Emits `__rt_win_proc_environment`, which turns a runtime associative array
/// into an owned, double-NUL UTF-8 block with Windows case-insensitive last-wins
/// de-duplication. Values use PHP scalar-to-string conversion.
fn emit_environment(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: Windows proc_open environment marshalling ---");
    emitter.label_global("__rt_win_proc_environment");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable environment frame
    emitter.instruction("sub rsp, 192");                                        // reserve indexed/hash metadata, iteration, and nested-call state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the runtime environment array
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // metadata allocation = null
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // output block allocation = null
    emitter.instruction("test rdi, rdi");                                       // null means inherit, not a custom block
    emitter.instruction("jz __rt_win_proc_environment_invalid");                // caller should not marshal omitted environment
    emitter.instruction("mov rax, QWORD PTR [rdi - 8]");                        // inspect heap storage kind
    emitter.instruction("and eax, 0xff");                                       // isolate kind byte
    emitter.instruction("cmp eax, 2");                                          // indexed storage represents numeric environment entries
    emitter.instruction("jne __rt_win_proc_environment_hash");                  // associative hashes preserve explicit string keys
    emitter.instruction("mov QWORD PTR [rbp - 136], 2");                        // remember indexed storage for the collection loop
    emitter.instruction("mov rax, QWORD PTR [rdi + 16]");                       // load the indexed element stride
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                      // preserve the stride across value conversion calls
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // numeric environment entries count by array length
    emitter.instruction("jmp __rt_win_proc_environment_count_ready");           // build the required double-NUL block
    emitter.label("__rt_win_proc_environment_hash");
    emitter.instruction("cmp eax, 3");                                          // associative storage kind?
    emitter.instruction("jne __rt_win_proc_environment_invalid");               // reject non-array runtime storage
    emitter.instruction("mov QWORD PTR [rbp - 136], 3");                        // remember associative storage for the collection loop
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass hash to count helper
    emitter.instruction("call __rt_hash_count");                                // number of live insertion-order entries
    emitter.label("__rt_win_proc_environment_count_ready");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve source entry count
    emitter.instruction("test rax, rax");                                       // any metadata rows needed?
    emitter.instruction("jz __rt_win_proc_environment_metadata_ready");         // empty environment skips metadata allocation
    emitter.instruction("mov rcx, 40");                                         // five words per retained entry
    emitter.instruction("mul rcx");                                             // bytes = count * 40, overflow in rdx
    emitter.instruction("test rdx, rdx");                                       // multiplication overflow?
    emitter.instruction("jnz __rt_win_proc_environment_nomem");                 // report unrepresentable allocation as ENOMEM
    emitter.instruction("call __rt_heap_alloc");                                // allocate borrowed-key/value metadata
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // retain metadata ownership
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz __rt_win_proc_environment_nomem");                  // publish ENOMEM
    emitter.label("__rt_win_proc_environment_metadata_ready");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // hash cursor = fresh walk
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // retained/deduplicated entry count = 0
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // processed source entries = 0

    emitter.label("__rt_win_proc_environment_collect_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // processed source entries
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // collected every hash entry?
    emitter.instruction("jae __rt_win_proc_environment_size_start");            // yes, compute exact block size
    emitter.instruction("cmp QWORD PTR [rbp - 136], 2");                        // indexed environment entries have no explicit names
    emitter.instruction("je __rt_win_proc_environment_collect_indexed");        // load the next numeric array value directly
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // hash iterator source
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next");                            // return key plus scalar payload triple
    emitter.instruction("cmp rax, -1");                                         // unexpected early end?
    emitter.instruction("je __rt_win_proc_environment_invalid");                // inconsistent hash metadata is invalid
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist next cursor
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // current key pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // current key length / integer sentinel
    emitter.instruction("mov QWORD PTR [rbp - 72], rcx");                       // current value low word
    emitter.instruction("mov QWORD PTR [rbp - 80], r8");                        // current value high word
    emitter.instruction("mov QWORD PTR [rbp - 88], r9");                        // current value runtime tag
    emitter.instruction("jmp __rt_win_proc_environment_collect_value");         // normalize/validate this associative value

    emitter.label("__rt_win_proc_environment_collect_indexed");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // current indexed environment entry offset
    emitter.instruction("imul rax, QWORD PTR [rbp - 144]");                     // scale by the indexed element stride
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the indexed environment base
    emitter.instruction("add rdi, 24");                                         // skip the indexed-array header
    emitter.instruction("add rdi, rax");                                        // address the current numeric environment value
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // numeric entries do not have a key pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], -1");                        // key-length sentinel means emit the raw converted value
    emitter.instruction("cmp QWORD PTR [rbp - 144], 16");                       // direct string elements use pointer/length pairs
    emitter.instruction("je __rt_win_proc_environment_indexed_string");         // retain their direct string payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the indexed header for its value-type stamp
    emitter.instruction("mov r10, QWORD PTR [r10 - 8]");                        // inspect packed indexed-array metadata
    emitter.instruction("shr r10, 8");                                          // move the value-type tag into the low byte
    emitter.instruction("and r10, 0x7f");                                       // isolate the direct element runtime tag
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the direct scalar or boxed Mixed payload
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve the low payload word
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // direct scalar slots have no high payload word
    emitter.instruction("mov QWORD PTR [rbp - 88], r10");                       // preserve the scalar or boxed-Mixed tag
    emitter.instruction("cmp r10, 0");                                          // direct integer values are valid environment scalars
    emitter.instruction("je __rt_win_proc_environment_collect_value");          // normalize it through the shared converter
    emitter.instruction("cmp r10, 2");                                          // direct float values are valid environment scalars
    emitter.instruction("je __rt_win_proc_environment_collect_value");          // normalize it through the shared converter
    emitter.instruction("cmp r10, 3");                                          // direct boolean values are valid environment scalars
    emitter.instruction("je __rt_win_proc_environment_collect_value");          // normalize it through the shared converter
    emitter.instruction("cmp r10, 7");                                          // boxed Mixed values carry their own scalar tag
    emitter.instruction("je __rt_win_proc_environment_collect_value");          // normalize the nested Mixed cell
    emitter.instruction("cmp r10, 8");                                          // direct null values cast to empty strings
    emitter.instruction("jne __rt_win_proc_environment_invalid");               // arrays/objects/resources cannot populate an environment block
    emitter.instruction("jmp __rt_win_proc_environment_collect_value");         // normalize the null scalar
    emitter.label("__rt_win_proc_environment_indexed_string");
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the direct string pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve the string pointer as value low word
    emitter.instruction("mov rax, QWORD PTR [rdi + 8]");                        // load the direct string byte length
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // preserve the string length as value high word
    emitter.instruction("mov QWORD PTR [rbp - 88], 1");                         // runtime tag 1 identifies a string payload

    emitter.label("__rt_win_proc_environment_collect_value");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // pass the current value tag to the scalar converter
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // pass the current value low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // pass the current value high word
    emitter.instruction("call __rt_win_proc_scalar_string");                    // identify unsupported and empty converted values before retaining rows
    emitter.instruction("cmp rdx, -1");                                         // unsupported environment value?
    emitter.instruction("je __rt_win_proc_environment_fail");                   // preserve the converter's EINVAL and clean up
    emitter.instruction("test rdx, rdx");                                       // php-src omits values whose string conversion is empty
    emitter.instruction("jz __rt_win_proc_environment_skip");                   // do not emit or deduplicate an empty value
    emitter.instruction("cmp QWORD PTR [rbp - 64], -1");                        // numeric key sentinel?
    emitter.instruction("je __rt_win_proc_environment_append");                 // raw numeric entries bypass string-key validation and deduplication
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // empty string keys also become raw environment entries
    emitter.instruction("je __rt_win_proc_environment_raw_key");                // normalize the empty key to the numeric sentinel
    emitter.instruction("xor r10d, r10d");                                      // key validation byte offset = 0
    emitter.label("__rt_win_proc_environment_key_scan");
    emitter.instruction("cmp r10, QWORD PTR [rbp - 64]");                       // scanned the complete key?
    emitter.instruction("jae __rt_win_proc_environment_dedup_start");           // key is representable
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload key base
    emitter.instruction("movzx eax, BYTE PTR [rdi + r10]");                     // load one key byte
    emitter.instruction("test al, al");                                         // embedded NUL?
    emitter.instruction("jz __rt_win_proc_environment_invalid");                // block entries cannot contain NUL
    emitter.instruction("inc r10");                                             // inspect the next key byte
    emitter.instruction("jmp __rt_win_proc_environment_key_scan");              // continue validation

    emitter.label("__rt_win_proc_environment_dedup_start");
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // previous-entry index = 0
    emitter.label("__rt_win_proc_environment_dedup_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload previous index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // compared all retained entries?
    emitter.instruction("jae __rt_win_proc_environment_append");                // no match, append a new row
    emitter.instruction("imul rax, 40");                                        // metadata byte offset
    emitter.instruction("add rax, QWORD PTR [rbp - 24]");                       // previous row address
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // previous key pointer
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // previous key length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // current key pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // current key length
    emitter.instruction("call __rt_win_proc_env_key_equal");                    // Unicode ordinal case-insensitive comparison
    emitter.instruction("cmp rax, -1");                                         // conversion/allocation failure?
    emitter.instruction("je __rt_win_proc_environment_fail");                   // preserve helper errno and clean up
    emitter.instruction("test rax, rax");                                       // case-insensitive duplicate?
    emitter.instruction("jnz __rt_win_proc_environment_replace");               // last occurrence wins
    emitter.instruction("inc QWORD PTR [rbp - 104]");                           // advance previous-entry index
    emitter.instruction("jmp __rt_win_proc_environment_dedup_loop");            // compare against the next retained key

    emitter.label("__rt_win_proc_environment_raw_key");
    emitter.instruction("mov QWORD PTR [rbp - 64], -1");                        // represent an empty string key as php-src's raw entry form
    emitter.instruction("jmp __rt_win_proc_environment_append");                // raw entries do not participate in key deduplication

    emitter.label("__rt_win_proc_environment_append");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // append index = retained count
    emitter.instruction("inc QWORD PTR [rbp - 40]");                            // retain one additional entry
    emitter.instruction("jmp __rt_win_proc_environment_store");                 // store current key/value
    emitter.label("__rt_win_proc_environment_replace");
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // overwrite the case-insensitive match
    emitter.label("__rt_win_proc_environment_store");
    emitter.instruction("imul rax, 40");                                        // metadata byte offset
    emitter.instruction("add rax, QWORD PTR [rbp - 24]");                       // selected metadata row
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // current key pointer
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store borrowed key pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // current key length
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store key length
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // current value low word
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store value low word
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // current value high word
    emitter.instruction("mov QWORD PTR [rax + 24], r10");                       // store value high word
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // current value tag
    emitter.instruction("mov QWORD PTR [rax + 32], r10");                       // store value runtime tag
    emitter.instruction("inc QWORD PTR [rbp - 48]");                            // one more source entry processed
    emitter.instruction("jmp __rt_win_proc_environment_collect_loop");          // collect the next entry

    emitter.label("__rt_win_proc_environment_skip");
    emitter.instruction("inc QWORD PTR [rbp - 48]");                            // consume the omitted empty-value source entry
    emitter.instruction("jmp __rt_win_proc_environment_collect_loop");          // continue collecting later environment entries

    emitter.label("__rt_win_proc_environment_size_start");
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // sizing row index = 0
    emitter.instruction("mov QWORD PTR [rbp - 112], 1");                        // reserve the final extra NUL
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // empty custom environment?
    emitter.instruction("jne __rt_win_proc_environment_size_loop");             // non-empty rows already contribute their first terminator
    emitter.instruction("mov QWORD PTR [rbp - 112], 2");                        // Windows requires two NULs for an empty environment block
    emitter.label("__rt_win_proc_environment_size_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload sizing row
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // sized every retained row?
    emitter.instruction("jae __rt_win_proc_environment_allocate");              // allocate the exact block
    emitter.instruction("imul rax, 40");                                        // metadata byte offset
    emitter.instruction("add rax, QWORD PTR [rbp - 24]");                       // current metadata row
    emitter.instruction("mov rdi, QWORD PTR [rax + 32]");                       // value tag
    emitter.instruction("mov rsi, QWORD PTR [rax + 16]");                       // value low word
    emitter.instruction("mov rdx, QWORD PTR [rax + 24]");                       // value high word
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                      // preserve row across scalar conversion
    emitter.instruction("call __rt_win_proc_scalar_string");                    // PHP scalar-to-string conversion
    emitter.instruction("cmp rdx, -1");                                         // unsupported value?
    emitter.instruction("je __rt_win_proc_environment_fail");                   // helper published EINVAL
    emitter.instruction("mov rax, QWORD PTR [rbp - 120]");                      // restore metadata row
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // key byte length or raw-entry sentinel
    emitter.instruction("cmp rcx, -1");                                         // raw numeric/empty-key entry?
    emitter.instruction("je __rt_win_proc_environment_size_raw");               // raw entries contain only the converted value and terminator
    emitter.instruction("add rcx, rdx");                                        // key + converted value
    emitter.instruction("jc __rt_win_proc_environment_nomem");                  // reject size overflow
    emitter.instruction("add rcx, 2");                                          // '=' plus per-entry NUL
    emitter.instruction("jc __rt_win_proc_environment_nomem");                  // reject delimiter overflow
    emitter.instruction("add QWORD PTR [rbp - 112], rcx");                      // accumulate exact block size
    emitter.instruction("jc __rt_win_proc_environment_nomem");                  // reject total overflow
    emitter.instruction("inc QWORD PTR [rbp - 104]");                           // advance sizing row
    emitter.instruction("jmp __rt_win_proc_environment_size_loop");             // size next entry
    emitter.label("__rt_win_proc_environment_size_raw");
    emitter.instruction("mov rcx, rdx");                                        // raw entries start with the converted value bytes
    emitter.instruction("inc rcx");                                             // include their per-entry NUL terminator
    emitter.instruction("jz __rt_win_proc_environment_nomem");                  // reject an overflowed raw-entry byte count
    emitter.instruction("add QWORD PTR [rbp - 112], rcx");                      // accumulate the raw entry size
    emitter.instruction("jc __rt_win_proc_environment_nomem");                  // reject total raw-block overflow
    emitter.instruction("inc QWORD PTR [rbp - 104]");                           // advance sizing row after the raw entry
    emitter.instruction("jmp __rt_win_proc_environment_size_loop");             // size the next retained row

    emitter.label("__rt_win_proc_environment_allocate");
    emitter.instruction("mov rax, QWORD PTR [rbp - 112]");                      // exact block size including final extra NUL
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned UTF-8 environment block
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // retain output ownership
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz __rt_win_proc_environment_nomem");                  // publish ENOMEM
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // output cursor = block base
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // writing row index = 0
    emitter.label("__rt_win_proc_environment_write_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload writing row
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // wrote every retained entry?
    emitter.instruction("jae __rt_win_proc_environment_write_final_nul");       // append the block terminator
    emitter.instruction("imul rax, 40");                                        // metadata byte offset
    emitter.instruction("add rax, QWORD PTR [rbp - 24]");                       // current row
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                      // preserve row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 128]");                      // destination cursor
    emitter.instruction("mov rsi, QWORD PTR [rax]");                            // key source
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // key byte count
    emitter.instruction("cmp rcx, -1");                                         // raw numeric/empty-key entry?
    emitter.instruction("je __rt_win_proc_environment_write_value");            // raw entries have no key or '=' delimiter
    emitter.instruction("rep movsb");                                           // copy environment name
    emitter.instruction("mov BYTE PTR [rdi], 0x3d");                            // append '='
    emitter.instruction("inc rdi");                                             // advance past '='
    emitter.label("__rt_win_proc_environment_write_value");
    emitter.instruction("mov QWORD PTR [rbp - 128], rdi");                      // preserve cursor across value conversion
    emitter.instruction("mov rax, QWORD PTR [rbp - 120]");                      // restore row
    emitter.instruction("mov rdi, QWORD PTR [rax + 32]");                       // value tag
    emitter.instruction("mov rsi, QWORD PTR [rax + 16]");                       // value low word
    emitter.instruction("mov rdx, QWORD PTR [rax + 24]");                       // value high word
    emitter.instruction("call __rt_win_proc_scalar_string");                    // reproduce scalar string for copying
    emitter.instruction("mov rdi, QWORD PTR [rbp - 128]");                      // restore destination cursor
    emitter.instruction("mov rsi, rax");                                        // converted value source
    emitter.instruction("mov rcx, rdx");                                        // converted value byte count
    emitter.instruction("rep movsb");                                           // copy the scalar text
    emitter.instruction("mov BYTE PTR [rdi], 0");                               // terminate this environment entry
    emitter.instruction("inc rdi");                                             // advance to the next entry
    emitter.instruction("mov QWORD PTR [rbp - 128], rdi");                      // persist output cursor
    emitter.instruction("inc QWORD PTR [rbp - 104]");                           // advance writing row
    emitter.instruction("jmp __rt_win_proc_environment_write_loop");            // write the next entry
    emitter.label("__rt_win_proc_environment_write_final_nul");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 128]");                      // final block cursor
    emitter.instruction("mov BYTE PTR [rdi], 0");                               // append the extra NUL (empty block becomes one NUL only)
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // empty custom environment needs another leading terminator
    emitter.instruction("jne __rt_win_proc_environment_free_metadata");         // non-empty blocks are already double-NUL terminated
    emitter.instruction("mov BYTE PTR [rdi + 1], 0");                           // complete the empty environment's double NUL
    emitter.label("__rt_win_proc_environment_free_metadata");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // metadata allocation
    emitter.instruction("test rax, rax");                                       // non-empty source allocated metadata
    emitter.instruction("jz __rt_win_proc_environment_success");                // empty environment has nothing to release
    emitter.instruction("call __rt_heap_free");                                 // release borrowed metadata rows
    emitter.label("__rt_win_proc_environment_success");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // return owned block pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");                      // return counted block length
    emitter.instruction("add rsp, 192");                                        // release environment frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return environment block pair

    emitter.label("__rt_win_proc_environment_invalid");
    emitter.instruction("mov QWORD PTR [rip + __rt_errno], 22");                // EINVAL: malformed environment runtime shape
    emitter.instruction("jmp __rt_win_proc_environment_fail");                  // clean up partial allocations
    emitter.label("__rt_win_proc_environment_nomem");
    emitter.instruction("mov QWORD PTR [rip + __rt_errno], 12");                // ENOMEM: allocation or size overflow
    emitter.label("__rt_win_proc_environment_fail");
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // optional partial output block
    emitter.instruction("test rax, rax");                                       // output allocated?
    emitter.instruction("jz __rt_win_proc_environment_fail_metadata");          // skip null output
    emitter.instruction("call __rt_heap_free");                                 // release partial block
    emitter.label("__rt_win_proc_environment_fail_metadata");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // optional metadata allocation
    emitter.instruction("test rax, rax");                                       // metadata allocated?
    emitter.instruction("jz __rt_win_proc_environment_fail_return");            // skip null metadata
    emitter.instruction("call __rt_heap_free");                                 // release metadata
    emitter.label("__rt_win_proc_environment_fail_return");
    emitter.instruction("xor eax, eax");                                        // null output signals failure
    emitter.instruction("mov rdx, -1");                                         // distinguish failure from a valid empty environment
    emitter.instruction("add rsp, 192");                                        // release environment frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return failure
}

/// Emits the scalar-to-string conversion used for dynamic environment values.
/// Input is `rdi=tag`, `rsi=value_lo`, `rdx=value_hi`; output is `rax`/`rdx`.
fn emit_scalar_string(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: Windows proc_open scalar environment conversion ---");
    emitter.label_global("__rt_win_proc_scalar_string");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable conversion frame
    emitter.instruction("cmp rdi, 7");                                          // boxed Mixed hash payload?
    emitter.instruction("jne __rt_win_proc_scalar_tag_ready");                  // direct values already expose their runtime tag
    emitter.instruction("mov rax, rsi");                                        // pass the nested Mixed pointer
    emitter.instruction("call __rt_mixed_unbox");                               // rax=tag, rdi=value_lo, rdx=value_hi
    emitter.instruction("mov rsi, rdi");                                        // normalize the low payload register
    emitter.instruction("mov rdi, rax");                                        // normalize the tag register
    emitter.label("__rt_win_proc_scalar_tag_ready");
    emitter.instruction("cmp rdi, 0");                                          // integer payload?
    emitter.instruction("je __rt_win_proc_scalar_int");                         // format with PHP integer semantics
    emitter.instruction("cmp rdi, 1");                                          // string payload?
    emitter.instruction("je __rt_win_proc_scalar_from_string");                 // return the borrowed string pair
    emitter.instruction("cmp rdi, 2");                                          // float payload?
    emitter.instruction("je __rt_win_proc_scalar_float");                       // format with the shared PHP float converter
    emitter.instruction("cmp rdi, 3");                                          // boolean payload?
    emitter.instruction("je __rt_win_proc_scalar_bool");                        // true="1", false=""
    emitter.instruction("cmp rdi, 8");                                          // null payload?
    emitter.instruction("jne __rt_win_proc_scalar_invalid");                    // arrays/objects/resources are not environment scalars
    emitter.instruction("xor eax, eax");                                        // null casts to an empty string pointer
    emitter.instruction("xor edx, edx");                                        // null casts to zero bytes
    emitter.instruction("jmp __rt_win_proc_scalar_done");                       // return the empty string
    emitter.label("__rt_win_proc_scalar_int");
    emitter.instruction("mov rax, rsi");                                        // integer input for the shared formatter
    emitter.instruction("call __rt_itoa");                                      // format a signed PHP integer
    emitter.instruction("jmp __rt_win_proc_scalar_done");                       // return the scratch slice
    emitter.label("__rt_win_proc_scalar_from_string");
    emitter.instruction("mov rax, rsi");                                        // return the borrowed string pointer
    emitter.instruction("jmp __rt_win_proc_scalar_done");                       // length is already in rdx
    emitter.label("__rt_win_proc_scalar_float");
    emitter.instruction("movq xmm0, rsi");                                      // move float bits into the formatter register
    emitter.instruction("call __rt_ftoa");                                      // format with PHP-compatible float semantics
    emitter.instruction("jmp __rt_win_proc_scalar_done");                       // return the scratch slice
    emitter.label("__rt_win_proc_scalar_bool");
    emitter.instruction("test rsi, rsi");                                       // false casts to an empty string
    emitter.instruction("jz __rt_win_proc_scalar_empty");                       // avoid a formatter call for false
    emitter.instruction("mov rax, 1");                                          // true casts exactly to integer text "1"
    emitter.instruction("call __rt_itoa");                                      // materialize the one-byte true string
    emitter.instruction("jmp __rt_win_proc_scalar_done");                       // return "1"
    emitter.label("__rt_win_proc_scalar_empty");
    emitter.instruction("xor eax, eax");                                        // empty scalar string pointer
    emitter.instruction("xor edx, edx");                                        // empty scalar string length
    emitter.instruction("jmp __rt_win_proc_scalar_done");                       // return empty
    emitter.label("__rt_win_proc_scalar_invalid");
    emitter.instruction("mov QWORD PTR [rip + __rt_errno], 22");                // EINVAL: environment value is not scalar
    emitter.instruction("xor eax, eax");                                        // null pointer indicates conversion failure
    emitter.instruction("mov rdx, -1");                                         // distinguish failure from a valid empty string
    emitter.label("__rt_win_proc_scalar_done");
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the borrowed scalar string pair
}

/// Emits a Unicode ordinal case-insensitive comparison for two counted UTF-8
/// environment names. Returns 1 equal, 0 unequal, or -1 on allocation/UTF error.
fn emit_key_equal(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: Windows proc_open environment key comparison ---");
    emitter.label_global("__rt_win_proc_env_key_equal");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable comparison frame
    emitter.instruction("sub rsp, 128");                                        // reserve owned conversions plus native shadow/stack arguments
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // first UTF-8 key pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // first UTF-8 key length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // second UTF-8 key pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // second UTF-8 key length
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // first narrow staging = null
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // first wide key = null
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // second narrow staging = null
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // second wide key = null
    emit_counted_key_conversion(emitter, 8, 16, 40, 48, "first");
    emit_counted_key_conversion(emitter, 24, 32, 56, 64, "second");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // first UTF-16 environment name
    emitter.instruction("mov rdx, -1");                                         // first string is NUL terminated
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // second UTF-16 environment name
    emitter.instruction("mov r9, -1");                                          // second string is NUL terminated
    emitter.instruction("mov QWORD PTR [rsp + 32], 1");                         // bIgnoreCase = TRUE
    emitter.instruction("call CompareStringOrdinal");                           // CSTR_EQUAL=2 for Unicode ordinal equality
    emitter.instruction("cmp eax, 2");                                          // equal result?
    emitter.instruction("sete al");                                             // normalize to one byte
    emitter.instruction("movzx eax, al");                                       // return 0/1
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve result across cleanup calls
    emitter.instruction("jmp __rt_win_proc_env_key_cleanup");                   // release all conversion buffers
    emitter.label("__rt_win_proc_env_key_nomem");
    emitter.instruction("mov QWORD PTR [rip + __rt_errno], 12");                // ENOMEM: key conversion allocation failed
    emitter.instruction("jmp __rt_win_proc_env_key_fail");                      // release partial conversions
    emitter.label("__rt_win_proc_env_key_utf8");
    emitter.instruction("mov QWORD PTR [rip + __rt_errno], 84");                // EILSEQ: environment name is not valid UTF-8
    emitter.label("__rt_win_proc_env_key_fail");
    emitter.instruction("mov QWORD PTR [rbp - 72], -1");                        // propagate comparison failure
    emitter.label("__rt_win_proc_env_key_cleanup");
    for (offset, next) in [(40, "n1"), (48, "w1"), (56, "n2"), (64, "done")] {
        emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {offset}]"));   // load one optional owned conversion buffer
        emitter.instruction("test rax, rax");                                   // was this buffer allocated?
        emitter.instruction(&format!("jz __rt_win_proc_env_key_cleanup_{next}")); // skip null ownership slots
        emitter.instruction("call __rt_heap_free");                             // release this conversion buffer
        emitter.label(&format!("__rt_win_proc_env_key_cleanup_{next}"));
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // restore equality/error result
    emitter.instruction("add rsp, 128");                                        // release conversion and native-call scratch
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return comparison status
}

/// Emits one counted UTF-8 to owned UTF-16 conversion within the key comparator.
fn emit_counted_key_conversion(
    emitter: &mut Emitter,
    pointer_offset: usize,
    length_offset: usize,
    narrow_offset: usize,
    wide_offset: usize,
    suffix: &str,
) {
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {length_offset}]")); // load counted key length
    emitter.instruction("inc rax");                                             // include a NUL terminator
    emitter.instruction("jz __rt_win_proc_env_key_nomem");                      // reject allocation-size overflow
    emitter.instruction("call __rt_heap_alloc");                                // allocate narrow NUL staging
    emitter.instruction(&format!("mov QWORD PTR [rbp - {narrow_offset}], rax")); // retain the owned staging buffer
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz __rt_win_proc_env_key_nomem");                      // publish ENOMEM
    emitter.instruction("mov rdi, rax");                                        // destination narrow cursor
    emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {pointer_offset}]")); // source key bytes
    emitter.instruction(&format!("mov rcx, QWORD PTR [rbp - {length_offset}]")); // source byte count
    emitter.instruction("rep movsb");                                           // copy the complete counted key
    emitter.instruction("mov BYTE PTR [rdi], 0");                               // append NUL
    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {narrow_offset}]")); // pass NUL-terminated UTF-8 key
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // strict UTF-8 conversion
    emitter.instruction(&format!("mov QWORD PTR [rbp - {wide_offset}], rax"));  // retain owned UTF-16 key
    emitter.instruction("test rax, rax");                                       // conversion succeeded?
    emitter.instruction("jz __rt_win_proc_env_key_utf8");                       // reject invalid UTF-8
    emitter.comment(&format!("--- converted {suffix} environment key ---"));
}

/// Emits `__rt_win_proc_options`, which extracts php-src's five documented
/// Windows options as a bit mask.
///
/// php-src's `get_option()` ignores unknown keys and treats only `true` or a
/// non-zero integer as enabled. Keep that permissive behavior here: malformed
/// option values are false rather than a compiler-specific `EINVAL` failure.
fn emit_options(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: Windows proc_open options marshalling ---");
    emitter.label_global("__rt_win_proc_options");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable lookup frame
    emitter.instruction("sub rsp, 64");                                         // reserve five lookup keys and nested-call alignment
    emitter.instruction("xor eax, eax");                                        // default result has no enabled option bits
    emitter.instruction("test rdi, rdi");                                       // null options use PHP defaults
    emitter.instruction("jz __rt_win_proc_options_done");                       // omitted options have no enabled flags
    emitter.instruction("mov rax, QWORD PTR [rdi - 8]");                        // inspect the array storage kind
    emitter.instruction("and eax, 0xff");                                       // isolate the low-byte kind
    emitter.instruction("cmp eax, 2");                                          // indexed storage has only numeric, ignored keys
    emitter.instruction("je __rt_win_proc_options_done");                       // php-src ignores unrecognized numeric entries
    emitter.instruction("cmp eax, 3");                                          // kind 3 identifies associative storage
    emitter.instruction("jne __rt_win_proc_options_done");                      // non-array values cannot carry recognized option keys
    emitter.instruction("mov QWORD PTR [rbp - 8], 0");                          // accumulated Windows option bits
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // preserve the associative options table
    emit_option_lookup(
        emitter,
        "bypass_shell",
        &[0x735f737361707962, 0x6c6c6568],
        12,
        1,
    );
    emit_option_lookup(
        emitter,
        "suppress_errors",
        &[0x7373657270707573, 0x0073726f7272655f],
        15,
        2,
    );
    emit_option_lookup(
        emitter,
        "blocking_pipes",
        &[0x676e696b636f6c62, 0x000073657069705f],
        14,
        4,
    );
    emit_option_lookup(
        emitter,
        "create_process_group",
        &[0x705f657461657263, 0x675f737365636f72, 0x70756f72],
        20,
        8,
    );
    emit_option_lookup(
        emitter,
        "create_new_console",
        &[0x6e5f657461657263, 0x6f736e6f635f7765, 0x656c],
        18,
        16,
    );
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the packed recognized option bits
    emitter.label("__rt_win_proc_options_done");
    emitter.instruction("add rsp, 64");                                         // release lookup scratch
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the packed option bits
}

/// Emits one `__rt_hash_get` lookup and conditionally sets one PHP Windows
/// process-option bit when its value is `true` or a non-zero integer.
fn emit_option_lookup(
    emitter: &mut Emitter,
    name: &str,
    words: &[u64],
    length: usize,
    bit: u8,
) {
    let suffix = name.replace('_', "_");
    let absent = format!("__rt_win_proc_options_{suffix}_absent");
    let tag_ready = format!("__rt_win_proc_options_{suffix}_tag_ready");
    let enabled = format!("__rt_win_proc_options_{suffix}_enabled");
    let done = format!("__rt_win_proc_options_{suffix}_done");
    emitter.comment(&format!("--- read proc_open option {name} ---"));
    for (index, word) in words.iter().enumerate() {
        let offset = 16 + index * 8;
        emitter.instruction(&format!("movabs rax, 0x{word:016x}"));             // materialize eight option-key bytes
        emitter.instruction(&format!("mov QWORD PTR [rbp - {offset}], rax"));   // store the lookup-key chunk
    }
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // associative options hash
    emitter.instruction("lea rsi, [rbp - 16]");                                 // contiguous option-key storage
    emitter.instruction(&format!("mov edx, {length}"));                         // exact option-key byte length
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=value_hi, rcx=value_tag
    emitter.instruction("test rax, rax");                                       // recognized option present?
    emitter.instruction(&format!("jz {absent}"));                               // absent options retain their default false value
    emitter.instruction("cmp rcx, 7");                                          // boxed Mixed payload?
    emitter.instruction(&format!("jne {tag_ready}"));                           // direct hash values already expose their tag
    emitter.instruction("mov rax, rdi");                                        // unbox the nested Mixed cell
    emitter.instruction("call __rt_mixed_unbox");                               // rax=tag, rdi=value_lo, rdx=value_hi
    emitter.instruction("mov rcx, rax");                                        // normalize the tag for scalar matching
    emitter.label(&tag_ready);
    emitter.instruction("cmp rcx, 3");                                          // boolean option value?
    emitter.instruction(&format!("je {enabled}"));                              // booleans use their low payload word
    emitter.instruction("cmp rcx, 0");                                          // integer option value?
    emitter.instruction(&format!("jne {done}"));                                // strings and other values are false in php-src
    emitter.label(&enabled);
    emitter.instruction("test rdi, rdi");                                       // true or non-zero integer?
    emitter.instruction(&format!("jz {done}"));                                 // false/zero leaves the bit clear
    emitter.instruction(&format!("or QWORD PTR [rbp - 8], {bit}"));             // enable this recognized Windows option
    emitter.label(&done);
    emitter.label(&absent);
}

/// Emits `__rt_win_proc_command_array`, which quotes a runtime indexed array of
/// strings and returns an owned UTF-8 command line in `rax`/`rdx`.
fn emit_command_array(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: Windows proc_open command-array marshalling ---");
    emitter.label_global("__rt_win_proc_command_array");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable marshalling frame
    emitter.instruction("sub rsp, 128");                                        // reserve indexed/hash scan state and nested-call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the command-array payload pointer
    emitter.instruction("test rdi, rdi");                                       // reject a null command array
    emitter.instruction("jz __rt_win_proc_command_invalid");                    // null cannot name an executable
    emitter.instruction("mov rax, QWORD PTR [rdi - 8]");                        // load packed indexed-array metadata
    emitter.instruction("and eax, 0xff");                                       // isolate the heap storage kind
    emitter.instruction("cmp eax, 2");                                          // kind 2 identifies indexed storage
    emitter.instruction("je __rt_win_proc_command_indexed");                    // indexed values preserve their natural order
    emitter.instruction("cmp eax, 3");                                          // kind 3 identifies associative storage
    emitter.instruction("jne __rt_win_proc_command_invalid");                   // reject non-array runtime storage
    emitter.instruction("mov QWORD PTR [rbp - 104], 3");                        // associative command arrays ignore keys and iterate insertion-order values
    emitter.instruction("call __rt_hash_count");                                // count the hash values that become argv entries
    emitter.instruction("jmp __rt_win_proc_command_count_ready");               // share empty and argc validation below
    emitter.label("__rt_win_proc_command_indexed");
    emitter.instruction("mov QWORD PTR [rbp - 104], 2");                        // remember indexed storage for direct element loads
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load argc
    emitter.label("__rt_win_proc_command_count_ready");
    emitter.instruction("test rax, rax");                                       // PHP requires a non-empty command array
    emitter.instruction("jz __rt_win_proc_command_invalid");                    // empty argv cannot be executed
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve argc
    emitter.instruction("cmp QWORD PTR [rbp - 104], 3");                        // associative commands do not use an indexed element stride
    emitter.instruction("je __rt_win_proc_command_hash_stride");                // their iterator supplies each tagged value directly
    emitter.instruction("mov rax, QWORD PTR [rdi + 16]");                       // load the runtime element stride
    emitter.instruction("cmp rax, 16");                                         // direct string elements occupy pointer/length pairs
    emitter.instruction("je __rt_win_proc_command_stride_ok");                  // accept a homogeneous string array
    emitter.instruction("cmp rax, 8");                                          // Mixed elements occupy boxed-pointer words
    emitter.instruction("jne __rt_win_proc_command_invalid");                   // no other element representation can be argv
    emitter.instruction("jmp __rt_win_proc_command_stride_ok");                 // retain the validated indexed stride
    emitter.label("__rt_win_proc_command_hash_stride");
    emitter.instruction("xor eax, eax");                                        // associative iteration has no indexed element stride
    emitter.label("__rt_win_proc_command_stride_ok");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the element stride
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // default: argv is not interpreted by cmd.exe
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // load argv[0] for bounded command-processor detection
    emit_load_command_element(emitter, "detect");
    emitter.instruction("test rdx, rdx");                                       // argv[0] must name a non-empty program
    emitter.instruction("jz __rt_win_proc_command_invalid");                    // reject an empty executable name
    emitter.instruction("call __rt_win_proc_command_uses_cmd");                 // detect cmd.exe/cmd/batch basename without path allocation
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // persist command-processor mode across both passes
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // sizing-pass element index = 0
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // output byte count = 0

    emitter.label("__rt_win_proc_command_size_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the sizing index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // processed every argv element?
    emitter.instruction("jae __rt_win_proc_command_allocate");                  // yes, allocate the exact output size
    emit_load_command_element(emitter, "size");
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // preserve the current argument pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // preserve the current argument length
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // executable name is the first argv element
    emitter.instruction("jne __rt_win_proc_command_first_valid");               // later empty arguments are valid and quoted
    emitter.instruction("test rdx, rdx");                                       // empty executable name?
    emitter.instruction("jz __rt_win_proc_command_invalid");                    // PHP rejects an empty argv[0]
    emitter.label("__rt_win_proc_command_first_valid");
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // this argument has no cmd metacharacter yet
    emitter.instruction("xor r8d, r8d");                                        // needs_quotes = false
    emitter.instruction("test rdx, rdx");                                       // empty arguments must be quoted
    emitter.instruction("setz r8b");                                            // remember the empty-argument quoting requirement
    emitter.instruction("xor r9d, r9d");                                        // scan offset = 0
    emitter.label("__rt_win_proc_command_quote_scan");
    emitter.instruction("cmp r9, rdx");                                         // reached the end of this argument?
    emitter.instruction("jae __rt_win_proc_command_quote_known");               // yes, the quote decision is complete
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load one UTF-8 byte
    emitter.instruction("test al, al");                                         // embedded NUL cannot survive CreateProcessW
    emitter.instruction("jz __rt_win_proc_command_invalid");                    // reject truncating argv data
    emitter.instruction("cmp al, 0x20");                                        // ASCII space is a CommandLineToArgvW separator
    emitter.instruction("je __rt_win_proc_command_mark_quote");                 // quote arguments containing spaces
    emitter.instruction("cmp al, 0x09");                                        // tab is the other Windows argv separator
    emitter.instruction("je __rt_win_proc_command_mark_quote");                 // quote arguments containing tabs
    emitter.instruction("cmp al, 0x22");                                        // embedded double quote needs escaping and outer quotes
    emitter.instruction("je __rt_win_proc_command_mark_quote");                 // mark the argument for quoted encoding
    emitter.instruction("inc r9");                                              // advance the quote-decision scan
    emitter.instruction("jmp __rt_win_proc_command_quote_scan");                // inspect the next byte
    emitter.label("__rt_win_proc_command_mark_quote");
    emitter.instruction("mov r8, 1");                                           // record that this argument needs quotes
    emitter.label("__rt_win_proc_command_quote_known");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // argv[0] is not parsed as a command-processor argument
    emitter.instruction("je __rt_win_proc_command_quote_finish");               // retain ordinary executable quoting
    emitter.instruction("cmp QWORD PTR [rbp - 88], 0");                         // cmd.exe or a batch-file target?
    emitter.instruction("je __rt_win_proc_command_quote_finish");               // ordinary executable has no caret escaping
    emitter.instruction("mov r8, 1");                                           // php-src quotes every argument interpreted by cmd.exe
    emitter.instruction("xor r9d, r9d");                                        // scan command-processor metacharacters
    emitter.label("__rt_win_proc_command_special_scan");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 56]");                        // scanned every byte in this command argument?
    emitter.instruction("jae __rt_win_proc_command_quote_finish");              // retain the accumulated special-character flag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload argument base for the metacharacter scan
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load one command argument byte
    emitter.instruction("cmp al, 0x28");                                        // opening parenthesis is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("cmp al, 0x29");                                        // closing parenthesis is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("cmp al, 0x21");                                        // exclamation mark is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("cmp al, 0x5e");                                        // caret itself must be escaped
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("cmp al, 0x22");                                        // a double quote needs cmd.exe escaping too
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("cmp al, 0x3c");                                        // input redirection marker is special
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("cmp al, 0x3e");                                        // output redirection marker is special
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("cmp al, 0x26");                                        // command separator is special
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("cmp al, 0x7c");                                        // pipeline separator is special
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("cmp al, 0x25");                                        // environment expansion marker is special
    emitter.instruction("je __rt_win_proc_command_special_found");              // remember it for caret escaping
    emitter.instruction("inc r9");                                              // advance the metacharacter scan
    emitter.instruction("jmp __rt_win_proc_command_special_scan");              // inspect the next command argument byte
    emitter.label("__rt_win_proc_command_special_found");
    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // enable caret escaping for this entire argument
    emitter.instruction("inc r9");                                              // continue scanning after the metacharacter
    emitter.instruction("jmp __rt_win_proc_command_special_scan");              // find later metacharacters too
    emitter.label("__rt_win_proc_command_quote_finish");
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // preserve needs_quotes for the sizing walk
    emitter.instruction("test r8, r8");                                         // unquoted arguments copy byte-for-byte
    emitter.instruction("jnz __rt_win_proc_command_size_quoted");               // quoted arguments need backslash expansion
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload accumulated output size
    emitter.instruction("add rax, rdx");                                        // add the raw argument length
    emitter.instruction("jc __rt_win_proc_command_nomem");                      // size_t overflow is reported as ENOMEM
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the enlarged output size
    emitter.instruction("jmp __rt_win_proc_command_size_separator");            // account for the separator

    emitter.label("__rt_win_proc_command_size_quoted");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload accumulated output size
    emitter.instruction("add rax, 2");                                          // include opening and closing quotes
    emitter.instruction("jc __rt_win_proc_command_nomem");                      // reject quote-size overflow
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // this cmd.exe argument has metacharacters?
    emitter.instruction("je __rt_win_proc_command_size_outer_ready");           // ordinary quotes need no caret prefix
    emitter.instruction("add rax, 2");                                          // prefix both surrounding quotes with carets
    emitter.instruction("jc __rt_win_proc_command_nomem");                      // reject caret-size overflow
    emitter.label("__rt_win_proc_command_size_outer_ready");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist quote bytes
    emitter.instruction("xor r9d, r9d");                                        // byte offset = 0
    emitter.instruction("xor r10d, r10d");                                      // pending backslash run = 0
    emitter.label("__rt_win_proc_command_size_bytes");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 56]");                        // consumed the whole argument?
    emitter.instruction("jae __rt_win_proc_command_size_trailing");             // double trailing backslashes before the close quote
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the argument pointer
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load the next argument byte
    emitter.instruction("cmp al, 0x5c");                                        // backslash?
    emitter.instruction("jne __rt_win_proc_command_size_non_slash");            // flush the accumulated run for another byte
    emitter.instruction("inc r10");                                             // extend the pending backslash run
    emitter.instruction("inc r9");                                              // consume this backslash
    emitter.instruction("jmp __rt_win_proc_command_size_bytes");                // keep scanning the run
    emitter.label("__rt_win_proc_command_size_non_slash");
    emitter.instruction("cmp al, 0x22");                                        // embedded quote doubles preceding backslashes
    emitter.instruction("jne __rt_win_proc_command_size_plain");                // plain bytes preserve the run unchanged
    emitter.instruction("shl r10, 1");                                          // double the preceding backslashes
    emitter.instruction("inc r10");                                             // add the escape slash before the quote
    emitter.label("__rt_win_proc_command_size_plain");
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // command-processor caret escaping enabled?
    emitter.instruction("je __rt_win_proc_command_size_no_caret");              // ordinary quoted bytes need no extra prefix
    emitter.instruction("cmp al, 0x28");                                        // opening parenthesis is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_size_caret");                 // include its caret prefix
    emitter.instruction("cmp al, 0x29");                                        // closing parenthesis is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_size_caret");                 // include its caret prefix
    emitter.instruction("cmp al, 0x21");                                        // exclamation mark is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_size_caret");                 // include its caret prefix
    emitter.instruction("cmp al, 0x5e");                                        // caret itself is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_size_caret");                 // include its caret prefix
    emitter.instruction("cmp al, 0x22");                                        // quote is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_size_caret");                 // include its caret prefix
    emitter.instruction("cmp al, 0x3c");                                        // input redirection marker is special
    emitter.instruction("je __rt_win_proc_command_size_caret");                 // include its caret prefix
    emitter.instruction("cmp al, 0x3e");                                        // output redirection marker is special
    emitter.instruction("je __rt_win_proc_command_size_caret");                 // include its caret prefix
    emitter.instruction("cmp al, 0x26");                                        // command separator is special
    emitter.instruction("je __rt_win_proc_command_size_caret");                 // include its caret prefix
    emitter.instruction("cmp al, 0x7c");                                        // pipeline separator is special
    emitter.instruction("je __rt_win_proc_command_size_caret");                 // include its caret prefix
    emitter.instruction("cmp al, 0x25");                                        // environment expansion marker is special
    emitter.instruction("jne __rt_win_proc_command_size_no_caret");             // non-special byte has no caret prefix
    emitter.label("__rt_win_proc_command_size_caret");
    emitter.instruction("inc r10");                                             // count the caret before this metacharacter
    emitter.label("__rt_win_proc_command_size_no_caret");
    emitter.instruction("inc r10");                                             // include the current non-backslash byte
    emitter.instruction("add QWORD PTR [rbp - 40], r10");                       // append the encoded run and byte length
    emitter.instruction("jc __rt_win_proc_command_nomem");                      // reject accumulated-size overflow
    emitter.instruction("xor r10d, r10d");                                      // reset the pending slash run
    emitter.instruction("inc r9");                                              // consume the current byte
    emitter.instruction("jmp __rt_win_proc_command_size_bytes");                // continue sizing this argument
    emitter.label("__rt_win_proc_command_size_trailing");
    emitter.instruction("shl r10, 1");                                          // trailing slashes are doubled before a closing quote
    emitter.instruction("add QWORD PTR [rbp - 40], r10");                       // include the expanded trailing run
    emitter.instruction("jc __rt_win_proc_command_nomem");                      // reject final-run overflow

    emitter.label("__rt_win_proc_command_size_separator");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload element index
    emitter.instruction("inc rax");                                             // advance to the next argv element
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist the next index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // is another argument coming?
    emitter.instruction("jae __rt_win_proc_command_size_loop");                 // no separator after the final argument
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // include one inter-argument space
    emitter.instruction("jc __rt_win_proc_command_nomem");                      // reject separator overflow
    emitter.instruction("jmp __rt_win_proc_command_size_loop");                 // size the next argument

    emitter.label("__rt_win_proc_command_allocate");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // exact command-line byte length
    emitter.instruction("inc rax");                                             // include a defensive trailing NUL
    emitter.instruction("jz __rt_win_proc_command_nomem");                      // wrapped allocation size is invalid
    emitter.instruction("call __rt_heap_alloc");                                // allocate the raw command-line staging buffer
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz __rt_win_proc_command_nomem");                      // publish ENOMEM on allocation failure
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // preserve the owned output base
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // initialize the output cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // writing-pass element index = 0

    emitter.label("__rt_win_proc_command_write_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload writing index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // emitted every argument?
    emitter.instruction("jae __rt_win_proc_command_done");                      // terminate and return the completed buffer
    emit_load_command_element(emitter, "write");
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // preserve argument pointer for writing
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // preserve argument length for writing
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // this argument has no cmd metacharacter yet
    emitter.instruction("xor r8d, r8d");                                        // needs_quotes = false
    emitter.instruction("test rdx, rdx");                                       // empty strings require quotes
    emitter.instruction("setz r8b");                                            // seed the quoting decision
    emitter.instruction("xor r9d, r9d");                                        // decision scan offset = 0
    emitter.label("__rt_win_proc_command_write_quote_scan");
    emitter.instruction("cmp r9, rdx");                                         // finished the decision scan?
    emitter.instruction("jae __rt_win_proc_command_write_quote_known");         // proceed with the selected encoding
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load one source byte
    emitter.instruction("cmp al, 0x20");                                        // space separator?
    emitter.instruction("je __rt_win_proc_command_write_mark_quote");           // quote the argument
    emitter.instruction("cmp al, 0x09");                                        // tab separator?
    emitter.instruction("je __rt_win_proc_command_write_mark_quote");           // quote the argument
    emitter.instruction("cmp al, 0x22");                                        // embedded quote?
    emitter.instruction("je __rt_win_proc_command_write_mark_quote");           // quote and escape the argument
    emitter.instruction("inc r9");                                              // inspect the next source byte
    emitter.instruction("jmp __rt_win_proc_command_write_quote_scan");          // continue the decision scan
    emitter.label("__rt_win_proc_command_write_mark_quote");
    emitter.instruction("mov r8, 1");                                           // select quoted encoding
    emitter.label("__rt_win_proc_command_write_quote_known");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // argv[0] is not parsed as a command-processor argument
    emitter.instruction("je __rt_win_proc_command_write_quote_finish");         // retain ordinary executable quoting
    emitter.instruction("cmp QWORD PTR [rbp - 88], 0");                         // cmd.exe or a batch-file target?
    emitter.instruction("je __rt_win_proc_command_write_quote_finish");         // ordinary executable has no caret escaping
    emitter.instruction("mov r8, 1");                                           // php-src quotes every argument interpreted by cmd.exe
    emitter.instruction("xor r9d, r9d");                                        // scan command-processor metacharacters
    emitter.label("__rt_win_proc_command_write_special_scan");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 56]");                        // scanned every byte in this command argument?
    emitter.instruction("jae __rt_win_proc_command_write_quote_finish");        // retain the accumulated special-character flag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload argument base for the metacharacter scan
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load one command argument byte
    emitter.instruction("cmp al, 0x28");                                        // opening parenthesis is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("cmp al, 0x29");                                        // closing parenthesis is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("cmp al, 0x21");                                        // exclamation mark is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("cmp al, 0x5e");                                        // caret itself must be escaped
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("cmp al, 0x22");                                        // a double quote needs cmd.exe escaping too
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("cmp al, 0x3c");                                        // input redirection marker is special
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("cmp al, 0x3e");                                        // output redirection marker is special
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("cmp al, 0x26");                                        // command separator is special
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("cmp al, 0x7c");                                        // pipeline separator is special
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("cmp al, 0x25");                                        // environment expansion marker is special
    emitter.instruction("je __rt_win_proc_command_write_special_found");        // remember it for caret escaping
    emitter.instruction("inc r9");                                              // advance the metacharacter scan
    emitter.instruction("jmp __rt_win_proc_command_write_special_scan");        // inspect the next command argument byte
    emitter.label("__rt_win_proc_command_write_special_found");
    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // enable caret escaping for this entire argument
    emitter.instruction("inc r9");                                              // continue scanning after the metacharacter
    emitter.instruction("jmp __rt_win_proc_command_write_special_scan");        // find later metacharacters too
    emitter.label("__rt_win_proc_command_write_quote_finish");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // restore source pointer after the scan
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // load destination cursor
    emitter.instruction("test r8, r8");                                         // quoted or raw copy?
    emitter.instruction("jnz __rt_win_proc_command_write_quoted");              // expand quoted arguments
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // raw argument byte count
    emitter.instruction("xchg rdi, rsi");                                       // rep movsb requires source in rsi and destination in rdi
    emitter.instruction("rep movsb");                                           // copy an unquoted argument byte-for-byte
    emitter.instruction("mov rsi, rdi");                                        // restore the destination cursor convention used below
    emitter.instruction("jmp __rt_win_proc_command_write_separator");           // append the optional separator

    emitter.label("__rt_win_proc_command_write_quoted");
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // cmd.exe metacharacter escaping required?
    emitter.instruction("je __rt_win_proc_command_write_open_quote");           // ordinary quote starts the encoded argument
    emitter.instruction("mov BYTE PTR [rsi], 0x5e");                            // caret-escape the opening quote for cmd.exe
    emitter.instruction("inc rsi");                                             // advance past the opening quote caret
    emitter.label("__rt_win_proc_command_write_open_quote");
    emitter.instruction("mov BYTE PTR [rsi], 0x22");                            // opening quote
    emitter.instruction("inc rsi");                                             // advance destination past the quote
    emitter.instruction("xor r9d, r9d");                                        // source offset = 0
    emitter.instruction("xor r10d, r10d");                                      // pending slash count = 0
    emitter.label("__rt_win_proc_command_write_bytes");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 56]");                        // consumed the argument?
    emitter.instruction("jae __rt_win_proc_command_write_trailing");            // flush trailing slashes and close quotes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload source base
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load current byte
    emitter.instruction("cmp al, 0x5c");                                        // backslash?
    emitter.instruction("jne __rt_win_proc_command_write_non_slash");           // delay slash emission until the following byte is known
    emitter.instruction("inc r10");                                             // count this pending slash
    emitter.instruction("inc r9");                                              // consume it
    emitter.instruction("jmp __rt_win_proc_command_write_bytes");               // continue the slash run
    emitter.label("__rt_win_proc_command_write_non_slash");
    emitter.instruction("mov rcx, r10");                                        // begin with the literal pending slash count
    emitter.instruction("cmp al, 0x22");                                        // embedded quote?
    emitter.instruction("jne __rt_win_proc_command_write_slashes");             // plain bytes retain the slash run
    emitter.instruction("shl rcx, 1");                                          // double slashes before a quote
    emitter.instruction("inc rcx");                                             // add the quote escape slash
    emitter.label("__rt_win_proc_command_write_slashes");
    emitter.instruction("test rcx, rcx");                                       // is there a slash run to flush?
    emitter.instruction("jz __rt_win_proc_command_write_byte");                 // no, copy the current byte directly
    emitter.label("__rt_win_proc_command_write_slash_loop");
    emitter.instruction("mov BYTE PTR [rsi], 0x5c");                            // write one required slash
    emitter.instruction("inc rsi");                                             // advance output
    emitter.instruction("loop __rt_win_proc_command_write_slash_loop");         // emit the complete expanded slash run
    emitter.label("__rt_win_proc_command_write_byte");
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // command-processor caret escaping enabled?
    emitter.instruction("je __rt_win_proc_command_write_plain_byte");           // ordinary quoted bytes need no extra prefix
    emitter.instruction("cmp al, 0x28");                                        // opening parenthesis is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_write_caret");                // emit its caret prefix
    emitter.instruction("cmp al, 0x29");                                        // closing parenthesis is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_write_caret");                // emit its caret prefix
    emitter.instruction("cmp al, 0x21");                                        // exclamation mark is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_write_caret");                // emit its caret prefix
    emitter.instruction("cmp al, 0x5e");                                        // caret itself is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_write_caret");                // emit its caret prefix
    emitter.instruction("cmp al, 0x22");                                        // quote is special to cmd.exe
    emitter.instruction("je __rt_win_proc_command_write_caret");                // emit its caret prefix
    emitter.instruction("cmp al, 0x3c");                                        // input redirection marker is special
    emitter.instruction("je __rt_win_proc_command_write_caret");                // emit its caret prefix
    emitter.instruction("cmp al, 0x3e");                                        // output redirection marker is special
    emitter.instruction("je __rt_win_proc_command_write_caret");                // emit its caret prefix
    emitter.instruction("cmp al, 0x26");                                        // command separator is special
    emitter.instruction("je __rt_win_proc_command_write_caret");                // emit its caret prefix
    emitter.instruction("cmp al, 0x7c");                                        // pipeline separator is special
    emitter.instruction("je __rt_win_proc_command_write_caret");                // emit its caret prefix
    emitter.instruction("cmp al, 0x25");                                        // environment expansion marker is special
    emitter.instruction("jne __rt_win_proc_command_write_plain_byte");          // non-special byte has no caret prefix
    emitter.label("__rt_win_proc_command_write_caret");
    emitter.instruction("mov BYTE PTR [rsi], 0x5e");                            // write the caret before this metacharacter
    emitter.instruction("inc rsi");                                             // advance after the caret prefix
    emitter.label("__rt_win_proc_command_write_plain_byte");
    emitter.instruction("mov BYTE PTR [rsi], al");                              // copy the non-backslash byte
    emitter.instruction("inc rsi");                                             // advance output
    emitter.instruction("xor r10d, r10d");                                      // reset pending slash count
    emitter.instruction("inc r9");                                              // consume the current source byte
    emitter.instruction("jmp __rt_win_proc_command_write_bytes");               // continue encoding
    emitter.label("__rt_win_proc_command_write_trailing");
    emitter.instruction("lea rcx, [r10 + r10]");                                // double trailing slashes before the closing quote
    emitter.label("__rt_win_proc_command_write_trailing_slashes");
    emitter.instruction("test rcx, rcx");                                       // any trailing slash left?
    emitter.instruction("jz __rt_win_proc_command_write_close");                // no, append the closing quote
    emitter.instruction("mov BYTE PTR [rsi], 0x5c");                            // write one doubled trailing slash
    emitter.instruction("inc rsi");                                             // advance output
    emitter.instruction("dec rcx");                                             // consume one slash
    emitter.instruction("jmp __rt_win_proc_command_write_trailing_slashes");    // flush the full run
    emitter.label("__rt_win_proc_command_write_close");
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // cmd.exe metacharacter escaping required?
    emitter.instruction("je __rt_win_proc_command_write_close_quote");          // ordinary quote closes the encoded argument
    emitter.instruction("mov BYTE PTR [rsi], 0x5e");                            // caret-escape the closing quote for cmd.exe
    emitter.instruction("inc rsi");                                             // advance past the closing quote caret
    emitter.label("__rt_win_proc_command_write_close_quote");
    emitter.instruction("mov BYTE PTR [rsi], 0x22");                            // closing quote
    emitter.instruction("inc rsi");                                             // advance output past it

    emitter.label("__rt_win_proc_command_write_separator");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload current element index
    emitter.instruction("inc rax");                                             // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist the writing index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // is this the final argument?
    emitter.instruction("jae __rt_win_proc_command_store_cursor");              // omit separator after the final argument
    emitter.instruction("mov BYTE PTR [rsi], 0x20");                            // append one inter-argument space
    emitter.instruction("inc rsi");                                             // advance destination
    emitter.label("__rt_win_proc_command_store_cursor");
    emitter.instruction("mov QWORD PTR [rbp - 80], rsi");                       // persist destination cursor
    emitter.instruction("jmp __rt_win_proc_command_write_loop");                // emit the next argument

    emitter.label("__rt_win_proc_command_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // load the final output cursor
    emitter.instruction("mov BYTE PTR [rax], 0");                               // append the defensive NUL terminator
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // return the owned output base
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the counted byte length
    emitter.instruction("add rsp, 128");                                        // release marshalling state
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the command-line pair

    emitter.label("__rt_win_proc_command_invalid");
    emitter.instruction("mov QWORD PTR [rip + __rt_errno], 22");                // EINVAL: invalid argv shape or element
    emitter.instruction("jmp __rt_win_proc_command_fail");                      // return a null buffer
    emitter.label("__rt_win_proc_command_nomem");
    emitter.instruction("mov QWORD PTR [rip + __rt_errno], 12");                // ENOMEM: allocation or size overflow
    emitter.label("__rt_win_proc_command_fail");
    emitter.instruction("xor eax, eax");                                        // null output pointer signals failure
    emitter.instruction("xor edx, edx");                                        // failed output has zero length
    emitter.instruction("add rsp, 128");                                        // release marshalling state
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return failure
}

/// Emits the shared load sequence for one command value. Indexed arrays use
/// their stamped element representation; associative arrays ignore keys and
/// re-walk insertion-order values, matching php-src's `ZEND_HASH_FOREACH_VAL`.
fn emit_load_command_element(emitter: &mut Emitter, suffix: &str) {
    let hash = format!("__rt_win_proc_command_{suffix}_hash");
    let hash_loop = format!("__rt_win_proc_command_{suffix}_hash_loop");
    let hash_value = format!("__rt_win_proc_command_{suffix}_hash_value");
    let mixed = format!("__rt_win_proc_command_{suffix}_mixed");
    let scalar = format!("__rt_win_proc_command_{suffix}_scalar");
    let ready = format!("__rt_win_proc_command_{suffix}_ready");
    emitter.instruction("cmp QWORD PTR [rbp - 104], 3");                        // associative commands must iterate their values rather than indexed slots
    emitter.instruction(&format!("je {hash}"));                                 // associative keys are intentionally ignored by php-src
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // load the current element index
    emitter.instruction("imul rax, QWORD PTR [rbp - 24]");                      // scale by the runtime element stride
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the indexed-array payload
    emitter.instruction("add rdi, 24");                                         // skip the indexed-array header
    emitter.instruction("add rdi, rax");                                        // address the selected element
    emitter.instruction("cmp QWORD PTR [rbp - 24], 8");                         // direct string elements use pointer/length pairs
    emitter.instruction(&format!("je {mixed}"));                                // scalar or Mixed element conversion uses the value-type stamp
    emitter.instruction("mov rdx, QWORD PTR [rdi + 8]");                        // direct string length
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // direct string pointer
    emitter.instruction(&format!("jmp {ready}"));                               // join the validated string path
    emitter.label(&mixed);
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the indexed-array base for its value-type stamp
    emitter.instruction("mov r10, QWORD PTR [r10 - 8]");                        // inspect packed indexed-array metadata
    emitter.instruction("shr r10, 8");                                          // move the value-type tag into the low byte
    emitter.instruction("and r10, 0x7f");                                       // isolate the storage value-type tag
    emitter.instruction("cmp r10, 7");                                          // boxed Mixed element representation?
    emitter.instruction(&format!("je {mixed}_unbox"));                          // unbox the nested cell before scalar conversion
    emitter.instruction("cmp r10, 0");                                          // direct integer elements need scalar conversion
    emitter.instruction(&format!("je {scalar}"));                               // pass direct integers to the shared PHP converter
    emitter.instruction("cmp r10, 2");                                          // direct float elements need scalar conversion
    emitter.instruction(&format!("je {scalar}"));                               // pass direct floats to the shared PHP converter
    emitter.instruction("cmp r10, 3");                                          // direct boolean elements need scalar conversion
    emitter.instruction(&format!("je {scalar}"));                               // pass direct booleans to the shared PHP converter
    emitter.instruction("cmp r10, 8");                                          // direct null elements need scalar conversion
    emitter.instruction("jne __rt_win_proc_command_invalid");                   // arrays/objects/resources cannot name command arguments
    emitter.label(&scalar);
    emitter.instruction("mov rsi, QWORD PTR [rdi]");                            // load the direct scalar payload
    emitter.instruction("mov rdi, r10");                                        // pass its runtime value tag to the scalar converter
    emitter.instruction("xor edx, edx");                                        // direct scalar slots have no high payload word
    emitter.instruction("call __rt_win_proc_scalar_string");                    // apply PHP scalar-to-string conversion
    emitter.instruction("cmp rdx, -1");                                         // unsupported scalar conversion?
    emitter.instruction("je __rt_win_proc_command_invalid");                    // retain the converter's EINVAL result
    emitter.instruction(&format!("jmp {ready}"));                               // expose the converted string pair
    emitter.label(&format!("{mixed}_unbox"));
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the boxed Mixed element pointer
    emitter.instruction("call __rt_mixed_unbox");                               // rax=tag, rdi=payload pointer, rdx=payload length
    emitter.instruction("mov rsi, rdi");                                        // move the unboxed low payload into scalar-converter input
    emitter.instruction("mov rdi, rax");                                        // move the unboxed runtime tag into scalar-converter input
    emitter.instruction("call __rt_win_proc_scalar_string");                    // apply PHP scalar-to-string conversion
    emitter.instruction("cmp rdx, -1");                                         // unsupported Mixed payload?
    emitter.instruction("je __rt_win_proc_command_invalid");                    // retain the converter's EINVAL result
    emitter.instruction(&format!("jmp {ready}"));                               // expose the converted string pair
    emitter.label(&hash);
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // restart the insertion-order hash cursor for this argv position
    emitter.instruction("mov QWORD PTR [rbp - 120], 0");                        // hash value index = 0
    emitter.label(&hash_loop);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the associative command hash to its iterator
    emitter.instruction("mov rsi, QWORD PTR [rbp - 112]");                      // pass the iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch key plus the next insertion-order value triple
    emitter.instruction("cmp rax, -1");                                         // did the hash end before its advertised count?
    emitter.instruction("je __rt_win_proc_command_invalid");                    // malformed storage cannot safely produce argv
    emitter.instruction("mov QWORD PTR [rbp - 112], rax");                      // persist the next iterator cursor
    emitter.instruction("mov rax, QWORD PTR [rbp - 120]");                      // reload the current hash value index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 32]");                       // did this value correspond to the requested argv element?
    emitter.instruction(&format!("je {hash_value}"));                           // convert this hash value and ignore its key
    emitter.instruction("inc QWORD PTR [rbp - 120]");                           // advance to the next hash value
    emitter.instruction(&format!("jmp {hash_loop}"));                           // continue until the requested value is reached
    emitter.label(&hash_value);
    emitter.instruction("mov rdi, r9");                                         // pass the hash value's runtime tag to the scalar converter
    emitter.instruction("mov rsi, rcx");                                        // pass the hash value's low payload word
    emitter.instruction("mov rdx, r8");                                         // pass the hash value's high payload word
    emitter.instruction("call __rt_win_proc_scalar_string");                    // convert the associative value with PHP rules
    emitter.instruction("cmp rdx, -1");                                         // unsupported hash value?
    emitter.instruction("je __rt_win_proc_command_invalid");                    // retain the converter's EINVAL result
    emitter.label(&ready);
}

/// Emits the bounded command-processor detector used by runtime argv marshalling.
///
/// Input is a counted UTF-8 `rdi`/`rdx` argv[0] pair; `rax` is one when its
/// final slash-separated component is `cmd`, `cmd.exe`, or ends in `.bat` or
/// `.cmd` under ASCII case-insensitive comparison. This intentionally avoids
/// php-src's `GetLongPathNameW`/`GetFullPathNameW` allocation and resolution
/// path while matching the direct literal path's bounded spelling contract.
fn emit_command_uses_cmd(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: Windows proc_open command-processor detection ---");
    emitter.label_global("__rt_win_proc_command_uses_cmd");
    emitter.instruction("xor eax, eax");                                        // default result: ordinary executable
    emitter.instruction("xor ecx, ecx");                                        // scan argv[0] from its first byte
    emitter.instruction("xor r8d, r8d");                                        // basename begins at argv[0] initially
    emitter.label("__rt_win_proc_command_uses_cmd_scan");
    emitter.instruction("cmp rcx, rdx");                                        // consumed the complete executable spelling?
    emitter.instruction("jae __rt_win_proc_command_uses_cmd_basename");         // inspect its final path component
    emitter.instruction("movzx r9d, BYTE PTR [rdi + rcx]");                     // load one executable-name byte
    emitter.instruction("cmp r9b, 0x5c");                                       // Windows path separator?
    emitter.instruction("je __rt_win_proc_command_uses_cmd_separator");         // begin a new basename after it
    emitter.instruction("cmp r9b, 0x2f");                                       // accept forward slashes in portable source spellings
    emitter.instruction("je __rt_win_proc_command_uses_cmd_separator");         // begin a new basename after it
    emitter.instruction("inc rcx");                                             // continue through this path component
    emitter.instruction("jmp __rt_win_proc_command_uses_cmd_scan");             // inspect the next executable-name byte
    emitter.label("__rt_win_proc_command_uses_cmd_separator");
    emitter.instruction("lea r8, [rcx + 1]");                                   // basename begins just after the latest separator
    emitter.instruction("inc rcx");                                             // continue scanning after the separator
    emitter.instruction("jmp __rt_win_proc_command_uses_cmd_scan");             // find the final path component
    emitter.label("__rt_win_proc_command_uses_cmd_basename");
    emitter.instruction("sub rdx, r8");                                         // isolate the basename byte length
    emitter.instruction("add rdi, r8");                                         // point at the final path component
    emitter.instruction("cmp rdx, 3");                                          // bare `cmd` has three bytes
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_exe");              // try cmd.exe or a batch extension
    emitter.instruction("movzx ecx, BYTE PTR [rdi]");                           // load the first bare command byte
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x63");                                        // expect `c`
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_exe");              // not the command processor
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 1]");                       // load the second bare command byte
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x6d");                                        // expect `m`
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_exe");              // not the command processor
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 2]");                       // load the third bare command byte
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x64");                                        // expect `d`
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_exe");              // not the command processor
    emitter.instruction("mov eax, 1");                                          // bare cmd.exe spelling is handled by the command processor
    emitter.instruction("ret");                                                 // return the positive detection result
    emitter.label("__rt_win_proc_command_uses_cmd_exe");
    emitter.instruction("cmp rdx, 7");                                          // cmd.exe has seven bytes
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_extension");        // inspect batch-file extensions instead
    emitter.instruction("movzx ecx, BYTE PTR [rdi]");                           // load cmd.exe byte zero
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x63");                                        // expect `c`
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_extension");        // not cmd.exe
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 1]");                       // load cmd.exe byte one
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x6d");                                        // expect `m`
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_extension");        // not cmd.exe
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 2]");                       // load cmd.exe byte two
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x64");                                        // expect `d`
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_extension");        // not cmd.exe
    emitter.instruction("cmp BYTE PTR [rdi + 3], 0x2e");                        // cmd.exe requires its literal dot
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_extension");        // not cmd.exe
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 4]");                       // load cmd.exe extension byte zero
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x65");                                        // expect `e`
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_extension");        // not cmd.exe
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 5]");                       // load cmd.exe extension byte one
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x78");                                        // expect `x`
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_extension");        // not cmd.exe
    emitter.instruction("movzx ecx, BYTE PTR [rdi + 6]");                       // load cmd.exe extension byte two
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x65");                                        // expect `e`
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_extension");        // not cmd.exe
    emitter.instruction("mov eax, 1");                                          // cmd.exe is interpreted by the command processor
    emitter.instruction("ret");                                                 // return the positive detection result
    emitter.label("__rt_win_proc_command_uses_cmd_extension");
    emitter.instruction("cmp rdx, 4");                                          // batch extensions require at least four bytes
    emitter.instruction("jb __rt_win_proc_command_uses_cmd_done");              // shorter names cannot be batch files
    emitter.instruction("lea r8, [rdi + rdx - 4]");                             // point at the final four-byte extension
    emitter.instruction("cmp BYTE PTR [r8], 0x2e");                             // batch extensions begin with a dot
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_done");             // not a batch-file extension
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 1]");                        // load extension byte one
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x62");                                        // `.bat` begins with b
    emitter.instruction("je __rt_win_proc_command_uses_cmd_bat");               // verify the remaining batch suffix
    emitter.instruction("cmp cl, 0x63");                                        // `.cmd` begins with c
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_done");             // neither supported batch suffix
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 2]");                        // load `.cmd` byte two
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x6d");                                        // expect m
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_done");             // not `.cmd`
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 3]");                        // load final `.cmd` suffix byte
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x64");                                        // `.cmd` ends in d
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_done");             // not `.cmd`
    emitter.instruction("jmp __rt_win_proc_command_uses_cmd_yes");              // `.cmd` is a command-processor program
    emitter.label("__rt_win_proc_command_uses_cmd_bat");
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 2]");                        // load `.bat` byte two
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x61");                                        // expect a
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_done");             // not `.bat`
    emitter.instruction("movzx ecx, BYTE PTR [r8 + 3]");                        // load final batch suffix byte
    emitter.instruction("or cl, 0x20");                                         // ASCII case-fold it
    emitter.instruction("cmp cl, 0x74");                                        // `.bat` ends in t
    emitter.instruction("jne __rt_win_proc_command_uses_cmd_done");             // not `.bat`
    emitter.label("__rt_win_proc_command_uses_cmd_yes");
    emitter.instruction("mov eax, 1");                                          // batch files execute through cmd.exe
    emitter.label("__rt_win_proc_command_uses_cmd_done");
    emitter.instruction("ret");                                                 // return the bounded detection result
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Structural tests for computed Windows `proc_open` argument marshalling.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These tests guard validation, Unicode de-duplication, overflow, and
    //!   ownership paths before the PE execution tests run under Wine.

    use std::collections::HashSet;

    use super::*;
    use crate::codegen_support::platform::Target;

    /// Emits the complete Windows marshalling section for structural assertions.
    fn windows_marshalling_assembly() -> String {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_proc_open_marshalling(&mut emitter);
        emitter.output().to_string()
    }

    /// Verifies every emitted marshalling label is unique, preventing assembler
    /// regressions when a helper branch and public entry point have similar names.
    #[test]
    fn windows_proc_marshalling_labels_are_unique() {
        let assembly = windows_marshalling_assembly();
        let mut labels = HashSet::new();
        for line in assembly.lines().filter(|line| line.ends_with(':')) {
            assert!(labels.insert(line), "duplicate runtime label: {line}");
        }
    }

    /// Verifies dynamic argv quoting includes embedded-quote and trailing-slash
    /// expansion plus explicit overflow/invalid-element failure paths.
    #[test]
    fn windows_proc_command_marshaller_is_checked_and_owned() {
        let assembly = windows_marshalling_assembly();
        assert!(assembly.contains("__rt_win_proc_command_array:"));
        assert!(assembly.contains("__rt_win_proc_command_uses_cmd:"));
        assert!(assembly.contains("__rt_win_proc_command_special_scan"));
        assert!(assembly.contains("__rt_win_proc_command_write_caret"));
        assert!(assembly.contains("cmp BYTE PTR [r8], 0x2e"));
        assert!(assembly.contains("shl r10, 1"));
        assert!(assembly.contains("__rt_win_proc_command_write_trailing_slashes"));
        assert!(assembly.contains("xchg rdi, rsi"));
        assert!(assembly.contains("mov rsi, rdi"));
        assert!(assembly.contains("call __rt_heap_alloc"));
        assert!(assembly.contains("[rip + __rt_errno], 12"));
        assert!(assembly.contains("[rip + __rt_errno], 22"));
    }

    /// Verifies environment marshalling uses Windows Unicode case-insensitive
    /// comparison, scalar conversion, last-wins replacement, and balanced frees.
    #[test]
    fn windows_proc_environment_marshaller_deduplicates_and_cleans_up() {
        let assembly = windows_marshalling_assembly();
        assert!(assembly.contains("call CompareStringOrdinal"));
        assert!(assembly.contains("__rt_win_proc_environment_replace"));
        assert!(assembly.contains("call __rt_win_proc_scalar_string"));
        assert!(assembly.contains("__rt_win_proc_environment_write_final_nul"));
        assert!(assembly.matches("call __rt_heap_free").count() >= 6);
    }

    /// Verifies options inspect every documented php-src key, ignore unknown
    /// keys, and support boxed Mixed scalar values.
    #[test]
    fn windows_proc_options_are_runtime_validated() {
        let assembly = windows_marshalling_assembly();
        assert!(assembly.contains("call __rt_hash_get"));
        assert!(assembly.contains("call __rt_mixed_unbox"));
        assert!(assembly.contains("__rt_win_proc_options_suppress_errors_enabled"));
        assert!(assembly.contains("__rt_win_proc_options_create_process_group_enabled"));
        assert!(assembly.contains("__rt_win_proc_options_create_new_console_enabled"));
        assert!(!assembly.contains("__rt_win_proc_options_invalid"));
    }
}
