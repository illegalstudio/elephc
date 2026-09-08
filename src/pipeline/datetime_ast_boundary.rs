//! Purpose:
//! Structural regression guard for the DateTime direct-AST production boundary.
//!
//! Called from:
//! - `crate::pipeline::tests::datetime_production_ast_builders_do_not_parse_embedded_php()`.
//!
//! Key details:
//! - Parses Rust test targets with `syn`, excludes every item whose `cfg` predicate requires
//!   tests, and walks the real DateTime/DatePeriod/timelib production module closure plus the
//!   direct-AST builder provider imported by generated declarations.
//! - Parser-oracles remain permitted under test-only predicates; production modules may not
//!   reference PHP lexer/parser namespaces, include external source, embed a PHP opening tag,
//!   or invoke an opaque macro/attribute expansion outside the reviewed safe allowlists.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::{self, Visit};
use syn::{
    Attribute, ExprPath, File, ForeignItem, ImplItem, Item, ItemMod, ItemUse, Meta, TraitItem,
    UseTree, Visibility,
};

/// Asserts that every reachable DateTime, DatePeriod, and timelib production declaration module
/// is direct Rust AST and that each parser-backed oracle is isolated behind `cfg(test)`.
pub(super) fn assert_direct_ast_production_boundary() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let [
        datetime_facade,
        date_period_facade,
        timelib_prelude,
        pipeline,
        containers,
        reflection_owner_helpers,
        synthetic_class,
    ] = ast_boundary_roots(&source_root);

    assert_production_modules(
        &datetime_facade,
        [
            "gate",
            "generated_declarations_fallback",
            "generated_declarations_timelib",
            "generated_injection",
        ],
    );
    assert_production_modules(&date_period_facade, []);
    assert_production_modules(&timelib_prelude, ["detect", "generated_timelib"]);

    let mut visited = BTreeSet::new();
    for root in [
        &datetime_facade,
        &date_period_facade,
        &timelib_prelude,
        &containers,
        &reflection_owner_helpers,
        &synthetic_class,
    ] {
        audit_production_module_tree(root, &mut visited);
    }
    assert_known_macro_free_glob_providers(&source_root);
    audit_pipeline_injection_phase(&pipeline);

    let datetime_source = read_source(&datetime_facade);
    assert!(
        datetime_source.contains(
            "pub(crate) use generated_injection::{inject_builtin_date_period, inject_builtin_datetime};"
        ),
        "DateTime production injection must be re-exported from generated_injection"
    );
}

/// Returns every production root whose source can construct DateTime-family declarations.
///
/// The generated DateTime declarations glob-import `crate::synthetic_class::*`, so that provider
/// belongs to this closure even though Rust module traversal does not follow `use` edges.
fn ast_boundary_roots(source_root: &Path) -> [PathBuf; 7] {
    [
        source_root.join("types/checker/builtin_types/datetime.rs"),
        source_root.join("types/checker/builtin_types/date_period.rs"),
        source_root.join("tz_prelude.rs"),
        source_root.join("pipeline.rs"),
        source_root.join("types/checker/builtin_spl_classes/containers.rs"),
        source_root.join("types/checker/builtin_types/reflection/owner_helpers.rs"),
        source_root.join("synthetic_class.rs"),
    ]
}

/// Asserts the complete set of non-test external modules declared by one production facade.
fn assert_production_modules<const N: usize>(path: &Path, expected: [&str; N]) {
    let actual = production_external_modules(&parse_source(path));
    let expected = expected.into_iter().map(str::to_owned).collect::<BTreeSet<_>>();
    assert_eq!(
        actual,
        expected,
        "{} changed its production module closure; direct-AST modules must be audited or parser oracles must stay behind #[cfg(test)]",
        path.display()
    );
}

/// Returns all non-test external module declarations from one parsed Rust source file.
fn production_external_modules(source: &File) -> BTreeSet<String> {
    source
        .items
        .iter()
        .filter(|item| !is_test_only_item(item))
        .filter_map(|item| match item {
            Item::Mod(module) if module.content.is_none() => Some(module_name(module)),
            _ => None,
        })
        .collect()
}

