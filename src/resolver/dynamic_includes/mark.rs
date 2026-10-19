//! Purpose:
//! Tags every include/require whose path is not literal-shaped with its SITE IDENTITY: the
//! canonical file, the lexical `Class::method` (or function, or `{main}`) and a 1-based ordinal.
//!
//! Called from:
//! - `crate::magic_constants::substitute_file_and_scope_constants`, once per physical file,
//!   right after the scope pass (so `__DIR__` and friends are already plain literals).
//!
//! Key details:
//! - The lexical scope is only known while the file is still its own; the resolver inlines files
//!   into one another, so the identity is captured here and carried on the AST as a marker call
//!   `__elephc_include_site(file, scope, ordinal, line, path)` around the ORIGINAL path.
//! - The ordinal counts, in source order, the non-literal-shaped include paths of one scope. A
//!   path the resolver can still fold (a `const`, a `PHP_VERSION` ternary) is counted too, so the
//!   numbering never depends on resolver state.
//! - The marker never survives resolution: every include path goes through the resolver, which
//!   either folds it (unwrapping the marker) or lowers it through the manifest.

use std::collections::HashMap;
use std::path::Path;

use crate::magic_constants::walker::{walk_program, Pass};
use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind, MagicConstant, Program};
use crate::span::Span;

use super::MARKER;

/// Wraps each dynamic include path of `program` in a site marker.
pub(crate) fn mark_include_sites(program: Program, file_path: &Path) -> Program {
    let canonical = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    let mut declared = Declared::default();
    declared.scan(&program, "");
    let mut pass = SiteMarkPass {
        declared,
        file: canonical.display().to_string(),
        namespace: None,
        namespace_stack: Vec::new(),
        class_stack: Vec::new(),
        function_stack: Vec::new(),
        counters: HashMap::new(),
    };
    walk_program(program, &mut pass)
}

/// `stmts` with every site marker removed (each path back to what the source wrote), for
/// comparisons that must not see the site identity: `spl_autoload_unregister` matches a closure
/// against the one registered earlier, whose includes carry other ordinals and lines.
pub(crate) fn strip_include_sites(stmts: Vec<crate::parser::ast::Stmt>) -> Vec<crate::parser::ast::Stmt> {
    struct Strip;
    impl Pass for Strip {
        fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
            ExprKind::MagicConstant(mc)
        }
        fn transform_include_path(&mut self, path: Expr, _span: Span) -> Expr {
            super::unwrap_site(&path).clone()
        }
    }
    walk_program(stmts, &mut Strip)
}

/// Whether a path is built only from literals, concatenation and constant references: the
/// shapes `fold_include_path` folds without any further knowledge.
pub(super) fn is_literal_shaped(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_) | ExprKind::ConstRef(_) => true,
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => is_literal_shaped(left) && is_literal_shaped(right),
        _ => false,
    }
}

/// Version-surface names this module folds (`version_fold`); a namespace declaring one of
/// them shadows the global for its unqualified uses.
const VERSION_CONSTANTS: [&str; 5] = [
    "PHP_VERSION",
    "PHP_VERSION_ID",
    "PHP_MAJOR_VERSION",
    "PHP_MINOR_VERSION",
    "PHP_RELEASE_VERSION",
];

/// Namespaced functions and constants this FILE declares (functions lowercased; constants with
/// a lowercased namespace part, as PHP compares them).
#[derive(Default)]
struct Declared {
    functions: std::collections::HashSet<String>,
    constants: std::collections::HashSet<String>,
    /// Local names bound by `use function` (lowercased) and `use const` anywhere in the file:
    /// `use function Other\\vc as version_compare;` makes an unqualified `version_compare` mean
    /// `Other\\vc`, in the global namespace too.
    imported_functions: std::collections::HashSet<String>,
    imported_constants: std::collections::HashSet<String>,
}

impl Declared {
    /// Records the declarations of `stmts` (namespace `ns`), descending into blocks and bodies.
    fn scan(&mut self, stmts: &[crate::parser::ast::Stmt], ns: &str) {
        use crate::parser::ast::StmtKind;
        let mut ns = ns.to_string();
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::NamespaceDecl { name } => {
                    ns = name.as_ref().map(Name::as_canonical).unwrap_or_default();
                }
                StmtKind::NamespaceBlock { name, body } => {
                    let inner = name.as_ref().map(Name::as_canonical).unwrap_or_default();
                    self.scan(body, &inner);
                }
                StmtKind::FunctionDecl { name, body, .. } => {
                    self.functions.insert(qualified_key(&ns, name).to_ascii_lowercase());
                    self.scan(body, &ns);
                }
                StmtKind::ConstDecl { name, .. } => {
                    self.constants.insert(qualified_key(&ns, name));
                }
                StmtKind::UseDecl { imports } => {
                    for item in imports {
                        match item.kind {
                            crate::parser::ast::UseKind::Function => {
                                self.imported_functions.insert(item.alias.to_ascii_lowercase());
                            }
                            crate::parser::ast::UseKind::Const => {
                                self.imported_constants.insert(item.alias.clone());
                            }
                            crate::parser::ast::UseKind::Class => {}
                        }
                    }
                }
                StmtKind::ExprStmt(expr) => {
                    if let ExprKind::FunctionCall { name, args } = &expr.kind {
                        if name.parts.len() == 1 && name.parts[0].eq_ignore_ascii_case("define") {
                            if let Some(ExprKind::StringLiteral(defined)) = args.first().map(|a| &a.kind) {
                                let defined = defined.trim_start_matches('\\');
                                let (space, short) = defined.rsplit_once('\\').unwrap_or(("", defined));
                                self.constants.insert(qualified_key(space, short));
                            }
                        }
                    }
                }
                StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                    self.scan(then_body, &ns);
                    for (_, body) in elseif_clauses {
                        self.scan(body, &ns);
                    }
                    if let Some(body) = else_body {
                        self.scan(body, &ns);
                    }
                }
                StmtKind::Synthetic(body)
                | StmtKind::While { body, .. }
                | StmtKind::DoWhile { body, .. }
                | StmtKind::For { body, .. }
                | StmtKind::Foreach { body, .. } => self.scan(body, &ns),
                StmtKind::Try { try_body, catches, finally_body } => {
                    self.scan(try_body, &ns);
                    for clause in catches {
                        self.scan(&clause.body, &ns);
                    }
                    if let Some(body) = finally_body {
                        self.scan(body, &ns);
                    }
                }
                _ => {}
            }
        }
    }

    /// The version names an unqualified use inside namespace `ns` does NOT resolve to the
    /// global for with certainty, comma-separated (empty in the global namespace).
    fn shadowed_in(&self, ns: &str) -> String {
        let in_namespace = !ns.is_empty();
        let mut names: Vec<&str> = VERSION_CONSTANTS
            .iter()
            .copied()
            .filter(|c| {
                self.imported_constants.contains(*c)
                    || (in_namespace && self.constants.contains(&qualified_key(ns, c)))
            })
            .collect();
        if self.imported_functions.contains("version_compare")
            || (in_namespace
                && self.functions.contains(&format!("{}\\version_compare", ns.to_ascii_lowercase())))
        {
            names.push("version_compare");
        }
        names.join(",")
    }
}

