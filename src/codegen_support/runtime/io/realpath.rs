//! Purpose:
//! Emits the `__rt_realpath`, `__rt_cstr` runtime helper assembly for realpath.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Entries in `_realpath_cache`. A power of two: the slot is the path hash's low bits.
pub(crate) const REALPATH_CACHE_SLOTS: usize = 64;
/// Bytes per entry: hash, key length, key pointer, result length, result pointer, epoch, stamp,
/// and one pad word so an entry is 64 bytes and its address is `base + slot << 6`.
pub(crate) const REALPATH_CACHE_ENTRY_BYTES: usize = 64;
/// Seconds an entry stays valid: php's default `realpath_cache_ttl`.
const REALPATH_CACHE_TTL_SECONDS: i64 = 120;


/// realpath: canonicalize a path through libc realpath().
/// Input:  x1/x2 = path string
/// Output: x1/x2 = canonical path (owned heap string), or x1=0/x2=0 on failure
///
/// On failure (e.g. non-existent path) the helper returns the empty string.
/// The type signature exposes `string|false` and PHP-side `=== false`
/// comparisons rely on the empty-string sentinel as for `file_get_contents`.
pub fn emit_realpath(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_realpath_linux_x86_64(emitter);
        emit_realpath_cache_invalidate_helper(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    // The uncached body follows the cached entry with no global label in between: under
    // `.subsections_via_symbols` a global label starts a new atom, which `-dead_strip` may drop
    // while the already-assembled branch into it still points there.
    emit_realpath_cached_aarch64(emitter);
    emitter.comment("--- runtime: realpath (uncached) ---");
    emitter.label("__rt_realpath_uncached");                                   // local: the cached entry point branches here conditionally

    // Frame layout (32 bytes):
    //   sp+ 0  : owned heap buffer pointer
    //   sp+ 8  : reserved (alignment)
    //   sp+16  : x29 / x30
    emitter.instruction("sub sp, sp, #32");                                     // reserve frame for the saved buffer pointer plus the saved frame regs
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer

    // -- null-terminate the input path through __rt_cstr --
    emitter.instruction("bl __rt_cstr");                                        // convert path to null-terminated C string, x0 = cstr ptr
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the C path pointer across the heap_alloc() call

    // -- allocate a 4096-byte owned buffer for the canonical path --
    emitter.instruction("mov x0, #4096");                                       // PATH_MAX-style buffer for the canonical result
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned heap storage; result in x0
    emitter.instruction("mov x9, #1");                                          // heap-kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // tag the buffer as an owned string in the uniform heap header
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the owned buffer pointer across the libc call

    // -- call libc realpath(c_path, buf) --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the C path pointer
    emitter.instruction("mov x0, x9");                                          // first argument: path
    emitter.instruction("ldr x1, [sp, #0]");                                    // second argument: owned destination buffer
    emitter.bl_c("realpath");                                                   // libc realpath(path, buf) → x0 = buf on success, NULL on failure
    emitter.instruction("cbz x0, __rt_realpath_fail");                          // libc returned NULL: report failure (empty string)

    // -- scan the owned buffer for the trailing null terminator --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the owned buffer as the result string pointer
    emitter.instruction("mov x2, #0");                                          // initialize the length counter to 0
    emitter.label("__rt_realpath_len");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load next byte from the owned buffer
    emitter.instruction("cbz w9, __rt_realpath_done");                          // null terminator → length found
    emitter.instruction("add x2, x2, #1");                                      // count one more non-null byte
    emitter.instruction("b __rt_realpath_len");                                 // continue scanning

    emitter.label("__rt_realpath_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the realpath() frame
    emitter.instruction("ret");                                                 // return canonical path in x1/x2

    emitter.label("__rt_realpath_fail");
    // libc returned NULL: free the unused owned buffer and return the empty string sentinel
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the owned buffer pointer for cleanup
    emitter.instruction("bl __rt_heap_free");                                   // release the unused buffer back to the heap
    emitter.instruction("mov x1, #0");                                          // return empty pointer (string|false sentinel)
    emitter.instruction("mov x2, #0");                                          // return zero length (string|false sentinel)
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address on the failure path
    emitter.instruction("add sp, sp, #32");                                     // deallocate the realpath() frame on the failure path
    emitter.instruction("ret");                                                 // return the empty-string result
    emit_realpath_cache_invalidate_helper(emitter);
}

/// x86_64 Linux-specific realpath emitter.
///
/// Mirrors the generic ARM64 realpath helper: calls `__rt_cstr` to null-terminate
/// the input path, allocates a 4096-byte owned heap buffer, invokes libc `realpath(path, buf)`,
/// scans the result for the null terminator to determine length, and returns the canonical
/// path in `rax/rdx` (pointer/length) or the empty-string sentinel (`rax=0, rdx=0`) on failure.
///
/// Frame layout uses `[rbp-8]` for the C path pointer and `[rbp-16]` for the owned buffer
/// pointer. On failure, the allocated buffer is freed via `__rt_heap_free` before returning.
fn emit_realpath_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emit_realpath_cached_x86_64(emitter);
    emitter.comment("--- runtime: realpath (uncached) ---");
    emitter.label("__rt_realpath_uncached");                                   // local: the cached entry point branches here conditionally

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while realpath uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the C path pointer and the owned buffer pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots: [rbp-8]=cpath, [rbp-16]=heap

    emitter.instruction("call __rt_cstr");                                      // convert path in rax/rdx into a null-terminated C path in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the C path pointer across the heap_alloc() call

    emitter.instruction("mov rax, 4096");                                       // request a 4096-byte owned buffer for the canonical path
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned heap storage; pointer returned in rax
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated buffer as a persisted elephc string in the uniform heap header
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the owned buffer pointer across the libc realpath() call

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // first argument: C path pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // second argument: owned destination buffer
    emitter.instruction("call realpath");                                       // libc realpath(path, buf) → rax = buf on success, NULL on failure
    emitter.instruction("test rax, rax");                                       // detect libc realpath() failure
    emitter.instruction("jz __rt_realpath_fail_x86");                           // jump to the failure path when libc returns NULL

    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the owned buffer to scan for its null terminator
    emitter.instruction("xor rdx, rdx");                                        // initialize result length to zero
    emitter.label("__rt_realpath_len_x86");
    emitter.instruction("mov r9b, BYTE PTR [r8 + rdx]");                        // load next byte from the owned buffer
    emitter.instruction("test r9b, r9b");                                       // null terminator?
    emitter.instruction("jz __rt_realpath_done_x86");                           // length found
    emitter.instruction("add rdx, 1");                                          // count one more non-null byte
    emitter.instruction("jmp __rt_realpath_len_x86");                           // continue scanning

    emitter.label("__rt_realpath_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return owned buffer pointer in the x86_64 string result register
    emitter.instruction("add rsp, 32");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return canonical path in rax/rdx

    emitter.label("__rt_realpath_fail_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the owned buffer for cleanup
    emitter.instruction("call __rt_heap_free");                                 // release the unused buffer back to the heap
    emitter.instruction("xor eax, eax");                                        // return empty-string pointer (string|false sentinel)
    emitter.instruction("xor edx, edx");                                        // return empty-string length (string|false sentinel)
    emitter.instruction("add rsp, 32");                                         // release the spill slots on the failure path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer on the failure path
    emitter.instruction("ret");                                                 // return the empty-string result
}

// ── realpath cache ──────────────────────────────────────────────────
//
// php answers realpath() from a process-wide cache (`realpath_cache_size`/`_ttl`) that
// survives requests; libc realpath() walks the path with a syscall per component, and a Twig
// page resolving its templates spent ~6% of a Symfony request there. `__rt_realpath` keeps the
// same cache: absolute inputs only (a relative one depends on the cwd), successful answers only,
// each valid for REALPATH_CACHE_TTL_SECONDS and until the epoch moves. Every helper that
// creates, removes or renames a path bumps the epoch, as php's plain-files wrapper clears its
// cache, and so does clearstatcache(). Key and result are malloc'd copies: the --web heap reset
// wipes the PHP heap between requests, and the cache must outlive it.
//
// Entry layout (REALPATH_CACHE_ENTRY_BYTES):
//   +0 hash  +8 key length  +16 key pointer  +24 result length  +32 result pointer
//   +40 epoch  +48 stamp (time())  +56 pad

/// Emits the inline epoch bump every path-changing helper runs on entry. Clobbers only `x9`/`x10`
/// on AArch64 and `r10` on x86_64, which no caller of those helpers passes arguments in.
pub(crate) fn emit_realpath_cache_invalidate(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_realpath_cache_epoch");
            emitter.instruction("ldr x10, [x9]");                               // load the realpath cache epoch
            emitter.instruction("add x10, x10, #1");                            // a path may change: every cached realpath is now stale
            emitter.instruction("str x10, [x9]");                               // publish the new epoch
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_realpath_cache_epoch");
            emitter.instruction("add QWORD PTR [r10], 1");                      // a path may change: every cached realpath is now stale
        }
    }
}

/// Emits `__rt_realpath_cache_invalidate`, the same bump as a C-ABI function for the eval
/// interpreter, whose filesystem builtins change paths without going through these helpers.
fn emit_realpath_cache_invalidate_helper(emitter: &mut Emitter) {
    emitter.comment("--- runtime: realpath_cache_invalidate ---");
    // A C symbol (Mach-O adds its underscore): the eval interpreter calls it from Rust.
    let symbol = emitter.target.extern_symbol("__rt_realpath_cache_invalidate");
    emitter.label_global(&symbol);
    emit_realpath_cache_invalidate(emitter);
    emitter.instruction("ret");                                                 // nothing to return
}

/// Emits the caching `__rt_realpath` (AArch64). Same contract as the uncached helper:
/// `x1`/`x2` in, an owned heap string (or `0`/`0` on failure) out in `x1`/`x2`.
fn emit_realpath_cached_aarch64(emitter: &mut Emitter) {
    emitter.comment("--- runtime: realpath (cached) ---");
    emitter.label_global("__rt_realpath");
    emitter.instruction("cbz x2, __rt_realpath_uncached");                      // an empty path is never cached
    emitter.instruction("ldrb w9, [x1]");                                       // first byte of the path
    emitter.instruction("cmp w9, #47");                                         // absolute path ('/')?
    emitter.instruction("b.ne __rt_realpath_uncached");                         // a relative path depends on the cwd: resolve it every time

    // Frame (80 bytes): +0 x19/x20, +16 x21/x22, +32 x23, +48 result ptr/len, +64 x29/x30.
    emitter.instruction("sub sp, sp, #80");                                     // reserve the cache frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the frame pointer
    emitter.instruction("stp x19, x20, [sp]");                                  // x19 = path pointer, x20 = path length
    emitter.instruction("stp x21, x22, [sp, #16]");                             // x21 = path hash, x22 = cache entry
    emitter.instruction("str x23, [sp, #32]");                                  // x23 = time() of this lookup
    emitter.instruction("mov x19, x1");                                         // keep the path pointer across libc calls
    emitter.instruction("mov x20, x2");                                         // keep the path length across libc calls

    // -- FNV-1a over the path bytes --
    emitter.instruction("movz x21, #0x2325");                                   // FNV-1a 64-bit offset basis, bits 15:0
    emitter.instruction("movk x21, #0x8422, lsl #16");                          // offset basis bits 31:16
    emitter.instruction("movk x21, #0xe484, lsl #32");                          // offset basis bits 47:32
    emitter.instruction("movk x21, #0xcbf2, lsl #48");                          // offset basis bits 63:48
    emitter.instruction("movz x10, #0x01b3");                                   // FNV-1a 64-bit prime, low bits
    emitter.instruction("movk x10, #0x100, lsl #32");                           // FNV-1a 64-bit prime, high bits
    emitter.instruction("mov x11, #0");                                         // byte index
    emitter.label("__rt_realpath_cache_hash");
    emitter.instruction("cmp x11, x20");                                        // every byte hashed?
    emitter.instruction("b.hs __rt_realpath_cache_hashed");                     // yes: pick the entry
    emitter.instruction("ldrb w12, [x19, x11]");                                // next path byte
    emitter.instruction("eor x21, x21, x12");                                   // fold it into the hash
    emitter.instruction("mul x21, x21, x10");                                   // multiply by the FNV prime
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("b __rt_realpath_cache_hash");                          // continue hashing
    emitter.label("__rt_realpath_cache_hashed");
    abi::emit_symbol_address(emitter, "x22", "_realpath_cache");
    emitter.instruction(&format!("and x9, x21, #{}", REALPATH_CACHE_SLOTS - 1)); // slot = hash low bits
    emitter.instruction("add x22, x22, x9, lsl #6");                            // x22 = this path's entry
    emitter.instruction("mov x0, #0");                                          // time(NULL)
    emitter.bl_c("time");                                                       // x0 = now, in seconds
    emitter.instruction("mov x23, x0");                                         // keep it for the TTL check and the stamp

    // -- hit test: same hash, length, epoch, still fresh, same bytes --
    emitter.instruction("ldr x9, [x22]");                                       // cached hash
    emitter.instruction("cmp x9, x21");                                         // same hash?
    emitter.instruction("b.ne __rt_realpath_cache_miss");                       // no: resolve
    emitter.instruction("ldr x9, [x22, #8]");                                   // cached key length
    emitter.instruction("cmp x9, x20");                                         // same length?
    emitter.instruction("b.ne __rt_realpath_cache_miss");                       // no: resolve
    abi::emit_symbol_address(emitter, "x10", "_realpath_cache_epoch");
    emitter.instruction("ldr x10, [x10]");                                      // current epoch
    emitter.instruction("ldr x9, [x22, #40]");                                  // epoch the entry was cached in
    emitter.instruction("cmp x9, x10");                                         // no path changed since?
    emitter.instruction("b.ne __rt_realpath_cache_miss");                       // a path changed: resolve again
    emitter.instruction("ldr x9, [x22, #48]");                                  // when the entry was cached
    emitter.instruction("sub x9, x23, x9");                                     // its age (huge if the clock went back)
    emitter.instruction(&format!("cmp x9, #{}", REALPATH_CACHE_TTL_SECONDS));   // still within the TTL?
    emitter.instruction("b.hs __rt_realpath_cache_miss");                       // expired: resolve again
    emitter.instruction("ldr x0, [x22, #16]");                                  // cached key bytes
    emitter.instruction("mov x1, x19");                                         // requested path bytes
    emitter.instruction("mov x2, x20");                                         // their length
    emitter.bl_c("memcmp");                                                     // w0 = 0 when the bytes match
    emitter.instruction("cbnz w0, __rt_realpath_cache_miss");                   // a hash collision: resolve

    // -- hit: hand back an owned copy of the cached result --
    emitter.instruction("ldr x0, [x22, #24]");                                  // result length
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = owned buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted string
    emitter.instruction("str x9, [x0, #-8]");                                   // tag the buffer as an owned string
    emitter.instruction("mov x19, x0");                                         // keep the buffer across memcpy
    emitter.instruction("ldr x1, [x22, #32]");                                  // cached result bytes
    emitter.instruction("ldr x2, [x22, #24]");                                  // their length
    emitter.bl_c("memcpy");                                                     // copy them into the owned buffer
    emitter.instruction("mov x1, x19");                                         // result pointer
    emitter.instruction("ldr x2, [x22, #24]");                                  // result length
    emitter.instruction("b __rt_realpath_cache_out");                           // done

    // -- miss: resolve, then remember a successful answer --
    emitter.label("__rt_realpath_cache_miss");
    emitter.instruction("mov x1, x19");                                         // path pointer
    emitter.instruction("mov x2, x20");                                         // path length
    emitter.instruction("bl __rt_realpath_uncached");                           // x1/x2 = owned result, or 0/0
    emitter.instruction("stp x1, x2, [sp, #48]");                               // keep the result to return
    emitter.instruction("cbz x2, __rt_realpath_cache_result");                  // failures are not cached
    emitter.instruction("ldr x0, [x22, #16]");                                  // previous key copy (NULL when empty)
    emitter.bl_c("free");                                                       // drop it
    emitter.instruction("ldr x0, [x22, #32]");                                  // previous result copy
    emitter.bl_c("free");                                                       // drop it
    emitter.instruction("str xzr, [x22, #16]");                                 // the entry owns no key until the copy lands
    emitter.instruction("str xzr, [x22, #32]");                                 // nor a result
    emitter.instruction("str xzr, [x22, #8]");                                  // and matches no length meanwhile
    emitter.instruction("mov x0, x20");                                         // key length
    emitter.bl_c("malloc");                                                     // x0 = key copy
    emitter.instruction("cbz x0, __rt_realpath_cache_result");                  // out of memory: just skip caching
    emitter.instruction("str x0, [x22, #16]");                                  // own the key copy
    emitter.instruction("mov x1, x19");                                         // key bytes
    emitter.instruction("mov x2, x20");                                         // key length
    emitter.bl_c("memcpy");                                                     // copy the key
    emitter.instruction("ldr x0, [sp, #56]");                                   // result length
    emitter.bl_c("malloc");                                                     // x0 = result copy
    emitter.instruction("cbz x0, __rt_realpath_cache_drop_key");                // out of memory: undo the key
    emitter.instruction("str x0, [x22, #32]");                                  // own the result copy
    emitter.instruction("ldr x1, [sp, #48]");                                   // result bytes
    emitter.instruction("ldr x2, [sp, #56]");                                   // result length
    emitter.bl_c("memcpy");                                                     // copy the result
    emitter.instruction("str x21, [x22]");                                      // hash
    emitter.instruction("str x20, [x22, #8]");                                  // key length (makes the entry matchable)
    emitter.instruction("ldr x9, [sp, #56]");                                   // result length
    emitter.instruction("str x9, [x22, #24]");                                  // store it
    abi::emit_symbol_address(emitter, "x10", "_realpath_cache_epoch");
    emitter.instruction("ldr x10, [x10]");                                      // current epoch
    emitter.instruction("str x10, [x22, #40]");                                 // the entry belongs to it
    emitter.instruction("str x23, [x22, #48]");                                 // and was cached now
    emitter.instruction("b __rt_realpath_cache_result");                        // return the resolved path
    emitter.label("__rt_realpath_cache_drop_key");
    emitter.instruction("ldr x0, [x22, #16]");                                  // the key copy just made
    emitter.bl_c("free");                                                       // release it
    emitter.instruction("str xzr, [x22, #16]");                                 // the entry owns nothing
    emitter.label("__rt_realpath_cache_result");
    emitter.instruction("ldp x1, x2, [sp, #48]");                               // the resolved result
    emitter.label("__rt_realpath_cache_out");
    emitter.instruction("ldr x23, [sp, #32]");                                  // restore x23
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore x21/x22
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore x19/x20
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the frame
    emitter.instruction("ret");                                                 // x1/x2 = canonical path
}

/// Emits the caching `__rt_realpath` (x86_64). Same contract as the uncached helper:
/// `rax`/`rdx` in, an owned heap string (or `0`/`0` on failure) out in `rax`/`rdx`.
fn emit_realpath_cached_x86_64(emitter: &mut Emitter) {
    emitter.comment("--- runtime: realpath (cached) ---");
    emitter.label_global("__rt_realpath");
    emitter.instruction("test rdx, rdx");                                       // an empty path is never cached
    emitter.instruction("jz __rt_realpath_uncached");                           // resolve it directly
    emitter.instruction("cmp BYTE PTR [rax], 47");                              // absolute path ('/')?
    emitter.instruction("jne __rt_realpath_uncached");                          // a relative path depends on the cwd: resolve it every time

    // Frame: rbp + rbx, r12-r15 pushed, then 24 bytes (+0 result ptr, +8 result len) keep rsp aligned.
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the frame
    emitter.instruction("push rbx");                                            // rbx = path pointer
    emitter.instruction("push r12");                                            // r12 = path length
    emitter.instruction("push r13");                                            // r13 = path hash
    emitter.instruction("push r14");                                            // r14 = cache entry
    emitter.instruction("push r15");                                            // r15 = time() of this lookup
    emitter.instruction("sub rsp, 24");                                         // result slots, 16-byte aligned for libc calls
    emitter.instruction("mov rbx, rax");                                        // keep the path pointer
    emitter.instruction("mov r12, rdx");                                        // keep the path length

    emitter.instruction("mov r13, -3750763034362895579");                       // FNV-1a 64-bit offset basis
    emitter.instruction("mov r10, 1099511628211");                              // FNV-1a 64-bit prime
    emitter.instruction("xor ecx, ecx");                                        // byte index
    emitter.label("__rt_realpath_cache_hash_x86");
    emitter.instruction("cmp rcx, r12");                                        // every byte hashed?
    emitter.instruction("jae __rt_realpath_cache_hashed_x86");                  // yes: pick the entry
    emitter.instruction("movzx r11d, BYTE PTR [rbx + rcx]");                    // next path byte
    emitter.instruction("xor r13, r11");                                        // fold it into the hash
    emitter.instruction("imul r13, r10");                                       // multiply by the FNV prime
    emitter.instruction("add rcx, 1");                                          // advance
    emitter.instruction("jmp __rt_realpath_cache_hash_x86");                    // continue hashing
    emitter.label("__rt_realpath_cache_hashed_x86");
    abi::emit_symbol_address(emitter, "r14", "_realpath_cache");
    emitter.instruction("mov rax, r13");                                        // hash
    emitter.instruction(&format!("and eax, {}", REALPATH_CACHE_SLOTS - 1));     // slot = hash low bits
    emitter.instruction("shl rax, 6");                                          // slot * entry size
    emitter.instruction("add r14, rax");                                        // r14 = this path's entry
    emitter.instruction("xor edi, edi");                                        // time(NULL)
    emitter.instruction("call time");                                           // rax = now, in seconds
    emitter.instruction("mov r15, rax");                                        // keep it for the TTL check and the stamp

    emitter.instruction("cmp QWORD PTR [r14], r13");                            // same hash?
    emitter.instruction("jne __rt_realpath_cache_miss_x86");                    // no: resolve
    emitter.instruction("cmp QWORD PTR [r14 + 8], r12");                        // same length?
    emitter.instruction("jne __rt_realpath_cache_miss_x86");                    // no: resolve
    abi::emit_symbol_address(emitter, "r10", "_realpath_cache_epoch");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // current epoch
    emitter.instruction("cmp QWORD PTR [r14 + 40], r10");                       // no path changed since?
    emitter.instruction("jne __rt_realpath_cache_miss_x86");                    // a path changed: resolve again
    emitter.instruction("mov rax, r15");                                        // now
    emitter.instruction("sub rax, QWORD PTR [r14 + 48]");                       // the entry's age (huge if the clock went back)
    emitter.instruction(&format!("cmp rax, {}", REALPATH_CACHE_TTL_SECONDS));   // still within the TTL?
    emitter.instruction("jae __rt_realpath_cache_miss_x86");                    // expired: resolve again
    emitter.instruction("mov rdi, QWORD PTR [r14 + 16]");                       // cached key bytes
    emitter.instruction("mov rsi, rbx");                                        // requested path bytes
    emitter.instruction("mov rdx, r12");                                        // their length
    emitter.instruction("call memcmp");                                         // eax = 0 when the bytes match
    emitter.instruction("test eax, eax");                                       // same bytes?
    emitter.instruction("jne __rt_realpath_cache_miss_x86");                    // a hash collision: resolve

    emitter.instruction("mov rax, QWORD PTR [r14 + 24]");                       // result length
    emitter.instruction("call __rt_heap_alloc");                                // rax = owned buffer
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // owned-string heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // tag the buffer as an owned string
    emitter.instruction("mov rbx, rax");                                        // keep the buffer across memcpy
    emitter.instruction("mov rdi, rax");                                        // destination
    emitter.instruction("mov rsi, QWORD PTR [r14 + 32]");                       // cached result bytes
    emitter.instruction("mov rdx, QWORD PTR [r14 + 24]");                       // their length
    emitter.instruction("call memcpy");                                         // copy them into the owned buffer
    emitter.instruction("mov rax, rbx");                                        // result pointer
    emitter.instruction("mov rdx, QWORD PTR [r14 + 24]");                       // result length
    emitter.instruction("jmp __rt_realpath_cache_out_x86");                     // done

    emitter.label("__rt_realpath_cache_miss_x86");
    emitter.instruction("mov rax, rbx");                                        // path pointer
    emitter.instruction("mov rdx, r12");                                        // path length
    emitter.instruction("call __rt_realpath_uncached");                         // rax/rdx = owned result, or 0/0
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // keep the result pointer
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // keep the result length
    emitter.instruction("test rdx, rdx");                                       // failures are not cached
    emitter.instruction("jz __rt_realpath_cache_result_x86");                   // return the failure
    emitter.instruction("mov rdi, QWORD PTR [r14 + 16]");                       // previous key copy (NULL when empty)
    emitter.instruction("call free");                                           // drop it
    emitter.instruction("mov rdi, QWORD PTR [r14 + 32]");                       // previous result copy
    emitter.instruction("call free");                                           // drop it
    emitter.instruction("mov QWORD PTR [r14 + 16], 0");                         // the entry owns no key until the copy lands
    emitter.instruction("mov QWORD PTR [r14 + 32], 0");                         // nor a result
    emitter.instruction("mov QWORD PTR [r14 + 8], 0");                          // and matches no length meanwhile
    emitter.instruction("mov rdi, r12");                                        // key length
    emitter.instruction("call malloc");                                         // rax = key copy
    emitter.instruction("test rax, rax");                                       // out of memory?
    emitter.instruction("jz __rt_realpath_cache_result_x86");                   // just skip caching
    emitter.instruction("mov QWORD PTR [r14 + 16], rax");                       // own the key copy
    emitter.instruction("mov rdi, rax");                                        // destination
    emitter.instruction("mov rsi, rbx");                                        // key bytes
    emitter.instruction("mov rdx, r12");                                        // key length
    emitter.instruction("call memcpy");                                         // copy the key
    emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                        // result length
    emitter.instruction("call malloc");                                         // rax = result copy
    emitter.instruction("test rax, rax");                                       // out of memory?
    emitter.instruction("jz __rt_realpath_cache_drop_key_x86");                 // undo the key
    emitter.instruction("mov QWORD PTR [r14 + 32], rax");                       // own the result copy
    emitter.instruction("mov rdi, rax");                                        // destination
    emitter.instruction("mov rsi, QWORD PTR [rsp]");                            // result bytes
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // result length
    emitter.instruction("call memcpy");                                         // copy the result
    emitter.instruction("mov QWORD PTR [r14], r13");                            // hash
    emitter.instruction("mov QWORD PTR [r14 + 8], r12");                        // key length (makes the entry matchable)
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // result length
    emitter.instruction("mov QWORD PTR [r14 + 24], rax");                       // store it
    abi::emit_symbol_address(emitter, "r10", "_realpath_cache_epoch");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // current epoch
    emitter.instruction("mov QWORD PTR [r14 + 40], r10");                       // the entry belongs to it
    emitter.instruction("mov QWORD PTR [r14 + 48], r15");                       // and was cached now
    emitter.instruction("jmp __rt_realpath_cache_result_x86");                  // return the resolved path
    emitter.label("__rt_realpath_cache_drop_key_x86");
    emitter.instruction("mov rdi, QWORD PTR [r14 + 16]");                       // the key copy just made
    emitter.instruction("call free");                                           // release it
    emitter.instruction("mov QWORD PTR [r14 + 16], 0");                         // the entry owns nothing
    emitter.label("__rt_realpath_cache_result_x86");
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // the resolved result pointer
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // and length
    emitter.label("__rt_realpath_cache_out_x86");
    emitter.instruction("add rsp, 24");                                         // release the result slots
    emitter.instruction("pop r15");                                             // restore r15
    emitter.instruction("pop r14");                                             // restore r14
    emitter.instruction("pop r13");                                             // restore r13
    emitter.instruction("pop r12");                                             // restore r12
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // rax/rdx = canonical path
}