/// Returns the semantic spelling of a Rust identifier without a raw-identifier marker.
///
/// `proc_macro2::Ident::to_string()` preserves `r#`, even though Rust resolves `r#name` as
/// `name`. Boundary comparisons must therefore use this spelling rather than `to_string()`.
fn semantic_ident_name(identifier: &syn::Ident) -> String {
    let rendered = identifier.to_string();
    rendered
        .strip_prefix("r#")
        .unwrap_or(&rendered)
        .to_owned()
}

/// Returns the semantic Rust module name used for closure membership and source-file lookup.
fn module_name(module: &ItemMod) -> String {
    semantic_ident_name(&module.ident)
}

/// Returns whether a single-segment path names `expected` after raw-identifier normalization.
fn path_is_ident(path: &syn::Path, expected: &str) -> bool {
    path.leading_colon.is_none()
        && path.segments.len() == 1
        && path
            .segments
            .first()
            .is_some_and(|segment| semantic_ident_name(&segment.ident) == expected)
}

/// Recursively audits a production module and every non-test child module it declares.
fn audit_production_module_tree(path: &Path, visited: &mut BTreeSet<PathBuf>) {
    let path = path.to_path_buf();
    if !visited.insert(path.clone()) {
        return;
    }
    let source = parse_source(&path);
    audit_syntax(&source, &path);
    audit_child_modules(&source.items, &module_directory(&path), visited);
}

/// Follows external children declared both at file scope and inside production inline modules.
fn audit_child_modules(
    items: &[Item],
    module_root: &Path,
    visited: &mut BTreeSet<PathBuf>,
) {
    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        if is_test_only_item(item) {
            continue;
        }
        if let Some((_, nested_items)) = &module.content {
            audit_child_modules(nested_items, &module_root.join(module_name(module)), visited);
        } else {
            audit_production_module_tree(&resolve_external_module(module_root, module), visited);
        }
    }
}