/// `ns\name` with the namespace part lowercased (PHP compares it case-insensitively).
fn qualified_key(ns: &str, name: &str) -> String {
    if ns.is_empty() {
        name.to_string()
    } else {
        format!("{}\\{}", ns.to_ascii_lowercase(), name)
    }
}

/// Lexical scope tracker for the marking walk.
struct SiteMarkPass {
    declared: Declared,
    file: String,
    namespace: Option<String>,
    namespace_stack: Vec<Option<String>>,
    /// Enclosing class-likes (class, interface, trait, enum), innermost last, namespace-qualified.
    class_stack: Vec<String>,
    /// Enclosing functions/methods, innermost last, each paired with the class depth it opened at.
    function_stack: Vec<(String, usize)>,
    counters: HashMap<String, i64>,
}

impl SiteMarkPass {
    fn qualify(&self, name: &str) -> String {
        match self.namespace.as_deref() {
            Some(ns) if !ns.is_empty() => format!("{}\\{}", ns, name),
            _ => name.to_string(),
        }
    }

    /// `Class::method`, `ns\function` or `{main}` for the current position.
    fn scope_id(&self) -> String {
        match self.function_stack.last() {
            Some((name, class_depth)) if *class_depth > 0 && *class_depth == self.class_stack.len() => {
                format!("{}::{}", self.class_stack[class_depth - 1], name)
            }
            Some((name, _)) => self.qualify(name),
            None => "{main}".to_string(),
        }
    }
}

impl Pass for SiteMarkPass {
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    fn transform_include_path(&mut self, path: Expr, span: Span) -> Expr {
        if is_literal_shaped(&path) || super::site_of(&path).is_some() {
            return path;
        }
        let scope = self.scope_id();
        let counter = self.counters.entry(scope.clone()).or_insert(0);
        *counter += 1;
        let ordinal = *counter;
        let path_span = path.span;
        let lit = |value: String| Expr::new(ExprKind::StringLiteral(value), path_span);
        Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified(MARKER),
                args: vec![
                    lit(self.file.clone()),
                    lit(scope),
                    Expr::new(ExprKind::IntLiteral(ordinal), path_span),
                    Expr::new(ExprKind::IntLiteral(i64::from(span.line)), path_span),
                    lit(self.declared.shadowed_in(self.namespace.as_deref().unwrap_or(""))),
                    path,
                ],
            },
            path_span,
        )
    }

    fn transform_include_value(&mut self, include: Expr) -> Expr {
        let ExprKind::IncludeValue { path, once, required } = &include.kind else {
            return include;
        };
        match super::site_of(path) {
            Some(site) => super::denied_value_replacement(&site, *once, *required, include.span)
                .unwrap_or(include),
            None => include,
        }
    }

    fn enter_namespace_decl(&mut self, name: &Option<Name>) {
        self.namespace = Some(name.as_ref().map(Name::as_canonical).unwrap_or_default());
    }

    fn enter_namespace_block(&mut self, name: &Option<Name>) {
        self.namespace_stack.push(self.namespace.clone());
        self.namespace = Some(name.as_ref().map(Name::as_canonical).unwrap_or_default());
    }

    fn leave_namespace_block(&mut self) {
        if let Some(previous) = self.namespace_stack.pop() {
            self.namespace = previous;
        }
    }

    fn enter_function(&mut self, name: &str) {
        // A function declared inside a method body is still a global function.
        self.function_stack.push((name.to_string(), 0));
    }

    fn leave_function(&mut self) {
        self.function_stack.pop();
    }

    fn enter_class(&mut self, name: &str) {
        let qualified = self.qualify(name);
        self.class_stack.push(qualified);
    }

    fn leave_class(&mut self) {
        self.class_stack.pop();
    }

    fn enter_trait(&mut self, name: &str) {
        let qualified = self.qualify(name);
        self.class_stack.push(qualified);
    }

    fn leave_trait(&mut self) {
        self.class_stack.pop();
    }

    fn enter_method(&mut self, name: &str, _type_params: &[crate::parser::ast::TypeParam]) {
        self.function_stack.push((name.to_string(), self.class_stack.len()));
    }

    fn leave_method(&mut self) {
        self.function_stack.pop();
    }
}
