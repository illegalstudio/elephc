//! Purpose:
//! Emits the `__rt_scandir`, `__rt_cstr` runtime helper assembly for scandir.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::runtime::data::{
    SCANDIR_ERRNO_WARNING_HEAD, SCANDIR_ERRNO_WARNING_MIDDLE, SCANDIR_OPEN_WARNING_HEAD,
    SCANDIR_OPEN_WARNING_MIDDLE,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_scandir` runtime helper for listing directory entries.
///
/// Dispatches to `emit_scandir_linux_x86_64` for x86_64; generates inline ARM64
/// assembly for all other targets.
///
/// Input: path string in x1/x2 (ptr/len) following the runtime string convention.
/// Output: x0 holds an `Array` of `String` filenames, or an empty array on error.
/// Side effects: calls `opendir`, `readdir`, `closedir` from libc; allocates
/// runtime memory for filename persistence and result array growth.
pub fn emit_scandir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_scandir_linux_x86_64(emitter);
        return;
    }

    let name_off = emitter.platform.dirent_name_offset();
    // libc `opendir` reports failure through errno, and the thread-local accessor is spelled
    // differently per platform — the same split `mb_strlen` and the flock helpers already carry.
    let errno_function = match emitter.platform {
        crate::codegen_support::platform::Platform::MacOS => "__error",
        crate::codegen_support::platform::Platform::Linux => "__errno_location",
        crate::codegen_support::platform::Platform::Windows => {
            panic!("Windows target is not yet supported (see issue #379)")
        }
    };

    emitter.blank();
    emitter.comment("--- runtime: scandir ---");
    emitter.label_global("__rt_scandir");

    // The array is allocated BEFORE `opendir`, and a null `DIR*` returns it empty
    // rather than entering the loop: `readdir(NULL)` is undefined and segfaulted on
    // a missing directory. The x86_64 emitter below already tested `opendir`, which
    // is why only one target crashed — the same one-architecture asymmetry the stat
    // field helpers had. (PHP answers `false` here; that needs the declared return
    // type to become a union and is tracked with the rest of that family.)
    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // frame: diagnostic slots plus the sorting order
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer
    emitter.instruction("str x0, [sp, #48]");                                   // hold $sorting_order across the listing

    // -- null-terminate path --
    emitter.instruction("bl __rt_cstr");                                        // convert path to C string, x0=cstr
    emitter.instruction("str x0, [sp, #24]");                                   // hold the C path across the result-array allocation

    // -- create the result array FIRST so an unopenable directory still has one to return --
    emitter.instruction("mov x0, #128");                                        // initial capacity of 128 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // create array, x0=array pointer
    emitter.instruction("str x0, [sp, #8]");                                    // save array pointer on stack

    // -- open directory --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the C path for opendir()
    emitter.bl_c("opendir");                                         // opendir(cstr), x0=DIR* or NULL
    // A directory that cannot be opened returns NULL, and the loop below fed that straight to
    // readdir(): scandir() on a missing path took the process down with it. x86_64 already
    // guarded this, so the crash only ever happened on AArch64.
    emitter.instruction("cbz x0, __rt_scandir_open_failed");                    // opendir() failed: say so the way php does
    emitter.instruction("str x0, [sp, #0]");                                    // save DIR pointer on stack

    // -- read directory entries in a loop --
    emitter.label("__rt_scandir_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload DIR pointer
    emitter.bl_c("readdir");                                         // readdir(DIR*), x0=dirent* or NULL
    emitter.instruction("cbz x0, __rt_scandir_close");                          // if NULL, no more entries

    // -- point at d_name and measure it until the terminating NUL --
    emitter.instruction(&format!("add x1, x0, #{}", name_off));                 // x1 = pointer to dirent.d_name for this platform
    emitter.instruction("mov x2, #0");                                          // x2 = filename length
    emitter.label("__rt_scandir_strlen");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load the next byte from d_name
    emitter.instruction("cbz w9, __rt_scandir_name_ready");                     // stop at the terminating NUL byte
    emitter.instruction("add x2, x2, #1");                                      // count one more filename byte
    emitter.instruction("b __rt_scandir_strlen");                               // continue scanning the filename
    emitter.label("__rt_scandir_name_ready");

    // -- push name string to array --
    // `__rt_array_push_str` persists the (pointer, length) pair itself, so persisting the
    // entry name here first allocated a SECOND owned block that nothing ever stored or
    // freed: one orphan per directory entry, every call. The x86_64 path below always
    // pushed the raw `d_name` pair directly, which is why this leaked on AArch64 alone.
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload array pointer
    emitter.instruction("bl __rt_array_push_str");                              // persist and push the name (x1 = d_name, x2 = its length)
    emitter.instruction("str x0, [sp, #8]");                                    // update array pointer after possible realloc
    emitter.instruction("b __rt_scandir_loop");                                 // continue reading entries

    // -- close directory and return --
    emitter.label("__rt_scandir_close");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload DIR pointer
    emitter.bl_c("closedir");                                        // closedir(DIR*)
    // php sorts the listing — ascending by default, descending for SCANDIR_SORT_DESCENDING,
    // and readdir order only when SCANDIR_SORT_NONE asks for it. elephc answered readdir
    // order for every call, which is filesystem-dependent: the same program printed a
    // different listing on a different machine.
    emitter.instruction("ldr x9, [sp, #48]");                                   // the requested sorting order
    emitter.instruction("cmp x9, #2");                                          // SCANDIR_SORT_NONE keeps readdir order
    emitter.instruction("b.eq __rt_scandir_ret");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the listing to sort
    emitter.instruction("cmp x9, #1");                                          // SCANDIR_SORT_DESCENDING?
    emitter.instruction("b.eq __rt_scandir_sort_desc");
    emitter.instruction("bl __rt_sort_str");                                    // ascending, php's default
    emitter.instruction("b __rt_scandir_ret");
    emitter.label("__rt_scandir_sort_desc");
    emitter.instruction("bl __rt_rsort_str");
    emitter.instruction("b __rt_scandir_ret");                                  // a directory that opened has nothing to warn about

    // -- the two lines php prints for a directory it cannot open --
    // Neither needs a composer of its own. `__rt_errno_warning` already appends `strerror` and
    // the newline, so it serves as the TAIL of both and only the beginning differs. The
    // pre-allocated result array is released and the return slot zeroed: php answers FALSE for
    // a directory it cannot open, and a null pointer is what the caller's boxing reads as
    // false — an empty listing here made failure indistinguishable from an empty directory.
    emitter.label("__rt_scandir_open_failed");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the listing that will never be filled
    emitter.instruction("bl __rt_heap_free");
    emitter.instruction("str xzr, [sp, #8]");                                   // the caller boxes the null as PHP false
    emitter.bl_c(errno_function);                                               // x0 = &errno for this thread
    emitter.instruction("ldrsw x9, [x0]");                                      // the errno libc opendir() set
    emitter.instruction("str x9, [sp, #32]");                                   // hold it across every fragment

    // -- "Warning: scandir(" --
    abi::emit_symbol_address(emitter, "x1", "_scandir_open_warn_head");
    emitter.instruction(&format!("mov x2, #{}", SCANDIR_OPEN_WARNING_HEAD.len()));
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    // -- the path, measured to its terminator --
    emitter.instruction("ldr x1, [sp, #24]");                                   // the NUL-terminated C path
    emitter.instruction("mov x9, #0");                                          // measured length
    emitter.label("__rt_scandir_path_scan");
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the next path byte
    emitter.instruction("cbz w10, __rt_scandir_path_scanned");                  // reached the terminator
    emitter.instruction("add x9, x9, #1");                                      // keep measuring
    emitter.instruction("b __rt_scandir_path_scan");
    emitter.label("__rt_scandir_path_scanned");
    emitter.instruction("mov x2, x9");                                          // the measured byte length
    emitter.instruction("bl __rt_diag_warning");
    // -- "): Failed to open directory: " + strerror + newline --
    abi::emit_symbol_address(emitter, "x0", "_scandir_open_warn_mid");
    emitter.instruction(&format!("mov x1, #{}", SCANDIR_OPEN_WARNING_MIDDLE.len()));
    emitter.instruction("ldr x2, [sp, #32]");                                   // the errno to describe
    emitter.instruction("bl __rt_errno_warning");

    // -- "Warning: scandir(): (errno " --
    abi::emit_symbol_address(emitter, "x1", "_scandir_errno_warn_head");
    emitter.instruction(&format!("mov x2, #{}", SCANDIR_ERRNO_WARNING_HEAD.len()));
    emitter.instruction("bl __rt_diag_warning");
    // -- the number itself. `__rt_itoa` formats into the shared concat arena and ADVANCES its
    //    cursor, so the entry value is restored afterwards: a loop over unreadable directories
    //    would otherwise eat the 64 KiB buffer a few bytes at a time.
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // the caller's concat write offset
    emitter.instruction("str x10, [sp, #40]");                                  // hold it across the conversion
    emitter.instruction("ldr x0, [sp, #32]");                                   // the errno to render
    emitter.instruction("bl __rt_itoa");                                        // x1/x2 = its decimal text
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [sp, #40]");
    emitter.instruction("str x10, [x9]");                                       // reclaim the diagnostic's scratch
    // -- "): " + strerror + newline --
    abi::emit_symbol_address(emitter, "x0", "_scandir_errno_warn_mid");
    emitter.instruction(&format!("mov x1, #{}", SCANDIR_ERRNO_WARNING_MIDDLE.len()));
    emitter.instruction("ldr x2, [sp, #32]");
    emitter.instruction("bl __rt_errno_warning");

    // -- restore frame and return --
    emitter.label("__rt_scandir_ret");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return array pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the `__rt_scandir` runtime helper for x86_64 Linux targets.
 ///
 /// Uses the System V AMD64 ABI: path in `rax/rdx`, result array returned in `rax`.
 /// Invokes `opendir`, `readdir`, `closedir` from libc; allocates a runtime `Array`
 /// with 128-entry initial capacity; persists each filename via `__rt_str_persist`
 /// before the next `readdir` call clobbers the `dirent` buffer.
fn emit_scandir_linux_x86_64(emitter: &mut Emitter) {
    let name_off = emitter.platform.dirent_name_offset();
    // See the AArch64 counterpart: the thread-local errno accessor is spelled per platform.
    let errno_function = match emitter.platform {
        crate::codegen_support::platform::Platform::MacOS => "__error",
        crate::codegen_support::platform::Platform::Linux => "__errno_location",
        crate::codegen_support::platform::Platform::Windows => {
            panic!("Windows target is not yet supported (see issue #379)")
        }
    };

    emitter.blank();
    emitter.comment("--- runtime: scandir ---");
    emitter.label_global("__rt_scandir");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while scandir() uses directory and result-array spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the C path, result array, and DIR* locals
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the C path, result array, DIR* handle, loop scratch, the failure diagnostic, and the sorting order
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // hold $sorting_order across the listing
    emitter.instruction("call __rt_cstr");                                      // convert the elephc directory string in rax/rdx into a null-terminated C path in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the C directory path pointer across the result-array allocation and opendir() call
    emitter.instruction("mov rdi, 128");                                        // request an initial result-array capacity of 128 directory entry names
    emitter.instruction("mov rsi, 16");                                         // request 16-byte payload slots because scandir() returns string ptr/len pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the destination string array that will collect the directory entry names
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination string array pointer across the directory iteration loop
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the C directory path pointer before opening the directory stream
    emitter.instruction("call opendir");                                        // open the directory stream through libc opendir()
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the DIR* handle across the readdir() loop and the final closedir() call
    emitter.instruction("test rax, rax");                                       // detect opendir() failure before entering the directory iteration loop
    emitter.instruction("jz __rt_scandir_open_failed_x");                       // say why the directory stream could not be opened, the way php does

    emitter.label("__rt_scandir_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the DIR* handle before asking libc for the next directory entry
    emitter.instruction("call readdir");                                        // fetch the next directory entry through libc readdir()
    emitter.instruction("test rax, rax");                                       // detect the end-of-directory marker before measuring a filename or appending it
    emitter.instruction("jz __rt_scandir_close");                               // stop iterating once libc readdir() reports that no more directory entries remain
    emitter.instruction(&format!("lea rsi, [rax + {}]", name_off));             // compute the pointer to dirent.d_name for the current Linux directory entry layout
    emitter.instruction("xor edx, edx");                                        // start the filename length counter at zero before scanning for the trailing null byte
    emitter.label("__rt_scandir_strlen");
    emitter.instruction("mov r8b, BYTE PTR [rsi + rdx]");                       // load the next filename byte from dirent.d_name while measuring its elephc string length
    emitter.instruction("test r8b, r8b");                                       // stop scanning once the trailing C null terminator is reached
    emitter.instruction("jz __rt_scandir_push");                                // continue into the append path once the current filename length is known
    emitter.instruction("add rdx, 1");                                          // advance the measured filename length after consuming one non-null byte
    emitter.instruction("jmp __rt_scandir_strlen");                             // continue scanning until the current directory entry name is fully measured

    emitter.label("__rt_scandir_push");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the destination string array pointer into the x86_64 append-helper receiver register
    emitter.instruction("call __rt_array_push_str");                            // persist and append the current directory entry name into the destination string array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the possibly-grown destination string array pointer after appending one directory entry
    emitter.instruction("jmp __rt_scandir_loop");                               // continue iterating until libc readdir() reports end-of-directory

    emitter.label("__rt_scandir_close");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the DIR* handle before closing the directory stream
    emitter.instruction("call closedir");                                       // close the directory stream through libc closedir()
    // See the AArch64 counterpart: php sorts the listing unless SCANDIR_SORT_NONE asks not to.
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // the requested sorting order
    emitter.instruction("cmp r9, 2");                                           // SCANDIR_SORT_NONE keeps readdir order
    emitter.instruction("je __rt_scandir_ret");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the listing to sort
    emitter.instruction("cmp r9, 1");                                           // SCANDIR_SORT_DESCENDING?
    emitter.instruction("je __rt_scandir_sort_desc_x");
    emitter.instruction("call __rt_sort_str");                                  // ascending, php's default
    emitter.instruction("jmp __rt_scandir_ret");
    emitter.label("__rt_scandir_sort_desc_x");
    emitter.instruction("call __rt_rsort_str");
    emitter.instruction("jmp __rt_scandir_ret");                                // a directory that opened has nothing to warn about

    // -- the two lines php prints for a directory it cannot open; see the AArch64 counterpart --
    emitter.label("__rt_scandir_open_failed_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the listing that will never be filled
    emitter.instruction("call __rt_heap_free");                                 // reads rax: the rax-first family
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // the caller boxes the null as PHP false
    emitter.instruction(&format!("call {errno_function}"));                     // rax = &errno for this thread
    emitter.instruction("movsxd rax, DWORD PTR [rax]");                         // the errno libc opendir() set
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // hold it across every fragment

    // -- "Warning: scandir(" --
    abi::emit_symbol_address(emitter, "rdi", "_scandir_open_warn_head");
    emitter.instruction(&format!("mov rsi, {}", SCANDIR_OPEN_WARNING_HEAD.len()));
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    // -- the path, measured to its terminator --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the NUL-terminated C path
    emitter.instruction("xor ecx, ecx");                                        // measured length
    emitter.label("__rt_scandir_path_scan_x");
    emitter.instruction("movzx eax, BYTE PTR [rdi + rcx]");                     // load the next path byte
    emitter.instruction("test al, al");
    emitter.instruction("jz __rt_scandir_path_scanned_x");                      // reached the terminator
    emitter.instruction("add rcx, 1");                                          // keep measuring
    emitter.instruction("jmp __rt_scandir_path_scan_x");
    emitter.label("__rt_scandir_path_scanned_x");
    emitter.instruction("mov rsi, rcx");                                        // the measured byte length
    emitter.instruction("call __rt_diag_warning");
    // -- "): Failed to open directory: " + strerror + newline --
    abi::emit_symbol_address(emitter, "rdi", "_scandir_open_warn_mid");
    emitter.instruction(&format!("mov rsi, {}", SCANDIR_OPEN_WARNING_MIDDLE.len()));
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // the errno to describe
    emitter.instruction("call __rt_errno_warning");

    // -- "Warning: scandir(): (errno " --
    abi::emit_symbol_address(emitter, "rdi", "_scandir_errno_warn_head");
    emitter.instruction(&format!("mov rsi, {}", SCANDIR_ERRNO_WARNING_HEAD.len()));
    emitter.instruction("call __rt_diag_warning");
    // -- the number itself, with the concat cursor reclaimed afterwards --
    abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // the caller's concat write offset
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // hold it across the conversion
    // `__rt_itoa` reads `rax`, not the SysV first-argument register: the runtime's formatting
    // helpers are the rax-first family, as `__rt_mixed_cast_string` and `__rt_heap_free` are.
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // the errno to render
    emitter.instruction("call __rt_itoa");                                      // rax/rdx = its decimal text
    emitter.instruction("mov rdi, rax");                                        // the diagnostic helper takes rdi/rsi
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");
    emitter.instruction("mov QWORD PTR [r10], r11");                            // reclaim the diagnostic's scratch
    // -- "): " + strerror + newline --
    abi::emit_symbol_address(emitter, "rdi", "_scandir_errno_warn_mid");
    emitter.instruction(&format!("mov rsi, {}", SCANDIR_ERRNO_WARNING_MIDDLE.len()));
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
    emitter.instruction("call __rt_errno_warning");

    emitter.label("__rt_scandir_ret");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the destination string array pointer in the canonical x86_64 integer result register
    // The release MUST match the prologue's `sub rsp, 64`. It said 32 — so `pop rbp` read
    // the saved-errno spill slot (rbp = 2, ENOENT) and `ret` jumped to the DIR* slot (NULL
    // on a failed open, a heap address on a successful one): every x86 scandir call
    // segfaulted, success and failure alike, which is exactly what CI's twelve red
    // linux-x86_64 shards had in common. AArch64 tears its frame down by absolute offsets
    // and never had the mismatch.
    emitter.instruction("add rsp, 64");                                         // release the temporary scandir() spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the directory entry array
    emitter.instruction("ret");                                                 // return the array of directory entry names to the caller
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::emit_scandir;

    /// `scandir()` must test `opendir()` before iterating, on BOTH architectures.
    ///
    /// The AArch64 emitter opened the directory and went straight into the loop, so a
    /// missing path reached `readdir(NULL)` and the program died with SIGSEGV where PHP
    /// answers `false`. x86_64 already tested the handle — which is exactly why a
    /// behavioural test running on one host could not see it, and why the parity is
    /// pinned on the EMITTED assembly of each target instead.
    #[test]
    fn scandir_tests_the_directory_handle_before_iterating_on_every_target() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_scandir(&mut emitter);
            let asm = emitter.output();
            let open_at = asm.find("opendir").expect("scandir opens the directory");
            let loop_at = asm
                .find("__rt_scandir_loop:")
                .expect("scandir iterates the directory");
            let guard = &asm[open_at..loop_at];
            let branches_away = guard.contains("cbz x0, __rt_scandir_ret")
                || guard.contains("jz __rt_scandir_ret");
            assert!(
                branches_away,
                "{arch:?}: a null DIR* must skip the loop, or readdir(NULL) segfaults:\n{guard}"
            );
        }
    }
}