/// Audits the orchestration body after frontend parsing without recursing into the legitimate parser frontend.
fn audit_pipeline_injection_phase(path: &Path) {
    let source = parse_source(path);
    let compile = source
        .items
        .iter()
        .find_map(|item| match item {
            Item::Fn(function) if semantic_ident_name(&function.sig.ident) == "compile" => {
                Some(function)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{} must retain pipeline::compile for the AST boundary audit", path.display()));
    audit_block(&compile.block, path);
}

/// Parses a tracked Rust source file for the structural production-boundary audit.
fn parse_source(path: &Path) -> File {
    let source = read_source(path);
    syn::parse_file(&source)
        .unwrap_or_else(|error| panic!("{} must parse as Rust for the AST boundary audit: {error}", path.display()))
}

/// Reads a repository source file, reporting its path if the audit cannot inspect it.
fn read_source(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable for the AST boundary audit: {error}", path.display()))
}

/// Returns the conventional directory that owns a Rust file's child modules.
fn module_directory(path: &Path) -> PathBuf {
    if path.file_name().and_then(|name| name.to_str()) == Some("mod.rs") {
        return path
            .parent()
            .expect("module source must have a parent directory")
            .to_path_buf();
    }
    path.with_extension("")
}

/// Resolves a standard Rust external module and rejects path indirection from the audited closure.
fn resolve_external_module(module_root: &Path, module: &ItemMod) -> PathBuf {
    assert!(
        !module_uses_path_indirection(module),
        "{}::{} uses #[path] or cfg_attr(..., path = ...), which bypasses the direct-AST module-closure audit",
        module_root.display(),
        module.ident
    );
    let module_name = module_name(module);
    let direct = module_root.join(format!("{module_name}.rs"));
    if direct.is_file() {
        return direct;
    }
    let nested = module_root.join(module_name).join("mod.rs");
    assert!(
        nested.is_file(),
        "{}::{} must resolve to {} or {} for the direct-AST module-closure audit",
        module_root.display(),
        module.ident,
        direct.display(),
        nested.display()
    );
    nested
}

/// Returns whether a module uses any direct or conditional `#[path]` indirection.
fn module_uses_path_indirection(module: &ItemMod) -> bool {
    module.attrs.iter().any(|attribute| {
        path_is_ident(attribute.path(), "path")
            || (path_is_ident(attribute.path(), "cfg_attr")
                && matches!(&attribute.meta, Meta::List(list) if list.tokens.to_string().contains("path")))
    })
}

/// Runs syntax and import-aware parser-boundary checks over non-test Rust items only.
fn audit_syntax(source: &File, path: &Path) {
    let mut visitor = ProductionSyntaxVisitor::default();
    visitor.visit_file(source);
    assert_violations_empty(visitor, path);
}

/// Runs parser-boundary checks over one selected production block.
fn audit_block(block: &syn::Block, path: &Path) {
    let mut visitor = ProductionSyntaxVisitor::default();
    visitor.visit_block(block);
    assert_violations_empty(visitor, path);
}

/// Fails with every syntax-boundary violation accumulated for one source target.
fn assert_violations_empty(visitor: ProductionSyntaxVisitor, path: &Path) {
    assert!(
        visitor.violations.is_empty(),
        "{} violates the direct-AST production boundary: {}",
        path.display(),
        visitor.violations.into_iter().collect::<Vec<_>>().join("; ")
    );
}

/// Returns whether an item is compiled solely for the Rust test configuration.
fn is_test_only_item(item: &Item) -> bool {
    item_attributes(item).iter().any(is_test_only_attribute)
}

/// Returns whether an attribute disables its item outside the test configuration.
fn is_test_only_attribute(attribute: &Attribute) -> bool {
    path_is_ident(attribute.path(), "test")
        || (path_is_ident(attribute.path(), "cfg")
            && matches!(
                &attribute.meta,
                Meta::List(list)
                    if list
                        .parse_args::<Meta>()
                        .is_ok_and(|predicate| cfg_predicate_requires_test(&predicate))
            ))
}

/// Returns whether a parsed `cfg` predicate cannot be true without the test configuration.
fn cfg_predicate_requires_test(predicate: &Meta) -> bool {
    match predicate {
        Meta::Path(path) => path_is_ident(path, "test"),
        Meta::List(list) if path_is_ident(&list.path, "all") => {
            cfg_predicates(list).iter().any(cfg_predicate_requires_test)
        }
        Meta::List(list) if path_is_ident(&list.path, "any") => {
            let predicates = cfg_predicates(list);
            !predicates.is_empty() && predicates.iter().all(cfg_predicate_requires_test)
        }
        _ => false,
    }
}

/// Parses the nested predicates of one `cfg(all(...))` or `cfg(any(...))` expression.
fn cfg_predicates(list: &syn::MetaList) -> Vec<Meta> {
    list.parse_args_with(
        syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
    )
    .map(|predicates| predicates.into_iter().collect())
    .unwrap_or_default()
}

/// Returns attributes from any `syn::Item` variant so cfg(test) filtering applies uniformly.
fn item_attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        Item::Verbatim(_) => &[],
        _ => &[],
    }
}

/// Returns attributes from any `syn::ImplItem` variant for test-only member filtering.
fn impl_item_attributes(item: &ImplItem) -> &[Attribute] {
    match item {
        ImplItem::Const(item) => &item.attrs,
        ImplItem::Fn(item) => &item.attrs,
        ImplItem::Type(item) => &item.attrs,
        ImplItem::Macro(item) => &item.attrs,
        ImplItem::Verbatim(_) => &[],
        _ => &[],
    }
}

/// Returns attributes from any `syn::TraitItem` variant for test-only member filtering.
fn trait_item_attributes(item: &TraitItem) -> &[Attribute] {
    match item {
        TraitItem::Const(item) => &item.attrs,
        TraitItem::Fn(item) => &item.attrs,
        TraitItem::Type(item) => &item.attrs,
        TraitItem::Macro(item) => &item.attrs,
        TraitItem::Verbatim(_) => &[],
        _ => &[],
    }
}

/// Returns attributes from any `syn::ForeignItem` variant for test-only member filtering.
fn foreign_item_attributes(item: &ForeignItem) -> &[Attribute] {
    match item {
        ForeignItem::Fn(item) => &item.attrs,
        ForeignItem::Static(item) => &item.attrs,
        ForeignItem::Type(item) => &item.attrs,
        ForeignItem::Macro(item) => &item.attrs,
        ForeignItem::Verbatim(_) => &[],
        _ => &[],
    }
}

/// Collects leaf paths from a Rust `use` tree, preserving aliases and glob imports semantically.
fn use_tree_paths(tree: &UseTree, prefix: &mut Vec<String>, paths: &mut Vec<Vec<String>>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(semantic_ident_name(&path.ident));
            use_tree_paths(&path.tree, prefix, paths);
            prefix.pop();
        }
        UseTree::Name(name) => {
            prefix.push(semantic_ident_name(&name.ident));
            paths.push(prefix.clone());
            prefix.pop();
        }
        UseTree::Rename(rename) => {
            prefix.push(semantic_ident_name(&rename.ident));
            paths.push(prefix.clone());
            prefix.pop();
        }
        UseTree::Glob(_) => paths.push(prefix.clone()),
        UseTree::Group(group) => {
            for nested in &group.items {
                use_tree_paths(nested, prefix, paths);
            }
        }
    }
}

