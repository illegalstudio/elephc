//! Purpose:
//! Reads Composer's GENERATED eager-file list, `<vendor-dir>/composer/autoload_files.php`, so the
//! closed world carries the files the program requires unconditionally on every single run.
//!
//! Called from:
//! - `crate::autoload::index::AutoloadIndex::from_project_root()`
//!
//! Key details:
//! - The generated list is Composer's own answer: complete, DEDUPLICATED, and in dependency order.
//!   The nested `composer.json` manifests carry the same SET, but only as an unordered pile — and
//!   eager files may depend on one another having already run, so order is part of the contract.
//! - Nothing here executes PHP. It parses the generated file with the compiler's own front end and
//!   folds the two shapes Composer emits (`$vendorDir . '/pkg/file.php'` and
//!   `__DIR__ . '/..' . '/pkg/file.php'`) into absolute paths.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::parser::ast::{BinOp, Expr, ExprKind, MagicConstant, StmtKind};
use crate::resolver::path_eval::{fold_dirname, is_dirname_call};

/// Composer's default vendor directory name, overridable through `config.vendor-dir`.
const DEFAULT_VENDOR_DIR: &str = "vendor";

/// Returns the eager `files` Composer would require on every run, in Composer's own order.
///
/// Empty when the project has no generated autoloader (nothing was ever `composer install`ed),
/// when no installed package declares `autoload.files`, or when the generated file has a shape
/// this reader does not recognize. In each of those cases the caller falls back to the root
/// manifest's own `files` section, which is what it used before this existed.
pub(super) fn generated_eager_files(project_root: &Path) -> Vec<PathBuf> {
    let manifest = vendor_dir(project_root)
        .join("composer")
        .join("autoload_files.php");
    if !manifest.is_file() {
        return Vec::new();
    }
    read_generated_eager_files(&manifest)
}

/// Resolves the project's vendor directory, honouring `config.vendor-dir` in the root manifest.
fn vendor_dir(project_root: &Path) -> PathBuf {
    let configured = std::fs::read_to_string(project_root.join("composer.json"))
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|json| {
            json.get("config")?
                .get("vendor-dir")?
                .as_str()
                .map(str::to_string)
        });
    match configured {
        Some(dir) if !dir.is_empty() => project_root.join(dir),
        _ => project_root.join(DEFAULT_VENDOR_DIR),
    }
}

/// Parses one generated `autoload_files.php` and folds its array values to existing files.
fn read_generated_eager_files(manifest: &Path) -> Vec<PathBuf> {
    let Ok(content) = crate::source::read_physical_source(manifest) else {
        return Vec::new();
    };
    let mode = crate::source::SourceMode::from_path(manifest);
    let Ok(tokens) = crate::lexer::tokenize_with_mode(&content, mode) else {
        return Vec::new();
    };
    let Ok(program) = crate::parser::parse_with_mode(&tokens, mode) else {
        return Vec::new();
    };
    let manifest_dir = manifest.parent().unwrap_or(Path::new("."));
    // Composer binds `$vendorDir` and `$baseDir` before the return, and every entry is written
    // relative to one of them. Tracking the two assignments is what makes the values foldable.
    let mut bindings: HashMap<String, String> = HashMap::new();
    let mut files = Vec::new();
    for stmt in &program {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                if let Some(folded) = fold_path(value, &bindings, manifest_dir) {
                    bindings.insert(name.clone(), folded);
                }
            }
            StmtKind::Return(Some(expr)) => {
                collect_entries(expr, &bindings, manifest_dir, &mut files);
            }
            _ => {}
        }
    }
    files
}

/// Appends every foldable, existing path in the returned array literal, preserving its order.
fn collect_entries(
    expr: &Expr,
    bindings: &HashMap<String, String>,
    manifest_dir: &Path,
    files: &mut Vec<PathBuf>,
) {
    let values: Vec<&Expr> = match &expr.kind {
        ExprKind::ArrayLiteralAssoc(entries) => entries.iter().map(|(_, value)| value).collect(),
        ExprKind::ArrayLiteral(entries) => entries.iter().collect(),
        _ => return,
    };
    for value in values {
        let Some(folded) = fold_path(value, bindings, manifest_dir) else {
            continue;
        };
        let path = PathBuf::from(folded);
        if !path.is_file() {
            continue;
        }
        let canonical = path.canonicalize().unwrap_or(path);
        if !files.contains(&canonical) {
            files.push(canonical);
        }
    }
}

