//! Purpose:
//! Emits `__rt_user_filter_brigade_invoke`, the runtime entry point that
//! drives the PHP-canonical 4-arg `filter($in, $out, &$consumed,
//! $closing): int` contract on top of bucket brigades.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (registered via
//!   `crate::codegen_support::runtime::io::user_filter_brigade::emit_*`).
//! - `__rt_apply_user_stream_filter` when the dispatched user filter's
//!   class metadata says vtable slot 3 (arity flag) is 1.
//!
//! Key details:
//! - Seeds an input brigade stdClass with one bucket holding the
//!   incoming stream bytes (matches what PHP's stream layer does for a
//!   single-segment read). The bucket carries `data` (string) and
//!   `datalen` (int) public properties, just like `stream_bucket_new`.
//! - Allocates a fresh output brigade and passes both to the user
//!   method as Mixed-boxed objects. The user method calls
//!   `stream_bucket_make_writeable($in)` to pop the input bucket and
//!   `stream_bucket_append($out, $bucket)` to enqueue results.
//! - After return, walks `$out->_buckets`, concatenates each bucket's
//!   `data` string into `_stream_filter_buf`, and returns the
//!   buffer/length to the caller in the standard string-result pair.
//! - `&$consumed` is passed as a Mixed(int=0) cell pointer. The method
//!   can write to it through normal Mixed by-ref semantics, but the
//!   caller does not currently propagate the value back into stream
//!   accounting. `$closing` is passed as Mixed(int=0) (we don't yet
//!   distinguish closing from non-closing dispatches).
//! - The method's `int` return value (PSFS_PASS_ON / FEED_ME /
//!   ERR_FATAL) is observed only to decide whether to emit output —
//!   empty output brigade always yields a zero-length result regardless
//!   of the status code. v1 limitation: FEED_ME doesn't request more
//!   input from the stream layer; ERR_FATAL doesn't propagate as an
//!   error to the caller.

use crate::codegen_support::{
    abi, emit::Emitter, platform::Arch, sentinels::emit_branch_if_null_container,
};

/// `__rt_user_filter_brigade_invoke`: dispatch a single 4-arg PHP filter
/// invocation through bucket brigades.
///
/// Input:  AArch64 x0=$this (obj), x1=buf_ptr, x2=buf_len, x3=method_ptr.
///         x86_64  rdi=$this (obj), rsi=buf_ptr, rdx=buf_len, rcx=method_ptr.
/// Output: AArch64 x1=result_ptr, x2=result_len (string-result pair).
///         x86_64  rax=result_ptr, rdx=result_len.
pub fn emit_user_filter_brigade_invoke(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_user_filter_brigade_invoke_linux_x86_64(emitter);
        return;
    }
    emit_user_filter_brigade_invoke_aarch64(emitter);
}