/// Returns whether one import path is the AST type namespace, the only parser import production builders may use.
fn is_parser_ast_import(path: &[String]) -> bool {
    path.starts_with(&["crate".to_string(), "parser".to_string(), "ast".to_string()])
}

/// Returns whether one expression path resolves through a PHP lexer/parser namespace instead of AST types.
fn is_php_source_namespace_path(path: &syn::Path) -> bool {
    let segments = path
        .segments
        .iter()
        .map(|segment| semantic_ident_name(&segment.ident))
        .collect::<Vec<_>>();
    let Some(namespace) = segments
        .iter()
        .position(|segment| segment == "lexer" || segment == "parser")
    else {
        return false;
    };
    segments[namespace] == "lexer"
        || segments
            .get(namespace + 1)
            .is_none_or(|segment| segment != "ast")
}

/// Returns whether raw literal bytes contain a PHP opening tag, case-insensitively.
fn contains_php_open_tag(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(bytes)
        .to_ascii_lowercase()
        .contains("<?php")
}

/// Returns whether a macro token tree names a PHP lexer/parser namespace through any grouping or attribute form.
fn macro_references_php_source_namespace(tokens: &TokenStream) -> bool {
    tokens.clone().into_iter().any(|token| match token {
        TokenTree::Ident(identifier) => {
            matches!(semantic_ident_name(&identifier).as_str(), "lexer" | "parser")
        }
        TokenTree::Group(group) => macro_references_php_source_namespace(&group.stream()),
        TokenTree::Literal(_) | TokenTree::Punct(_) => false,
    })
}

/// Returns whether a macro invocation is a deterministic compiler builtin whose expansion cannot introduce PHP parsing.
fn is_known_safe_macro(path: &syn::Path) -> bool {
    path.leading_colon.is_none()
        && path.segments.len() == 1
        && path.segments.last().is_some_and(|segment| {
            is_known_safe_macro_name(&semantic_ident_name(&segment.ident))
        })
}

/// Returns whether one unqualified macro name belongs to the audited compiler builtin allowlist.
fn is_known_safe_macro_name(name: &str) -> bool {
    matches!(
        name,
        "assert"
            | "assert_eq"
            | "debug_assert"
            | "eprintln"
            | "format"
            | "matches"
            | "panic"
            | "unreachable"
            | "vec"
    )
}

/// Returns whether an attribute is a compiler builtin without an opaque expansion surface.
fn is_known_safe_attribute(attribute: &Attribute) -> bool {
    if attribute.path().leading_colon.is_some() || attribute.path().segments.len() != 1 {
        return false;
    }
    let Some(segment) = attribute.path().segments.last() else {
        return false;
    };
    match semantic_ident_name(&segment.ident).as_str() {
        "allow" | "cfg" | "deprecated" | "doc" | "inline" | "must_use" | "repr" | "warn"
        | "deny" | "forbid" | "cold" | "non_exhaustive" | "test" => true,
        // A production cfg_attr can enable an arbitrary proc-macro attribute, so it has no
        // stable direct-AST proof unless it is refactored into an explicitly audited item.
        "cfg_attr" | "derive" => false,
        _ => false,
    }
}

