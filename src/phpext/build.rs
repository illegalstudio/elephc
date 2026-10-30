//! Purpose:
//! Builds the native producer object and links it with a matched PHP SDK adapter.
//!
//! Called from:
//! - The compiler backend's PHP extension output branch.
//!
//! Key details:
//! - No runtime cache object can omit producer state isolation or output routing.
//! - All transient adapter/object files belong to one private staging directory.

use std::{collections::HashMap, path::{Path, PathBuf}, process::Command};
use crate::{codegen_support::{self, platform::Target, RuntimeFeatures}, exports::ExportedFunction};

/// Emits the combined user/runtime assembly with a bounded, request-owned state region.
pub fn assembly(user: &str, heap_size: usize, target: Target, features: RuntimeFeatures) -> Result<String, String> {
    let runtime = codegen_support::generate_phpext_runtime(heap_size, target, features);
    let runtime = super::prune::runtime(user, &runtime, target)?;
    super::state::combine(&[user, &runtime], target)
}

/// Uses the verified SDK compiler to assemble and atomically publish one extension module.
pub fn link(asm: &str, exports: &HashMap<String, ExportedFunction>, target: Target, output: &Path) -> Result<(), String> {
    if exports.is_empty() { return Err("--emit phpext requires at least one #[Export] function".into()); }
    let parent = output.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let sdk_path = std::env::var_os("ELEPHC_PHPEXT_PHP_CONFIG").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("php-config"));
    let sdk = super::PhpSdk::discover(&sdk_path, target, parent)?;
    let scratch = super::scratch::Scratch::create(parent)?;
    let source = scratch.join("native.s");
    let object = scratch.join("native.o");
    std::fs::write(&source, asm).map_err(|e| format!("write PHP native assembly: {e}"))?;
    // The native SDK was already checked for architecture and object format.
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    let words = super::flags::split_words(&cc)?;
    let mut command = Command::new(words.first().ok_or("CC is empty")?);
    command.args(&words[1..]).args(["-c", "-fPIC", "-x", "assembler"]).arg(&source).arg("-o").arg(&object);
    let result = command.output().map_err(|e| format!("start PHP native assembler: {e}"))?;
    if !result.status.success() {
        return Err(format!("PHP native assembly failed:\n{}", String::from_utf8_lossy(&result.stderr)));
    }
    let default_module = output.file_stem().and_then(|s| s.to_str()).ok_or("invalid PHP module filename")?;
    let module = std::env::var("ELEPHC_PHPEXT_MODULE_NAME").unwrap_or_else(|_| default_module.to_string());
    let version = std::env::var("ELEPHC_PHPEXT_MODULE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let mut sorted = exports.values().collect::<Vec<_>>();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let source = super::producer::render(&module, &version, &sorted)?;
    sdk.compile_extension_with_objects(&source, output, &[object])
}
