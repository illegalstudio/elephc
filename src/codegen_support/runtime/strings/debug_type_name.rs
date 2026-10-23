//! Purpose:
//! Emits `__rt_debug_type_name`, which maps an unboxed runtime value to the type name PHP's
//! `get_debug_type()` reports.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//! - `crate::codegen::lower_inst::builtins::lower_get_debug_type`, for boxed, object and
//!   resource operands (statically known scalar types fold to a literal at the call site).
//!
//! Key details:
//! - The input is exactly what `__rt_mixed_unbox` leaves behind (tag, low payload, high
//!   payload), so a boxed operand is unboxed and handed over without any shuffling.
//! - Scalars use php-src's zend type names (`int`, `float`, `bool`, `null`, ...), never
//!   `gettype()`'s historical `integer` / `double` / `boolean` / `NULL`.
//! - Objects read `_class_debug_name_entries`, the twin of `_class_name_entries` that spells an
//!   anonymous class the way php-src does (`class@anonymous`, `Parent@anonymous`,
//!   `Interface@anonymous`) instead of elephc's synthetic `class@anonymous#N`. The reserved
//!   incomplete-class id (`-2`) answers `__PHP_Incomplete_Class`.
//! - Resources reuse the predicates of `__rt_resource_type_name`: the implicit default context
//!   payload, the sign bit an explicit close stamps, and subtype 9 for a stream filter.
//! - ALL NAMES ARE PERSISTENT `.data` LITERALS, so the result is borrowed static storage, the
//!   same contract as `gettype()` and `get_class()`.
//! - IT IS A LEAF. No nested `bl`/`call`, so it ends with `ret` on both targets.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::resource_inventory::DEFAULT_CONTEXT_PAYLOAD;

/// The `.data` literals this helper answers with, as `(symbol, text)`.
///
/// `crate::codegen_support::runtime::data::fixed` emits them from this table, so the lengths
/// the helper loads can never drift from the bytes it points at.
pub const DEBUG_TYPE_NAMES: [(&str, &str); 11] = [
    ("_debug_type_null", "null"),
    ("_debug_type_bool", "bool"),
    ("_debug_type_int", "int"),
    ("_debug_type_float", "float"),
    ("_debug_type_string", "string"),
    ("_debug_type_array", "array"),
    ("_debug_type_closure", "Closure"),
    ("_debug_type_resource_stream", "resource (stream)"),
    ("_debug_type_resource_closed", "resource (closed)"),
    ("_debug_type_resource_filter", "resource (stream filter)"),
    ("_debug_type_resource_context", "resource (stream-context)"),
];

/// Byte length of `__PHP_Incomplete_Class`, the literal `_incomplete_class_name` holds.
pub const INCOMPLETE_CLASS_NAME_LEN: i64 = 22;

/// Returns the byte length of one `DEBUG_TYPE_NAMES` literal.
fn debug_type_name_len(symbol: &str) -> i64 {
    DEBUG_TYPE_NAMES
        .iter()
        .find(|(name, _)| *name == symbol)
        .map(|(_, text)| text.len() as i64)
        .unwrap_or_else(|| panic!("unknown get_debug_type literal {symbol}"))
}

/// The runtime tags this helper dispatches on, with the literal each one answers. Tags 6
/// (object) and 9 (resource) have their own arms; every other tag, null included, is `null`.
const SCALAR_TAG_ARMS: [(u8, &str); 7] = [
    (0, "_debug_type_int"),
    (1, "_debug_type_string"),
    (2, "_debug_type_float"),
    (3, "_debug_type_bool"),
    (4, "_debug_type_array"),
    (5, "_debug_type_array"),
    (10, "_debug_type_closure"),
];

