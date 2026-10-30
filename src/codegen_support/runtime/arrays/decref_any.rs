//! Purpose:
//! Emits the `__rt_decref_any`, `__rt_decref_any_done` runtime helper assembly for decref any.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Packed integer arrays are not swept graph nodes, so even the final sweep must release
//!   their real owners normally. Other graph children retain sweep-only suppression.
//! - Decrement helpers are release paths for refcounted values and must balance recursive frees with GC cycle collection.

use crate::codegen_support::{abi, emit::Emitter};
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::{REFERENCE_CELL_HEAP_KIND, CALLABLE_HEAP_KIND};


/// Uniform release dispatcher for mixed heap-backed values.
///
/// Reads the heap kind tag from the value's header, validates the pointer against the
/// managed heap window, and dispatches to the appropriate concrete release helper
/// (string, array, hash, object/throwable, mixed). Skips GC-tracked children during active cycle
/// collection to avoid double-frees when the collector reclaims them directly.
///
/// Input: x0 (ARM64) or rax (x86_64) = heap-backed value pointer
/// Output: none
pub fn emit_decref_any(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_decref_any_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: decref_any ---");
    emitter.label_global("__rt_decref_any");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_decref_any_done");                        // skip null values immediately
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the pointer below the heap buffer?
    emitter.instruction("b.lo __rt_decref_any_done");                           // non-heap values need no release
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the pointer at or beyond the heap end?
    emitter.instruction("b.hs __rt_decref_any_done");                           // skip invalid or non-heap pointers

    // -- inspect the full kind word so collector-only flags stay visible --
    emitter.instruction("ldr x11, [x0, #-8]");                                  // load the full 64-bit kind word from the heap header

    // -- only the final sweep skips graph children that it reclaims directly --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_gc_freeing_unreachable");
    emitter.instruction("ldr x12, [x12]");                                      // load sweep-only child-release suppression
    emitter.instruction("cbz x12, __rt_decref_any_dispatch");                   // destructor callbacks still balance real graph owners
    emitter.instruction("and x13, x11, #0xff");                                 // isolate the low-byte heap kind tag
    emitter.instruction("cmp x13, #2");                                         // is this a refcounted indexed array?
    emitter.instruction("b.lo __rt_decref_any_dispatch");                       // strings should still be freed immediately
    // -- integer leaves are released normally because the collector never sweeps them --
    emitter.instruction("b.ne __rt_decref_any_graph_kind");                     // other heap kinds keep their existing graph classification
    emitter.instruction("ubfx x14, x11, #8, #7");                               // isolate array value_type without the COW or collector flags
    emitter.instruction("cmp x14, #4");                                         // only reference-bearing indexed arrays participate in the graph
    emitter.instruction("b.lo __rt_decref_any_dispatch");                       // scalar and string array leaves retain ordinary cleanup
    emitter.instruction("cmp x14, #10");                                        // callable slots own descriptors with managed capture graphs
    emitter.instruction("b.eq __rt_decref_any_callable_tag_62");                // admit callable ownership without admitting scalar tags
    emitter.instruction("cmp x14, #7");                                         // reject unknown element storage from sweep suppression
    emitter.instruction("b.hi __rt_decref_any_dispatch");                       // packed integers cannot form cycles and still own their array buffer
    emitter.label("__rt_decref_any_callable_tag_62");
    emitter.label("__rt_decref_any_graph_kind");
    emitter.instruction(&format!("cmp x13, #{CALLABLE_HEAP_KIND}"));            // callable descriptors own graph captures
    emitter.instruction("b.eq __rt_decref_any_graph_child");                    // select callable traversal or ownership dispatch
    emitter.instruction(&format!("cmp x13, #{REFERENCE_CELL_HEAP_KIND}"));      // owned reference cells are collector graph nodes
    emitter.instruction("b.eq __rt_decref_any_graph_child");                    // apply sweep suppression to reference cells
    emitter.instruction("cmp x13, #6");                                         // is this within the refcounted array/hash/object/mixed/throwable range?
    emitter.instruction("b.hi __rt_decref_any_dispatch");                       // raw/untyped blocks are not part of refcounted graph cleanup
    emitter.label("__rt_decref_any_graph_child");
    emitter.instruction("tbz x11, #17, __rt_decref_any_dispatch");              // only current snapshot members suppress ordinary releases
    emitter.instruction("mov x14, #1");                                         // prepare a single-bit reachable mask
    emitter.instruction("lsl x14, x14, #16");                                   // x14 = GC reachable bit in the kind word
    emitter.instruction("tst x11, x14");                                        // does this child stay reachable from an external root?
    emitter.instruction("b.eq __rt_decref_any_done");                           // unreachable refcounted children are reclaimed by the collector itself

    // -- dispatch to the concrete release routine --
    emitter.label("__rt_decref_any_dispatch");
    emitter.instruction("and x11, x11, #0xff");                                 // keep only the low-byte heap kind tag
    emitter.instruction("cmp x11, #1");                                         // is this an owned string buffer?
    emitter.instruction("b.eq __rt_decref_any_string");                         // release strings via heap_free_safe
    emitter.instruction("cmp x11, #2");                                         // is this an indexed array?
    emitter.instruction("b.eq __rt_decref_any_array");                          // release arrays through __rt_decref_array
    emitter.instruction("cmp x11, #3");                                         // is this an associative array / hash?
    emitter.instruction("b.eq __rt_decref_any_hash");                           // release hashes through __rt_decref_hash
    emitter.instruction("cmp x11, #4");                                         // is this an object instance?
    emitter.instruction("b.eq __rt_decref_any_object");                         // release objects through __rt_decref_object
    emitter.instruction("cmp x11, #5");                                         // is this a boxed mixed value?
    emitter.instruction("b.eq __rt_decref_any_mixed");                          // release mixed cells through __rt_decref_mixed
    emitter.instruction(&format!("cmp x11, #{CALLABLE_HEAP_KIND}"));            // callable descriptors own graph captures
    emitter.instruction("b.eq __rt_decref_any_callable");                       // select callable traversal or ownership dispatch
    emitter.instruction(&format!("cmp x11, #{REFERENCE_CELL_HEAP_KIND}"));      // recognize independently owned reference cells
    emitter.instruction("b.eq __rt_decref_any_reference");                      // select a local dispatch stub for final-owner cell cleanup
    emitter.instruction("cmp x11, #6");                                         // is this a throwable object?
    emitter.instruction("b.eq __rt_decref_any_object");                         // release throwables through the object decref helper
    emitter.instruction("ret");                                                 // unknown/raw kinds need no release

    emitter.label("__rt_decref_any_string");
    emitter.instruction("ldr w11, [x0, #-12]");                                 // load the persisted string's current owner count
    emitter.instruction("subs w11, w11, #1");                                   // release exactly the ownership represented by this value
    emitter.instruction("str w11, [x0, #-12]");                                 // publish the reduced owner count before deciding whether to reclaim
    emitter.instruction("b.ne __rt_decref_any_done");                           // another owner, such as a hash key anchor, still keeps the string alive
    emitter.instruction("b __rt_heap_free");                                    // reclaim this already-validated string after its final owner releases it

    emitter.label("__rt_decref_any_array");
    emitter.instruction("b __rt_decref_array");                                 // tail-call to array decref

    emitter.label("__rt_decref_any_hash");
    emitter.instruction("b __rt_decref_hash");                                  // tail-call to hash decref

    emitter.label("__rt_decref_any_object");
    emitter.instruction("b __rt_decref_object");                                // tail-call to object decref

    emitter.label("__rt_decref_any_mixed");
    emitter.instruction("b __rt_decref_mixed");                                 // tail-call to mixed-cell decref

    emitter.label("__rt_decref_any_callable");
    abi::emit_jump(emitter, "__rt_callable_descriptor_release");

    emitter.label("__rt_decref_any_reference");
    abi::emit_jump(emitter, "__rt_reference_cell_release");

    emitter.label("__rt_decref_any_done");
    emitter.instruction("ret");                                                 // nothing to release
}