/// Folds one generated path expression: string literals, `__DIR__`, `dirname()`, tracked
/// variables, and concatenations of those. Anything else yields `None` and is skipped.
fn fold_path(
    expr: &Expr,
    bindings: &HashMap<String, String>,
    manifest_dir: &Path,
) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(text) => Some(text.clone()),
        ExprKind::Variable(name) => bindings.get(name).cloned(),
        ExprKind::MagicConstant(MagicConstant::Dir) => {
            Some(manifest_dir.to_string_lossy().into_owned())
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let left = fold_path(left, bindings, manifest_dir)?;
            let right = fold_path(right, bindings, manifest_dir)?;
            Some(left + &right)
        }
        ExprKind::FunctionCall { name, args } if is_dirname_call(name) => {
            let path = fold_path(args.first()?, bindings, manifest_dir)?;
            let levels = match args.get(1) {
                None => 1,
                Some(arg) => match &arg.kind {
                    ExprKind::IntLiteral(levels) => *levels,
                    _ => return None,
                },
            };
            fold_dirname(&path, levels)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fixture(tag: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "elephc-composer-files-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        Fixture(root)
    }

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// The generated list is read in ITS order, not in path order, because eager files may
    /// depend on one another having already run.
    #[test]
    fn generated_eager_files_preserve_composer_order() {
        let fixture = fixture("order");
        let root = &fixture.0;
        write(&root.join("composer.json"), "{\"autoload\":{\"psr-4\":{}}}");
        write(&root.join("vendor/z/later.php"), "<?php\n");
        write(&root.join("vendor/a/earlier.php"), "<?php\n");
        write(
            &root.join("vendor/composer/autoload_files.php"),
            "<?php\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\n\
             return array(\n  'aa' => $vendorDir . '/z/later.php',\n  \
             'bb' => $vendorDir . '/a/earlier.php',\n);\n",
        );
        let files = generated_eager_files(root);
        let names: Vec<String> = files
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["later.php".to_string(), "earlier.php".to_string()]);
    }

    /// `__DIR__ . '/..'` is the other shape Composer emits (in `autoload_static.php`), and a
    /// missing file is dropped rather than compiled.
    #[test]
    fn generated_eager_files_fold_dir_relative_entries_and_drop_absent_ones() {
        let fixture = fixture("dir");
        let root = &fixture.0;
        write(&root.join("composer.json"), "{\"autoload\":{\"psr-4\":{}}}");
        write(&root.join("vendor/pkg/present.php"), "<?php\n");
        write(
            &root.join("vendor/composer/autoload_files.php"),
            "<?php\nreturn array(\n  'aa' => __DIR__ . '/..' . '/pkg/present.php',\n  \
             'bb' => __DIR__ . '/..' . '/pkg/absent.php',\n);\n",
        );
        let files = generated_eager_files(root);
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].ends_with("pkg/present.php"), "{files:?}");
    }

    /// A configured `config.vendor-dir` moves the generated file, and a project without one
    /// contributes nothing instead of failing.
    #[test]
    fn generated_eager_files_honour_configured_vendor_dir_and_tolerate_absence() {
        let fixture = fixture("vendordir");
        let root = &fixture.0;
        write(
            &root.join("composer.json"),
            "{\"config\":{\"vendor-dir\":\"libs\"},\"autoload\":{\"psr-4\":{}}}",
        );
        assert!(generated_eager_files(root).is_empty());
        write(&root.join("libs/pkg/boot.php"), "<?php\n");
        write(
            &root.join("libs/composer/autoload_files.php"),
            "<?php\n$vendorDir = dirname(__DIR__);\nreturn array('aa' => $vendorDir . '/pkg/boot.php');\n",
        );
        let files = generated_eager_files(root);
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].ends_with("pkg/boot.php"), "{files:?}");
    }
}
