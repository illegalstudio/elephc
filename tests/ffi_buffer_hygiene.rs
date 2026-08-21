//! Purpose:
//! Tripwire for the process-global FFI return-buffer class: a `static`
//! `Mutex<CString>` (or `Mutex<Vec<u8>>` / `Mutex<String>`, and their `RwLock`
//! siblings, however wrapped in `Lazy`/`OnceLock`) whose contents are handed out
//! as a raw pointer is a cross-thread use-after-free — assigning the cell drops
//! the previous contents and frees the exact bytes a previously handed-out
//! pointer still references. The mutex serializes the write; it does nothing for
//! the lifetime of the read.
//!
//! Key details:
//! - This shipped as a real intermittent CI failure: `elephc_pdo_sqlstate`
//!   answered garbage bytes instead of "00000" whenever the pg and my live
//!   round-trips ran concurrently in one process, because every pdo string
//!   return funneled through one process-global `Mutex<CString>`. The same
//!   pattern was then swept out of the tz/image/phar bridges. The durable form
//!   of that sweep is this test, not the memory of it.
//! - The rule (CONTRIBUTING.md § "Expose a stable C ABI"): returned buffers live
//!   in `thread_local!` cells — a thread can only invalidate pointers it was
//!   itself handed — or the caller owns the allocation. Id-keyed registries that
//!   hand out OWNED values (no interior pointer escapes) may stay global; they
//!   do not match this scan because their payload is not a bare buffer type.
//! - Detection is deliberately a coarse textual net over a precise semantic
//!   check: it flags the *cell shape*, not the pointer escape, because every
//!   instance of the shape found in this repo either escaped a pointer or was
//!   one refactor away from it. A genuinely justified future exception goes on
//!   ALLOWED with its reason — and the allowlist is shrink-only: an entry whose
//!   static disappears must be removed here, so the list cannot rot.

use std::fs;
use std::path::{Path, PathBuf};

/// Justified exceptions, as (`path suffix`, `static name`, reason). Empty today:
/// the R7 sweep converted every instance. An addition needs a reason that
/// explains why the handed-out bytes cannot outlive their validity — not merely
/// that the cell is convenient.
const ALLOWED: &[(&str, &str, &str)] = &[];

/// A `static` item whose type spells a lock around a bare buffer type. Matches
/// through wrappers (`Lazy<Mutex<CString>>`, `OnceLock<RwLock<Vec<u8>>>`) because
/// the scan only requires the lock and the payload to appear in the type text.
fn is_suspect(static_type: &str) -> bool {
    let compact: String = static_type.chars().filter(|c| !c.is_whitespace()).collect();
    // For every lock opening in the type text, strip any `path::` qualifiers the
    // payload may carry (`Mutex<std::ffi::CString>` is the same hazard as
    // `Mutex<CString>`) and test the payload that immediately follows. Payload
    // POSITION matters: `Mutex<HashMap<String, …>>` is the id-keyed registry
    // shape and must not match, so only the token directly inside the lock counts.
    for lock in ["Mutex<", "RwLock<"] {
        let mut search = compact.as_str();
        while let Some(at) = search.find(lock) {
            let mut payload = &search[at + lock.len()..];
            loop {
                let Some(sep) = payload.find("::") else { break };
                if payload[..sep].chars().all(|c| c.is_alphanumeric() || c == '_') {
                    payload = &payload[sep + 2..];
                } else {
                    break;
                }
            }
            if payload.starts_with("CString") || payload.starts_with("String") || payload.starts_with("Vec<u8>") {
                return true;
            }
            search = &search[at + lock.len()..];
        }
    }
    false
}

fn scan_file(path: &Path, findings: &mut Vec<String>) {
    let Ok(source) = fs::read_to_string(path) else {
        return;
    };
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        // `static NAME: <type> = ...;` — the type text between ':' and '=' (or
        // line end) is what the shape check inspects. One line is enough: every
        // real instance declared the full type on the `static`'s own line.
        let Some(rest) = trimmed.strip_prefix("static ").or_else(|| {
            trimmed.strip_prefix("pub static ").or_else(|| trimmed.strip_prefix("pub(crate) static "))
        }) else {
            continue;
        };
        let Some((name_part, type_part)) = rest.split_once(':') else {
            continue;
        };
        let static_name = name_part.trim().trim_start_matches("mut ").trim();
        let static_type = type_part.split('=').next().unwrap_or(type_part);
        if !is_suspect(static_type) {
            continue;
        }
        let allowed = ALLOWED.iter().any(|(suffix, name, _)| {
            path.to_string_lossy().ends_with(suffix) && *name == static_name
        });
        if !allowed {
            findings.push(format!("{}:{} static {static_name}: {}", path.display(), index + 1, static_type.trim()));
        }
    }
}

fn scan_tree(root: &Path, findings: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if name == "target" || name == ".git" {
                continue;
            }
            scan_tree(&path, findings);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            scan_file(&path, findings);
        }
    }
}

/// Every workspace source tree is free of process-global lock-around-buffer
/// statics, except the reasoned allowlist above.
#[test]
fn no_process_global_ffi_return_buffers() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut findings = Vec::new();
    scan_tree(&manifest_dir.join("crates"), &mut findings);
    scan_tree(&manifest_dir.join("src"), &mut findings);
    assert!(
        findings.is_empty(),
        "process-global lock-around-buffer statics found — a raw pointer handed out \
         of such a cell is freed by the NEXT call from any thread (see \
         CONTRIBUTING.md § \"Expose a stable C ABI\"; convert to thread_local! or \
         caller-owned, or add a reasoned ALLOWED entry):\n{}",
        findings.join("\n")
    );
}

/// The allowlist is shrink-only: every entry must still name a real static, so a
/// converted or deleted cell cannot leave a stale exception behind.
#[test]
fn ffi_buffer_allowlist_entries_still_exist() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (suffix, name, _reason) in ALLOWED {
        let mut hit = false;
        let mut stack = vec![manifest_dir.join("crates"), manifest_dir.join("src")];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.to_string_lossy().ends_with(suffix)
                    && fs::read_to_string(&path).is_ok_and(|s| s.contains(name))
                {
                    hit = true;
                }
            }
        }
        assert!(hit, "stale ALLOWED entry ({suffix}, {name}): the static no longer exists — remove it");
    }
}
