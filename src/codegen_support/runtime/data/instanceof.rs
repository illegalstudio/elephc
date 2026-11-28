//! Purpose:
//! Formats class/interface metadata used by runtime instanceof and exception matching helpers.
//! This keeps emitted parent-id and interface table layout coupled to object runtime checks.
//!
//! Called from:
//! - `crate::codegen_support::runtime::data::user` while formatting user class metadata.
//!
//! Key details:
//! - Table shape must match __rt_dynamic_instanceof and __rt_exception_matches exactly.

use crate::types::{ClassInfo, InterfaceInfo};
use elephc_memoir::values::class_metadata::InstanceofTargetRow;

/// Emits the global `instanceof` target lookup table used by `__rt_dynamic_instanceof`
/// at runtime. Each entry contains a pointer to a name string, its length, the class/interface
/// id, and a flag indicating whether the entry is a class (0) or interface (1).
///
/// The table is sorted by `sorted_classes` followed by `sorted_interfaces` so that linear scans
/// produce deterministic results. Entries appear in pairs: first the plain name, then the
/// name prefixed with a backslash (the "absolute" form used for namespace-qualified lookups).
pub(super) fn emit_instanceof_target_lookup_data(
    out: &mut String,
    sorted_interfaces: &[(&String, &InterfaceInfo)],
    sorted_classes: &[(&String, &ClassInfo)],
) {
    let entry_count = (sorted_classes.len() + sorted_interfaces.len()) * 2;
    out.push_str(".globl _instanceof_target_count\n_instanceof_target_count:\n");
    out.push_str(&format!("    .quad {}\n", entry_count));
    out.push_str(".globl _instanceof_target_entries\n_instanceof_target_entries:\n");
    for (class_name, class_info) in sorted_classes {
        emit_instanceof_target_row(
            out,
            format!("_instanceof_name_class_{}", class_info.class_id),
            class_name.len(),
            class_info.class_id,
            0,
        );
        emit_instanceof_target_row(
            out,
            format!("_instanceof_name_class_abs_{}", class_info.class_id),
            class_name.len() + 1,
            class_info.class_id,
            0,
        );
    }
    for (interface_name, interface_info) in sorted_interfaces {
        emit_instanceof_target_row(
            out,
            format!("_instanceof_name_interface_{}", interface_info.interface_id),
            interface_name.len(),
            interface_info.interface_id,
            1,
        );
        emit_instanceof_target_row(
            out,
            format!("_instanceof_name_interface_abs_{}", interface_info.interface_id),
            interface_name.len() + 1,
            interface_info.interface_id,
            1,
        );
    }
    for (class_name, class_info) in sorted_classes {
        out.push_str(&format!(
            ".globl _instanceof_name_class_{}\n_instanceof_name_class_{}:\n",
            class_info.class_id, class_info.class_id
        ));
        out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(class_name)));
        out.push_str(&format!(
            ".globl _instanceof_name_class_abs_{}\n_instanceof_name_class_abs_{}:\n",
            class_info.class_id, class_info.class_id
        ));
        out.push_str(&format!(
            "    .ascii \"{}\"\n",
            escaped_ascii(&format!("\\{}", class_name))
        ));
    }
    for (interface_name, interface_info) in sorted_interfaces {
        out.push_str(&format!(
            ".globl _instanceof_name_interface_{}\n_instanceof_name_interface_{}:\n",
            interface_info.interface_id, interface_info.interface_id
        ));
        out.push_str(&format!("    .ascii \"{}\"\n", escaped_ascii(interface_name)));
        out.push_str(&format!(
            ".globl _instanceof_name_interface_abs_{}\n_instanceof_name_interface_abs_{}:\n",
            interface_info.interface_id, interface_info.interface_id
        ));
        out.push_str(&format!(
            "    .ascii \"{}\"\n",
            escaped_ascii(&format!("\\{}", interface_name))
        ));
    }
    out.push_str("    .p2align 3\n");
}

/// Emits one typed dynamic `instanceof` target metadata row.
fn emit_instanceof_target_row(
    out: &mut String,
    name: String,
    name_len: usize,
    target_id: u64,
    target_kind: u64,
) {
    let row = InstanceofTargetRow {
        name,
        name_len: name_len.to_string(),
        target_id: target_id.to_string(),
        target_kind: target_kind.to_string(),
    };
    for word in row.into_words() {
        out.push_str(&format!("    .quad {word}\n"));
    }
}

/// Converts a string to an assembly-friendly ASCII escape sequence, delegating to `escaped_bytes`.
/// Handles newlines (`\n`), tabs (`\t`), backslashes (`\\`), double quotes (`\"`), and any byte
/// outside the printable ASCII range (32–126) by encoding it as a 3-digit octal escape (`\NNN`).
pub(super) fn escaped_ascii(value: &str) -> String {
    escaped_bytes(value.as_bytes())
}

/// Converts a byte slice into an ASCII escape sequence string for use in `.ascii` directives.
/// Printable ASCII (0x20–0x7e) is emitted as-is; special bytes are replaced with:
/// - `\n` for newline
/// - `\t` for tab
/// - `\\` for backslash
/// - `\"` for double quote
/// - `\NNN` (3-digit octal) for any other byte, including null and high bytes
pub(crate) fn escaped_bytes(bytes: &[u8]) -> String {
    let mut escaped = String::new();
    for &byte in bytes {
        match byte {
            b'\n' => escaped.push_str("\\n"),
            b'\t' => escaped.push_str("\\t"),
            b'\\' => escaped.push_str("\\\\"),
            b'"' => escaped.push_str("\\\""),
            0x20..=0x7e => escaped.push(byte as char),
            _ => escaped.push_str(&format!("\\{:03o}", byte)),
        }
    }
    escaped
}
