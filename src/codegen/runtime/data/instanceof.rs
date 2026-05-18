//! Purpose:
//! Formats class/interface metadata used by runtime instanceof and exception matching helpers.
//! This keeps emitted parent-id and interface table layout coupled to object runtime checks.
//!
//! Called from:
//! - `crate::codegen::runtime::data::user` while formatting user class metadata.
//!
//! Key details:
//! - Table shape must match __rt_dynamic_instanceof and __rt_exception_matches exactly.

use crate::types::{ClassInfo, InterfaceInfo};

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
        out.push_str(&format!("    .quad _instanceof_name_class_{}\n", class_info.class_id));
        out.push_str(&format!("    .quad {}\n", class_name.len()));
        out.push_str(&format!("    .quad {}\n", class_info.class_id));
        out.push_str("    .quad 0\n");
        out.push_str(&format!(
            "    .quad _instanceof_name_class_abs_{}\n",
            class_info.class_id
        ));
        out.push_str(&format!("    .quad {}\n", class_name.len() + 1));
        out.push_str(&format!("    .quad {}\n", class_info.class_id));
        out.push_str("    .quad 0\n");
    }
    for (interface_name, interface_info) in sorted_interfaces {
        out.push_str(&format!(
            "    .quad _instanceof_name_interface_{}\n",
            interface_info.interface_id
        ));
        out.push_str(&format!("    .quad {}\n", interface_name.len()));
        out.push_str(&format!("    .quad {}\n", interface_info.interface_id));
        out.push_str("    .quad 1\n");
        out.push_str(&format!(
            "    .quad _instanceof_name_interface_abs_{}\n",
            interface_info.interface_id
        ));
        out.push_str(&format!("    .quad {}\n", interface_name.len() + 1));
        out.push_str(&format!("    .quad {}\n", interface_info.interface_id));
        out.push_str("    .quad 1\n");
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

pub(super) fn escaped_ascii(value: &str) -> String {
    escaped_bytes(value.as_bytes())
}

pub(super) fn escaped_bytes(bytes: &[u8]) -> String {
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
