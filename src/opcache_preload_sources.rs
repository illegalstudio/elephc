//! Purpose:
//! Turns the file `opcache.preload` names into compile-time source, so its declarations join the
//! closed world without its startup statements running once per request.
//!
//! Called from:
//! - `crate::pipeline::compile()`, immediately before the autoload pass.
//!
//! Key details:
//! - Only the `require`/`include` operations the file performs are kept; everything else is dropped.
//! - The resolver follows each one, so the whole preloaded world arrives as ordinary declarations.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use crate::span::Span;

/// Loads the program `opcache.preload` describes, as declarations for the closed world.
///
/// php preloads at STARTUP: it runs the file once, before any request, and what survives is the
/// symbol table it built. A compiled binary has no startup — its `--web` handler re-runs the
/// top-level body for every request — so splicing the file in whole would re-run its startup work
/// on each one. Symfony's generated preload file makes that concrete and expensive: it ends with
/// `Preloader::preload($classes)`, which reflects over every class it loaded, and a request that
/// did that did not finish.
///
/// What a preload file means to a compiler is therefore its REQUIRES: the set of files whose
/// declarations become permanently available. Those are kept, in order, and everything else in the
/// file is dropped — the SAPI guard, the container's `->set()`, the `Preloader` call. The resolver
/// then follows each require exactly as it follows one written in the entry file, so the whole
/// preloaded world arrives as ordinary declarations with no per-request cost at all.
///
/// A require whose path is not statically resolvable contributes nothing rather than being guessed
/// at, which is the same position every other dynamic include is already in.
///
/// The result is DECLARATIONS ONLY. That is what survives a php preload: the symbol table it built,
/// not the work it did building it. Keeping the executable statements too was measured, and it is
/// the whole reason this filter exists -- Symfony's generated preload file ends with
/// `Preloader::preload($classes)`, which reflects over every class it loaded, and a request that
/// re-ran that did not finish (9,474 interpreter frames inside `Preloader` before the timeout).
///
/// The preload graph is resolved on its OWN pass, deliberately: sharing one pass with the entry
/// would let `require_once` mark `vendor/autoload.php` as already included, and the entry's own
/// `require_once` of it would then be skipped -- taking the composer autoloader REGISTRATION with
/// it, which is request work, not preload work. Resolving separately keeps both, and the caller
/// drops any declaration the entry already makes so nothing is declared twice.
pub fn preload_declarations(
    preload_path: &Path,
    base_dir: &Path,
    defines: &HashSet<String>,
    autoload: &crate::autoload::Registry,
) -> Result<PreloadedDeclarations, CompileError> {
    let content = crate::source::read_physical_source(preload_path).map_err(|error| {
        CompileError::new(
            Span::dummy(),
            &format!(
                "opcache.preload: cannot read '{}': {}",
                preload_path.display(),
                error
            ),
        )
    })?;
    let file_label = preload_path.display().to_string();
    let source_mode = crate::source::SourceMode::from_path(preload_path);
    let tokens = crate::lexer::tokenize_with_mode(&content, source_mode)
        .map_err(|error| error.with_file(file_label.clone()))?;
    let parsed = crate::parser::parse_with_mode(&tokens, source_mode)
        .map_err(|error| error.with_file(file_label.clone()))?;
    // `__DIR__` and `__FILE__` are substituted here, which is what makes the paths foldable:
    // Symfony writes every require as `__DIR__.'/…'` or `dirname(__DIR__, 3).'/…'`.
    let parsed =
        crate::source::finalize_physical_program(parsed, &content, preload_path, source_mode, defines)?;
    let preload_dir = preload_path.parent().unwrap_or(base_dir);
    let mut requires = include_statements(&parsed);
    let (unit_functions, mut include_units) = compile_file_units(&parsed, preload_dir, defines);
    requires.extend(unit_functions);
    let (directory_requires, directory_units, directory_unit_functions) =
        preload_directory_closure(preload_path, preload_dir, defines, &include_units);
    requires.extend(directory_requires);
    requires.extend(directory_unit_functions);
    include_units.extend(directory_units);
    // The classes the script itself names -- Symfony's generated
    // `App_Kernel*Container.preload.php` lists them as `$classes[] = '…';` after its requires --
    // join the first pass like the requires do. Only requires reach `resolved`, so a scan of it
    // alone never saw them, and a script made of nothing but that list returned here empty.
    requires.extend(named_class_requires(&[&parsed], autoload, &HashSet::new()));
    if requires.is_empty() {
        return Ok(PreloadedDeclarations::default());
    }
    // A preloaded file the compiler cannot parse is left out of the closed world rather than
    // failing the build: php's autoloader still loads its classes at run time, as it does for
    // any class the preload file did not name.
    let _skip_unparseable = crate::resolver::skip_unparseable_includes();
    let (resolved, first_pass_files, sources) =
        crate::resolver::resolve_collecting_includes_continuing(
            requires,
            preload_dir,
            defines,
        )?;
    let mut class_files: HashMap<String, String> = sources
        .class_likes
        .into_iter()
        .map(|(name, path)| (crate::names::php_symbol_key(name.trim_start_matches('\\')), path))
        .collect();
    // A file the script requires can name classes too (a wrapper script requiring Symfony's), so
    // the RESOLVED program is searched as well, and whatever it finds is resolved in a second pass
    // whose declarations are de-duplicated against the first. A named file the first pass already
    // took (one the script named itself, or a class file under the preload directory, which
    // `preload_directory_closure` requires) is not required again: the second pass's declarations
    // come back wrapped in the resolver's namespace block, where the de-duplication cannot see
    // them, and the class would be declared twice.
    let first_pass_files: HashSet<PathBuf> = first_pass_files.into_iter().collect();
    let named = named_class_requires(&[&resolved], autoload, &first_pass_files);
    let resolved = if named.is_empty() {
        resolved
    } else {
        let (extra, _, extra_sources) =
            crate::resolver::resolve_collecting_includes_continuing(
                named,
                preload_dir,
                defines,
            )?;
        for (name, path) in extra_sources.class_likes {
            class_files
                .entry(crate::names::php_symbol_key(name.trim_start_matches('\\')))
                .or_insert(path);
        }
        let already = declared_names(&resolved);
        let mut merged = resolved;
        merged.extend(without_redeclarations(extra, &already));
        merged
    };
    let resolved = crate::name_resolver::resolve(resolved)?;
    let mut compiled_whole = Vec::new();
    collect_fully_declared_sources(&resolved, &mut compiled_whole);
    Ok(PreloadedDeclarations {
        declarations: first_declaration_of_each_name(retain_declarations(resolved)),
        compiled_whole,
        include_units,
        class_files,
    })
}