/// x86_64 Linux implementation of the uniform release dispatcher.
/// Uses the x86_64 heap wrapper header format with a high-word magic marker to distinguish
/// managed heap payloads from foreign or static pointers before dispatching to concrete
/// release helpers (string, array, hash, object/throwable, mixed).
/// Input: rax = heap-backed value pointer
/// Output: none (tail-calls to specialized release helpers)
fn emit_decref_any_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: decref_any ---");
    emitter.label_global("__rt_decref_any");

    emitter.instruction("test rax, rax");                                       // skip null heap-backed payload pointers so non-values do not participate in x86_64 release traffic
    emitter.instruction("jz __rt_decref_any_done");                             // null payloads own no heap storage and therefore need no release work
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("cmp rax, r10");                                        // reject values below the managed x86_64 heap before reading a header word
    emitter.instruction("jb __rt_decref_any_done");                             // scalar integers and static data below the heap own no runtime storage
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current x86_64 heap bump extent
    emitter.instruction("add r11, r10");                                        // compute the managed heap end address
    emitter.instruction("cmp rax, r11");                                        // is the candidate pointer outside the live heap window?
    emitter.instruction("jae __rt_decref_any_done");                            // non-heap values above the managed heap own no runtime storage
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("mov r11, r10");                                        // preserve the full heap kind word before isolating the ownership marker
    emitter.instruction("shr r11, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(                                                        // verify that the payload belongs to the x86_64 heap wrapper before dispatching a release helper
        &format!("cmp r11d, 0x{:x}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)
    );
    emitter.instruction("jne __rt_decref_any_done");                            // foreign/static pointers must be ignored by the uniform x86_64 release dispatcher
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_gc_freeing_unreachable");
    emitter.instruction("cmp QWORD PTR [r11], 0");                              // destructor callbacks use ordinary balanced reference counts
    emitter.instruction("je __rt_decref_any_dispatch");                         // skip collector suppression outside the final sweep
    emitter.instruction("test r10, 0x20000");                                   // only snapshot members are independently swept
    emitter.instruction("jz __rt_decref_any_dispatch");                         // outside children still release their actual owners
    emitter.instruction("test r10, 0x10000");                                   // surviving graph children still need ordinary decrements
    emitter.instruction("jnz __rt_decref_any_dispatch");                        // preserve normal cleanup for reachable children
    emitter.instruction("mov r11, r10");                                        // keep the full kind word until dispatch
    emitter.instruction("and r11d, 0xff");                                      // inspect the concrete heap kind
    emitter.instruction("cmp r11d, 2");                                         // strings and raw allocations are not swept graph nodes
    emitter.instruction("jb __rt_decref_any_dispatch");                         // release non-graph children through their ordinary helper
    // -- integer leaves are released normally because the collector never sweeps them --
    emitter.instruction("jne __rt_decref_any_graph_kind");                      // other heap kinds keep their existing graph classification
    emitter.instruction("mov r11, r10");                                        // preserve the full header while selecting array storage
    emitter.instruction("shr r11, 8");                                          // move the array value_type into the low bits
    emitter.instruction("and r11d, 0x7f");                                      // exclude the persistent COW and transient collector flags
    emitter.instruction("cmp r11d, 4");                                         // only reference-bearing arrays are swept graph nodes
    emitter.instruction("jb __rt_decref_any_dispatch");                         // scalar and string array leaves retain ordinary cleanup
    emitter.instruction("cmp r11d, 10");                                        // callable slots own descriptors with managed capture graphs
    emitter.instruction("je __rt_decref_any_callable_tag_171");                 // admit callable ownership without admitting scalar tags
    emitter.instruction("cmp r11d, 7");                                         // reject unknown element storage from sweep suppression
    emitter.instruction("ja __rt_decref_any_dispatch");                         // unsupported storage keeps ordinary ownership
    emitter.label("__rt_decref_any_callable_tag_171");
    emitter.instruction("mov r11d, 2");                                         // restore the concrete kind before the existing graph-range test
    emitter.label("__rt_decref_any_graph_kind");
    emitter.instruction("cmp r11d, 5");                                         // the collector directly reclaims array, hash, object, and Mixed nodes
    emitter.instruction("jbe __rt_decref_any_done");                            // never decrement already reclaimed or independently pinned doomed nodes
    emitter.instruction(&format!("cmp r11d, {CALLABLE_HEAP_KIND}"));            // callable descriptors own graph captures
    emitter.instruction("je __rt_decref_any_done");                             // select callable traversal or ownership dispatch
    emitter.instruction(&format!("cmp r11d, {REFERENCE_CELL_HEAP_KIND}"));      // owned reference cells are independently swept graph nodes
    emitter.instruction("je __rt_decref_any_done");                             // leave doomed cell retirement to the collector
    emitter.label("__rt_decref_any_dispatch");
    emitter.instruction("and r10, 0xff");                                       // isolate the low-byte uniform heap kind tag for the concrete release dispatch
    emitter.instruction("cmp r10, 1");                                          // does this heap-backed payload own a persisted string buffer?
    emitter.instruction("je __rt_decref_any_string");                           // strings release through heap_free_safe on x86_64
    emitter.instruction("cmp r10, 2");                                          // does this heap-backed payload point at an indexed array?
    emitter.instruction("je __rt_decref_any_array");                            // indexed arrays release through the x86_64 array decref helper
    emitter.instruction("cmp r10, 3");                                          // does this heap-backed payload point at an associative array?
    emitter.instruction("je __rt_decref_any_hash");                             // hashes release through the x86_64 hash decref helper
    emitter.instruction("cmp r10, 4");                                          // does this heap-backed payload point at an object instance?
    emitter.instruction("je __rt_decref_any_object");                           // objects release through the x86_64 object decref helper
    emitter.instruction("cmp r10, 5");                                          // does this heap-backed payload point at a boxed mixed cell?
    emitter.instruction("je __rt_decref_any_mixed");                            // mixed cells release through the x86_64 mixed decref helper
    emitter.instruction(&format!("cmp r10, {CALLABLE_HEAP_KIND}"));             // callable descriptors own graph captures
    emitter.instruction("je __rt_decref_any_callable");                         // select callable traversal or ownership dispatch
    emitter.instruction(&format!("cmp r10, {REFERENCE_CELL_HEAP_KIND}"));       // recognize independently owned reference cells
    emitter.instruction("je __rt_decref_any_reference");                        // select a local dispatch stub for final-owner cell cleanup
    emitter.instruction("cmp r10, 6");                                          // does this heap-backed payload point at a throwable object (issue #448)?
    emitter.instruction("je __rt_decref_any_object");                           // throwables release through the x86_64 object decref helper like plain objects
    emitter.instruction("jmp __rt_decref_any_done");                            // unknown/raw heap kinds need no release work in the current x86_64 bootstrap runtime

    emitter.label("__rt_decref_any_string");
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the persisted string's current x86_64 owner count
    emitter.instruction("sub r10d, 1");                                         // release exactly the ownership represented by this value
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // publish the reduced owner count before deciding whether to reclaim
    emitter.instruction("jne __rt_decref_any_done");                            // another owner, such as a hash key anchor, still keeps the string alive
    emitter.instruction("jmp __rt_heap_free");                                  // reclaim this already-validated string after its final owner releases it

    emitter.label("__rt_decref_any_array");
    emitter.instruction("jmp __rt_decref_array");                               // tail-call to the indexed-array decref helper on x86_64

    emitter.label("__rt_decref_any_hash");
    emitter.instruction("jmp __rt_decref_hash");                                // tail-call to the associative-array decref helper on x86_64

    emitter.label("__rt_decref_any_object");
    emitter.instruction("jmp __rt_decref_object");                              // tail-call to the object decref helper on x86_64

    emitter.label("__rt_decref_any_mixed");
    emitter.instruction("jmp __rt_decref_mixed");                               // tail-call to the mixed-box decref helper on x86_64

    emitter.label("__rt_decref_any_callable");
    abi::emit_jump(emitter, "__rt_callable_descriptor_release");

    emitter.label("__rt_decref_any_reference");
    abi::emit_jump(emitter, "__rt_reference_cell_release");

    emitter.label("__rt_decref_any_done");
    emitter.instruction("ret");                                                 // nothing to release for null, foreign, or unsupported heap kinds
}

