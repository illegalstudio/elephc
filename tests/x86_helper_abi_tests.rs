//! Purpose:
//! Guards the x86_64 runtime against handing a RAX-reading helper its argument in RDI.
//!
//! Called from:
//! - `cargo test --test x86_helper_abi_tests` through Rust's test harness.
//!
//! Key details:
//! - Four single-argument runtime helpers read their argument from RAX on x86_64 rather than from
//!   RDI. A call site that sets RDI instead passes whatever the previous call left in RAX, and
//!   nothing reports it: the wrong pointer is freed, or a block of the wrong size is allocated.
//! - Every instance of this cost a day of CI: `__rt_stream_pending_put` allocated a block whose
//!   size was a stale return value (so the holding area was never populated on x86 and its whole
//!   test surface passed vacuously), `__rt_stream_pending_clear` and `__rt_stream_pending_consume`
//!   freed the STREAM STATE instead of that block, `__rt_filter_absorb_params` unboxed a node
//!   pointer instead of `$params`, and `__rt_user_wrapper_opendir` leaked its instance.
//! - The check reads the EMITTED assembly, because that is where the mistake is visible: the
//!   emitters are per-arch and a reviewer comparing them by eye is what missed these.

use std::collections::BTreeSet;

/// The single-argument helpers whose x86_64 entry reads RAX.
///
/// Asserted below rather than assumed: each one's own first instructions are checked to be reading
/// RAX, so a helper that changes its convention breaks this list rather than silently invalidating
/// the audit that rests on it.
const RAX_ARGUMENT_HELPERS: [&str; 4] = [
    "__rt_heap_free",
    "__rt_heap_alloc",
    "__rt_decref_any",
    "__rt_mixed_unbox",
];

/// Returns the x86_64 runtime assembly, as the linux-x86_64 target emits it.
fn x86_runtime() -> String {
    let target = elephc::codegen_support::platform::Target::parse("linux-x86_64")
        .expect("linux-x86_64 is a supported target");
    elephc::codegen::generate_runtime_with_features(
        8_388_608,
        target,
        elephc::codegen::RuntimeFeatures::none(),
    )
}

/// Returns true when this instruction writes the named register or its 32-bit half.
fn writes(instruction: &str, wide: &str, narrow: &str) -> bool {
    let Some((mnemonic, operands)) = instruction.split_once(' ') else {
        return false;
    };
    if !matches!(mnemonic, "mov" | "lea" | "add" | "sub" | "xor" | "movzx" | "cdqe") {
        return false;
    }
    let destination = operands.split(',').next().unwrap_or("").trim();
    destination == wide || destination == narrow
}

/// Each helper reads RAX, and this is where that is established rather than assumed.
#[test]
fn the_audited_helpers_read_their_argument_from_rax() {
    let asm = x86_runtime();
    let lines: Vec<&str> = asm.lines().map(str::trim).collect();
    for helper in RAX_ARGUMENT_HELPERS {
        let entry = lines
            .iter()
            .position(|line| *line == format!("{helper}:"))
            .unwrap_or_else(|| panic!("{helper} is not defined in the x86_64 runtime"));
        let first = lines[entry + 1..]
            .iter()
            .find(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('.'))
            .unwrap_or_else(|| panic!("{helper} has an empty body"));
        assert!(
            first.contains("rax"),
            "{helper}'s first instruction is `{first}`, which does not read rax — either the \
             convention changed or this list is wrong, and both make the audit below meaningless"
        );
    }
}

/// No call site sets RDI for a helper that reads RAX.
///
/// The window walks back to the previous label, blank line or CALL — a call redefines RAX, so
/// nothing before it is this call's argument setup. A site that sets NEITHER register is left
/// alone: its argument is the previous call's result, already in RAX, which is the ordinary shape
/// here and the reason the window must stop there rather than read through it.
#[test]
fn no_x86_call_site_hands_a_rax_helper_its_argument_in_rdi() {
    let asm = x86_runtime();
    let lines: Vec<&str> = asm.lines().map(str::trim).collect();
    let mut label = "<file scope>".to_string();
    let mut offenders: BTreeSet<String> = BTreeSet::new();

    for (index, line) in lines.iter().enumerate() {
        if line.ends_with(':') && !line.starts_with('.') {
            label = line.trim_end_matches(':').to_string();
        }
        let Some(callee) = line.strip_prefix("call ") else {
            continue;
        };
        if !RAX_ARGUMENT_HELPERS.contains(&callee) {
            continue;
        }
        let mut sets_rax = false;
        let mut sets_rdi = false;
        for previous in lines[..index].iter().rev() {
            if previous.is_empty()
                || previous.ends_with(':')
                || previous.starts_with('.')
                || previous.starts_with("call ")
            {
                break;
            }
            sets_rax |= writes(previous, "rax", "eax");
            sets_rdi |= writes(previous, "rdi", "edi");
        }
        if sets_rdi && !sets_rax {
            offenders.insert(format!("{callee} in {label}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "these x86_64 call sites set RDI for a helper that reads RAX, so each passes whatever the \
         previous call left behind:\n  {}",
        offenders.into_iter().collect::<Vec<_>>().join("\n  ")
    );
}