/// What the `opcache.preload` graph contributed to the compilation.
#[derive(Default)]
pub struct PreloadedDeclarations {
    /// The declarations the graph supplies, to be prepended to the entry program.
    pub declarations: Program,
    /// The graph files whose WHOLE content became those declarations.
    ///
    /// [`retain_declarations`] keeps a file's declarations and drops everything else, so a file
    /// that also has executable statements is only half-compiled and must still run at runtime --
    /// `vendor/autoload.php` registers the Composer autoloader that way, and marking it
    /// already-included would lose the registration. A file that is NOTHING BUT declarations has
    /// been compiled whole, and re-executing its top level once per request only rebuilds class
    /// records the binary already carries: MEASURED on the Symfony `--web` fixture, 44.5% of a
    /// warm request is the interpreter re-executing included top levels, and
    /// `App_KernelProdContainer.php` is in that set with its 1,186 symbols already compiled in.
    pub compiled_whole: Vec<std::path::PathBuf>,
    /// The physical file declaring each class-like the graph supplied, keyed like every symbol
    /// table (`php_symbol_key`). A preloaded class php could equally have autoloaded, so this is
    /// what lets `autoload::deferral` hand one the checker refuses back to the runtime.
    pub class_files: HashMap<String, String>,
    /// The files `opcache_compile_file()` named that became callable units, each with the
    /// function that runs its top level. See [`compile_file_units`].
    pub include_units: Vec<crate::ir::IncludeUnit>,
}

/// Turns every eligible file the preload script names with `opcache_compile_file()` into a
/// function whose body is that file's top level.
///
/// php's `opcache_compile_file()` compiles a file without running it, and a later `include` of it
/// runs the cached code. A compiled binary gets the same split from a function: the unit is
/// `function __elephc_include_unit_<id>(): mixed { return require '<canonical path>'; }`, which
/// the resolver expands exactly as it expands any static value include, guard handling and
/// `return` value included. A runtime include whose path resolves to the file then calls the unit
/// instead of handing the file to the interpreter. See `lower_dynamic_include`.
///
/// An include runs in its CALLER's scope, while the unit runs in its own. The two agree only for a
/// file that cannot observe the difference, so [`unit_file_is_eligible`] admits a deliberately
/// small shape and everything else keeps being interpreted: that costs time, never correctness.
fn compile_file_units(
    program: &[Stmt],
    preload_dir: &Path,
    defines: &HashSet<String>,
) -> (Program, Vec<crate::ir::IncludeUnit>) {
    let mut targets = Vec::new();
    collect_compile_file_targets(program, &mut targets);
    let mut functions = Vec::new();
    let mut units: Vec<crate::ir::IncludeUnit> = Vec::new();
    for (argument, span) in targets {
        let Some(literal) = fold_literal_path(&argument) else {
            continue;
        };
        let path = Path::new(&literal);
        let path = if path.is_absolute() { path.to_path_buf() } else { preload_dir.join(path) };
        let Ok(canonical) = path.canonicalize() else {
            continue;
        };
        if units.iter().any(|unit| unit.canonical_path == canonical)
            || !unit_file_is_eligible(&canonical, defines)
        {
            continue;
        }
        let function = unit_function_name(&canonical);
        functions.push(unit_function_decl(&function, &canonical, span));
        units.push(crate::ir::IncludeUnit { canonical_path: canonical, function });
    }
    (functions, units)
}

/// The PHP files living next to the preload script, joined to the closed world.
///
/// A framework writes its preload script into the directory of its compiled artifacts -- Symfony
/// generates `var/cache/<env>/App_Kernel*Container.preload.php` beside the container stub, its
/// `bundles.php`, the routing caches and the dumped Twig templates -- and then `require`s those
/// artifacts at run time through paths it computes, which no static walk can follow. php compiles
/// each of them on first use; a binary that links no interpreter has to have compiled them
/// already. So every `.php` file under the preload script's directory joins the build:
///
/// - a file that is nothing but declarations (a dumped template class) is required like a file
///   the script names, so its symbols are compiled and a later `include` of it runs nothing;
/// - a file an include unit can run (the container stub, `return [...]` caches; see
///   [`unit_file_is_eligible`]) becomes a unit, so the runtime include of its path runs compiled
///   code and hands back its value;
/// - anything else is left alone, exactly as before.
///
/// The walk is sorted, so two builds of one tree compile the same program.
fn preload_directory_closure(
    preload_path: &Path,
    preload_dir: &Path,
    defines: &HashSet<String>,
    known_units: &[crate::ir::IncludeUnit],
) -> (Program, Vec<crate::ir::IncludeUnit>, Program) {
    let mut files = Vec::new();
    collect_php_files(preload_dir, &mut files);
    files.sort();
    let preload_canonical = preload_path.canonicalize().ok();
    let span = Span::dummy();
    let mut declaration_files = Vec::new();
    let mut units: Vec<crate::ir::IncludeUnit> = Vec::new();
    let mut unit_functions = Vec::new();
    // The files a unit includes itself. Symfony's container stub is a unit whose
    // `elseif (!include __DIR__.'/ContainerX/App_Kernel…Container.php')` loads the container class
    // behind a `class_exists(…, false)` guard, and a cache directory keeps the stubs of earlier
    // builds next to the live one. Requiring such a class file here as well declared it twice --
    // once from this closure, once from the stub's include -- and failed the build with
    // "Duplicate class declaration" over a stale container php never loads twice. The unit's
    // include already takes the file's declarations into the program.
    let mut unit_included: HashSet<PathBuf> = HashSet::new();
    for path in files {
        let Ok(canonical) = path.canonicalize() else {
            continue;
        };
        if Some(&canonical) == preload_canonical.as_ref()
            || known_units.iter().any(|unit| unit.canonical_path == canonical)
        {
            continue;
        }
        match classify_directory_file(&canonical, defines) {
            DirectoryFile::Declarations => declaration_files.push(canonical),
            DirectoryFile::Unit(program) => {
                crate::parser::ast::visit::walk_nodes(&program, &mut |node| {
                    let path: &Expr = match node {
                        crate::parser::ast::visit::Node::Stmt(Stmt {
                            kind: StmtKind::Include { path, .. },
                            ..
                        }) => path,
                        crate::parser::ast::visit::Node::Expr(Expr {
                            kind: ExprKind::IncludeValue { path, .. },
                            ..
                        }) => path,
                        _ => return,
                    };
                    if let Some(target) = fold_literal_path(path)
                        .and_then(|target| Path::new(&target).canonicalize().ok())
                    {
                        unit_included.insert(target);
                    }
                });
                let function = unit_function_name(&canonical);
                unit_functions.push(unit_function_decl(&function, &canonical, span));
                units.push(crate::ir::IncludeUnit { canonical_path: canonical, function });
            }
            DirectoryFile::Other => {}
        }
    }
    let requires = declaration_files
        .into_iter()
        .filter(|canonical| !unit_included.contains(canonical))
        .map(|canonical| {
            Stmt::new(
                StmtKind::Include {
                    path: Expr::new(
                        ExprKind::StringLiteral(canonical.display().to_string()),
                        span,
                    ),
                    once: true,
                    required: true,
                },
                span,
            )
        })
        .collect();
    (requires, units, unit_functions)
}

