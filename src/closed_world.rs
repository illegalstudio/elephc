//! Purpose:
//! Owns the `--closed-world` build mode: "link no interpreter".
//!
//! Called from:
//! - `crate::cli` (flag and `ELEPHC_CLOSED_WORLD`), `crate::pipeline::compile()` (sets the mode),
//!   the lowering of `eval()` and runtime `include`/`require`, and the final bridge check.
//!
//! Key details:
//! - The state is thread-local like `crate::strict_php`: the front end and lowering run on the one
//!   compile thread, and codegen reads the copy the lowered module carries (`Module::closed_world`).
//! - In this mode a construct that would need the eval bridge either lowers to a native
//!   equivalent (a runtime include over the compiled-source table) or to a runtime error
//!   (`eval()` of a non-literal string), and anything left over fails the build with the reasons.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

thread_local! {
    static CLOSED_WORLD: Cell<bool> = const { Cell::new(false) };
    static USAGE_MANIFEST: RefCell<Option<Rc<UsageManifest>>> = const { RefCell::new(None) };
}

/// The first line of a usage manifest file.
pub const USAGE_MANIFEST_HEADER: &str = "elephc-usage 1";

/// What an application uses, recorded by running it under php: the class-likes it loaded and the
/// files it included.
///
/// A closed-world build roots only these instead of every class php *could* load, and a class
/// name built at run time (`new $class`, `class_exists($name)`) can only name one of them. Files
/// matter because an autoloader includes a class file before php knows whether the class can be
/// declared: a file whose parent class is missing is included, fails, and declares nothing.
#[derive(Debug, Default, Clone)]
pub struct UsageManifest {
    /// Class-likes as recorded, in file order, without a leading `\`.
    pub classes: Vec<String>,
    /// Files php included, as recorded.
    pub files: Vec<std::path::PathBuf>,
    keys: HashSet<String>,
}

impl UsageManifest {
    /// Parses a manifest: the header line, then `class <Name>` and `file <path>` lines; `#` starts
    /// a comment.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines().enumerate().map(|(n, line)| (n + 1, line.trim()));
        match lines.find(|(_, line)| !line.is_empty() && !line.starts_with('#')) {
            Some((_, USAGE_MANIFEST_HEADER)) => {}
            Some((n, other)) => {
                return Err(format!("line {n}: expected `{USAGE_MANIFEST_HEADER}`, found `{other}`"))
            }
            None => return Err(format!("empty manifest, expected `{USAGE_MANIFEST_HEADER}`")),
        }
        let mut manifest = Self::default();
        for (n, line) in lines {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match line.split_once(char::is_whitespace) {
                Some(("file", path)) => manifest.files.push(path.trim().into()),
                Some(("class", name)) => {
                    let name = name.trim().trim_start_matches('\\');
                    if name.is_empty() || name.contains(char::is_whitespace) {
                        return Err(format!("line {n}: malformed class name `{}`", name));
                    }
                    manifest.add_class(name);
                }
                _ => return Err(format!("line {n}: unknown entry `{line}`")),
            }
        }
        Ok(manifest)
    }

    /// Reads and parses the manifest at `path`.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("cannot read usage manifest {}: {err}", path.display()))?;
        Self::parse(&text).map_err(|err| format!("usage manifest {}: {err}", path.display()))
    }

    /// Adds a class-like found in one of the recorded files.
    pub fn add_class(&mut self, name: &str) {
        let name = name.trim_start_matches('\\');
        if self.keys.insert(crate::names::php_symbol_key(name)) {
            self.classes.push(name.to_string());
        }
    }

    /// Whether the application was seen loading class-like `name` (case-insensitive).
    pub fn uses_class(&self, name: &str) -> bool {
        self.keys.contains(&crate::names::php_symbol_key(name.trim_start_matches('\\')))
    }
}

/// Installs (or clears) the usage manifest of the compilation running on this thread.
pub fn set_usage_manifest(manifest: Option<UsageManifest>) {
    USAGE_MANIFEST.with(|cell| *cell.borrow_mut() = manifest.map(Rc::new));
}

