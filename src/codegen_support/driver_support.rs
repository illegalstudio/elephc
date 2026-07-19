//! Purpose:
//! Provides driver-level helpers that bridge generated user code with runtime conventions.
//! Emits runtime assembly fragments and target-aware stderr writes.
//!
//! Called from:
//! - `crate::runtime_cache` through runtime generation re-exports.
//! - EIR codegen frame helpers that report heap-debug counters.
//!
//! Key details:
//! - Runtime feature selection must stay deterministic for runtime caching.

use super::abi;
use super::emit::Emitter;
use super::platform::{Arch, Target};
use super::runtime;
use super::runtime_features::RuntimeFeatures;

/// Emits a write syscall for a labeled literal string to stderr, using the given
/// label (from the data section) and its byte length. Handles target-specific
/// register conventions for the write syscall arguments.
pub(crate) fn emit_write_literal_stderr(emitter: &mut Emitter, label: &str, len: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            crate::codegen_support::abi::emit_symbol_address(emitter, "x1", label); // load the page address of the stderr literal on AArch64
            emitter.instruction(&format!("mov x2, #{}", len));                  // materialize the stderr literal byte length in the AArch64 write-length register
            emitter.instruction("mov x0, #2");                                  // target the stderr file descriptor on AArch64
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", label);
            emitter.instruction(&format!("mov edx, {}", len));                  // materialize the stderr literal byte length in the x86_64 write-length register
            emitter.instruction("mov edi, 2");                                  // target the stderr file descriptor on x86_64
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // write the requested literal bytes to stderr on x86_64
        }
    }
}

/// Emits a write syscall for the current string in result registers to stderr.
/// Loads pointer/length from the appropriate ABI registers for the target.
pub(crate) fn emit_write_current_string_stderr(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // target the stderr file descriptor on AArch64
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            emitter.instruction(&format!("mov rsi, {}", ptr_reg));              // move the current string pointer into the x86_64 write buffer register
            emitter.instruction(&format!("mov rdx, {}", len_reg));              // move the current string length into the x86_64 write length register
            emitter.instruction("mov edi, 2");                                  // target the stderr file descriptor on x86_64
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // write the current string payload to stderr on x86_64
        }
    }
}

/// Assembles the complete runtime assembly string for a given heap size and target.
#[allow(dead_code)]
pub fn generate_runtime(heap_size: usize, target: Target) -> String {
    generate_runtime_with_features(heap_size, target, RuntimeFeatures::all())
}

/// Assembles runtime assembly for the requested optional helper families.
pub fn generate_runtime_with_features(
    heap_size: usize,
    target: Target,
    features: RuntimeFeatures,
) -> String {
    generate_runtime_with_features_pic(heap_size, target, features, false)
}

/// Same as `generate_runtime_with_features` but emits position-independent
/// data references when `pic` is true. Required for the runtime object linked
/// into a `--emit cdylib` artifact, where cross-section symbol references must
/// resolve through the GOT instead of via direct PC-relative relocations.
pub fn generate_runtime_with_features_pic(
    heap_size: usize,
    target: Target,
    features: RuntimeFeatures,
    pic: bool,
) -> String {
    // macOS executables strip unreachable runtime helpers per-symbol: internal
    // labels are renamed to assembler-local (`L`-prefixed) labels and a
    // `.subsections_via_symbols` footer lets the linker's `-dead_strip` drop
    // whole unreferenced `__rt_*` helpers as single atoms. cdylibs (pic) never
    // strip, and Linux uses per-section `--gc-sections` instead, so both keep
    // the monolithic object.
    let dead_strip = !pic && target.platform == crate::codegen_support::platform::Platform::MacOS;
    let mut emitter = if pic {
        Emitter::new_pic(target)
    } else {
        Emitter::new(target)
    };
    emitter.dead_strip = dead_strip;
    emitter.emit_text_prelude();
    runtime::emit_runtime(&mut emitter, features);
    // Rename internal labels to `L`-locals in the runtime text only; the `.data`
    // below never references them, so it is appended unchanged.
    let internal_labels = emitter.take_internal_labels();
    let mut output = if dead_strip {
        crate::codegen_support::emit::localize_internal_labels(&emitter.output(), &internal_labels)
    } else {
        emitter.output()
    };
    output.push('\n');
    output.push_str(&runtime::emit_runtime_data_fixed(heap_size, target));
    // The PIC runtime object only ever links into an ELF cdylib, where every
    // runtime global must bind locally: hidden visibility prevents dynamic
    // preemption (two loaded elephc modules aliasing one runtime state) and
    // keeps the .so's dynamic symbol table down to the public ABI.
    if pic && target.platform == crate::codegen_support::platform::Platform::Linux {
        output = crate::codegen_support::visibility::append_hidden_directives(
            &output,
            &std::collections::HashSet::new(),
        );
    }
    // Footer that enables atom subdivision for `-dead_strip`. Emitted last so it
    // applies to the whole runtime object (text helpers and the `.data` table).
    if dead_strip {
        output.push_str(".subsections_via_symbols\n");
    }
    output
}
