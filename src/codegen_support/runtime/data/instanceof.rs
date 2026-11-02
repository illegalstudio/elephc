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
    // The names in entry order, exactly as the entries below spell them.
    let mut names: Vec<String> = Vec::with_capacity(entry_count);
    for (class_name, _) in sorted_classes {
        names.push((*class_name).clone());
        names.push(format!("\\{}", class_name));
    }
    for (interface_name, _) in sorted_interfaces {
        names.push((*interface_name).clone());
        names.push(format!("\\{}", interface_name));
    }
    emit_instanceof_target_hash(out, &names);
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

/// FNV-1a offset basis the lookup hash starts from; `__rt_instanceof_lookup` uses the same.
pub(crate) const INSTANCEOF_HASH_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a prime the lookup hash multiplies by; `__rt_instanceof_lookup` uses the same.
pub(crate) const INSTANCEOF_HASH_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Hashes a class-like name the way `__rt_instanceof_lookup` does: FNV-1a over its bytes with
/// ASCII `A`-`Z` folded to lowercase, because PHP class names compare case-insensitively.
pub(crate) fn instanceof_name_hash(name: &[u8]) -> u64 {
    name.iter().fold(INSTANCEOF_HASH_BASIS, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(INSTANCEOF_HASH_PRIME)
    })
}

/// Builds the open-addressing index over the entry names: one slot per power-of-two bucket,
/// holding the entry's index plus one, or 0 when empty.
///
/// WHY. The lookup used to compare the requested name against every entry, two per class-like,
/// and on the Symfony `--web` fixture that scan was the hottest single frame of a request
/// (`__rt_instanceof_lookup` 6.8% of all samples) because every `is_a` the eval bridge answers
/// goes through it. The index makes it one hash of the requested name plus, almost always, one
/// comparison.
///
/// The table is at most half full, so a probe always reaches an empty slot and a miss
/// terminates. Entries are inserted in table order with linear probing, so when two entries
/// share a name the earlier one sits earlier on the probe path and is found first, exactly as
/// the linear scan found it.
pub(crate) fn instanceof_target_hash_slots(names: &[String]) -> Vec<u32> {
    let size = (names.len() * 2).max(1).next_power_of_two();
    let mask = size - 1;
    let mut slots = vec![0u32; size];
    for (index, name) in names.iter().enumerate() {
        let mut slot = (instanceof_name_hash(name.as_bytes()) as usize) & mask;
        while slots[slot] != 0 {
            slot = (slot + 1) & mask;
        }
        slots[slot] = u32::try_from(index + 1).expect("instanceof target count fits 32 bits");
    }
    slots
}

/// Emits `_instanceof_target_hash_mask` and `_instanceof_target_hash`, the index
/// `__rt_instanceof_lookup` probes instead of scanning `_instanceof_target_entries`.
fn emit_instanceof_target_hash(out: &mut String, names: &[String]) {
    let slots = instanceof_target_hash_slots(names);
    out.push_str(".globl _instanceof_target_hash_mask\n_instanceof_target_hash_mask:\n");
    out.push_str(&format!("    .quad {}\n", slots.len() - 1));
    out.push_str(".globl _instanceof_target_hash\n_instanceof_target_hash:\n");
    for chunk in slots.chunks(16) {
        let row: Vec<String> = chunk.iter().map(u32::to_string).collect();
        out.push_str(&format!("    .long {}\n", row.join(", ")));
    }
    out.push_str("    .p2align 3\n");
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Probes the index the way `__rt_instanceof_lookup` does and returns the entry index found.
    fn probe(slots: &[u32], names: &[String], requested: &str) -> Option<usize> {
        let mask = slots.len() - 1;
        let mut slot = (instanceof_name_hash(requested.as_bytes()) as usize) & mask;
        loop {
            let entry = slots[slot];
            if entry == 0 {
                return None;
            }
            let index = entry as usize - 1;
            if names[index].eq_ignore_ascii_case(requested) {
                return Some(index);
            }
            slot = (slot + 1) & mask;
        }
    }

    /// Every name is found at its own entry through any ASCII casing, a name listed twice
    /// resolves to its first entry as the linear scan did, and a miss stops at an empty slot.
    #[test]
    fn index_finds_every_entry_case_insensitively_and_first_duplicate_wins() {
        let mut names: Vec<String> = (0..500)
            .flat_map(|i| [format!("App\\Model\\Entity{i}"), format!("\\App\\Model\\Entity{i}")])
            .collect();
        names.push("app\\model\\ENTITY7".to_string());
        let slots = instanceof_target_hash_slots(&names);
        assert!(slots.len().is_power_of_two() && slots.len() >= names.len() * 2);
        for (index, name) in names.iter().enumerate().take(1000) {
            assert_eq!(probe(&slots, &names, name), Some(index));
            assert_eq!(probe(&slots, &names, &name.to_ascii_uppercase()), Some(index));
        }
        assert_eq!(probe(&slots, &names, "App\\Model\\Entity7"), Some(14));
        assert_eq!(probe(&slots, &names, "App\\Model\\Missing"), None);
    }

    /// A program with no class-like still gets a one-slot, empty index, so the lookup misses.
    #[test]
    fn empty_index_has_one_empty_slot() {
        assert_eq!(instanceof_target_hash_slots(&[]), vec![0]);
    }
}