/// How [`preload_directory_closure`] treats one file.
enum DirectoryFile {
    /// Only declarations: compiled like a preloaded file.
    Declarations,
    /// Runnable as an include unit; carries the parsed file.
    Unit(Program),
    /// Neither: left to its includer, as before.
    Other,
}

/// Parses `path` once and decides how the directory closure treats it.
fn classify_directory_file(path: &Path, defines: &HashSet<String>) -> DirectoryFile {
    let Ok(content) = crate::source::read_physical_source(path) else {
        return DirectoryFile::Other;
    };
    let mode = crate::source::SourceMode::from_path(path);
    let Ok(tokens) = crate::lexer::tokenize_with_mode(&content, mode) else {
        return DirectoryFile::Other;
    };
    let Ok(parsed) = crate::parser::parse_with_mode(&tokens, mode) else {
        return DirectoryFile::Other;
    };
    let Ok(program) = crate::source::finalize_physical_program(parsed, &content, path, mode, defines) else {
        return DirectoryFile::Other;
    };
    if unit_body_is_eligible(&program) {
        DirectoryFile::Unit(program)
    } else if !program.is_empty() && top_level_is_only_declarations(&program) {
        DirectoryFile::Declarations
    } else {
        DirectoryFile::Other
    }
}

/// Whether a freshly parsed file holds nothing but declarations (`namespace`/`use` allowed).
fn top_level_is_only_declarations(body: &[Stmt]) -> bool {
    let mut declares = false;
    for stmt in body {
        match &stmt.kind {
            StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::FunctionDecl { .. } => declares = true,
            StmtKind::NamespaceDecl { .. } | StmtKind::UseDecl { .. } => {}
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                if !top_level_is_only_declarations(body) {
                    return false;
                }
                declares = true;
            }
            _ => return false,
        }
    }
    declares
}

/// Appends every `.php` file under `dir`, recursively.
fn collect_php_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect_php_files(&path, out);
        } else if kind.is_file()
            && (path.extension().is_some_and(|ext| ext == "php") || opens_with_php_tag(&path))
        {
            out.push(path);
        }
    }
}

/// Whether a file of any other name is PHP too.
///
/// `include` goes by the content: symfony/ux-live-component dumps its template map to
/// `var/cache/prod/live_components_twig_templates.map`, and a closed world that only knew `.php`
/// answered that include with a failed open.
fn opens_with_php_tag(path: &Path) -> bool {
    let mut head = [0u8; 5];
    std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut head))
        .is_ok()
        && &head == b"<?php"
}


/// Appends the argument of every `opcache_compile_file(<one argument>)` statement.
fn collect_compile_file_targets(body: &[Stmt], out: &mut Vec<(Expr, Span)>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::ExprStmt(expr) => {
                if let ExprKind::FunctionCall { name, args } = &expr.kind {
                    let is_compile_file = name
                        .last_segment()
                        .is_some_and(|base| base.eq_ignore_ascii_case("opcache_compile_file"));
                    if is_compile_file && args.len() == 1 {
                        out.push((args[0].clone(), stmt.span));
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                collect_compile_file_targets(body, out);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_compile_file_targets(then_body, out);
                for (_, body) in elseif_clauses {
                    collect_compile_file_targets(body, out);
                }
                if let Some(body) = else_body {
                    collect_compile_file_targets(body, out);
                }
            }
            _ => {}
        }
    }
}

/// Compiles a unit for every `include`/`require` in the program whose path is a literal file
/// that can run as its own function (`unit_file_is_eligible`), wherever the include sits.
///
/// The resolver inlines a top-level literal include, but one inside a function or a method stays
/// a run-time include: `Composer\InstalledVersions::getInstalled()` does
/// `self::$installed = require __DIR__ . '/installed.php';`. Off the closed world the interpreter
/// ran that file; a binary with no interpreter has to have compiled it. The unit is the same one a
/// preload's `opcache_compile_file()` gets, so the runtime include table answers it the same way.
/// `known` are the units already planned (the preload's), which are not built twice.
pub fn literal_include_units(
    program: &[Stmt],
    defines: &HashSet<String>,
    known: &[crate::ir::IncludeUnit],
) -> Result<(Program, Vec<crate::ir::IncludeUnit>), CompileError> {
    let mut targets: Vec<(Expr, Span)> = Vec::new();
    crate::parser::ast::visit::walk_nodes(program, &mut |node| match node {
        crate::parser::ast::visit::Node::Expr(expr) => match &expr.kind {
            ExprKind::IncludeValue { path, .. } => targets.push(((**path).clone(), expr.span)),
            // What the resolver leaves for an include it did not inline: the path, then the flags.
            ExprKind::FunctionCall { name, args }
                if name.as_str().trim_start_matches('\\') == crate::names::DYNAMIC_INCLUDE_FUNCTION =>
            {
                if let Some(path) = args.first() {
                    targets.push((path.clone(), expr.span));
                }
            }
            _ => {}
        },
        crate::parser::ast::visit::Node::Stmt(stmt) => {
            if let StmtKind::Include { path, .. } = &stmt.kind {
                targets.push((path.clone(), stmt.span));
            }
        }
    });
    let mut paths: Vec<(PathBuf, Span)> = Vec::new();
    for (argument, span) in targets {
        let Some(literal) = fold_literal_path(&argument) else {
            continue;
        };
        // A relative path is resolved against the include path and the working directory when
        // it runs, which a compile-time name cannot stand for.
        let path = Path::new(&literal);
        if !path.is_absolute() {
            continue;
        }
        if let Ok(canonical) = path.canonicalize() {
            paths.push((canonical, span));
        }
    }
    units_for_paths(paths, defines, known)
}

