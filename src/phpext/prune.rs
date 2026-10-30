//! Purpose:
//! Removes unreachable runtime functions and commons before laying out request snapshots.
//!
//! Called from:
//! - `super::build::assembly()` after generating the producer runtime.
//!
//! Key details:
//! - Uses the same global-function atom boundaries as native linker dead stripping.
//! - Address references and cross-function local labels retain their defining atom.
//! - Initialized data and the C adapter are conservative reachability roots.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::codegen_support::platform::Target;

/// Retains runtime dependencies of user code, fixed data and the host adapter.
pub(super) fn runtime(user: &str, runtime: &str, target: Target) -> Result<String, String> {
    let (text, data) = runtime.split_once("\n.data\n")
        .ok_or("PHP extension runtime has no fixed data section")?;
    let mut commons = Vec::new();
    let mut prefix = String::new();
    let mut atoms = Vec::<String>::new();
    for line in text.lines() {
        if line.trim().starts_with(".comm ") {
            commons.push(line);
            continue;
        }
        if line.starts_with(".globl ") { atoms.push(String::new()); }
        let output = atoms.last_mut().unwrap_or(&mut prefix);
        output.push_str(line);
        output.push('\n');
    }
    let mut fixed = String::new();
    for line in data.lines() {
        if line.trim().starts_with(".comm ") { commons.push(line); }
        else { fixed.push_str(line); fixed.push('\n'); }
    }
    let mut owners = HashMap::new();
    for (index, atom) in atoms.iter().enumerate() {
        for line in atom.lines() {
            let line = line.trim();
            let name = line.strip_suffix(':').or_else(|| {
                line.strip_prefix(".set ").and_then(|value| value.split_once(',').map(|v| v.0.trim()))
            });
            if let Some(name) = name.filter(|name| !name.bytes().all(|b| b.is_ascii_digit())) {
                owners.insert(name, index);
            }
        }
    }
    let adapter = concat!(include_str!("registration.c"), include_str!("activation.c"), include_str!("transport.c"));
    let mut referenced = HashSet::new();
    let mut queue = VecDeque::new();
    for root in [user, prefix.as_str(), fixed.as_str(), adapter] {
        for symbol in symbols(root) {
            referenced.insert(symbol);
            // C identifiers use the native platform spelling when no asm alias is present.
            if let Some(&owner) = owners.get(symbol).or_else(|| {
                owners.get(target.extern_symbol(symbol).as_str())
            }) { queue.push_back(owner); }
        }
    }
    let mut live = HashSet::new();
    while let Some(index) = queue.pop_front() {
        if !live.insert(index) { continue; }
        for symbol in symbols(&atoms[index]) {
            referenced.insert(symbol);
            if let Some(&owner) = owners.get(symbol) { queue.push_back(owner); }
        }
    }
    let mut out = prefix.clone();
    for (index, atom) in atoms.iter().enumerate() {
        if live.contains(&index) {
            // Each surviving atom starts aligned in text, even if its predecessor was removed.
            out.push_str(".text\n.p2align 2\n");
            out.push_str(atom);
        }
    }
    out.push_str(".data\n");
    out.push_str(&fixed);
    for common in commons {
        let name = common.trim().strip_prefix(".comm ").unwrap()
            .split(',').next().unwrap().trim();
        if referenced.contains(name) {
            out.push_str(common);
            out.push('\n');
        }
    }
    Ok(crate::codegen_support::visibility::append_hidden_directives(
        &out, &HashSet::new(), target.platform,
    ))
}

/// Recognizes assembly symbol tokens across ELF/Mach-O relocation suffixes and expressions.
fn symbols(source: &str) -> impl Iterator<Item = &str> {
    source.split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '.' | '$'))
        .filter(|word| !word.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Address tables and cross-atom local targets retain functions and their mutable storage.
    #[test]
    fn phpext_pruning_preserves_transitive_and_address_dependencies() {
        for name in ["linux-x86_64", "linux-aarch64", "macos-aarch64"] {
            let target = Target::parse(name).unwrap();
            let source = ".text\n.globl entry\nentry:\n.quad local_target\n.globl helper\nhelper:\nlocal_target:\n.quad _live_buf\n.globl dead\ndead:\n.quad _dead_buf\n.data\ntable:\n.quad entry\n.comm _live_buf,64,8\n.comm _dead_buf,1048576,8\n";
            let result = runtime("", source, target).unwrap();
            assert!(result.contains("helper:\nlocal_target:"), "{name}");
            assert!(result.contains(".comm _live_buf,64,8"), "{name}");
            assert!(!result.contains("dead:"), "{name}");
            assert!(!result.contains("_dead_buf"), "{name}");
        }
    }

    /// Small hosted heaps must not copy megabytes of unreachable platform scratch per call.
    #[test]
    fn phpext_minimal_snapshot_has_no_unreachable_scratch_floor() {
        for name in ["linux-x86_64", "linux-aarch64", "macos-aarch64"] {
            let target = Target::parse(name).unwrap();
            let heap = 65_536;
            let user = "\n.quad __rt_heap_init, __rt_zval_pack, __rt_zval_unpack, __rt_zval_free\n";
            let result = crate::phpext::build::assembly(
                user, heap, target, crate::codegen_support::RuntimeFeatures::none(),
            ).unwrap();
            for unused in ["_http_resp_buf", "_https_resp_buf", "_phar_write_out", "_servent_buf",
                "_user_wrapper_drain_buf", "_ser_obj_ptrs", "_ser_obj_idxs", "_unser_values"] {
                assert!(!result.contains(unused), "{name}: retained {unused}");
            }
            let end = target.extern_symbol("elephc_state_zero_end");
            let offset = result.lines().find_map(|line| line.strip_prefix(&format!(".set {end},")))
                .unwrap().rsplit_once('+').unwrap().1.parse::<usize>().unwrap();
            assert!(offset <= heap * 8, "{name}: small-heap zero state is {offset} bytes");
        }
    }
}