/// Returns whether a production glob import comes from a reviewed macro-free type/helper namespace.
fn is_known_macro_free_glob_import(path: &[String]) -> bool {
    matches!(
        path,
        [crate_name, parser, ast]
            if crate_name == "crate" && parser == "parser" && ast == "ast"
    ) || matches!(
        path,
        [crate_name, synthetic_class] if crate_name == "crate" && synthetic_class == "synthetic_class"
    )
}

/// Verifies each retained glob provider cannot define or re-export an allowlisted macro name.
fn assert_known_macro_free_glob_providers(source_root: &Path) {
    for path in [
        source_root.join("parser/ast/mod.rs"),
        source_root.join("synthetic_class.rs"),
    ] {
        let source = parse_source(&path);
        let mut visitor = MacroProviderVisitor::default();
        visitor.visit_file(&source);
        assert!(
            visitor.violations.is_empty(),
            "{} cannot back an approved production glob import: {}",
            path.display(),
            visitor.violations.into_iter().collect::<Vec<_>>().join("; ")
        );
    }
}

/// Rejects aliases, direct imports, and unchecked globs that could shadow a macro allowlist name.
fn audit_macro_name_bindings(
    tree: &UseTree,
    prefix: &mut Vec<String>,
    violations: &mut BTreeSet<String>,
) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(semantic_ident_name(&path.ident));
            audit_macro_name_bindings(&path.tree, prefix, violations);
            prefix.pop();
        }
        UseTree::Name(name) => {
            if is_known_safe_macro_name(&semantic_ident_name(&name.ident)) {
                violations.insert(format!(
                    "imports a macro-allowlist name through {}::{}",
                    prefix.join("::"),
                    name.ident
                ));
            }
        }
        UseTree::Rename(rename) => {
            if is_known_safe_macro_name(&semantic_ident_name(&rename.rename)) {
                violations.insert(format!(
                    "aliases {}::{} to macro-allowlist name {}",
                    prefix.join("::"),
                    rename.ident,
                    rename.rename
                ));
            }
        }
        UseTree::Glob(_) if !is_known_macro_free_glob_import(prefix) => {
            violations.insert(format!(
                "imports an unchecked macro-shadowing glob from {}",
                prefix.join("::")
            ));
        }
        UseTree::Glob(_) => {}
        UseTree::Group(group) => {
            for nested in &group.items {
                audit_macro_name_bindings(nested, prefix, violations);
            }
        }
    }
}

/// Records every parser-bearing path or token stream exposed by a production attribute.
fn audit_attribute_boundary(attribute: &Attribute, violations: &mut BTreeSet<String>) {
    if is_php_source_namespace_path(attribute.path()) {
        violations.insert(format!(
            "references PHP lexer/parser namespace in a production attribute {}",
            attribute.path().segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>().join("::")
        ));
    }
    if !is_known_safe_attribute(attribute) {
        violations.insert(format!(
            "uses opaque or unapproved production attribute {}",
            attribute.path().segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>().join("::")
        ));
    }
    if let Meta::List(list) = &attribute.meta {
        if contains_php_open_tag(list.tokens.to_string().as_bytes()) {
            violations.insert("embeds a PHP opening tag in a production attribute".to_string());
        }
        if macro_references_php_source_namespace(&list.tokens) {
            violations.insert("references lexer/parser namespace in a production attribute".to_string());
        }
    }
}

/// Visits a retained glob provider and rejects macro definitions or public bindings of safe macro names.
#[derive(Default)]
struct MacroProviderVisitor {
    violations: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for MacroProviderVisitor {
    /// Rejects any local macro definition that could shadow a compiler builtin macro name.
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item
            .ident
            .as_ref()
            .is_some_and(|identifier| is_known_safe_macro_name(&semantic_ident_name(identifier)))
        {
            self.violations.insert(format!(
                "defines allowlisted macro {}",
                item.ident.as_ref().expect("checked macro identifier")
            ));
        }
        visit::visit_item_macro(self, item);
    }

    /// Rejects public re-exports that could make an opaque macro visible through an approved glob.
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        if !matches!(item.vis, Visibility::Inherited) {
            audit_macro_name_bindings(&item.tree, &mut Vec::new(), &mut self.violations);
        }
        visit::visit_item_use(self, item);
    }
}

