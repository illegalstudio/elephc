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
//! - `@file id=<source_id> path_hex=<hex>` markers announce physical source
//!   paths; one `.file` is emitted per path and `.loc` uses that file number.
//!   Duplicate span identities for one path alias to the same file number.
//! - An unoptimized `--debug` build also turns scalar `@local` markers into
//!   `DW_TAG_variable` children with `DW_OP_fbreg` locations. Rich values stay
//!   in the DAP metadata, and `--debug-optimized` omits the variable DIEs.
//! - Every value spliced into assembly text is attacker-influenced (the source
//!   path and the working directory come from the invocation). Quoted operands
//!   go through `escape_asm_string()`, and the one unquoted operand (a
//!   subprogram entry symbol) is validated by `is_plain_asm_symbol()`; an
//!   unescaped `\` or `"` in a path would otherwise terminate the directive
//!   string early and let the rest of the path be assembled as directives.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::codegen::platform::{Arch, Platform};
use crate::codegen_support::runtime::data::instanceof::escaped_bytes;

/// One `@fn`..`@endfn` region collected during injection, used to emit its
/// `DW_TAG_subprogram`: the PHP-level name, the entry symbol (`DW_AT_low_pc`),
/// and the index of the injected `Lelephc_fend_<idx>` end label.
struct SubprogramInfo {
    name: String,
    symbol: String,
    end_label: usize,
    /// Scalar PHP locals whose frame slots can be described directly in DWARF.
    locals: Vec<LocalInfo>,
}

/// One PHP local rendered as a DWARF `DW_TAG_variable`.
struct LocalInfo {
    name: String,
    /// Positive distance below the frame pointer, matching the debug metadata.
    frame_offset: usize,
    /// Type label key: `int`, `float`, `bool`, `string` or `ref`.
    php_type: &'static str,
}

/// Returns `asm` extended with DWARF debug information for `source_path`:
/// a `.file`/`.loc` line table derived from the `@src` source-map markers, a
/// compile unit with one subprogram per `@fn` region, and — when
/// `php_variables` is set — scalar PHP locals under each subprogram.
pub fn inject_line_directives(
    asm: &str,
    source_path: &str,
    platform: Platform,
    arch: Arch,
    php_variables: bool,
) -> String {
    let mut out = String::with_capacity(asm.len() + asm.len() / 4);
    out.push_str(&format!(
        ".file 1 \"{}\"\n",
        escape_asm_string(source_path)
    ));

    let mut subprograms: Vec<SubprogramInfo> = Vec::new();
    // File 1 is the entry file. Included/autoloaded sources are assigned the
    // next numbers in the order their `@file` markers appear, so `.loc` can
    // name the physical file instead of collapsing every source into file 1.
    let mut file_numbers: HashMap<usize, usize> = HashMap::new();
    file_numbers.insert(0, 1);
    let mut path_numbers: HashMap<String, usize> = HashMap::new();
    path_numbers.insert(source_path.to_string(), 1);
    let mut next_file_number = 2usize;
    let mut open: Option<(String, String, Vec<LocalInfo>)> = None;
    for line in asm.lines() {
        out.push_str(line);
        out.push('\n');
        if let Some(marker) = line.split("@file ").nth(1) {
            if let (Some(source_id), Some(path_hex)) =
                (marker_number(marker, "id"), marker_value(marker, "path_hex"))
            {
                if source_id != 0 && !file_numbers.contains_key(&source_id) {
                    if let Some(path) = decode_hex_path(path_hex) {
                        // The same physical file can be parsed under more than one
                        // source identity (include variants, autoload probes).
                        // Reuse the first file number instead of emitting a duplicate
                        // `.file` and a second DWARF file entry.
                        if let Some(number) = path_numbers.get(&path).copied() {
                            file_numbers.insert(source_id, number);
                        } else {
                            let number = next_file_number;
                            next_file_number += 1;
                            file_numbers.insert(source_id, number);
                            path_numbers.insert(path.clone(), number);
                            let _ = writeln!(
                                out,
                                "    .file {} \"{}\"",
                                number,
                                escape_asm_string(&path)
                            );
                        }
                    }
                }
            }
            continue;
        }
        if let Some(marker) = line.split("@src ").nth(1) {
            if let (Some(php_line), Some(php_col)) =
                (marker_number(marker, "line"), marker_number(marker, "col"))
            {
                let file_number = marker_number(marker, "file")
                    .and_then(|source_id| file_numbers.get(&source_id).copied())
                    .unwrap_or(1);
                let _ = writeln!(out, "    .loc {} {} {}", file_number, php_line, php_col);
            }
            continue;
        }
        if let Some(marker) = line.split("@fn ").nth(1) {
            // Compiler-generated bodies (`_class_propinit_*`, …) have no user-written PHP
            // source; a DW_TAG_subprogram for them only buries the real functions — they
            // were 47 of the 54 subprograms a small program emitted.
            if marker.contains("synthetic=1") {
                continue;
            }
            if let (Some(name), Some(symbol)) =
                (marker_value(marker, "name"), marker_value(marker, "symbol"))
            {
                if is_plain_asm_symbol(symbol) {
                    open = Some((name.to_string(), symbol.to_string(), Vec::new()));
                }
            }
            continue;
        }
        if php_variables {
            if let Some(marker) = line.split("@local ").nth(1) {
                if let Some((_, _, locals)) = open.as_mut() {
                    if let Some(local) = parse_debug_local(marker) {
                        locals.push(local);
                    }
                }
                continue;
            }
        }
        if line.contains("@endfn") {
            if let Some((name, symbol, locals)) = open.take() {
                let end_label = subprograms.len();
                let _ = writeln!(out, "Lelephc_fend_{}:", end_label);
                subprograms.push(SubprogramInfo {
                    name,
                    symbol,
                    end_label,
                    locals,
                });
            }
        }
    }

    out.push_str(&debug_info_sections(
        source_path,
        platform,
        arch,
        php_variables,
        &subprograms,
    ));
    out
}