/// Compiles a unit for every constant data file of the project (`<?php return [...];`).
///
/// A closed-world binary answers a run-time include only from what it compiled, and a framework
/// computes many of those paths when it runs: Symfony's kernel requires
/// `$this->getBundlesPath()`, the intl polyfills require
/// `__DIR__.'/Resources/unidata/'.$name.'.php'` through a variable. No static reading of the
/// include site can name the file, and the alternative is php's "Failed opening required" fatal.
/// The files themselves are easy to know: one whose whole top level is a `return` of a constant
/// expression (`unit_file_is_eligible`) runs the same as a function, so each becomes a unit and
/// the include table answers it wherever the path came from. A cheap textual check skips the
/// rest of the tree before anything is parsed.
pub fn project_data_units(
    project_root: &Path,
    defines: &HashSet<String>,
    known: &[crate::ir::IncludeUnit],
) -> Result<(Program, Vec<crate::ir::IncludeUnit>), CompileError> {
    let mut files = Vec::new();
    collect_php_files(project_root, &mut files);
    files.sort();
    let paths = files
        .into_iter()
        .filter(|path| starts_with_return(path))
        .filter_map(|path| path.canonicalize().ok())
        .map(|path| (path, Span::dummy()))
        .collect();
    units_for_paths(paths, defines, known)
}

/// Whether the first statement of a PHP file is a `return` or a variable assignment, after its
/// open tag, comments, `declare` and `namespace`/`use` lines: the only shapes
/// `unit_file_is_eligible` can admit. A cheap filter before that parse, not the decision.
fn starts_with_return(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let text = String::from_utf8_lossy(&bytes);
    let Some(rest) = text.trim_start().strip_prefix("<?php") else {
        return false;
    };
    let mut rest = rest;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("//").or_else(|| rest.strip_prefix('#')) {
            rest = after.split_once('\n').map_or("", |(_, tail)| tail);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail);
        } else if ["namespace ", "use ", "declare("].iter().any(|kw| rest.starts_with(kw)) {
            rest = rest.split_once(';').map_or("", |(_, tail)| tail);
        } else {
            break;
        }
    }
    rest.starts_with('$')
        || rest.strip_prefix("return").is_some_and(|tail| tail.starts_with([' ', '(', '[', '\n', '\t']))
}

/// Builds and resolves one unit per eligible path that `known` does not already cover.
fn units_for_paths(
    paths: Vec<(PathBuf, Span)>,
    defines: &HashSet<String>,
    known: &[crate::ir::IncludeUnit],
) -> Result<(Program, Vec<crate::ir::IncludeUnit>), CompileError> {
    let mut functions = Vec::new();
    let mut units: Vec<crate::ir::IncludeUnit> = Vec::new();
    for (canonical, span) in paths {
        if known.iter().chain(units.iter()).any(|unit| unit.canonical_path == canonical)
            || !unit_file_is_eligible(&canonical, defines)
        {
            continue;
        }
        let function = unit_function_name(&canonical);
        functions.push(unit_function_decl(&function, &canonical, span));
        units.push(crate::ir::IncludeUnit { canonical_path: canonical, function });
    }
    if functions.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    // Resolved like the preload's units: the resolver inlines each unit's `require` of its file,
    // so the function returns the file's value instead of including it again at run time.
    let base_dir = units[0].canonical_path.parent().unwrap_or(Path::new("/")).to_path_buf();
    let (resolved, _, _) = crate::resolver::resolve_collecting_includes_continuing(
        functions, &base_dir, defines,
    )?;
    Ok((crate::name_resolver::resolve(resolved)?, units))
}

/// Folds a path built from string literals and `.`; `__DIR__` is a literal by this point.
fn fold_literal_path(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        ExprKind::BinaryOp {
            left,
            op: crate::parser::ast::BinOp::Concat,
            right,
        } => Some(fold_literal_path(left)? + &fold_literal_path(right)?),
        _ => None,
    }
}

/// Whether a file can run as its own function without anyone telling the difference.
///
/// Admitted: `namespace` and `use` statements, then one final `return <expression>` built only
/// from literals, arrays, constants, `X::class`/`X::CONST` of a NAMED class, `new` of a named
/// class and operators over those, plus locals the file writes before reading them and method
/// calls on those. Nothing in that shape reads a caller's variable, depends on the calling class
/// (`self`, `static`, `$this`, a closure's bound scope), declares a nameable symbol, or includes
/// another file -- which are exactly the ways an include's scope shows through.
fn unit_file_is_eligible(path: &Path, defines: &HashSet<String>) -> bool {
    parse_physical_file(path, defines).is_some_and(|program| unit_body_is_eligible(&program))
}

/// Parses one file of the tree the way an include of it would, or `None` when it does not parse.
pub(crate) fn parse_physical_file(path: &Path, defines: &HashSet<String>) -> Option<Program> {
    let content = crate::source::read_physical_source(path).ok()?;
    let mode = crate::source::SourceMode::from_path(path);
    let tokens = crate::lexer::tokenize_with_mode(&content, mode).ok()?;
    let parsed = crate::parser::parse_with_mode(&tokens, mode).ok()?;
    crate::source::finalize_physical_program(parsed, &content, path, mode, defines).ok()
}

/// The statement-level half of [`unit_file_is_eligible`].
fn unit_body_is_eligible(body: &[Stmt]) -> bool {
    let mut locals = HashSet::new();
    let mut returned = false;
    for stmt in body {
        // The parser hoists an anonymous class behind the statements of its file: it declares
        // nothing the caller can name, and its methods have their own scope.
        if matches!(&stmt.kind, StmtKind::ClassDecl { name, .. } if name.contains("class@anonymous")) {
            continue;
        }
        if returned {
            return false;
        }
        match &stmt.kind {
            StmtKind::NamespaceDecl { .. } | StmtKind::UseDecl { .. } => {}
            StmtKind::Return(Some(value)) if unit_expr_is_eligible(value, &locals) => returned = true,
            // A file-level local written before any read (Symfony's dumped translation catalogues:
            // `$catalogue = new MessageCatalogue(...); ...; return $catalogue;`). It reads nothing
            // of the caller. An include would also leave it in the caller's scope, but these files
            // hand their result back through `return`, which is what the caller reads.
            StmtKind::Assign { name, value } if unit_expr_is_eligible(value, &locals) => {
                locals.insert(name.clone());
            }
            // Guards around the value (`if (!class_exists(...)) { class_alias(...); }`): control
            // flow and calls over eligible expressions observe nothing of the caller either.
            other if unit_guard_statement_is_eligible(other, &locals) => {}
            _ => return false,
        }
    }
    returned
}