/// Adds to the installed manifest the classes its recorded files declare, so a class name built at
/// run time can name them too.
pub fn add_usage_classes(names: &[String]) {
    USAGE_MANIFEST.with(|cell| {
        if let Some(manifest) = cell.borrow_mut().as_mut() {
            let manifest = Rc::make_mut(manifest);
            for name in names {
                manifest.add_class(name);
            }
        }
    });
}

/// The usage manifest of a closed-world compilation, when one was given.
pub fn usage_manifest() -> Option<Rc<UsageManifest>> {
    if !is_enabled() {
        return None;
    }
    USAGE_MANIFEST.with(|cell| cell.borrow().clone())
}

/// Enables or disables the closed-world mode for the compilation running on this thread.
pub fn set_enabled(enabled: bool) {
    CLOSED_WORLD.with(|cell| cell.set(enabled));
}

/// Whether the compilation running on this thread links no interpreter.
pub fn is_enabled() -> bool {
    CLOSED_WORLD.with(|cell| cell.get())
}

/// Whether a closed-world build can never see class-like `name`: it is neither declared in the
/// program (`declared`) nor a builtin class contract. With no autoloader or interpreter to supply
/// it, any run-time use is php's `Error: Class "X" not found`, which a guarded optional
/// dependency (`if (class_exists(...)) { return X::m(...); }`) never reaches.
pub fn class_is_absent(name: &str, declared: impl Fn(&str) -> bool) -> bool {
    let name = name.trim_start_matches('\\');
    is_enabled()
        && !name.is_empty()
        && !declared(name)
        && elephc_builtin_contract::lookup_class(name).is_none()
}

/// Whether `ELEPHC_CLOSED_WORLD` asks for the mode (`1`, `true`, `on`).
pub fn requested_by_environment() -> bool {
    matches!(
        std::env::var("ELEPHC_CLOSED_WORLD").as_deref(),
        Ok("1") | Ok("true") | Ok("on")
    )
}

/// The message a non-literal `eval()` raises in a closed-world program.
pub const EVAL_UNAVAILABLE_MESSAGE: &str = "eval() is not available in a closed-world build";

/// Builds the table a closed-world runtime include consults: one record per catalog source.
///
/// - a source the compiler already carries whole (autoloaded or preloaded declaration files)
///   is [`ClosedIncludeKind::Declarations`]: including it again runs nothing;
/// - a source with a compiled unit function runs that function ([`ClosedIncludeKind::Unit`]);
/// - every other source is part of the world but cannot be replayed from an include
///   ([`ClosedIncludeKind::NotCompiled`]); its `_once` guard still answers `true` after the
///   compiler's own inclusion ran it.
pub fn include_table(module: &crate::ir::Module) -> Vec<crate::ir::ClosedInclude> {
    use crate::ir::{ClosedInclude, ClosedIncludeKind};
    let Some(catalog) = module.source_catalog() else {
        return Vec::new();
    };
    let units: std::collections::HashMap<&std::path::Path, &str> = module
        .include_units
        .iter()
        .filter(|unit| {
            module
                .functions
                .iter()
                .any(|function| function.name.eq_ignore_ascii_case(&unit.function))
        })
        .map(|unit| (unit.canonical_path.as_path(), unit.function.as_str()))
        .collect();
    let mut table: Vec<ClosedInclude> = catalog
        .iter()
        .map(|(_, source)| {
            let kind = if let Some(function) = units.get(source.canonical_path.as_path()) {
                ClosedIncludeKind::Unit((*function).to_string())
            } else if module.preincluded_sources.contains(&source.canonical_path) {
                ClosedIncludeKind::Declarations
            } else {
                ClosedIncludeKind::NotCompiled
            };
            ClosedInclude { canonical_path: source.canonical_path.clone(), kind }
        })
        .collect();
    // A file left out of the program has no catalog entry, yet its unit answers its include: a
    // class file whose parent exists nowhere fails the way php's load of it does.
    let mut unlisted: Vec<(&std::path::Path, &str)> = units
        .iter()
        .filter(|(path, _)| !table.iter().any(|entry| entry.canonical_path == **path))
        .map(|(path, function)| (*path, *function))
        .collect();
    unlisted.sort();
    table.extend(unlisted.into_iter().map(|(path, function)| ClosedInclude {
        canonical_path: path.to_path_buf(),
        kind: ClosedIncludeKind::Unit(function.to_string()),
    }));
    table
}