/// Renders the hand-encoded `.debug_abbrev` and `.debug_info` sections: one
/// DWARF32 version-4 compile unit whose `DW_AT_stmt_list` points at the line
/// table built from the injected `.loc` directives, containing one
/// `DW_TAG_subprogram` per `@fn` region (entry symbol as `DW_AT_low_pc`, the
/// injected end label as `DW_AT_high_pc`).
///
/// When `php_variables` is set (an unoptimized `--debug` build), each scalar PHP
/// local becomes a `DW_TAG_variable` child with a frame-relative location. Rich
/// values (strings, arrays, objects, references) stay in the DAP metadata: their
/// native slots are multi-word or reference-cell encodings, and exposing a raw
/// pointer as DWARF would be misleading.
fn debug_info_sections(
    source_path: &str,
    platform: Platform,
    arch: Arch,
    php_variables: bool,
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
    // `DW_AT_stmt_list` is an offset into the LINKED `.debug_line`, and this unit
    // is not the only contributor to it: every libc object linked in brings its
    // own line program. A literal 0 names whichever landed first, which after a
    // Linux link is musl's `rcrt1.c` — so `addr2line` followed this unit's
    // pointer into someone else's table and answered `:?` for every PHP address.
    //
    // macOS never saw it, and that is the whole reason it went unnoticed:
    // `dsymutil` rebuilds the debug map from the OBJECT files, where this unit is
    // the only one and 0 is right. Linking is what moves it.
    //
    // The fix is the idiom clang and gcc use: put a label at the top of the
    // section and let the assembler append its generated program after it.
    // The same hazard applies to `debug_abbrev_offset` in the unit header: it is an
    // offset into the LINKED `.debug_abbrev`, and a literal 0 names musl's table
    // rather than this unit's. The unit then decodes against the wrong
    // abbreviations, which is why `readelf --debug-dump=info` reported only the
    // two libc units and never this one, even though the PHP path was sitting in
    // the section's bytes.
    let (line_anchor_section, stmt_list, abbrev_offset) = match platform {
        Platform::Linux => (
            "    .section .debug_line,\"\",@progbits\nLelephc_debug_line0:\n",
            "Lelephc_debug_line0",
            "Lelephc_debug_abbrev0",
        ),
        // Mach-O keeps each object's DWARF separate until `dsymutil` reads it, so
        // 0 is the correct offset there and a label would not survive the pass.
        _ => ("", "0", "0"),
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
    // Abbrev 2 (variables): DW_TAG_subprogram (0x2e), has children;
    //   name, low_pc, high_pc, and DW_AT_frame_base(0x40)/exprloc(0x18).
    // Abbrev 3: DW_TAG_variable (0x34), no children;
    //   name, DW_AT_type(0x49)/ref4(0x13), DW_AT_location(0x02)/exprloc(0x18).
    // Abbrev 4: DW_TAG_base_type (0x24), no children;
    //   name, DW_AT_encoding(0x3e)/data1(0x0b), DW_AT_byte_size(0x0b)/data1.
    // Abbrev 5: DW_TAG_structure_type (0x13), has children;
    //   name, DW_AT_byte_size(0x0b)/data1.
    // Abbrev 6: DW_TAG_member (0x0d), no children;
    //   name, DW_AT_type(0x49)/ref4(0x13),
    //   DW_AT_data_member_location(0x38)/data1(0x0b).
    let abbrev_anchor = if platform == Platform::Linux { "Lelephc_debug_abbrev0:\n" } else { "" };
    let mut out = String::new();
    out.push_str(abbrev_section);
    out.push('\n');
    out.push_str(abbrev_anchor);
    out.push_str(
        "    .byte 1\n    .byte 0x11\n    .byte 1\n    .byte 0x25, 0x08\n    .byte 0x13, 0x05\n    .byte 0x03, 0x08\n    .byte 0x1b, 0x08\n    .byte 0x10, 0x17\n    .byte 0, 0\n",
    );
    if php_variables {
        out.push_str(
            "    .byte 2\n    .byte 0x2e\n    .byte 1\n    .byte 0x03, 0x08\n    .byte 0x11, 0x01\n    .byte 0x12, 0x07\n    .byte 0x40, 0x18\n    .byte 0, 0\n    .byte 3\n    .byte 0x34\n    .byte 0\n    .byte 0x03, 0x08\n    .byte 0x49, 0x13\n    .byte 0x02, 0x18\n    .byte 0, 0\n    .byte 4\n    .byte 0x24\n    .byte 0\n    .byte 0x03, 0x08\n    .byte 0x3e, 0x0b\n    .byte 0x0b, 0x0b\n    .byte 0, 0\n    .byte 5\n    .byte 0x13\n    .byte 1\n    .byte 0x03, 0x08\n    .byte 0x0b, 0x0b\n    .byte 0, 0\n    .byte 6\n    .byte 0x0d\n    .byte 0\n    .byte 0x03, 0x08\n    .byte 0x49, 0x13\n    .byte 0x38, 0x0b\n    .byte 0, 0\n",
        );
    } else {
        out.push_str(
            "    .byte 2\n    .byte 0x2e\n    .byte 0\n    .byte 0x03, 0x08\n    .byte 0x11, 0x01\n    .byte 0x12, 0x07\n    .byte 0, 0\n",
        );
    }
    out.push_str("    .byte 0\n");
    out.push_str(info_section);
    out.push('\n');
    let _ = write!(
        out,
        "    .long Lelephc_debug_cu_end - Lelephc_debug_cu_start\nLelephc_debug_cu_start:\n    .short 4\n    .long {abbrev_offset}\n    .byte 8\n    .byte 1\n    .asciz \"elephc\"\n    .short 0x0002\n    .asciz \"{name}\"\n    .asciz \"{comp_dir}\"\n    .long {stmt_list}\n"
    );
    if php_variables {
        out.push_str(
            "Lelephc_debug_type_int:\n    .byte 4\n    .asciz \"int\"\n    .byte 0x05\n    .byte 8\nLelephc_debug_type_float:\n    .byte 4\n    .asciz \"float\"\n    .byte 0x04\n    .byte 8\nLelephc_debug_type_bool:\n    .byte 4\n    .asciz \"bool\"\n    .byte 0x02\n    .byte 8\nLelephc_debug_type_ptr:\n    .byte 4\n    .asciz \"ptr\"\n    .byte 0x01\n    .byte 8\nLelephc_debug_type_size:\n    .byte 4\n    .asciz \"size_t\"\n    .byte 0x07\n    .byte 8\nLelephc_debug_type_ref:\n    .byte 4\n    .asciz \"php_ref\"\n    .byte 0x01\n    .byte 8\nLelephc_debug_type_string:\n    .byte 5\n    .asciz \"php_string\"\n    .byte 16\n    .byte 6\n    .asciz \"ptr\"\n    .long Lelephc_debug_type_ptr - Lelephc_debug_cu_start\n    .byte 0\n    .byte 6\n    .asciz \"len\"\n    .long Lelephc_debug_type_size - Lelephc_debug_cu_start\n    .byte 8\n    .byte 0\n",
        );
    }
    let frame_base = match arch {
        Arch::AArch64 => "0x8d, 0x00",
        Arch::X86_64 => "0x76, 0x00",
    };
    for subprogram in subprograms {
        let _ = write!(
            out,
            "    .byte 2\n    .asciz \"{}\"\n    .quad {}\n    .quad Lelephc_fend_{} - {}\n",
            escape_asm_string(&subprogram.name),
            subprogram.symbol,
            subprogram.end_label,
            subprogram.symbol
        );
        if php_variables {
            let _ = writeln!(out, "    .byte 2\n    .byte {frame_base}");
            for local in &subprogram.locals {
                push_variable_die(&mut out, local);
            }
            out.push_str("    .byte 0\n");
        }
    }
    out.push_str("    .byte 0\nLelephc_debug_cu_end:\n");
    out.push_str(line_anchor_section);
    out
}

/// Parses one `@local` marker into the subset DWARF can describe.
///
/// Reference cells and compiler-owned storage are skipped: their frame word is
/// an indirection, not the PHP value. Sensitive parameters are skipped so DWARF
/// cannot leak what the runtime call capture redacts.
fn parse_debug_local(marker: &str) -> Option<LocalInfo> {
    if marker_value(marker, "kind").unwrap_or("php_local") != "php_local" {
        return None;
    }
    if marker_value(marker, "storage").unwrap_or("raw") != "raw" {
        return None;
    }
    if marker_value(marker, "sensitive") == Some("1") {
        return None;
    }
    let name = decode_hex_path(marker_value(marker, "name_hex")?)?;
    let frame_offset = marker_number(marker, "offset")?;
    if frame_offset == 0 || frame_offset > i64::MAX as usize {
        return None;
    }
    let type_text = decode_hex_path(marker_value(marker, "type_hex")?)?;
    let category = marker_value(marker, "category").unwrap_or("");
    let php_type = match category {
        "string" => "string",
        "array" | "object" | "mixed" | "iterable" | "callable" | "resource" => "ref",
        _ => match type_text.as_str() {
            "int" => "int",
            "float" => "float",
            "bool" => "bool",
            "string" => "string",
            _ => return None,
        },
    };
    Some(LocalInfo {
        name,
        frame_offset,
        php_type,
    })
}

/// Emits one `DW_TAG_variable` DIE with a `DW_OP_fbreg` location.
fn push_variable_die(out: &mut String, local: &LocalInfo) {
    let mut location = vec![0x91u8]; // DW_OP_fbreg
    location.extend(sleb128(-(local.frame_offset as i64)));
    let _ = write!(
        out,
        "    .byte 3\n    .asciz \"{}\"\n    .long Lelephc_debug_type_{} - Lelephc_debug_cu_start\n    .byte {}\n    .byte ",
        escape_asm_string(&local.name),
        local.php_type,
        location.len()
    );
    for (index, byte) in location.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "0x{byte:02x}");
    }
    out.push('\n');
}