/// A statement that may precede the final `return` of a unit: an `if` chain, a call statement,
/// or a bare `return;`, built only from eligible expressions.
fn unit_guard_statement_is_eligible(kind: &StmtKind, locals: &HashSet<String>) -> bool {
    let block_ok =
        |body: &[Stmt]| body.iter().all(|stmt| unit_guard_statement_is_eligible(&stmt.kind, locals));
    match kind {
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            unit_expr_is_eligible(condition, locals)
                && block_ok(then_body)
                && elseif_clauses
                    .iter()
                    .all(|(condition, body)| unit_expr_is_eligible(condition, locals) && block_ok(body))
                && else_body.as_deref().is_none_or(block_ok)
        }
        StmtKind::ExprStmt(expr) => unit_expr_is_eligible(expr, locals),
        StmtKind::Include { path, .. } => unit_expr_is_eligible(path, locals),
        StmtKind::Return(None) => true,
        StmtKind::Return(Some(value)) => unit_expr_is_eligible(value, locals),
        _ => false,
    }
}

/// The expression-level half of [`unit_file_is_eligible`]: an allow-list, so a construct it does
/// not know keeps the file interpreted.
fn unit_expr_is_eligible(expr: &Expr, locals: &HashSet<String>) -> bool {
    use crate::parser::ast::StaticReceiver;
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_) => true,
        ExprKind::ClassConstant { receiver } | ExprKind::ScopedConstantAccess { receiver, .. } => {
            matches!(receiver, StaticReceiver::Named(_))
        }
        // A local the file itself wrote: reading it observes nothing of the caller.
        ExprKind::Variable(name) => locals.contains(name),
        ExprKind::MethodCall { object, args, .. } => {
            matches!(object.kind, ExprKind::Variable(_))
                && unit_expr_is_eligible(object, locals)
                && args.iter().all(|arg| unit_expr_is_eligible(arg, locals))
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(|item| unit_expr_is_eligible(item, locals)),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .all(|(key, value)| unit_expr_is_eligible(key, locals) && unit_expr_is_eligible(value, locals)),
        ExprKind::NewObject { args, .. } => args.iter().all(|arg| unit_expr_is_eligible(arg, locals)),
        ExprKind::Negate(inner) | ExprKind::Not(inner) => unit_expr_is_eligible(inner, locals),
        ExprKind::FunctionCall { args, .. } => args.iter().all(|arg| unit_expr_is_eligible(arg, locals)),
        // A static closure without captures that names no calling class: its body is its own
        // scope (`return static function (array $context) { return new Kernel(...); }`).
        // The check reads the body's debug form for the receiver variants, as WHOLE words: a
        // substring test also matched class names (`StaticEnvVarLoader` in framework-bundle's
        // `Resources/config/secrets.php`) and kept every such config file out of the build.
        ExprKind::Closure { is_static: true, captures, body, .. } if captures.is_empty() => {
            let text = format!("{body:?}");
            !["This", "Self_", "Static", "Parent", "ThisExpr"]
                .iter()
                .any(|word| contains_whole_word(&text, word))
        }
        ExprKind::IncludeValue { path, .. } => unit_expr_is_eligible(path, locals),
        ExprKind::BinaryOp { left, right, .. } => {
            unit_expr_is_eligible(left, locals) && unit_expr_is_eligible(right, locals)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            unit_expr_is_eligible(condition, locals)
                && unit_expr_is_eligible(then_expr, locals)
                && unit_expr_is_eligible(else_expr, locals)
        }
        _ => false,
    }
}

/// Whether `word` occurs in `text` with no identifier character on either side.
fn contains_whole_word(text: &str, word: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

/// Names a unit after its canonical path, so the same file always gets the same function.
fn unit_function_name(canonical: &Path) -> String {
    let hash = canonical
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    format!("__elephc_include_unit_{hash:016x}")
}

/// Builds `function <name>(): mixed { return require '<canonical>'; }`.
fn unit_function_decl(name: &str, canonical: &Path, span: Span) -> Stmt {
    let path = Expr::new(ExprKind::StringLiteral(canonical.display().to_string()), span);
    let require = Expr::new(
        ExprKind::IncludeValue {
            path: Box::new(path),
            once: false,
            required: true,
        },
        span,
    );
    Stmt::new(
        StmtKind::FunctionDecl {
            type_params: Vec::new(),
            name: name.to_string(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: Some(crate::parser::ast::TypeExpr::Named(crate::names::Name::unqualified(
                "mixed",
            ))),
            by_ref_return: false,
            body: vec![Stmt::new(StmtKind::Return(Some(require)), span)],
        },
        span,
    )
}

/// The refusal that names a parent class-like no file declares.
const MISSING_PARENT_REFUSAL: &str = "Unknown class referenced during inheritance flattening: ";

/// The unit of a class file whose parent no file declares.
///
/// Such a file is left out of the program, but php can still try to load it: symfony's
/// `FileLoader` probes every class under `src/`, a fixture over a dev-only bundle included. Loading
/// asks the autoloaders for the parent and then fails, and Symfony's `ClassExistenceResource` relies
/// on that first step: its autoloader is what turns the failure into a `ReflectionException`.
pub fn missing_parent_unit(canonical: &Path, parent: &str) -> (Stmt, crate::ir::IncludeUnit) {
    let parent = parent.trim().trim_start_matches('\\');
    let span = Span::dummy();
    let text = |value: String| Expr::new(ExprKind::StringLiteral(value), span);
    let autoload = Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("spl_autoload_call"),
            args: vec![text(parent.to_string())],
        },
        span,
    );
    let error = Expr::new(
        ExprKind::NewObject {
            class_name: crate::names::Name::unqualified("Error"),
            args: vec![text(format!("Class \"{parent}\" not found"))],
        },
        span,
    );
    let function = unit_function_name(canonical);
    let declaration = Stmt::new(
        StmtKind::FunctionDecl {
            type_params: Vec::new(),
            name: function.clone(),
            params: Vec::new(),
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: Some(crate::parser::ast::TypeExpr::Named(crate::names::Name::unqualified(
                "mixed",
            ))),
            by_ref_return: false,
            body: vec![
                Stmt::new(StmtKind::ExprStmt(autoload), span),
                Stmt::new(StmtKind::Throw(error), span),
            ],
        },
        span,
    );
    (declaration, crate::ir::IncludeUnit { canonical_path: canonical.to_path_buf(), function })
}