/// Resolves an unboxed runtime value to its `get_debug_type()` name.
///
/// # Inputs
/// - `x0` / `rax`: runtime value tag.
/// - `x1` / `rdi`: low payload word (the object pointer, or the resource payload).
/// - `x2` / `rdx`: high payload word (the resource subtype, where 9 is a stream filter).
///
/// # Outputs
/// - `x1` / `rax`: pointer to the type-name bytes (the target's string-result pointer register).
/// - `x2` / `rdx`: type-name byte length (the target's string-result length register).
///
/// # ABI details
/// - Leaf helper: no nested `bl`/`call`, so it ends with `ret` on both targets.
/// - Clobbers the string-result register pair and the target scratch registers.
pub fn emit_debug_type_name(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_debug_type_name_aarch64(emitter),
        Arch::X86_64 => emit_debug_type_name_x86_64(emitter),
    }
}

/// Loads one literal into the AArch64 string-result pair and returns.
fn emit_aarch64_literal_return(emitter: &mut Emitter, label: &str, symbol: &str) {
    emitter.label(label);
    abi::emit_symbol_address(emitter, "x1", symbol);
    abi::emit_load_int_immediate(emitter, "x2", debug_type_name_len(symbol));
    emitter.instruction("ret");                                                 // return the static get_debug_type() name
}

/// Loads one literal into the x86_64 string-result pair and returns.
fn emit_x86_64_literal_return(emitter: &mut Emitter, label: &str, symbol: &str) {
    emitter.label(label);
    abi::emit_symbol_address(emitter, "rax", symbol);
    abi::emit_load_int_immediate(emitter, "rdx", debug_type_name_len(symbol));
    emitter.instruction("ret");                                                 // return the static get_debug_type() name
}

/// AArch64 body of `emit_debug_type_name`.
fn emit_debug_type_name_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: debug_type_name (get_debug_type() name of an unboxed value) ---");
    emitter.label_global("__rt_debug_type_name");

    // -- dispatch on the runtime tag --
    for (tag, symbol) in SCALAR_TAG_ARMS {
        emitter.instruction(&format!("cmp x0, #{tag}"));                        // test the runtime tag against this scalar arm
        emitter.instruction(&format!("b.eq __rt_debug_type_name{symbol}"));     // answer the scalar arm's static type name
    }
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 is an object payload
    emitter.instruction("b.eq __rt_debug_type_name_object");                    // objects answer their class debug name
    emitter.instruction("cmp x0, #9");                                          // runtime tag 9 is a resource payload
    emitter.instruction("b.eq __rt_debug_type_name_resource");                  // resources answer their type in parentheses
    emitter.instruction("b __rt_debug_type_name_debug_type_null");              // null and every other tag are PHP null

    // -- objects: look the class id up in the debug-name table --
    emitter.label("__rt_debug_type_name_object");
    emitter.instruction("cbz x1, __rt_debug_type_name_debug_type_null");        // a null object pointer is PHP null
    emitter.instruction("ldr x9, [x1]");                                        // load the object's concrete runtime class id
    emitter.instruction("cmn x9, #2");                                          // reserved id -2 denotes an incomplete unserialized object
    emitter.instruction("b.eq __rt_debug_type_name_incomplete");                // expose PHP's incomplete-class name
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the dense class-name table length
    emitter.instruction("cmp x9, x10");                                         // validate the class id before indexing the debug-name table
    emitter.instruction("b.hs __rt_debug_type_name_missing");                   // unknown class ids answer an empty name
    abi::emit_symbol_address(emitter, "x10", "_class_debug_name_entries");
    emitter.instruction("lsl x9, x9, #4");                                      // scale the class id by the 16-byte debug-name row size
    emitter.instruction("add x10, x10, x9");                                    // address the class debug-name row
    emitter.instruction("ldr x1, [x10]");                                       // load the class debug-name pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // load the class debug-name length
    emitter.instruction("ret");                                                 // return the class debug name

    // -- incomplete unserialized objects --
    emitter.label("__rt_debug_type_name_incomplete");
    abi::emit_symbol_address(emitter, "x1", "_incomplete_class_name");
    abi::emit_load_int_immediate(emitter, "x2", INCOMPLETE_CLASS_NAME_LEN);
    emitter.instruction("ret");                                                 // return the incomplete-class name

    // -- unknown class ids answer an empty name --
    emitter.label("__rt_debug_type_name_missing");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");
    abi::emit_load_int_immediate(emitter, "x2", 0);
    emitter.instruction("ret");                                                 // return the empty name for an unknown class id

    // -- resources: default context, closed, stream filter, or stream --
    emitter.label("__rt_debug_type_name_resource");
    abi::emit_load_int_immediate(emitter, "x9", DEFAULT_CONTEXT_PAYLOAD);
    emitter.instruction("cmp x1, x9");                                          // recognize PHP's implicit default stream context
    emitter.instruction("b.eq __rt_debug_type_name_debug_type_resource_context"); // the reserved context has its own type label
    emitter.instruction("tbnz x1, #63, __rt_debug_type_name_debug_type_resource_closed"); // a negative payload is the -id sentinel an explicit close stamped
    emitter.instruction("cmp x2, #9");                                          // stream-filter resources carry subtype 9
    emitter.instruction("b.eq __rt_debug_type_name_debug_type_resource_filter"); // preserve PHP's distinct stream filter resource name
    emitter.instruction("b __rt_debug_type_name_debug_type_resource_stream");   // every other open resource is a stream

    // -- literal-returning arms shared by every branch above --
    let mut arms: Vec<&str> = SCALAR_TAG_ARMS.iter().map(|(_, symbol)| *symbol).collect();
    arms.dedup();
    arms.extend([
        "_debug_type_null",
        "_debug_type_resource_stream",
        "_debug_type_resource_closed",
        "_debug_type_resource_filter",
        "_debug_type_resource_context",
    ]);
    for symbol in arms {
        emit_aarch64_literal_return(emitter, &format!("__rt_debug_type_name{symbol}"), symbol);
    }
}