/// Encodes a signed value as DWARF LEB128 for a location expression.
fn sleb128(mut value: i64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        let sign_clear = byte & 0x40 == 0;
        let done = (value == 0 && sign_clear) || (value == -1 && !sign_clear);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
            break;
        }
    }
    bytes
}

/// Escapes a value for use inside a double-quoted assembler string literal.
///
/// Delegates to the shared `escaped_bytes()` encoder that already backs the
/// runtime `.ascii` data section, so `.file` and every `.asciz` emitted here
/// obey one escaping contract: `\` and `"` are backslash-escaped, newline and
/// tab use their short forms, and every other byte outside printable ASCII
/// (carriage return, NUL, UTF-8 continuation bytes) becomes a 3-digit octal
/// escape. Both GNU `as` and the LLVM integrated assembler decode all of those
/// back to the original bytes, so the DWARF payload is byte-exact.
///
/// This is a security boundary, not cosmetics: a path containing `\"` would
/// otherwise close the directive string early and let the remainder of the path
/// be assembled as directives.
fn escape_asm_string(value: &str) -> String {
    escaped_bytes(value.as_bytes())
}

/// Returns whether `symbol` is a plain assembler symbol name, safe to splice
/// into the unquoted `.quad <symbol>` operands of a `DW_TAG_subprogram`.
///
/// Entry symbols are mangled by codegen down to `[A-Za-z0-9_$.]`, so anything
/// else means a malformed or forged `@fn` marker. Such a region is skipped the
/// same way a malformed `@src` marker is: debug-info injection must never fail a
/// build the plain path would accept, and it must never emit an operand that the
/// assembler could read as extra syntax.
fn is_plain_asm_symbol(symbol: &str) -> bool {
    !symbol.is_empty()
        && symbol
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'.'))
}