/// Visits only production Rust items and records syntax that would reintroduce PHP parsing.
#[derive(Default)]
struct ProductionSyntaxVisitor {
    violations: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for ProductionSyntaxVisitor {
    /// Skips every test-only item before walking any contained imports, literals, or expressions.
    fn visit_item(&mut self, item: &'ast Item) {
        if is_test_only_item(item) {
            return;
        }
        visit::visit_item(self, item);
    }

    /// Skips test-only implementation members before traversing their bodies.
    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        if impl_item_attributes(item).iter().any(is_test_only_attribute) {
            return;
        }
        visit::visit_impl_item(self, item);
    }

    /// Skips test-only trait members before traversing their bodies.
    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        if trait_item_attributes(item).iter().any(is_test_only_attribute) {
            return;
        }
        visit::visit_trait_item(self, item);
    }

    /// Skips test-only foreign members before traversing their signatures and attributes.
    fn visit_foreign_item(&mut self, item: &'ast ForeignItem) {
        if foreign_item_attributes(item).iter().any(is_test_only_attribute) {
            return;
        }
        visit::visit_foreign_item(self, item);
    }

    /// Rejects lexer imports, source-inclusion macro aliases, and parser imports outside AST types.
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        let mut paths = Vec::new();
        use_tree_paths(&item.tree, &mut Vec::new(), &mut paths);
        audit_macro_name_bindings(&item.tree, &mut Vec::new(), &mut self.violations);
        for path in paths {
            if path.iter().any(|segment| segment == "lexer") {
                self.violations
                    .insert(format!("imports lexer namespace through {}", path.join("::")));
            }
            if path.iter().any(|segment| segment == "parser") && !is_parser_ast_import(&path) {
                self.violations
                    .insert(format!("imports parser namespace through {}", path.join("::")));
            }
            if path.last().is_some_and(|segment| {
                matches!(segment.as_str(), "include" | "include_str" | "include_bytes")
            }) {
                self.violations.insert(format!(
                    "imports source-inclusion macro through {}",
                    path.join("::")
                ));
            }
        }
        visit::visit_item_use(self, item);
    }

    /// Rejects PHP lexer/parser paths in both call and function-value positions.
    fn visit_expr_path(&mut self, expression: &'ast ExprPath) {
        if is_php_source_namespace_path(&expression.path) {
            self.violations
                .insert(format!("references PHP lexer/parser path {}", expression.path.segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>().join("::")));
        }
        visit::visit_expr_path(self, expression);
    }

    /// Rejects opaque attributes and parser-bearing metadata before any attribute expansion.
    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        audit_attribute_boundary(attribute, &mut self.violations);
        visit::visit_attribute(self, attribute);
    }

    /// Rejects source-inclusion and parser-bearing macro paths or token streams in every macro position.
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if is_php_source_namespace_path(&mac.path) {
            self.violations.insert(format!(
                "references PHP lexer/parser namespace in a production macro {}",
                mac.path.segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>().join("::")
            ));
        }
        if !is_known_safe_macro(&mac.path) {
            self.violations.insert(format!(
                "uses opaque or unapproved production macro {}",
                mac.path.segments.iter().map(|segment| segment.ident.to_string()).collect::<Vec<_>>().join("::")
            ));
        }
        let macro_name = mac
            .path
            .segments
            .last()
            .map(|segment| semantic_ident_name(&segment.ident));
        if macro_name.as_deref().is_some_and(|name| {
            matches!(name, "include" | "include_str" | "include_bytes")
        }) {
            self.violations
                .insert("includes external source from a production macro".to_string());
        }
        let compact = mac
            .tokens
            .to_string()
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        if contains_php_open_tag(compact.as_bytes()) {
            self.violations
                .insert("embeds a PHP opening tag in a production macro".to_string());
        }
        if macro_references_php_source_namespace(&mac.tokens) {
            self.violations
                .insert("references lexer/parser namespace in a production macro".to_string());
        }
        visit::visit_macro(self, mac);
    }

    /// Rejects PHP opening tags in production string literals while ignoring ordinary comments by construction.
    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        if contains_php_open_tag(literal.value().as_bytes()) {
            self.violations
                .insert("embeds a PHP opening tag in a production string literal".to_string());
        }
        visit::visit_lit_str(self, literal);
    }

    /// Rejects PHP opening tags in production byte-string literals.
    fn visit_lit_byte_str(&mut self, literal: &'ast syn::LitByteStr) {
        if contains_php_open_tag(&literal.value()) {
            self.violations
                .insert("embeds a PHP opening tag in a production byte-string literal".to_string());
        }
        visit::visit_lit_byte_str(self, literal);
    }

    /// Rejects PHP opening tags in production C-string literals.
    fn visit_lit_cstr(&mut self, literal: &'ast syn::LitCStr) {
        if contains_php_open_tag(literal.value().as_bytes()) {
            self.violations
                .insert("embeds a PHP opening tag in a production C-string literal".to_string());
        }
        visit::visit_lit_cstr(self, literal);
    }
}