#[cfg(test)]
mod packed_integer_tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    /// Removes only this test's assembly and objects on success or assertion failure.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        /// Cleans the isolated cross-assembly directory.
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    /// Integer leaves retain normal ownership even while graph nodes suppress back-edge releases.
    #[test]
    fn packed_integer_leaf_sweep_release_all_targets() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
            .join(format!("integer-leaf-release-{}-{nonce}", std::process::id())));
        fs::create_dir_all(&dir.0).unwrap();
        let clang = Command::new("clang").arg("--version").output().is_ok();
        for (name, triple) in [
            ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13.0"),
            ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
        ] {
            for pic in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.pic_data_refs = pic;
                emit_decref_any(&mut emitter);
                let assembly = emitter.output();
                let x86 = name == "linux-x86_64";
                let mask = if x86 { "and r11d, 0x7f" } else { "ubfx x14, x11, #8, #7" };
                let (_, gate) = assembly.split_once(mask).expect("integer storage gate");
                let dispatch = if x86 { "jb __rt_decref_any_dispatch" }
                    else { "b.lo __rt_decref_any_dispatch" };
                assert!(gate.find(dispatch).unwrap() < gate.find("__rt_decref_any_graph_kind:\n").unwrap(),
                    "{name}, PIC={pic}: integer leaf must bypass graph suppression");
                assert!(assembly.contains(if x86 { "test r10, 0x10000" } else { "tst x11, x14" }),
                    "{name}: ordinary graph-child reachability remains required");
                if clang {
                    let path = dir.0.join(format!("{name}-{pic}.s"));
                    let prefix = if x86 { ".intel_syntax noprefix\n" } else { "" };
                    fs::write(&path, format!(".text\n{prefix}{assembly}")).unwrap();
                    let result = Command::new("clang").args(["-target", triple, "-c"]).arg(&path)
                        .arg("-o").arg(path.with_extension("o")).output().unwrap();
                    assert!(result.status.success(), "{name}, PIC={pic}: {}",
                        String::from_utf8_lossy(&result.stderr));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies every supported architecture routes throwable heap kind 6 through
    /// object release, including ARM64's active-cycle collection range.
    #[test]
    fn test_decref_any_dispatches_throwable_kind_on_all_supported_architectures() {
        for platform in [Platform::MacOS, Platform::Linux] {
            let mut emitter = Emitter::new(Target::new(platform, Arch::AArch64));
            emit_decref_any(&mut emitter);
            let assembly = emitter.output();
            assert!(
                assembly.contains("    cmp x13, #6\n"),
                "ARM64 collector range excludes throwables on {platform:?}:\n{assembly}"
            );
            assert!(
                assembly.contains("    cmp x11, #6\n    b.eq __rt_decref_any_object\n"),
                "ARM64 dispatcher excludes throwables on {platform:?}:\n{assembly}"
            );
        }

        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_decref_any(&mut emitter);
        let assembly = emitter.output();
        assert!(
            assembly.contains("    cmp r10, 6\n    je __rt_decref_any_object\n"),
            "x86_64 dispatcher excludes throwables:\n{assembly}"
        );
    }

    /// String dispatch releases one owner and reaches heap reclamation only at refcount zero.
    #[test]
    fn test_decref_any_string_dispatch_is_refcount_aware_on_all_supported_architectures() {
        for platform in [Platform::MacOS, Platform::Linux] {
            let mut emitter = Emitter::new(Target::new(platform, Arch::AArch64));
            emit_decref_any(&mut emitter);
            let assembly = emitter.output();
            assert!(
                assembly.contains(
                    "__rt_decref_any_string:\n    ldr w11, [x0, #-12]\n    subs w11, w11, #1\n    str w11, [x0, #-12]\n    b.ne __rt_decref_any_done\n    b __rt_heap_free\n"
                ),
                "ARM64 string release bypasses its refcount on {platform:?}:\n{assembly}"
            );
        }

        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_decref_any(&mut emitter);
        let assembly = emitter.output();
        assert!(
            assembly.contains(
                "__rt_decref_any_string:\n    mov r10d, DWORD PTR [rax - 12]\n    sub r10d, 1\n    mov DWORD PTR [rax - 12], r10d\n    jne __rt_decref_any_done\n    jmp __rt_heap_free\n"
            ),
            "x86_64 string release bypasses its refcount:\n{assembly}"
        );
    }
}