/// Decodes the lowercase/uppercase hexadecimal path payload of an `@file`
/// marker. A malformed payload is skipped, never fatal: debug-info injection
/// must not reject assembly the plain path would accept.
fn decode_hex_path(hex: &str) -> Option<String> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks_exact(2) {
        let text = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(text, 16).ok()?);
    }
    String::from_utf8(bytes).ok()
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

    /// Verifies the Linux unit points at ITS OWN abbrev table and line program,
    /// by label rather than by a literal zero.
    ///
    /// Both offsets are into sections the linker CONCATENATES. A literal 0 names
    /// whatever landed first, which after a Linux link is musl's — so the unit
    /// decoded against the wrong abbreviations, `readelf --debug-dump=info`
    /// reported two compile units for a binary that has thirty-seven, and every
    /// unit after this one was dropped with it. `addr2line` answered `:?` for
    /// every PHP address, and a debugger lost the same ground.
    ///
    /// macOS keeps 0, and that is why this went unseen: `dsymutil` rebuilds the
    /// debug map from the OBJECT files, where this unit is the only contributor
    /// and 0 is correct. Linking is what moves it.
    #[test]
    fn the_linux_unit_references_its_own_sections_by_label() {
        let out = inject_line_directives(ASM, "a.php", Platform::Linux, Arch::AArch64, false);
        assert!(out.contains("Lelephc_debug_abbrev0:"), "abbrev table not anchored: {out}");
        assert!(out.contains("Lelephc_debug_line0:"), "line program not anchored: {out}");
        assert!(
            out.contains(".long Lelephc_debug_abbrev0"),
            "unit header still names a literal abbrev offset: {out}"
        );
        assert!(
            out.contains(".long Lelephc_debug_line0"),
            "DW_AT_stmt_list still names a literal line offset: {out}"
        );

        // Mach-O keeps the literal, because there it is the right answer.
        let macos = inject_line_directives(ASM, "a.php", Platform::MacOS, Arch::AArch64, false);
        assert!(!macos.contains("Lelephc_debug_abbrev0"), "{macos}");
        assert!(!macos.contains("Lelephc_debug_line0"), "{macos}");
    }

    /// Verifies an included source announced by `@file` gets its own `.file`
    /// number and that `@src file=<id>` emits `.loc <number>`, so lldb/gdb name
    /// `lib.php` instead of collapsing it into the entry file.
    #[test]
    fn included_source_gets_its_own_file_directive() {
        let asm = "\
    # @file id=0 path_hex=6d61696e2e706870
    # @fn name=foo symbol=_php_foo file=0
_php_foo:
    # @src file=0 line=3 col=5
    ret
    # @endfn name=foo
    # @file id=7 path_hex=6c69622e706870
    # @file id=8 path_hex=6c69622e706870
    # @fn name=bar symbol=_php_bar file=7
_php_bar:
    # @src file=7 line=9 col=3
    # @src file=8 line=10 col=1
    ret
    # @endfn name=bar
";
        for platform in [Platform::Linux, Platform::MacOS] {
            let out = inject_line_directives(asm, "main.php", platform, Arch::AArch64, false);
            assert!(out.contains(".file 1 \"main.php\""), "{out}");
            assert_eq!(out.matches(".file 2 \"lib.php\"").count(), 1, "{out}");
            assert!(out.contains(".loc 1 3 5"), "{out}");
            assert!(out.contains(".loc 2 9 3"), "{out}");
            assert!(out.contains(".loc 2 10 1"), "{out}");
        }
    }

    /// Verifies every `@src` marker gets a matching `.loc` on the next line,
    /// the module starts with the `.file` directive, and the compile unit is
    /// appended with one subprogram per `@fn` region.
    #[test]
    fn injects_file_header_loc_and_compile_unit() {
        let out = inject_line_directives(ASM, "a.php", Platform::MacOS, Arch::AArch64, false);
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

        let linux = inject_line_directives(ASM, "a.php", Platform::Linux, Arch::AArch64, false);
        assert!(
            linux.contains(".section .debug_info,\"\",@progbits"),
            "{linux}"
        );
    }

    /// Verifies an unoptimized debug build publishes scalar PHP locals as
    /// frame-relative DWARF variables, while reference cells, strings and
    /// sensitive parameters stay out of `.debug_info`.
    #[test]
    fn emits_scalar_php_variables_when_debugging() {
        let asm = "\
    # @fn name=compute symbol=_php_compute
_php_compute:
    # @local name_hex=6e offset=16 type_hex=696e74 category=scalar kind=php_local storage=raw
    # @local name_hex=726174696f offset=32 type_hex=666c6f6174 category=scalar kind=php_local storage=raw
    # @local name_hex=666c6167 offset=48 type_hex=626f6f6c category=scalar kind=php_local storage=raw
    # @local name_hex=6e616d65 offset=64 type_hex=737472696e67 category=string kind=php_local storage=raw
    # @local name_hex=6974656d73 offset=112 type_hex=61727261793c696e743e category=array kind=php_local storage=raw
    # @local name_hex=726566 offset=80 type_hex=696e74 category=scalar kind=php_local storage=ref_cell
    # @local name_hex=736563726574 offset=96 type_hex=696e74 category=scalar kind=php_local storage=raw sensitive=1
    # @src line=4 col=5
    ret
    # @endfn name=compute
";
        let x86 = inject_line_directives(
            asm,
            "a.php",
            Platform::Linux,
            Arch::X86_64,
            true,
        );
        for kind in ["int", "float", "bool", "string", "ref"] {
            assert!(x86.contains(&format!("Lelephc_debug_type_{kind}:")), "{kind}: {x86}");
        }
        for name in ["n", "ratio", "flag", "name", "items"] {
            assert!(x86.contains(&format!(".asciz \"{name}\"")), "{name}: {x86}");
        }
        for name in ["ref", "secret"] {
            assert!(!x86.contains(&format!(".asciz \"{name}\"")), "{name}: {x86}");
        }
        assert!(x86.contains(".asciz \"php_string\""), "{x86}");
        assert!(x86.contains(".asciz \"php_ref\""), "{x86}");
        assert!(
            x86.contains(".long Lelephc_debug_type_string - Lelephc_debug_cu_start"),
            "{x86}"
        );
        assert!(
            x86.contains(".long Lelephc_debug_type_ref - Lelephc_debug_cu_start"),
            "{x86}"
        );
        assert!(x86.contains("    .byte 0x76, 0x00"), "{x86}");
        assert!(x86.contains("    .byte 0x91, 0x70"), "{x86}");
        assert!(x86.contains("    .byte 0x91, 0x60"), "{x86}");
        assert!(x86.contains("    .byte 0x91, 0x50"), "{x86}");

        let aarch64 = inject_line_directives(
            asm,
            "a.php",
            Platform::Linux,
            Arch::AArch64,
            true,
        );
        assert!(aarch64.contains("    .byte 0x8d, 0x00"), "{aarch64}");

        let plain = inject_line_directives(
            asm,
            "a.php",
            Platform::Linux,
            Arch::X86_64,
            false,
        );
        assert!(!plain.contains("Lelephc_debug_type_int:"), "{plain}");
        assert!(!plain.contains(".asciz \"n\""), "{plain}");
        assert_eq!(sleb128(-16), vec![0x70]);
        assert_eq!(sleb128(-64), vec![0x40]);
        assert_eq!(sleb128(-128), vec![0x80, 0x7f]);
    }

    /// Verifies non-marker lines pass through untouched and malformed markers
    /// are skipped instead of failing.
    #[test]
    fn skips_malformed_markers() {
        let asm = "    # @src line=x col=1\n    ret\n";
        let out = inject_line_directives(asm, "a.php", Platform::MacOS, Arch::AArch64, false);
        assert!(
            !out.lines().any(|line| line.trim_start().starts_with(".loc ")),
            "{out}"
        );
        assert!(out.contains("    ret\n"), "{out}");
    }

    /// Verifies the assembler-string escaper covers every byte class that could
    /// terminate or reinterpret a quoted directive operand: backslash, double
    /// quote, newline, carriage return, tab, NUL, and non-ASCII UTF-8 bytes.
    /// Ordinary path characters must survive verbatim so normal builds are
    /// byte-identical to the pre-hardening output.
    #[test]
    fn escapes_every_assembler_string_metacharacter() {
        assert_eq!(escape_asm_string("plain/path-1_2.php"), "plain/path-1_2.php");
        assert_eq!(escape_asm_string("bs\\lash"), "bs\\\\lash");
        assert_eq!(escape_asm_string("q\"uote"), "q\\\"uote");
        assert_eq!(escape_asm_string("a\nb"), "a\\nb");
        assert_eq!(escape_asm_string("a\tb"), "a\\tb");
        assert_eq!(escape_asm_string("a\rb"), "a\\015b");
        assert_eq!(escape_asm_string("a\0b"), "a\\000b");
        assert_eq!(escape_asm_string("é"), "\\303\\251");
        // The historical breakout: `\` was left raw while `"` became `\"`, so the
        // pair rendered as an escaped backslash followed by a real closing quote.
        assert_eq!(escape_asm_string("a\\\";"), "a\\\\\\\";");
    }

    /// Verifies a source path carrying assembler metacharacters cannot escape
    /// any quoted directive: the `.file` header, the compile unit `DW_AT_name`,
    /// and the whole module must stay one directive per line. Regression test
    /// for the `--debug-info` source-path directive injection.
    #[test]
    fn source_path_cannot_break_out_of_quoted_directives() {
        let path = "sec/a\\\"; .globl pwned; pwned = 7; #\nb\t.php";
        let out = inject_line_directives(ASM, path, Platform::Linux, Arch::AArch64, false);

        assert_eq!(
            out.lines().next().unwrap(),
            ".file 1 \"sec/a\\\\\\\"; .globl pwned; pwned = 7; #\\nb\\t.php\""
        );
        assert!(
            out.contains(".asciz \"sec/a\\\\\\\"; .globl pwned; pwned = 7; #\\nb\\t.php\""),
            "{out}"
        );
        for line in out.lines() {
            assert!(
                !line.starts_with(".globl pwned"),
                "path bytes reached the assembler as a directive: {out}"
            );
        }
        // Every quoted directive must keep an even number of unescaped quotes,
        // which is what stops the remainder of the line from being assembled.
        for line in out.lines().filter(|line| line.contains('"')) {
            let mut quotes = 0usize;
            let mut escaped = false;
            for byte in line.bytes() {
                match (escaped, byte) {
                    (true, _) => escaped = false,
                    (false, b'\\') => escaped = true,
                    (false, b'"') => quotes += 1,
                    _ => {}
                }
            }
            assert_eq!(quotes % 2, 0, "unbalanced quotes in `{line}`: {out}");
        }
    }

    /// Verifies a namespaced PHP function name (which legitimately contains
    /// backslashes) is escaped inside its `DW_AT_name` string instead of being
    /// emitted raw.
    #[test]
    fn escapes_namespaced_subprogram_name() {
        let asm = "    # @fn name=App\\Deep\\greet symbol=_fn_App_N_Deep_N_greet\n\
                   _fn_App_N_Deep_N_greet:\n    ret\n    # @endfn name=App\\Deep\\greet\n";
        let out = inject_line_directives(asm, "a.php", Platform::Linux, Arch::AArch64, false);
        assert!(out.contains(".asciz \"App\\\\Deep\\\\greet\""), "{out}");
        assert!(out.contains(".quad _fn_App_N_Deep_N_greet"), "{out}");
    }

    /// Verifies an `@fn` marker whose entry symbol is not a plain assembler
    /// symbol is dropped: `.quad` takes an unquoted expression, so a forged
    /// symbol would otherwise splice raw syntax into `.debug_info`.
    #[test]
    fn skips_subprogram_with_non_symbol_entry() {
        let asm = "    # @fn name=foo symbol=_ok;.globl_pwned\n_ok:\n    ret\n    # @endfn name=foo\n";
        let out = inject_line_directives(asm, "a.php", Platform::Linux, Arch::AArch64, false);
        assert!(!out.contains(".quad"), "{out}");
        assert!(!out.contains("Lelephc_fend_0"), "{out}");
        assert!(out.contains("    ret\n"), "{out}");

        assert!(is_plain_asm_symbol("_fn_App_N_Deep_N_greet"));
        assert!(is_plain_asm_symbol("l_.str$1"));
        assert!(!is_plain_asm_symbol(""));
        assert!(!is_plain_asm_symbol("_ok;.globl x"));
        assert!(!is_plain_asm_symbol("_ok\"x"));
    }
}