/// Emits the user filter brigade invoke aarch64 stream runtime helper.
fn emit_user_filter_brigade_invoke_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_filter_brigade_invoke ---");
    emitter.label_global("__rt_user_filter_brigade_invoke");

    // Stack frame (128 bytes):
    //   [sp, #0]   = $this obj
    //   [sp, #8]   = original buf_ptr
    //   [sp, #16]  = original buf_len
    //   [sp, #24]  = method ptr
    //   [sp, #32]  = in_brigade obj
    //   [sp, #40]  = out_brigade obj
    //   [sp, #48]  = consumed Mixed (int=0)
    //   [sp, #56]  = closing  Mixed (int=0)
    //   [sp, #64]  = bucket obj (transient during setup)
    //   [sp, #72]  = mixed_bucket cell ptr (transient)
    //   [sp, #80]  = mixed_buckets_array cell ptr (transient)
    //   [sp, #112] = saved x29
    //   [sp, #120] = saved x30
    emitter.instruction("sub sp, sp, #128");                                    // allocate the brigade-invoke local frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish a stable frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save $this
    emitter.instruction("str x1, [sp, #8]");                                    // save buf_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save buf_len
    emitter.instruction("str x3, [sp, #24]");                                   // save method_ptr

    // -- Create the input brigade stdClass --
    abi::emit_call_label(emitter, "__rt_stdclass_new");                         // x0 = in_brigade obj
    emitter.instruction("str x0, [sp, #32]");                                   // save in_brigade

    // -- Create one bucket stdClass --
    abi::emit_call_label(emitter, "__rt_stdclass_new");                         // x0 = bucket obj
    emitter.instruction("str x0, [sp, #64]");                                   // save bucket

    // bucket->data = Mixed(string from original input).
    emitter.instruction("ldr x1, [sp, #8]");                                    // buf_ptr → payload lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // buf_len → payload hi
    emitter.instruction("mov x0, #1");                                          // tag = 1 (string)
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = mixed(string)
    emitter.instruction("mov x3, x0");                                          // value → 4th stdclass_set arg
    emitter.instruction("ldr x0, [sp, #64]");                                   // bucket obj
    abi::emit_symbol_address(emitter, "x1", "_brigade_data_key");
    emitter.instruction("mov x2, #4");                                          // prepare AArch64 call argument
    abi::emit_call_label(emitter, "__rt_stdclass_set");

    // bucket->datalen = Mixed(int buf_len).
    emitter.instruction("ldr x1, [sp, #16]");                                   // buf_len → int payload
    emitter.instruction("mov x2, #0");                                          // prepare AArch64 call argument
    emitter.instruction("mov x0, #0");                                          // tag = 0 (int)
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction("mov x3, x0");                                          // prepare AArch64 call argument
    emitter.instruction("ldr x0, [sp, #64]");                                   // bucket obj
    abi::emit_symbol_address(emitter, "x1", "_brigade_datalen_key");
    emitter.instruction("mov x2, #7");                                          // prepare AArch64 call argument
    abi::emit_call_label(emitter, "__rt_stdclass_set");

    // -- Box the bucket as Mixed(obj) --
    emitter.instruction("ldr x1, [sp, #64]");                                   // bucket obj → payload lo
    emitter.instruction("mov x2, #0");                                          // prepare AArch64 call argument
    emitter.instruction("mov x0, #6");                                          // tag = 6 (object)
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = mixed(bucket)
    emitter.instruction("str x0, [sp, #72]");                                   // save mixed_bucket

    // -- Allocate a 1-slot indexed array with Mixed payload type and push the bucket --
    emitter.instruction("mov x0, #4");                                          // capacity 4 (min stride for new arrays)
    emitter.instruction("mov x1, #8");                                          // 8-byte slots (Mixed pointer)
    abi::emit_call_label(emitter, "__rt_array_new");                            // x0 = empty array
    // Stamp value_type = 7 (boxed Mixed) on the packed kind word.
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load runtime value
    emitter.instruction("mov x12, #0x80ff");                                    // preserve low byte + COW bit
    emitter.instruction("and x10, x10, x12");                                   // mask runtime value
    emitter.instruction("mov x11, #7");                                         // value_type = boxed Mixed
    emitter.instruction("lsl x11, x11, #8");                                    // shift runtime value
    emitter.instruction("orr x10, x10, x11");                                   // combine runtime bit flags
    emitter.instruction("str x10, [x0, #-8]");                                  // store runtime value
    // Push the boxed bucket onto the array.
    emitter.instruction("ldr x1, [sp, #72]");                                   // mixed_bucket
    abi::emit_call_label(emitter, "__rt_array_push_int");                       // x0 = updated array
    // Box the array as Mixed(indexed-array).
    emitter.instruction("mov x1, x0");                                          // prepare AArch64 call argument
    emitter.instruction("mov x2, #0");                                          // prepare AArch64 call argument
    emitter.instruction("mov x0, #4");                                          // tag = 4 (indexed array)
    abi::emit_call_label(emitter, "__rt_mixed_from_value");                     // x0 = mixed(_buckets array)
    emitter.instruction("str x0, [sp, #80]");                                   // save mixed_buckets_array

    // -- in_brigade->_buckets = mixed_buckets_array --
    emitter.instruction("mov x3, x0");                                          // prepare AArch64 call argument
    emitter.instruction("ldr x0, [sp, #32]");                                   // in_brigade
    abi::emit_symbol_address(emitter, "x1", "_brigade_buckets_key");
    emitter.instruction("mov x2, #8");                                          // prepare AArch64 call argument
    abi::emit_call_label(emitter, "__rt_stdclass_set");

    // -- Create output brigade stdClass --
    abi::emit_call_label(emitter, "__rt_stdclass_new");                         // x0 = out_brigade obj
    emitter.instruction("str x0, [sp, #40]");                                   // save out_brigade

    // -- Create consumed Mixed(int=0) --
    emitter.instruction("mov x1, #0");                                          // prepare AArch64 call argument
    emitter.instruction("mov x2, #0");                                          // prepare AArch64 call argument
    emitter.instruction("mov x0, #0");                                          // tag = int
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction("str x0, [sp, #48]");                                   // save consumed mixed cell

    // -- Create closing Mixed(int=0) --
    emitter.instruction("mov x1, #0");                                          // prepare AArch64 call argument
    emitter.instruction("mov x2, #0");                                          // prepare AArch64 call argument
    emitter.instruction("mov x0, #0");                                          // prepare AArch64 call argument
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction("str x0, [sp, #56]");                                   // save closing mixed cell

    // -- Call filter($this, $in, $out, &consumed, $closing) --
    // The user method's params are inferred as Object (no typehint, default
    // codegen treats them as object refs since the only call site — this
    // dispatcher — hands them stdClass instances). Pass raw obj pointers
    // for $in / $out, and Mixed(int) cell pointers for $consumed / $closing
    // (those flow through the regular Mixed arg path on the method side).
    emitter.instruction("ldr x0, [sp, #0]");                                    // $this
    emitter.instruction("ldr x1, [sp, #32]");                                   // in_brigade obj (raw)
    emitter.instruction("ldr x2, [sp, #40]");                                   // out_brigade obj (raw)
    emitter.instruction("ldr x3, [sp, #48]");                                   // consumed mixed (pseudo by-ref)
    emitter.instruction("ldr x4, [sp, #56]");                                   // closing mixed
    emitter.instruction("ldr x5, [sp, #24]");                                   // method ptr
    emitter.instruction("blr x5");                                              // invoke filter()

    // -- Walk out_brigade._buckets and concatenate the data fields --
    emitter.instruction("ldr x0, [sp, #40]");                                   // out_brigade obj
    abi::emit_symbol_address(emitter, "x1", "_brigade_buckets_key");
    emitter.instruction("mov x2, #8");                                          // prepare AArch64 call argument
    abi::emit_call_label(emitter, "__rt_stdclass_get");                         // x0 = Mixed*
    emitter.instruction("cbz x0, __rt_ufbi_empty");                             // no _buckets → empty output
    emitter.instruction("ldr x9, [x0]");                                        // tag
    emitter.instruction("cmp x9, #4");                                          // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_ufbi_empty");                                // branch when the checked value is nonzero or different
    emitter.instruction("ldr x9, [x0, #8]");                                    // array ptr
    emit_branch_if_null_container(emitter, "x9", "x11", "__rt_ufbi_empty");
    emitter.instruction("ldr x10, [x9]");                                       // length
    emitter.instruction("cbz x10, __rt_ufbi_empty");                            // branch when the checked value is zero or equal

    // Concatenate loop. x9=array, x10=length, x12=base (_stream_filter_buf),
    // x13=write offset.
    abi::emit_symbol_address(emitter, "x12", "_stream_filter_buf");
    emitter.instruction("mov x13, #0");                                         // write cursor
    emitter.instruction("mov x14, #0");                                         // bucket index

    emitter.label("__rt_ufbi_walk_loop");
    emitter.instruction("cmp x14, x10");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_ufbi_walk_done");                            // branch when comparison is at least target
    emitter.instruction("add x15, x9, #24");                                    // first payload slot
    emitter.instruction("ldr x0, [x15, x14, lsl #3]");                          // x0 = mixed(bucket) ptr
    emitter.instruction("cbz x0, __rt_ufbi_walk_next");                         // branch when the checked value is zero or equal
    emitter.instruction("ldr x16, [x0]");                                       // load runtime value
    emitter.instruction("cmp x16, #6");                                         // tag = obj?
    emitter.instruction("b.ne __rt_ufbi_walk_next");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldr x0, [x0, #8]");                                    // bucket obj
    emit_branch_if_null_container(emitter, "x0", "x16", "__rt_ufbi_walk_next");

    // Save walk state across the stdclass_get call (x0-x18 are caller-saved).
    // Use non-overlapping 16-byte slots: 88..104, 104..120 (the previous
    // 88+96 overlapped at byte 96 and clobbered x10 with x12, which
    // corrupted the length and caused the copy loop to read past the
    // bucket's data string).
    emitter.instruction("stp x9, x10, [sp, #88]");                              // [88..104] = array + length
    emitter.instruction("stp x12, x13, [sp, #104]");                            // [104..120] = buf base + write cursor (non-overlapping)
    emitter.instruction("str x14, [sp, #80]");                                  // [80..88] = bucket index (slot reused — was scratch earlier)
    abi::emit_symbol_address(emitter, "x1", "_brigade_data_key");
    emitter.instruction("mov x2, #4");                                          // prepare AArch64 call argument
    abi::emit_call_label(emitter, "__rt_stdclass_get");                         // x0 = Mixed*
    emitter.instruction("ldp x9, x10, [sp, #88]");                              // restore array + length
    emitter.instruction("ldp x12, x13, [sp, #104]");                            // restore buf base + write cursor
    emitter.instruction("ldr x14, [sp, #80]");                                  // restore bucket index

    emitter.instruction("cbz x0, __rt_ufbi_walk_next");                         // branch when the checked value is zero or equal
    emitter.instruction("ldr x16, [x0]");                                       // load runtime value
    emitter.instruction("cmp x16, #1");                                         // tag = string?
    emitter.instruction("b.ne __rt_ufbi_walk_next");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldr x17, [x0, #8]");                                   // string ptr
    // x18 is the "platform register" on Apple AArch64 — reserved for the OS
    // (TLS pointer) and clobbered by the kernel at arbitrary points. We
    // can't use it for the loop's length variable. Use x26 (callee-saved
    // scratch) instead, both here for the length and below for the copy loop.
    emitter.instruction("ldr x26, [x0, #16]");                                  // string len
    emitter.instruction("mov x25, #0");                                         // copy index
    emitter.label("__rt_ufbi_copy_byte");
    emitter.instruction("cmp x25, x26");                                        // compare runtime values for the next branch
    emitter.instruction("b.hs __rt_ufbi_copy_done");                            // stop copying once the source string length is reached
    emitter.instruction("ldrb w19, [x17, x25]");                                // load runtime value
    emitter.instruction("strb w19, [x12, x13]");                                // store runtime value
    emitter.instruction("add x13, x13, #1");                                    // advance runtime pointer or counter
    emitter.instruction("add x25, x25, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_ufbi_copy_byte");                               // continue at target label
    emitter.label("__rt_ufbi_copy_done");

    emitter.label("__rt_ufbi_walk_next");
    emitter.instruction("add x14, x14, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_ufbi_walk_loop");                               // continue at target label

    emitter.label("__rt_ufbi_walk_done");
    emitter.instruction("mov x1, x12");                                         // result ptr (filter buf base)
    emitter.instruction("mov x2, x13");                                         // result len
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_ufbi_empty");
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");
    emitter.instruction("mov x2, #0");                                          // prepare AArch64 call argument
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for user filter brigade invoke.
fn emit_user_filter_brigade_invoke_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: user_filter_brigade_invoke ---");
    emitter.label_global("__rt_user_filter_brigade_invoke");

    // Frame layout (rbp-relative, 128 bytes):
    //   [rbp -   8] $this obj
    //   [rbp -  16] buf_ptr
    //   [rbp -  24] buf_len
    //   [rbp -  32] method_ptr
    //   [rbp -  40] in_brigade obj
    //   [rbp -  48] out_brigade obj
    //   [rbp -  56] consumed mixed
    //   [rbp -  64] closing  mixed
    //   [rbp -  72] bucket obj (transient) / later mixed_in
    //   [rbp -  80] mixed_bucket    / later mixed_out
    //   [rbp -  88] mixed_buckets_array
    //   [rbp - 112] walk-state save slots
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 128");                                        // local frame (16-byte aligned: prior call pushed ret, push rbp → 0-mod-16; -128 stays 0-mod-16)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // $this
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // buf_ptr
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // buf_len
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // method_ptr

    // -- Create input brigade --
    abi::emit_call_label(emitter, "__rt_stdclass_new");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // store runtime value

    // -- Create bucket --
    abi::emit_call_label(emitter, "__rt_stdclass_new");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // bucket obj

    // bucket->data = Mixed(string)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // buf_ptr → payload lo
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // buf_len → payload hi
    emitter.instruction("mov rax, 1");                                          // tag string
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction("mov rcx, rax");                                        // mixed_str → 4th arg
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // bucket obj
    abi::emit_symbol_address(emitter, "rsi", "_brigade_data_key");
    emitter.instruction("mov rdx, 4");                                          // prepare SysV call argument
    abi::emit_call_label(emitter, "__rt_stdclass_set");

    // bucket->datalen = Mixed(int=len)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // buf_len → int payload
    emitter.instruction("xor esi, esi");                                        // clear register value
    emitter.instruction("mov rax, 0");                                          // tag int
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction("mov rcx, rax");                                        // prepare SysV call argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_brigade_datalen_key");
    emitter.instruction("mov rdx, 7");                                          // prepare SysV call argument
    abi::emit_call_label(emitter, "__rt_stdclass_set");

    // -- Box bucket as Mixed(obj) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // prepare SysV call argument
    emitter.instruction("xor esi, esi");                                        // clear register value
    emitter.instruction("mov rax, 6");                                          // tag obj
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // mixed_bucket

    // -- Allocate _buckets array, push bucket, box as Mixed(indexed-array) --
    emitter.instruction("mov rdi, 4");                                          // prepare SysV call argument
    emitter.instruction("mov rsi, 8");                                          // prepare SysV call argument
    abi::emit_call_label(emitter, "__rt_array_new");
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // move runtime value between registers
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // move runtime value between registers
    emitter.instruction("and r10, r11");                                        // mask runtime value
    emitter.instruction("mov r11, 7");                                          // move runtime value between registers
    emitter.instruction("shl r11, 8");                                          // shift runtime value
    emitter.instruction("or r10, r11");                                         // combine runtime bit flags
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // store runtime value
    emitter.instruction("mov rdi, rax");                                        // prepare SysV call argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // mixed_bucket
    abi::emit_call_label(emitter, "__rt_array_push_int");
    emitter.instruction("mov rdi, rax");                                        // prepare SysV call argument
    emitter.instruction("xor esi, esi");                                        // clear register value
    emitter.instruction("mov rax, 4");                                          // tag indexed-array
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // mixed_buckets_array

    // -- in_brigade->_buckets = mixed_buckets_array --
    emitter.instruction("mov rcx, rax");                                        // prepare SysV call argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // in_brigade
    abi::emit_symbol_address(emitter, "rsi", "_brigade_buckets_key");
    emitter.instruction("mov rdx, 8");                                          // prepare SysV call argument
    abi::emit_call_label(emitter, "__rt_stdclass_set");

    // -- Create output brigade --
    abi::emit_call_label(emitter, "__rt_stdclass_new");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // out_brigade

    // -- consumed Mixed(int=0) --
    emitter.instruction("xor edi, edi");                                        // clear register value
    emitter.instruction("xor esi, esi");                                        // clear register value
    emitter.instruction("mov rax, 0");                                          // prepare runtime result value
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value

    // -- closing Mixed(int=0) --
    emitter.instruction("xor edi, edi");                                        // clear register value
    emitter.instruction("xor esi, esi");                                        // clear register value
    emitter.instruction("mov rax, 0");                                          // prepare runtime result value
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // store runtime value

    // -- Call filter($this, $in, $out, $consumed, $closing) --
    // Pass raw obj pointers for $in/$out (the user method's params are
    // inferred as Object). $consumed and $closing are passed as Mixed(int)
    // cells through the regular Mixed-arg ABI path.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // $this
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // in_brigade obj (raw)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // out_brigade obj (raw)
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // consumed mixed
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // closing mixed
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // method ptr
    emitter.instruction("call r11");                                            // call selected function pointer

    // -- Walk out_brigade._buckets, concatenate data --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_brigade_buckets_key");
    emitter.instruction("mov rdx, 8");                                          // prepare SysV call argument
    abi::emit_call_label(emitter, "__rt_stdclass_get");
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ufbi_empty_x");                                // branch when the checked value is zero or equal
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // move runtime value between registers
    emitter.instruction("cmp r10, 4");                                          // compare runtime values for the next branch
    emitter.instruction("jne __rt_ufbi_empty_x");                               // branch when the checked value is nonzero or different
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // array
    emit_branch_if_null_container(emitter, "r10", "r11", "__rt_ufbi_empty_x");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // length
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ufbi_empty_x");                                // branch when the checked value is zero or equal

    // Use r12 = array base, r13 = length, r14 = bucket index, r15 = write cursor.
    emitter.instruction("mov QWORD PTR [rbp - 96], r12");                       // save callee-saved regs
    emitter.instruction("mov QWORD PTR [rbp - 104], r13");                      // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 112], r14");                      // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 120], r15");                      // store runtime value
    emitter.instruction("lea r12, [r10 + 24]");                                 // first payload slot
    emitter.instruction("mov r13, r11");                                        // length
    emitter.instruction("xor r14, r14");                                        // bucket index
    emitter.instruction("xor r15, r15");                                        // write cursor

    emitter.label("__rt_ufbi_walk_loop_x");
    emitter.instruction("cmp r14, r13");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_ufbi_walk_done_x");                           // branch when comparison is at least target
    emitter.instruction("mov rdi, QWORD PTR [r12 + r14 * 8]");                  // Mixed(bucket)
    emitter.instruction("test rdi, rdi");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ufbi_walk_next_x");                            // branch when the checked value is zero or equal
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // move runtime value between registers
    emitter.instruction("cmp r10, 6");                                          // tag = obj?
    emitter.instruction("jne __rt_ufbi_walk_next_x");                           // branch when the checked value is nonzero or different
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // bucket obj
    emit_branch_if_null_container(emitter, "rdi", "r10", "__rt_ufbi_walk_next_x");
    abi::emit_symbol_address(emitter, "rsi", "_brigade_data_key");
    emitter.instruction("mov rdx, 4");                                          // prepare SysV call argument
    abi::emit_call_label(emitter, "__rt_stdclass_get");
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ufbi_walk_next_x");                            // branch when the checked value is zero or equal
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // move runtime value between registers
    emitter.instruction("cmp r10, 1");                                          // string?
    emitter.instruction("jne __rt_ufbi_walk_next_x");                           // branch when the checked value is nonzero or different
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // str ptr
    emitter.instruction("mov r11, QWORD PTR [rax + 16]");                       // str len
    abi::emit_symbol_address(emitter, "r8", "_stream_filter_buf");              // load runtime data address

    emitter.instruction("xor r9, r9");                                          // clear register value
    emitter.label("__rt_ufbi_copy_byte_x");
    emitter.instruction("cmp r9, r11");                                         // compare runtime values for the next branch
    emitter.instruction("jge __rt_ufbi_copy_done_x");                           // branch when comparison is at least target
    emitter.instruction("movzx eax, BYTE PTR [r10 + r9]");                      // load runtime value
    emitter.instruction("mov BYTE PTR [r8 + r15], al");                         // store runtime value
    emitter.instruction("inc r15");                                             // advance runtime pointer or counter
    emitter.instruction("inc r9");                                              // advance runtime pointer or counter
    emitter.instruction("jmp __rt_ufbi_copy_byte_x");                           // continue at target label
    emitter.label("__rt_ufbi_copy_done_x");

    emitter.label("__rt_ufbi_walk_next_x");
    emitter.instruction("inc r14");                                             // advance runtime pointer or counter
    emitter.instruction("jmp __rt_ufbi_walk_loop_x");                           // continue at target label

    emitter.label("__rt_ufbi_walk_done_x");
    abi::emit_symbol_address(emitter, "rax", "_stream_filter_buf");             // load runtime data address
    emitter.instruction("mov rdx, r15");                                        // result len
    emitter.instruction("mov r12, QWORD PTR [rbp - 96]");                       // restore callee-saved regs
    emitter.instruction("mov r13, QWORD PTR [rbp - 104]");                      // move runtime value between registers
    emitter.instruction("mov r14, QWORD PTR [rbp - 112]");                      // move runtime value between registers
    emitter.instruction("mov r15, QWORD PTR [rbp - 120]");                      // move runtime value between registers
    emitter.instruction("mov rsp, rbp");                                        // move runtime value between registers
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_ufbi_empty_x");
    abi::emit_symbol_address(emitter, "rax", "_stream_filter_buf");             // load runtime data address
    emitter.instruction("xor edx, edx");                                        // clear register value
    emitter.instruction("mov rsp, rbp");                                        // move runtime value between registers
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}
