//! Purpose:
//! Lint over the emitted linux-x86_64 runtime: a call to a helper whose x86_64 entry reads its
//! pointer from `rax` must not be fed through a SysV argument register alone.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `__rt_incref`, the `__rt_decref_*` family, `__rt_heap_kind`, `__rt_mixed_unbox` and friends
//!   take their pointer in `rax` (the aarch64 `x0`), unlike the SysV helpers (`rdi`). A caller that
//!   only loads `rdi` retains or releases whatever `rax` happened to hold: nothing is retained
//!   (use-after-free later) or an unrelated block is released.
//! - The scan walks back from each call to the previous label/jump/call; a call whose window loads
//!   an argument register but never writes `rax` (other than `mov rdi, rax`) is reported.

use crate::codegen_support::platform::Target;
use crate::codegen_support::runtime_features::RuntimeFeatures;

/// Runtime helpers whose linux-x86_64 entry reads its pointer from `rax`.
const RAX_INPUT_HELPERS: &[&str] = &[
    "__rt_incref",
    "__rt_decref_any",
    "__rt_decref_array",
    "__rt_decref_hash",
    "__rt_decref_mixed",
    "__rt_decref_object",
    "__rt_heap_kind",
    "__rt_heap_free",
    "__rt_mixed_unbox",
    "__rt_mixed_deref",
    "__rt_mixed_cast_string",
    "__rt_mixed_cast_int",
    "__rt_mixed_cast_float",
    "__rt_mixed_cast_bool",
    "__rt_object_free_deep",
];

/// Returns the instruction text of one assembly line, without its trailing comment.
fn instruction_of(line: &str) -> &str {
    line.split("//").next().unwrap().split(" #").next().unwrap().trim()
}

/// Returns whether `instruction` writes `rax` (or `eax`/`al`) as its destination.
fn writes_rax(instruction: &str) -> bool {
    let Some((mnemonic, operands)) = instruction.split_once(' ') else {
        return false;
    };
    if mnemonic == "pop" {
        return operands.trim() == "rax";
    }
    let destination = operands.split(',').next().unwrap().trim();
    matches!(destination, "rax" | "eax" | "ax" | "al")
        && !matches!(mnemonic, "cmp" | "test" | "push")
}

/// Returns whether `instruction` loads a SysV argument register from anything but `rax`.
fn loads_argument_register(instruction: &str) -> bool {
    let Some(("mov" | "lea" | "pop" | "xor", operands)) = instruction.split_once(' ') else {
        return false;
    };
    let mut parts = operands.split(',').map(str::trim);
    let destination = parts.next().unwrap_or("");
    let source = parts.next().unwrap_or("");
    matches!(destination, "rdi" | "rsi" | "edi" | "esi") && source != "rax"
}

/// Lists every `call` to an `rax`-input helper whose straight-line window loads `rdi`/`rsi`
/// without ever writing `rax`.
fn violations(asm: &str) -> Vec<String> {
    let lines: Vec<&str> = asm.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let call = instruction_of(line);
        let Some(callee) = call.strip_prefix("call ") else { continue };
        if !RAX_INPUT_HELPERS.contains(&callee) {
            continue;
        }
        let mut loaded_argument = false;
        let mut wrote_rax = false;
        for earlier in lines[..index].iter().rev().take(14) {
            let text = instruction_of(earlier);
            if text.is_empty() {
                continue;
            }
            if text.ends_with(':') || text.starts_with("jmp ") || text.starts_with("call ")
                || text == "ret" || text.starts_with('.')
            {
                break;
            }
            if writes_rax(text) || text == "mov rdi, rax" {
                wrote_rax = true;
                break;
            }
            loaded_argument |= loads_argument_register(text);
        }
        if loaded_argument && !wrote_rax {
            found.push(format!("line {}: {} fed through rdi/rsi without rax", index + 1, call));
        }
    }
    found
}

/// Every `rax`-input helper call in the x86_64 runtime gets its pointer in `rax`.
#[test]
fn rax_input_helpers_are_never_fed_through_rdi_alone_in_the_x86_64_runtime() {
    for features in [RuntimeFeatures::none(), RuntimeFeatures::all()] {
        let asm = crate::codegen_support::generate_runtime_with_features(
            8_388_608,
            Target::parse("linux-x86_64").expect("linux-x86_64 is a supported target"),
            features,
        );
        let found = violations(&asm);
        assert!(found.is_empty(), "x86_64 runtime register-convention mismatches:\n{}", found.join("\n"));
    }
}
