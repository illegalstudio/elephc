//! Purpose:
//! Injects DWARF debug information into generated user assembly when
//! `--debug-info` is requested, so standard debuggers (lldb, gdb) and profilers
//! map compiled code back to PHP source without custom tooling.
//!
//! Called from:
//! - `crate::pipeline::compile()` after user assembly generation, before the
//!   assembly is written and assembled.
//!
//! Key details:
//! - Reuses the source-map markers codegen already emits: each `@src` marker is
//!   followed by a matching `.loc 1 <L> <C>` (the assembler builds `.debug_line`
//!   from them), and each `@fn`/`@endfn` pair becomes a `DW_TAG_subprogram`.
//! - A line table alone is invisible to DWARF consumers: they discover it
//!   through a compile unit's `DW_AT_stmt_list`, and the macOS linker only
//!   emits the debug map (`N_OSO`) that `dsymutil`/lldb follow when the object
//!   declares subprograms AND the compile unit carries `DW_AT_comp_dir` (the
//!   linker needs it to build the `N_SO` stab; without it the debug map is
//!   silently dropped). All of it is hand-encoded here, the same way
//!   `-g`-enabled assemblers do it.
//! - Markers only appear in the text section, which keeps `.loc` legal.

use std::fmt::Write as _;

use crate::codegen::platform::Platform;

/// One `@fn`..`@endfn` region collected during injection, used to emit its
/// `DW_TAG_subprogram`: the PHP-level name, the entry symbol (`DW_AT_low_pc`),
/// and the index of the injected `Lelephc_fend_<idx>` end label.
struct SubprogramInfo {
    name: String,
    symbol: String,
    end_label: usize,
}

/// Returns `asm` extended with DWARF debug information for `source_path`:
/// a `.file`/`.loc` line table derived from the `@src` source-map markers, and
/// a compile unit with one subprogram per `@fn` region.
pub fn inject_line_directives(asm: &str, source_path: &str, platform: Platform) -> String {
    let mut out = String::with_capacity(asm.len() + asm.len() / 4);
    out.push_str(&format!(
        ".file 1 \"{}\"\n",
        source_path.replace('"', "\\\"")
    ));

    let mut subprograms: Vec<SubprogramInfo> = Vec::new();
    let mut open: Option<(String, String)> = None;
    for line in asm.lines() {
        out.push_str(line);
        out.push('\n');
        if let Some(marker) = line.split("@src ").nth(1) {
            if let (Some(php_line), Some(php_col)) =
                (marker_number(marker, "line"), marker_number(marker, "col"))
            {
                let _ = writeln!(out, "    .loc 1 {} {}", php_line, php_col);
            }
            continue;
        }
        if let Some(marker) = line.split("@fn ").nth(1) {
            if let (Some(name), Some(symbol)) =
                (marker_value(marker, "name"), marker_value(marker, "symbol"))
            {
                open = Some((name.to_string(), symbol.to_string()));
            }
            continue;
        }
        if line.contains("@endfn") {
            if let Some((name, symbol)) = open.take() {
                let end_label = subprograms.len();
                let _ = writeln!(out, "Lelephc_fend_{}:", end_label);
                subprograms.push(SubprogramInfo {
                    name,
                    symbol,
                    end_label,
                });
            }
        }
    }

    out.push_str(&debug_info_sections(source_path, platform, &subprograms));
    out
}