/// The parent a checker refusal says no file declares.
pub fn parent_of_missing_parent_refusal(refusal: &str) -> Option<&str> {
    refusal.split_once(MISSING_PARENT_REFUSAL).map(|(_, parent)| parent)
}

/// Appends the path of every include guard whose body is entirely declarations.
fn collect_fully_declared_sources(body: &[Stmt], out: &mut Vec<std::path::PathBuf>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::IncludeOnceGuard { source_path, body, .. } => {
                if is_entirely_declarations(body) {
                    out.push(source_path.clone());
                }
                collect_fully_declared_sources(body, out);
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                collect_fully_declared_sources(body, out)
            }
            _ => {}
        }
    }
}

/// Returns true when nothing in this statement list survives to run at runtime.
///
/// A nested include guard counts: its own file is judged separately by the walk above, and an
/// `IncludeOnceMark` is the resolver's note that a repeated include hits an already-spliced file,
/// which does nothing on its own.
fn is_entirely_declarations(body: &[Stmt]) -> bool {
    body.iter().all(|stmt| match &stmt.kind {
        StmtKind::ClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::ConstDecl { .. }
        | StmtKind::IncludeOnceMark { .. } => true,
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => is_entirely_declarations(body),
        _ => false,
    })
}

/// Turns every class the preload script NAMES into a require of the file that declares it.
///
/// php's `opcache.preload` contract is that the script names what to preload, and a class name is
/// resolved through the autoloader exactly as `Preloader::preload()` does. Reading only the
/// script's `require` statements sees half of it: Symfony's generated preload file lists **121**
/// classes as `$classes[] = 'Some\Class';` after its requires, and those never entered the closed
/// world.
///
/// MEASURED on the Symfony `--web` fixture, ONE warm request re-executed the top level of 24 files
/// through the interpreter, six of them class-only files named in exactly that list --
/// `CompiledUrlMatcher`, `UrlMatcher`, `RedirectableUrlMatcherInterface`,
/// `CompiledUrlMatcherTrait`, `RedirectableCompiledUrlMatcher`, `TwigBundle`. They are reached at
/// runtime through `new $this->options['matcher_class']` and `bundles.php`'s `new $class()`, which
/// no static walk follows, so the compiler never saw them from the entry point.
///
/// Only a literal that the autoloader can resolve becomes a require; anything else is left alone,
/// so a preload script full of ordinary strings costs nothing.
fn named_class_requires(
    programs: &[&[Stmt]],
    autoload: &crate::autoload::Registry,
    already_included: &HashSet<PathBuf>,
) -> Program {
    let mut names = Vec::new();
    for program in programs {
        collect_string_literals(program, &mut names);
    }
    let mut seen = HashSet::new();
    let mut requires = Vec::new();
    for name in names {
        let trimmed = name.trim_start_matches('\\');
        if trimmed.is_empty() || !trimmed.contains('\\') {
            continue;
        }
        let Some(path) = autoload.psr4().lookup(trimmed) else {
            continue;
        };
        if path
            .canonicalize()
            .is_ok_and(|canonical| already_included.contains(&canonical))
        {
            continue;
        }
        let path = path.display().to_string();
        if !seen.insert(path.clone()) {
            continue;
        }
        requires.push(Stmt::new(
            StmtKind::Include {
                path: Expr::new(ExprKind::StringLiteral(path), Span::dummy()),
                once: true,
                required: true,
            },
            Span::dummy(),
        ));
    }
    requires
}

/// Appends every string literal reachable in one statement list.
fn collect_string_literals(body: &[Stmt], out: &mut Vec<String>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::ExprStmt(expr) => collect_string_literals_in_expr(expr, out),
            StmtKind::ArrayPush { value, .. } => collect_string_literals_in_expr(value, out),
            StmtKind::Assign { value, .. } => collect_string_literals_in_expr(value, out),
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => collect_string_literals(body, out),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_string_literals(then_body, out);
                for (_, body) in elseif_clauses {
                    collect_string_literals(body, out);
                }
                if let Some(body) = else_body {
                    collect_string_literals(body, out);
                }
            }
            StmtKind::Foreach { body, .. } | StmtKind::While { body, .. } => {
                collect_string_literals(body, out)
            }
            _ => {}
        }
    }
}

/// Appends every string literal inside one expression.
fn collect_string_literals_in_expr(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::StringLiteral(value) => out.push(value.clone()),
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_string_literals_in_expr(item, out);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                collect_string_literals_in_expr(key, out);
                collect_string_literals_in_expr(value, out);
            }
        }
        ExprKind::FunctionCall { args, .. } => {
            for arg in args {
                collect_string_literals_in_expr(arg, out);
            }
        }
        ExprKind::MethodCall { object, args, .. } => {
            collect_string_literals_in_expr(object, out);
            for arg in args {
                collect_string_literals_in_expr(arg, out);
            }
        }
        _ => {}
    }
}

/// Keeps the declarations of one resolved program and drops everything else.
///
/// A namespace block is flattened rather than kept: its declarations have already been
/// canonicalized by the name resolver, so the block is only a scope marker by this point and
/// keeping it would re-scope the statements that follow it in the caller's program.
fn retain_declarations(program: Program) -> Program {
    let mut kept = Vec::new();
    retain_declarations_in(program, &mut kept);
    kept
}

