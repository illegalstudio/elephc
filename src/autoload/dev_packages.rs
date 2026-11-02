//! Purpose:
//! Names the installed Composer packages a test build must not autoload: `phpunit/phpunit`
//! and the packages ONLY it requires, read from `vendor/composer/installed.json` and the
//! project's own `composer.json`.
//!
//! Called from:
//! - `crate::pipeline::compile_inner` for an `elephc test` build, through
//!   `autoload::Registry::build_excluding`.
//!
//! Key details:
//! - The compiler provides `PHPUnit\Framework\TestCase` itself, so an installed PHPUnit is
//!   for php-src runs only. Left in the index, its `autoload.files` entry
//!   (`src/Framework/Assert/Functions.php`) is inlined into every build and drags the
//!   runner's own classes in through autoloading, where a runtime-dynamic `require` is a
//!   compile error. Excluding the package and its dependency closure keeps the suite's
//!   other dev dependencies (fakers, mocks) available.
//! - The closure walks the `require` maps recorded in `installed.json` so the list never
//!   has to name PHPUnit's dependencies by hand; without the file only `phpunit/phpunit`
//!   itself is excluded.
//! - A package PHPUnit SHARES with the project is kept: everything reachable from the
//!   project's own `require` / `require-dev` (and from installed packages outside PHPUnit's
//!   tree) without passing through `phpunit/phpunit` stays in the index, so a project that
//!   uses `myclabs/deep-copy` itself still autoloads it.

use std::collections::{HashSet, VecDeque};
use std::path::Path;

/// The package the compiler stands in for.
const PHPUNIT: &str = "phpunit/phpunit";

/// Returns `phpunit/phpunit` and every installed package reachable only through it: its
/// `require` closure in `<project_root>/vendor/composer/installed.json`, minus whatever the
/// project (its `composer.json` `require` / `require-dev`) or an installed package outside
/// PHPUnit's tree reaches without going through `phpunit/phpunit`.
pub fn phpunit_closure(project_root: &Path) -> HashSet<String> {
    let only_phpunit = HashSet::from([PHPUNIT.to_string()]);
    let installed = project_root.join("vendor/composer/installed.json");
    let Ok(content) = std::fs::read_to_string(&installed) else {
        return only_phpunit;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return only_phpunit;
    };
    // Composer 2 writes `{"packages": [...]}`; Composer 1 wrote the bare array.
    let packages = json
        .get("packages")
        .and_then(|packages| packages.as_array())
        .or_else(|| json.as_array())
        .cloned()
        .unwrap_or_default();
    let requires = |name: &str| -> Vec<String> {
        packages
            .iter()
            .find(|package| package.get("name").and_then(|n| n.as_str()) == Some(name))
            .map(package_requires)
            .unwrap_or_default()
    };
    let phpunit_tree = closure([PHPUNIT.to_string()], &requires, None);
    // Everything needed WITHOUT passing through PHPUnit: the project's own requirements,
    // and every installed package PHPUnit's tree does not contain (another dependency's
    // own requirements are needed whether or not the project names it).
    let mut roots = project_requires(project_root);
    for package in &packages {
        if let Some(name) = package.get("name").and_then(|n| n.as_str()) {
            if !phpunit_tree.contains(name) {
                roots.push(name.to_string());
            }
        }
    }
    let needed_elsewhere = closure(roots, &requires, Some(PHPUNIT));
    phpunit_tree
        .into_iter()
        .filter(|name| name == PHPUNIT || !needed_elsewhere.contains(name))
        .collect()
}

/// The package names reachable from `roots` through `requires`, never entering `stop`.
fn closure(
    roots: impl IntoIterator<Item = String>,
    requires: &dyn Fn(&str) -> Vec<String>,
    stop: Option<&str>,
) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut queue: VecDeque<String> = roots
        .into_iter()
        .filter(|name| Some(name.as_str()) != stop)
        .collect();
    while let Some(name) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        for dependency in requires(&name) {
            if Some(dependency.as_str()) != stop && !seen.contains(&dependency) {
                queue.push_back(dependency);
            }
        }
    }
    seen
}