/// Renders the hand-encoded `.debug_abbrev` and `.debug_info` sections: one
/// DWARF32 version-4 compile unit whose `DW_AT_stmt_list` points at the line
/// table built from the injected `.loc` directives, containing one
/// `DW_TAG_subprogram` per `@fn` region (entry symbol as `DW_AT_low_pc`, the
/// injected end label as `DW_AT_high_pc`).
fn debug_info_sections(
    source_path: &str,
    platform: Platform,
    subprograms: &[SubprogramInfo],
) -> String {
    let (abbrev_section, info_section) = match platform {
        Platform::MacOS => (
            ".section __DWARF,__debug_abbrev,regular,debug",
            ".section __DWARF,__debug_info,regular,debug",
        ),
        Platform::Linux => (
            ".section .debug_abbrev,\"\",@progbits",
            ".section .debug_info,\"\",@progbits",
        ),
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };
    let name = escape_asm_string(source_path);
    let comp_dir = escape_asm_string(
        &std::env::current_dir()
            .map(|dir| dir.display().to_string())
            .unwrap_or_default(),
    );

    // Abbrev 1: DW_TAG_compile_unit (0x11), has children;
    //   DW_AT_producer(0x25)/string(0x08), DW_AT_language(0x13)/data2(0x05),
    //   DW_AT_name(0x03)/string(0x08), DW_AT_comp_dir(0x1b)/string(0x08),
    //   DW_AT_stmt_list(0x10)/sec_offset(0x17).
    // Abbrev 2: DW_TAG_subprogram (0x2e), no children;
    //   DW_AT_name(0x03)/string(0x08), DW_AT_low_pc(0x11)/addr(0x01),
    //   DW_AT_high_pc(0x12)/data8(0x07).
    let mut out = format!(
        "\
{abbrev_section}
    .byte 1
    .byte 0x11
    .byte 1
    .byte 0x25, 0x08
    .byte 0x13, 0x05
    .byte 0x03, 0x08
    .byte 0x1b, 0x08
    .byte 0x10, 0x17
    .byte 0, 0
    .byte 2
    .byte 0x2e
    .byte 0
    .byte 0x03, 0x08
    .byte 0x11, 0x01
    .byte 0x12, 0x07
    .byte 0, 0
    .byte 0
{info_section}
    .long Lelephc_debug_cu_end - Lelephc_debug_cu_start
Lelephc_debug_cu_start:
    .short 4
    .long 0
    .byte 8
    .byte 1
    .asciz \"elephc\"
    .short 0x0002
    .asciz \"{name}\"
    .asciz \"{comp_dir}\"
    .long 0
"
    );
    for subprogram in subprograms {
        let _ = write!(
            out,
            "    .byte 2\n    .asciz \"{}\"\n    .quad {}\n    .quad Lelephc_fend_{} - {}\n",
            escape_asm_string(&subprogram.name),
            subprogram.symbol,
            subprogram.end_label,
            subprogram.symbol
        );
    }
    out.push_str("    .byte 0\nLelephc_debug_cu_end:\n");
    out
}

/// Escapes a value for use inside a double-quoted assembler string.
fn escape_asm_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Parses a numeric `key=value` token from a marker tail, returning `None` for
/// missing or malformed values (a malformed marker is skipped, not fatal:
/// debug-info injection must never fail a build the plain path would accept).
fn marker_number(marker: &str, key: &str) -> Option<usize> {
    marker_value(marker, key).and_then(|value| value.parse::<usize>().ok())
}

/// Returns the value of a space-separated `key=value` token, or `None`.
fn marker_value<'a>(marker: &'a str, key: &str) -> Option<&'a str> {
    marker
        .split_whitespace()
        .find_map(|token| token.strip_prefix(key)?.strip_prefix('='))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASM: &str = "\
    # @fn name=foo symbol=_php_foo
_php_foo:
    # @src line=3 col=5 end=3:12 op=store_local
    str x0, [sp]
    ret
    # @endfn name=foo
";

    /// Verifies every `@src` marker gets a matching `.loc` on the next line,
    /// the module starts with the `.file` directive, and the compile unit is
    /// appended with one subprogram per `@fn` region.
    #[test]
    fn injects_file_header_loc_and_compile_unit() {
        let out = inject_line_directives(ASM, "a.php", Platform::MacOS);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], ".file 1 \"a.php\"");
        let marker_idx = lines
            .iter()
            .position(|line| line.contains("@src line=3"))
            .expect("marker present");
        assert_eq!(lines[marker_idx + 1].trim(), ".loc 1 3 5");
        assert_eq!(out.matches(".loc 1").count(), 1);
        assert!(out.contains("__DWARF,__debug_info"), "{out}");
        assert!(out.contains(".asciz \"a.php\""), "{out}");
        assert!(out.contains(".asciz \"foo\""), "{out}");
        assert!(out.contains(".quad _php_foo"), "{out}");
        assert!(out.contains(".quad Lelephc_fend_0 - _php_foo"), "{out}");
        let end_label_idx = lines
            .iter()
            .position(|line| *line == "Lelephc_fend_0:")
            .expect("end label injected");
        assert!(
            lines[end_label_idx - 1].contains("@endfn"),
            "end label should sit right after the function region: {out}"
        );

        let linux = inject_line_directives(ASM, "a.php", Platform::Linux);
        assert!(
            linux.contains(".section .debug_info,\"\",@progbits"),
            "{linux}"
        );
    }

    /// Verifies non-marker lines pass through untouched and malformed markers
    /// are skipped instead of failing.
    #[test]
    fn skips_malformed_markers() {
        let asm = "    # @src line=x col=1\n    ret\n";
        let out = inject_line_directives(asm, "a.php", Platform::MacOS);
        assert!(!out.contains(".loc"), "{out}");
        assert!(out.contains("    ret\n"), "{out}");
    }
}
