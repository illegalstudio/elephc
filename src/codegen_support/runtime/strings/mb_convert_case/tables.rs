//! Purpose:
//! Emits gated Unicode case-mapping tables for `__rt_mb_convert_case`.
//!
//! Called from:
//! - `super::emit_mb_convert_case()`.
//!
//! Key details:
//! - Tables live in `.rodata` so Linux `--gc-sections` can drop them with the helper.
//! - Simple tables are `(from, to)` pairs; full tables are `(from, len, c0, c1, c2)`.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Platform;
use crate::mb_case::{case_tables, FullMap};

/// Emits every case-mapping table used by the convert-case runtime helper.
pub(super) fn emit_case_tables(emitter: &mut Emitter) {
    let tables = case_tables();
    match emitter.platform {
        Platform::Linux => emitter.raw(".section .rodata,\"a\",@progbits"),
        Platform::MacOS => emitter.raw(".section __TEXT,__const"),
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    }
    emit_range_table(emitter, "_mb_cc_ignorable", tables.ignorable);
    emit_range_table(emitter, "_mb_cc_cased", &tables.cased);
    emit_pair_table(emitter, "_mb_cc_simple_upper", &tables.simple_upper);
    emit_pair_table(emitter, "_mb_cc_simple_lower", &tables.simple_lower);
    emit_pair_table(emitter, "_mb_cc_simple_title", &tables.simple_title);
    emit_full_table(emitter, "_mb_cc_full_upper", &tables.full_upper);
    emit_full_table(emitter, "_mb_cc_full_lower", &tables.full_lower);
    emit_full_table(emitter, "_mb_cc_full_title", &tables.full_title);
    emit_full_table(emitter, "_mb_cc_full_fold", &tables.full_fold);
}

/// Emits a count-prefixed inclusive range table.
fn emit_range_table(emitter: &mut Emitter, name: &str, ranges: &[(u32, u32)]) {
    emit_object_label(emitter, name);
    emitter.raw(&format!("    .long {}", ranges.len()));
    for &(lo, hi) in ranges {
        emitter.raw(&format!("    .long {}, {}", lo, hi));
    }
}

/// Emits a count-prefixed 1:1 mapping table.
fn emit_pair_table(emitter: &mut Emitter, name: &str, pairs: &[(u32, u32)]) {
    emit_object_label(emitter, name);
    emitter.raw(&format!("    .long {}", pairs.len()));
    for &(from, to) in pairs {
        emitter.raw(&format!("    .long {}, {}", from, to));
    }
}

/// Emits a count-prefixed 1:N expansion table.
fn emit_full_table(emitter: &mut Emitter, name: &str, maps: &[FullMap]) {
    emit_object_label(emitter, name);
    emitter.raw(&format!("    .long {}", maps.len()));
    for map in maps {
        emitter.raw(&format!(
            "    .long {}, {}, {}, {}, {}",
            map.from, map.len, map.out[0], map.out[1], map.out[2]
        ));
    }
}

/// Emits a 4-byte-aligned object label that is visible to `emit_symbol_address`.
fn emit_object_label(emitter: &mut Emitter, name: &str) {
    emitter.raw("    .p2align 2");
    match emitter.platform {
        Platform::Linux => {
            emitter.raw(&format!(".globl {}", name));
            emitter.raw(&format!(".type {}, @object", name));
            emitter.raw(&format!("{}:", name));
        }
        Platform::MacOS => {
            emitter.raw(&format!(".globl {}", name));
            emitter.raw(&format!("{}:", name));
        }
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    }
}