/// Appends every declaration in one statement list to `out`, recursing into grouping statements.
fn retain_declarations_in(body: Program, out: &mut Program) {
    for stmt in body {
        match stmt.kind {
            StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::FunctionDecl { .. }
            | StmtKind::ConstDecl { .. } => out.push(stmt),
            // A function declared in a file the resolver included is renamed to a private variant
            // symbol, and these two statements are what bind the PUBLIC name to it: the group
            // emits the dispatcher, the mark stores the variant's address into it. Dropping them
            // while keeping the declaration left a body nothing could call -- Symfony's
            // `twig_is_root_form()`, declared after `FormExtension`, was undefined for every
            // caller. php binds such a function while preloading, so the binding belongs here.
            StmtKind::FunctionVariantGroup { .. } | StmtKind::FunctionVariantMark { .. } => {
                out.push(stmt)
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => retain_declarations_in(body, out),
            // A guarded declaration survives php's preload whenever its guard held at startup.
            // Keep the guard with only its declarations: folding settles a target-decided one
            // (symfony/polyfill-php86's `if (\PHP_VERSION_ID < 80600) { enum SortDirection {} }`
            // stub, which this filter used to drop whole), and `conditional_classes` lifts what
            // a runtime condition still guards. Executable statements stay dropped.
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } if guard_needs_no_dropped_statement(&condition)
                && elseif_clauses
                    .iter()
                    .all(|(condition, _)| guard_needs_no_dropped_statement(condition)) =>
            {
                let then_body = retain_declarations(then_body);
                let elseif_clauses: Vec<_> = elseif_clauses
                    .into_iter()
                    .map(|(condition, body)| (condition, retain_declarations(body)))
                    .collect();
                let else_body = else_body.map(retain_declarations);
                let declares = !then_body.is_empty()
                    || elseif_clauses.iter().any(|(_, body)| !body.is_empty())
                    || else_body.as_ref().is_some_and(|body| !body.is_empty());
                if declares {
                    out.push(Stmt {
                        kind: StmtKind::If {
                            condition,
                            then_body,
                            elseif_clauses,
                            else_body,
                        },
                        ..stmt
                    });
                }
            }
            _ => {}
        }
    }
}

/// Whether a guard's condition can be evaluated without any statement the preload filter drops.
///
/// Only literals, constants, operators and symbol-existence probes over literal names qualify:
/// Carbon's `require $transMethod->hasReturnType() ? A : B` becomes a guard over a variable the
/// filter removed, and keeping it left the condition reading an undefined `$transMethod`.
fn guard_needs_no_dropped_statement(condition: &Expr) -> bool {
    match &condition.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. } => true,
        ExprKind::Not(inner) | ExprKind::Negate(inner) => guard_needs_no_dropped_statement(inner),
        ExprKind::BinaryOp { left, right, .. } => {
            guard_needs_no_dropped_statement(left) && guard_needs_no_dropped_statement(right)
        }
        ExprKind::FunctionCall { name, args } => {
            let probe = name.last_segment().is_some_and(|base| {
                [
                    "class_exists",
                    "interface_exists",
                    "trait_exists",
                    "enum_exists",
                    "function_exists",
                    "defined",
                    "extension_loaded",
                ]
                .iter()
                .any(|probe| base.eq_ignore_ascii_case(probe))
            });
            probe && args.iter().all(guard_needs_no_dropped_statement)
        }
        _ => false,
    }
}

/// Returns the names every declaration in one program introduces.
///
/// Used by the caller to drop a preloaded declaration the entry program makes for itself: both
/// halves reach `vendor/autoload.php`, and php declares each class once.
pub fn declared_names(program: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_declared_names(program, &mut names);
    names
}

/// Appends the declaration names of one statement list, recursing into grouping statements.
fn collect_declared_names(body: &[Stmt], out: &mut HashSet<String>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. }
            | StmtKind::InterfaceDecl { name, .. }
            | StmtKind::TraitDecl { name, .. }
            | StmtKind::EnumDecl { name, .. }
            | StmtKind::FunctionDecl { name, .. }
            | StmtKind::ConstDecl { name, .. } => {
                out.insert(declaration_key(name));
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => collect_declared_names(body, out),
            // A guarded declaration is still the program's declaration of that name: the guard
            // `retain_declarations` keeps must not be declared a second time beside it.
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_declared_names(then_body, out);
                for (_, body) in elseif_clauses {
                    collect_declared_names(body, out);
                }
                if let Some(body) = else_body {
                    collect_declared_names(body, out);
                }
            }
            _ => {}
        }
    }
}

/// Drops every declaration in `preloaded` whose name `declared` already contains.
pub fn without_redeclarations(preloaded: Program, declared: &HashSet<String>) -> Program {
    preloaded
        .into_iter()
        .filter_map(|mut stmt| match stmt.kind {
            StmtKind::ClassDecl { ref name, .. }
            | StmtKind::InterfaceDecl { ref name, .. }
            | StmtKind::TraitDecl { ref name, .. }
            | StmtKind::EnumDecl { ref name, .. }
            | StmtKind::FunctionDecl { ref name, .. }
            | StmtKind::ConstDecl { ref name, .. } => {
                (!declared.contains(&declaration_key(name))).then_some(stmt)
            }
            // The binding of a variant the entry already declares is the entry's own.
            StmtKind::FunctionVariantGroup { ref variants, .. } => {
                (!variants.iter().all(|variant| declared.contains(&declaration_key(variant))))
                    .then_some(stmt)
            }
            StmtKind::FunctionVariantMark { ref variant, .. } => {
                (!declared.contains(&declaration_key(variant))).then_some(stmt)
            }
            // A kept guard (see `retain_declarations_in`) loses the declarations the entry makes
            // itself, and goes away when nothing is left in it.
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                let then_body = without_redeclarations(then_body, declared);
                let elseif_clauses: Vec<_> = elseif_clauses
                    .into_iter()
                    .map(|(condition, body)| (condition, without_redeclarations(body, declared)))
                    .collect();
                let else_body = else_body.map(|body| without_redeclarations(body, declared));
                let declares = !then_body.is_empty()
                    || elseif_clauses.iter().any(|(_, body)| !body.is_empty())
                    || else_body.as_ref().is_some_and(|body| !body.is_empty());
                stmt.kind = StmtKind::If {
                    condition,
                    then_body,
                    elseif_clauses,
                    else_body,
                };
                declares.then_some(stmt)
            }
            _ => Some(stmt),
        })
        .collect()
}