/// The package requirements of one `installed.json` entry; `php`, `ext-*` and `lib-*` are
/// platform requirements and carry no `/`, so they are skipped.
fn package_requires(package: &serde_json::Value) -> Vec<String> {
    package
        .get("require")
        .and_then(|r| r.as_object())
        .map(|requires| requires.keys().filter(|name| name.contains('/')).cloned().collect())
        .unwrap_or_default()
}

/// The project's own `require` and `require-dev` package names from its `composer.json`.
fn project_requires(project_root: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(project_root.join("composer.json")) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Vec::new();
    };
    ["require", "require-dev"]
        .iter()
        .filter_map(|section| json.get(*section).and_then(|s| s.as_object()))
        .flat_map(|section| section.keys().filter(|name| name.contains('/')).cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the PHPUnit dependency closure read from `installed.json`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test writes its own `vendor/composer/installed.json` under a scratch directory.

    use super::*;

    /// Writes `installed.json` with the given packages under a fresh scratch root.
    fn project_with(packages: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "elephc_dev_packages_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("vendor/composer")).unwrap();
        std::fs::write(root.join("vendor/composer/installed.json"), packages).unwrap();
        root
    }

    /// The closure follows `require` transitively, skips platform requirements, and leaves
    /// unrelated dev packages alone.
    #[test]
    fn walks_phpunit_requires_transitively() {
        let root = project_with(
            r#"{"packages": [
                {"name": "phpunit/phpunit", "require": {"php": ">=8.3", "ext-dom": "*", "sebastian/diff": "^6", "myclabs/deep-copy": "^1"}},
                {"name": "sebastian/diff", "require": {"sebastian/exporter": "^6"}},
                {"name": "sebastian/exporter", "require": {}},
                {"name": "myclabs/deep-copy", "require": {}},
                {"name": "fakerphp/faker", "require": {"php": ">=8.1"}}
            ], "dev-package-names": ["phpunit/phpunit", "sebastian/diff", "sebastian/exporter", "myclabs/deep-copy", "fakerphp/faker"]}"#,
        );
        let closure = phpunit_closure(&root);
        let mut sorted: Vec<&str> = closure.iter().map(String::as_str).collect();
        sorted.sort();
        assert_eq!(
            sorted,
            vec!["myclabs/deep-copy", "phpunit/phpunit", "sebastian/diff", "sebastian/exporter"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A package PHPUnit shares with the project, or with another installed package, is
    /// kept; only what nothing else needs is excluded.
    #[test]
    fn keeps_packages_needed_outside_phpunit() {
        let root = project_with(
            r#"{"packages": [
                {"name": "phpunit/phpunit", "require": {"sebastian/diff": "^6", "myclabs/deep-copy": "^1", "sebastian/exporter": "^6"}},
                {"name": "sebastian/diff", "require": {}},
                {"name": "myclabs/deep-copy", "require": {}},
                {"name": "sebastian/exporter", "require": {}},
                {"name": "acme/differ", "require": {"sebastian/diff": "^6"}}
            ]}"#,
        );
        std::fs::write(
            root.join("composer.json"),
            r#"{"require": {"php": ">=8.3", "acme/differ": "^1"}, "require-dev": {"phpunit/phpunit": "^13", "myclabs/deep-copy": "^1"}}"#,
        )
        .unwrap();
        let closure = phpunit_closure(&root);
        let mut sorted: Vec<&str> = closure.iter().map(String::as_str).collect();
        sorted.sort();
        assert_eq!(sorted, vec!["phpunit/phpunit", "sebastian/exporter"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Without `installed.json` only PHPUnit itself is excluded.
    #[test]
    fn falls_back_to_phpunit_alone() {
        let root = std::env::temp_dir().join(format!("elephc_dev_packages_none_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(phpunit_closure(&root), HashSet::from([PHPUNIT.to_string()]));
        let _ = std::fs::remove_dir_all(&root);
    }
}
