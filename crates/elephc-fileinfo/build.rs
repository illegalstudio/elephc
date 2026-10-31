//! Purpose:
//! Links optional bridge unit tests to the managed libmagic and PCRE2 archives.
//!
//! Called from:
//! - Cargo before building elephc-fileinfo.
//!
//! Key details:
//! - Ordinary staticlibs keep unresolved native symbols for final PHP-program linking.
//! - PCRE2 is libmagic's regex engine, so native tests need both managed packages.

use std::path::{Path, PathBuf};

/// Reports whether `dir` holds both required archives.
fn has_archives(dir: &Path, names: &[&str]) -> bool {
    names.iter().all(|name| dir.join(name).is_file())
}

/// Finds the newest managed pcre2 library directory beside the durable native cache.
fn find_pcre2_library() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ELEPHC_FILEINFO_PCRE2_LIB_DIR") {
        let dir = PathBuf::from(dir);
        if has_archives(&dir, &["libpcre2-posix.a", "libpcre2-8.a"]) {
            return Some(dir);
        }
    }
    let root = if let Some(root) = std::env::var_os("ELEPHC_NATIVE_CACHE") {
        PathBuf::from(root)
    } else if let Some(root) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(root).join("elephc/native")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".cache/elephc/native")
    };
    let package_root = root.join("artifacts/pcre2");
    let mut candidates = Vec::new();
    collect_library_dirs(&package_root, 0, &mut candidates);
    let target = std::env::var("TARGET").unwrap_or_default();
    candidates
        .into_iter()
        .filter_map(|path| {
            let relative = path.strip_prefix(&package_root).ok()?;
            let components: Vec<String> = relative.components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect();
            if components.len() < 7 || components[4] != target {
                return None;
            }
            let mut key: Vec<u32> = components[0].split('.')
                .filter_map(|part| part.parse().ok())
                .collect();
            key.push(components[1].strip_prefix('r')?.parse().ok()?);
            Some((key, path))
        })
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)))
        .map(|(_, path)| path)
}

/// Walks the artifact tree without following symlinks, collecting complete `lib` directories.
fn collect_library_dirs(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|name| name == "lib")
            && has_archives(&path, &["libpcre2-posix.a", "libpcre2-8.a"])
        {
            out.push(path);
            continue;
        }
        collect_library_dirs(&path, depth + 1, out);
    }
}

/// Enables native tests only when their exact managed archives are supplied.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(elephc_fileinfo_native)");
    println!("cargo:rerun-if-env-changed=ELEPHC_FILEINFO_LIBMAGIC_LIB_DIR");
    println!("cargo:rerun-if-env-changed=ELEPHC_FILEINFO_PCRE2_LIB_DIR");
    println!("cargo:rerun-if-env-changed=ELEPHC_NATIVE_CACHE");
    let Some(libmagic) = std::env::var_os("ELEPHC_FILEINFO_LIBMAGIC_LIB_DIR").map(PathBuf::from) else {
        return;
    };
    if !has_archives(&libmagic, &["libelephc_magic_database.a", "libmagic.a"]) {
        return;
    }
    let Some(pcre2) = find_pcre2_library() else { return; };
    println!("cargo:rustc-link-search=native={}", libmagic.display());
    println!("cargo:rustc-link-search=native={}", pcre2.display());
    println!("cargo:rustc-link-lib=static:-bundle=elephc_magic_database");
    println!("cargo:rustc-link-lib=static:-bundle=magic");
    println!("cargo:rustc-link-lib=static:-bundle=pcre2-posix");
    println!("cargo:rustc-link-lib=static:-bundle=pcre2-8");
    println!("cargo:rustc-cfg=elephc_fileinfo_native");
}