/// Keeps the first declaration of every name in `program`, in order.
///
/// One preload graph can reach the same class file twice: Symfony's preload script requires its
/// container stub, and the stub also becomes an include unit (`preload_directory_closure`) whose
/// include of the container class is resolved on its own. The entry program used to hide that,
/// because the LAST preload's declarations are filtered against an entry that declares them too;
/// any earlier preload (`--env=dev` beside `--env=prod`) reached the checker with the container
/// class twice -- "Duplicate class declaration: ContainerX\App_KernelDevContainer".
///
/// Only plain declarations take part. A variant group or mark binds a name a declaration beside it
/// already holds (the renamed body), so filtering them against `seen` would unbind the function,
/// and a kept guard is its own branch decision.
pub fn first_declaration_of_each_name(program: Program) -> Program {
    let mut seen = HashSet::new();
    program
        .into_iter()
        .filter(|stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, .. }
            | StmtKind::InterfaceDecl { name, .. }
            | StmtKind::TraitDecl { name, .. }
            | StmtKind::EnumDecl { name, .. }
            | StmtKind::FunctionDecl { name, .. }
            | StmtKind::ConstDecl { name, .. } => seen.insert(declaration_key(name)),
            _ => true,
        })
        .collect()
}

/// Canonicalizes a declaration name the way every other symbol table in the compiler keys one.
fn declaration_key(name: &str) -> String {
    crate::names::php_symbol_key(name.trim_start_matches('\\'))
}

/// Keeps one bare include statement per `require`/`include` the program performs, in source order.
fn include_statements(program: &[Stmt]) -> Program {
    let mut kept = Vec::new();
    collect_include_statements(program, &mut kept);
    kept
}

/// Walks one statement list, appending a bare include statement for every include it performs.
fn collect_include_statements(body: &[Stmt], out: &mut Program) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Include { .. } => out.push(stmt.clone()),
            // A preload file's body is a flat list in practice, but a `namespace {}` block or an
            // `if` guard around the requires costs nothing to look inside.
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                collect_include_statements(body, out);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_include_statements(then_body, out);
                for (_, body) in elseif_clauses {
                    collect_include_statements(body, out);
                }
                if let Some(body) = else_body {
                    collect_include_statements(body, out);
                }
            }
            StmtKind::ExprStmt(expr) => collect_include_exprs(expr, stmt.span, out),
            _ => {}
        }
    }
}

/// Appends a bare include statement for the include operation inside one expression, if any.
///
/// The operation is kept even when it sits inside a larger expression, because that is how a
/// generated container writes it: `(require __DIR__.'/Container.php')->set(…)` requires the file
/// and then calls a method on what it returned. The require is the part that declares something;
/// the method call is startup work, and dropping it is the point.
///
/// The shapes looked into are the ones a require is ever written in — a method call's receiver, an
/// assignment's value, an operand, a call argument. An include buried anywhere else contributes
/// nothing, which is the same answer a dynamic include already gets.
fn collect_include_exprs(expr: &Expr, span: Span, out: &mut Program) {
    match &expr.kind {
        ExprKind::IncludeValue {
            path,
            once,
            required,
        } => out.push(Stmt::new(
            StmtKind::Include {
                path: (**path).clone(),
                once: *once,
                required: *required,
            },
            span,
        )),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_include_exprs(object, span, out);
            for arg in args {
                collect_include_exprs(arg, span, out);
            }
        }
        ExprKind::Assignment { target, value, .. } => {
            collect_include_exprs(target, span, out);
            collect_include_exprs(value, span, out);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_include_exprs(left, span, out);
            collect_include_exprs(right, span, out);
        }
        ExprKind::FunctionCall { args, .. } | ExprKind::StaticMethodCall { args, .. } => {
            for arg in args {
                collect_include_exprs(arg, span, out);
            }
        }
        _ => {}
    }
}

/// Resolves the `opcache.preload` directive to a path.
///
/// A relative directive is resolved against the entry file's directory: php resolves a relative
/// `opcache.preload` against the process's working directory at startup, which is the deployment's
/// rather than the compiler's, and the entry's directory is the closest thing a compile has to it.
pub fn resolve_preload_path(directive: &str, base_dir: &Path) -> Option<PathBuf> {
    let trimmed = directive.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = PathBuf::from(trimmed);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        base_dir.join(candidate)
    };
    Some(candidate.canonicalize().unwrap_or(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a fixture the way `unit_file_is_eligible` does, without a file.
    fn program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// A static config closure is a unit even when a class it names contains a receiver word;
    /// one that really reaches `self`/`static` is not.
    #[test]
    fn a_static_closure_naming_a_static_prefixed_class_is_a_unit() {
        let config = "<?php\nreturn static function ($c) { $c->set('v', StaticEnvVarLoader::class); };";
        assert!(unit_body_is_eligible(&program(config)));
        let scoped = "<?php\nreturn static function ($c) { return static::X; };";
        assert!(!unit_body_is_eligible(&program(scoped)));
        let selfish = "<?php\nreturn static function ($c) { return self::make(); };";
        assert!(!unit_body_is_eligible(&program(selfish)));
    }

    /// Symfony's dumped translation catalogue writes its locals before reading them; a file that
    /// reads a variable of its caller, or uses one first and writes it later, stays out.
    #[test]
    fn a_file_that_writes_its_locals_before_reading_them_is_a_unit() {
        let catalogue = "<?php\nuse A\\C;\n$c = new C('fr', []);\n$e = new C('en', []);\n$c->add($e);\nreturn $c;";
        assert!(unit_body_is_eligible(&program(catalogue)));
        let reads_caller = "<?php\nreturn $context;";
        assert!(!unit_body_is_eligible(&program(reads_caller)));
        let reads_first = "<?php\n$a = $b;\n$b = 1;\nreturn $a;";
        assert!(!unit_body_is_eligible(&program(reads_first)));
        let caller_receiver = "<?php\n$x = 1;\n$this->add($x);\nreturn $x;";
        assert!(!unit_body_is_eligible(&program(caller_receiver)));
        let conditional = "<?php\nif (A) { $x = 1; }\nreturn 1;";
        assert!(!unit_body_is_eligible(&program(conditional)));
    }

    /// A dumped cache value holds anonymous classes, which the parser hoists behind the `return`;
    /// a named declaration there still keeps the file out.
    #[test]
    fn an_anonymous_class_behind_the_return_keeps_a_file_a_unit() {
        let dumped = "<?php\nreturn [new class() implements I { public function v(): mixed { return 1; } }];";
        assert!(unit_body_is_eligible(&program(dumped)));
        let declares = "<?php\nreturn 1;\nclass Named {}";
        assert!(!unit_body_is_eligible(&program(declares)));
    }
}