#[cfg(test)]
mod raw_identifier_tests {
    use super::*;

    /// Keeps the direct-AST provider in the audited root set despite its `use`-edge reachability.
    #[test]
    fn synthetic_class_provider_remains_an_audited_root() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let synthetic_class = source_root.join("synthetic_class.rs");

        assert!(ast_boundary_roots(&source_root).contains(&synthetic_class));
    }

    /// Verifies raw identifiers compare by their semantic spelling rather than `r#` display text.
    #[test]
    fn raw_identifier_names_drop_the_display_marker() {
        let parser = syn::parse_str::<syn::Ident>("r#parser")
            .expect("raw parser identifier must parse");
        let lexer = syn::parse_str::<syn::Ident>("r#lexer")
            .expect("raw lexer identifier must parse");

        assert_eq!(semantic_ident_name(&parser), "parser");
        assert_eq!(semantic_ident_name(&lexer), "lexer");
    }

    /// Verifies raw parser paths trigger the same production-boundary violation as plain paths.
    #[test]
    fn raw_parser_paths_remain_boundary_violations() {
        let path = syn::parse_str::<syn::ExprPath>("crate::r#parser::parse")
            .expect("raw parser path must parse");

        assert!(is_php_source_namespace_path(&path.path));
    }

    /// Verifies raw import segments normalize before alias and source-inclusion checks consume them.
    #[test]
    fn raw_import_segments_preserve_their_semantic_names() {
        let import = syn::parse_str::<ItemUse>("use crate::r#parser::parse as frontend;")
            .expect("raw parser import must parse");
        let include = syn::parse_str::<ItemUse>("use std::r#include_str as source;")
            .expect("raw include import must parse");
        let mut paths = Vec::new();

        use_tree_paths(&import.tree, &mut Vec::new(), &mut paths);
        assert_eq!(paths, vec![vec!["crate", "parser", "parse"]]);

        paths.clear();
        use_tree_paths(&include.tree, &mut Vec::new(), &mut paths);
        assert_eq!(paths, vec![vec!["std", "include_str"]]);
    }

    /// Verifies raw module declarations collect and resolve with their semantic source-file name.
    #[test]
    fn raw_module_names_preserve_semantic_closure_paths() {
        let source = syn::parse_file("mod r#parser;")
            .expect("raw parser module declaration must parse");
        let Item::Mod(module) = source.items.into_iter().next().expect("module item expected") else {
            panic!("parsed item must be a module declaration");
        };

        assert_eq!(module_name(&module), "parser");
        assert_eq!(
            production_external_modules(&syn::parse_file("mod r#parser;").expect("module must parse")),
            BTreeSet::from(["parser".to_string()])
        );
        assert_eq!(Path::new("boundary").join(module_name(&module)), PathBuf::from("boundary/parser"));
    }

    /// Verifies raw alias targets still trigger semantic macro-allowlist shadow detection.
    #[test]
    fn raw_macro_alias_targets_remain_detectable() {
        let import = syn::parse_str::<ItemUse>("use crate::helper as r#vec;")
            .expect("raw macro alias target must parse");
        let mut violations = BTreeSet::new();

        audit_macro_name_bindings(&import.tree, &mut Vec::new(), &mut violations);

        assert!(violations.iter().any(|violation| violation.contains("macro-allowlist name")));
    }
}