/// x86_64 body of `emit_debug_type_name`.
///
/// `rdx` is both the subtype input and the string-length output, so every resource test runs
/// before an arm loads its literal.
fn emit_debug_type_name_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: debug_type_name (get_debug_type() name of an unboxed value) ---");
    emitter.label_global("__rt_debug_type_name");

    // -- dispatch on the runtime tag --
    for (tag, symbol) in SCALAR_TAG_ARMS {
        emitter.instruction(&format!("cmp rax, {tag}"));                        // test the runtime tag against this scalar arm
        emitter.instruction(&format!("je __rt_debug_type_name{symbol}_x86"));   // answer the scalar arm's static type name
    }
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 is an object payload
    emitter.instruction("je __rt_debug_type_name_object_x86");                  // objects answer their class debug name
    emitter.instruction("cmp rax, 9");                                          // runtime tag 9 is a resource payload
    emitter.instruction("je __rt_debug_type_name_resource_x86");                // resources answer their type in parentheses
    emitter.instruction("jmp __rt_debug_type_name_debug_type_null_x86");        // null and every other tag are PHP null

    // -- objects: look the class id up in the debug-name table --
    emitter.label("__rt_debug_type_name_object_x86");
    emitter.instruction("test rdi, rdi");                                       // test whether the object pointer is null
    emitter.instruction("je __rt_debug_type_name_debug_type_null_x86");         // a null object pointer is PHP null
    emitter.instruction("mov r8, QWORD PTR [rdi]");                             // load the object's concrete runtime class id
    emitter.instruction("cmp r8, -2");                                          // reserved id -2 denotes an incomplete unserialized object
    emitter.instruction("je __rt_debug_type_name_incomplete_x86");              // expose PHP's incomplete-class name
    emitter.instruction("mov r9, QWORD PTR [rip + _class_name_count]");         // load the dense class-name table length
    emitter.instruction("cmp r8, r9");                                          // validate the class id before indexing the debug-name table
    emitter.instruction("jae __rt_debug_type_name_missing_x86");                // unknown class ids answer an empty name
    emitter.instruction("lea r10, [rip + _class_debug_name_entries]");          // materialize the class debug-name table base
    emitter.instruction("shl r8, 4");                                           // scale the class id by the 16-byte debug-name row size
    emitter.instruction("mov rax, QWORD PTR [r10 + r8]");                       // load the class debug-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + r8 + 8]");                   // load the class debug-name length
    emitter.instruction("ret");                                                 // return the class debug name

    // -- incomplete unserialized objects --
    emitter.label("__rt_debug_type_name_incomplete_x86");
    abi::emit_symbol_address(emitter, "rax", "_incomplete_class_name");
    abi::emit_load_int_immediate(emitter, "rdx", INCOMPLETE_CLASS_NAME_LEN);
    emitter.instruction("ret");                                                 // return the incomplete-class name

    // -- unknown class ids answer an empty name --
    emitter.label("__rt_debug_type_name_missing_x86");
    emitter.instruction("lea rax, [rip + _class_name_missing]");                // return the shared empty class-name pointer
    emitter.instruction("xor edx, edx");                                        // return zero bytes for an unknown class id
    emitter.instruction("ret");                                                 // return the empty name for an unknown class id

    // -- resources: default context, closed, stream filter, or stream --
    emitter.label("__rt_debug_type_name_resource_x86");
    abi::emit_load_int_immediate(emitter, "r10", DEFAULT_CONTEXT_PAYLOAD);
    emitter.instruction("cmp rdi, r10");                                        // recognize PHP's implicit default stream context
    emitter.instruction("je __rt_debug_type_name_debug_type_resource_context_x86"); // the reserved context has its own type label
    emitter.instruction("test rdi, rdi");                                       // inspect the payload sign
    emitter.instruction("js __rt_debug_type_name_debug_type_resource_closed_x86"); // a negative payload is the -id sentinel an explicit close stamped
    emitter.instruction("cmp rdx, 9");                                          // stream-filter resources carry subtype 9
    emitter.instruction("je __rt_debug_type_name_debug_type_resource_filter_x86"); // preserve PHP's distinct stream filter resource name
    emitter.instruction("jmp __rt_debug_type_name_debug_type_resource_stream_x86"); // every other open resource is a stream

    // -- literal-returning arms shared by every branch above --
    let mut arms: Vec<&str> = SCALAR_TAG_ARMS.iter().map(|(_, symbol)| *symbol).collect();
    arms.dedup();
    arms.extend([
        "_debug_type_null",
        "_debug_type_resource_stream",
        "_debug_type_resource_closed",
        "_debug_type_resource_filter",
        "_debug_type_resource_context",
    ]);
    for symbol in arms {
        emit_x86_64_literal_return(emitter, &format!("__rt_debug_type_name{symbol}_x86"), symbol);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Every literal the helper can answer is declared exactly once.
    #[test]
    fn every_arm_names_a_declared_literal() {
        for (_, symbol) in SCALAR_TAG_ARMS {
            assert!(debug_type_name_len(symbol) > 0);
        }
        let mut symbols: Vec<&str> = DEBUG_TYPE_NAMES.iter().map(|(symbol, _)| *symbol).collect();
        symbols.sort_unstable();
        symbols.dedup();
        assert_eq!(symbols.len(), DEBUG_TYPE_NAMES.len());
    }

    /// The helper stays a leaf on both targets and reads the debug-name table, never the
    /// plain class-name rows.
    #[test]
    fn the_helper_is_a_leaf_that_reads_the_debug_name_table() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_debug_type_name(&mut emitter);
            let asm = emitter.output();
            assert!(!asm.contains("    bl ") && !asm.contains("    call "), "{asm}");
            assert!(asm.contains("_class_debug_name_entries"), "{asm}");
            assert!(!asm.contains("_class_name_entries"), "{asm}");
        }
    }
}
