//! Purpose:
//! Compiles the templates a function or method includes through a computed path into closures
//! that run in that caller's scope, so a closed-world binary renders them instead of failing the
//! include. php's own view renderers are this shape: Symfony's `HtmlErrorRenderer::include()`
//! does `extract($context); include $dir.'/Resources/'.$name;` and the template reads both the
//! extracted variables and `$this`.
//!
//! Called from:
//! - `crate::pipeline::compile()` in closed-world mode, after the literal-include units.
//!
//! Key details:
//! - An include runs in its caller's scope, which a unit function (`opcache_preload_sources`)
//!   cannot see. A SCOPED unit is a closure over the template's body taking the caller's
//!   variables as one array: `function (array $__elephc_vars) { $v = $__elephc_vars['v'] ?? null;
//!   ...template...; return 1; }`. The include site hands it `get_defined_vars()` and binds it to
//!   `$this` and the lexical class, which is what the template sees of its caller in php.
//! - The closure cannot write back into the caller. The rewrite is therefore only done where
//!   nothing could observe a write: the site is the one include of a function body, outside any
//!   loop and closure, the body holds no aliasing statement and reads its scope by no name-based
//!   function, and no variable the template mentions is mentioned by the caller from the
//!   site's statement on. Every other site keeps the ordinary include.
//! - Candidate templates are the files under the directory the site's path starts with as a
//!   literal (`dirname(__DIR__).'/Resources/'.$name` names `.../Resources/`). The site looks the
//!   run-time path up by `realpath()`; a path naming no compiled template takes the ordinary
//!   include, so a wrong guess costs a lookup, never a behaviour.
//! - Everything is built as AST; the template bodies are the files' own parsed statements.

use std::collections::BTreeSet;
use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::names::{Name, NameKind, DYNAMIC_INCLUDE_FUNCTION};
use crate::parser::ast::visit::{walk_nodes, walk_nodes_mut, Node, NodeMut};
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::span::Span;

/// The parameter a scoped unit receives its caller's variables in.
const VARS_PARAM: &str = "__elephc_vars";

/// The most files one site's directory may hold before the site is left alone: a prefix that
/// names a whole package tree is not a template directory.
const MAX_TEMPLATES_PER_SITE: usize = 64;

/// Variables php provides in every scope, which a template reads without its caller's help.
const SUPERGLOBALS: &[&str] = &[
    "GLOBALS", "_SERVER", "_GET", "_POST", "_FILES", "_COOKIE", "_SESSION", "_REQUEST", "_ENV",
];

/// Functions that read or write the scope by NAME, which a copied scope would answer wrongly.
const SCOPE_BY_NAME: &[&str] = &[
    "get_defined_vars",
    "compact",
    "extract",
    "func_get_args",
    "func_get_arg",
    "func_num_args",
    "eval",
    "parse_str",
];

/// What a rewritten site binds the unit to.
#[derive(Clone, Copy)]
enum Receiver {
    /// A method with `$this`: `Closure::bind($u, $this, self::class)`.
    Instance,
    /// A static method: `Closure::bind($u, null, self::class)`.
    Static,
    /// A free function: the unit is called unbound.
    Function,
}

/// Rewrites every eligible include site of `program` and returns the lookup functions the
/// rewritten sites call, to be spliced into the program.
pub fn rewrite_scoped_include_sites(
    program: &mut Program,
    defines: &HashSet<String>,
    known: &[crate::ir::IncludeUnit],
) -> Result<Program, CompileError> {
    let known: HashSet<PathBuf> = known.iter().map(|unit| unit.canonical_path.clone()).collect();
    let mut state = State { defines, known, lookups: HashMap::default(), functions: Vec::new() };
    rewrite_in_declarations(program, &mut state);
    if state.functions.is_empty() {
        return Ok(Vec::new());
    }
    crate::name_resolver::resolve(state.functions)
}

struct State<'a> {
    defines: &'a HashSet<String>,
    known: HashSet<PathBuf>,
    /// Lookup function already built for one template list.
    lookups: HashMap<Vec<PathBuf>, String>,
    functions: Program,
}

/// Visits every function and class method declared in `stmts`, at any statement depth.
fn rewrite_in_declarations(stmts: &mut [Stmt], state: &mut State<'_>) {
    for stmt in stmts {
        match &mut stmt.kind {
            StmtKind::FunctionDecl { params, body, .. } => {
                let params: Vec<String> = params.iter().map(|param| param.0.clone()).collect();
                rewrite_body(body, &params, Receiver::Function, state);
            }
            StmtKind::ClassDecl { methods, .. } => {
                for method in methods.iter_mut().filter(|method| method.has_body) {
                    let params: Vec<String> =
                        method.params.iter().map(|param| param.0.clone()).collect();
                    let receiver = if method.is_static { Receiver::Static } else { Receiver::Instance };
                    rewrite_body(&mut method.body, &params, receiver, state);
                }
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => rewrite_in_declarations(body, state),
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                rewrite_in_declarations(then_body, state);
                for (_, body) in elseif_clauses {
                    rewrite_in_declarations(body, state);
                }
                if let Some(body) = else_body {
                    rewrite_in_declarations(body, state);
                }
            }
            StmtKind::IfDef { then_body, else_body, .. } => {
                rewrite_in_declarations(then_body, state);
                if let Some(body) = else_body {
                    rewrite_in_declarations(body, state);
                }
            }
            _ => {}
        }
    }
}

/// The path operand of a run-time include, when `node` is one that is not `_once`.
fn include_site_path<'a>(node: Node<'a>) -> Option<&'a Expr> {
    match node {
        Node::Expr(Expr { kind: ExprKind::FunctionCall { name, args }, .. })
            if is_dynamic_include(name) =>
        {
            match args.as_slice() {
                [path, Expr { kind: ExprKind::BoolLiteral(false), .. }, _] => Some(path),
                _ => None,
            }
        }
        Node::Stmt(Stmt { kind: StmtKind::Include { path, once: false, .. }, .. }) => Some(path),
        _ => None,
    }
}

fn is_dynamic_include(name: &Name) -> bool {
    name.as_str().trim_start_matches('\\') == DYNAMIC_INCLUDE_FUNCTION
}

/// Whether `node` is any run-time include, `_once` included.
fn is_any_include(node: Node<'_>) -> bool {
    match node {
        Node::Expr(Expr { kind: ExprKind::FunctionCall { name, .. }, .. }) => is_dynamic_include(name),
        Node::Expr(Expr { kind: ExprKind::IncludeValue { .. }, .. }) => true,
        Node::Stmt(Stmt { kind: StmtKind::Include { .. }, .. }) => true,
        _ => false,
    }
}

fn count_includes(stmts: &[Stmt]) -> usize {
    let mut count = 0;
    walk_nodes(stmts, &mut |node| count += usize::from(is_any_include(node)));
    count
}

fn rewrite_body(body: &mut [Stmt], params: &[String], receiver: Receiver, state: &mut State<'_>) {
    if count_includes(body) != 1 || !body_admits_a_scoped_site(body) {
        return;
    }
    let Some(index) = body.iter().position(|stmt| count_includes(std::slice::from_ref(stmt)) == 1) else {
        return;
    };
    let mut path = None;
    walk_nodes(std::slice::from_ref(&body[index]), &mut |node| {
        if let Some(found) = include_site_path(node) {
            path = Some(found.clone());
        }
    });
    let Some(path) = path else {
        return;
    };
    // Every name the caller mentions from the site's statement on: a template mentioning one of
    // them could write it, and the caller would then read the value php leaves there.
    let mut caller_names: BTreeSet<String> = params.iter().cloned().collect();
    collect_names(&body[index..], &mut caller_names);
    let templates = candidate_templates(&path, &caller_names, state);
    if templates.is_empty() {
        return;
    }
    let lookup = lookup_function(templates, state);
    let mut done = false;
    walk_nodes_mut(std::slice::from_mut(&mut body[index]), &mut |node| {
        if done {
            return;
        }
        match node {
            NodeMut::Expr(expr) => {
                if let ExprKind::FunctionCall { name, args } = &expr.kind {
                    if is_dynamic_include(name) {
                        let required = matches!(args.get(2).map(|arg| &arg.kind), Some(ExprKind::BoolLiteral(true)));
                        *expr = scoped_site(args[0].clone(), required, &lookup, receiver, expr.span);
                        done = true;
                    }
                }
            }
            NodeMut::Stmt(stmt) => {
                if let StmtKind::Include { path, required, .. } = &stmt.kind {
                    let value = scoped_site(path.clone(), *required, &lookup, receiver, stmt.span);
                    *stmt = Stmt::new(StmtKind::ExprStmt(value), stmt.span);
                    done = true;
                }
            }
        }
    });
}

/// The site is outside every loop and closure, and the body neither aliases a variable nor
/// reaches its scope by name.
fn body_admits_a_scoped_site(body: &[Stmt]) -> bool {
    let mut admits = true;
    walk_nodes(body, &mut |node| match node {
        Node::Stmt(stmt) => match &stmt.kind {
            StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } | StmtKind::For { body, .. } => {
                admits &= count_includes(body) == 0;
            }
            StmtKind::Foreach { body, value_by_ref, .. } => {
                admits &= !*value_by_ref && count_includes(body) == 0;
            }
            StmtKind::RefAssign { .. } | StmtKind::Global { .. } | StmtKind::StaticVar { .. } => admits = false,
            _ => {}
        },
        Node::Expr(expr) => match &expr.kind {
            ExprKind::Closure { body, .. } => admits &= count_includes(body) == 0,
            // `extract()` only writes: the template is handed the scope it leaves. A read by name
            // could look for a variable the template wrote.
            ExprKind::FunctionCall { name, .. }
                if calls_scope_by_name(name) && !function_is(name, "extract") =>
            {
                admits = false
            }
            _ => {}
        },
    });
    admits
}

fn calls_scope_by_name(name: &Name) -> bool {
    SCOPE_BY_NAME.iter().any(|function| function_is(name, function))
}

/// Whether a call names `function`, namespace fallback included (`Ns\extract` is `extract`).
fn function_is(name: &Name, function: &str) -> bool {
    let called = name.as_str().trim_start_matches('\\');
    called.rsplit('\\').next().unwrap_or(called).eq_ignore_ascii_case(function)
}

/// Every variable name `stmts` mentions, closure bodies and captures included.
fn collect_names(stmts: &[Stmt], out: &mut BTreeSet<String>) {
    walk_nodes(stmts, &mut |node| match node {
        Node::Expr(expr) => match &expr.kind {
            ExprKind::Variable(name)
            | ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name)
            | ExprKind::ClosureCall { var: name, .. } => {
                out.insert(name.clone());
            }
            ExprKind::Closure { captures, .. } => out.extend(captures.iter().cloned()),
            _ => {}
        },
        Node::Stmt(stmt) => match &stmt.kind {
            StmtKind::Assign { name, .. }
            | StmtKind::TypedAssign { name, .. }
            | StmtKind::ArrayAssign { array: name, .. }
            | StmtKind::ArrayPush { array: name, .. } => {
                out.insert(name.clone());
            }
            StmtKind::Foreach { key_var, value_var, .. } => {
                out.extend(key_var.iter().cloned());
                out.insert(value_var.clone());
            }
            StmtKind::ListUnpack { vars, .. } => out.extend(vars.iter().cloned()),
            _ => {}
        },
    });
}

/// The templates under the directories the site's path starts with, each with the variables
/// it mentions, sorted by path.
fn candidate_templates(
    path: &Expr,
    caller_names: &BTreeSet<String>,
    state: &State<'_>,
) -> Vec<(PathBuf, Program, BTreeSet<String>)> {
    let mut prefixes = Vec::new();
    literal_prefixes(path, &mut prefixes);
    let mut files = BTreeSet::new();
    for prefix in prefixes {
        let Some(dir) = prefix.rfind('/').map(|at| &prefix[..=at]) else {
            continue;
        };
        let dir = Path::new(dir);
        // A package or project root is not a template directory.
        if !dir.is_absolute() || !dir.is_dir() || dir.join("composer.json").exists() {
            continue;
        }
        let mut found = Vec::new();
        collect_files(dir, &mut found);
        files.extend(found.into_iter().filter_map(|file| crate::source::canonical(&file).ok()));
    }
    if files.len() > MAX_TEMPLATES_PER_SITE {
        return Vec::new();
    }
    files
        .into_iter()
        .filter(|file| !state.known.contains(file))
        .filter_map(|file| {
            let program = template_program(&file, state.defines)?;
            if !template_is_eligible(&program) {
                return None;
            }
            let mut names = BTreeSet::new();
            collect_names(&program, &mut names);
            names.retain(|name| name != "this" && !SUPERGLOBALS.contains(&name.as_str()));
            if names.contains(VARS_PARAM) || names.iter().any(|name| caller_names.contains(name)) {
                return None;
            }
            Some((file, program, names))
        })
        .collect()
}

/// A template's statements. A file with no PHP tag at all (a stylesheet, an SVG icon) is
/// output as it is, which is what php's include of it does.
fn template_program(file: &Path, defines: &HashSet<String>) -> Option<Program> {
    let content = std::fs::read_to_string(file).ok()?;
    if !content.contains("<?") {
        return Some(vec![crate::synthetic_class::s_echo(crate::synthetic_class::e_str(&content))]);
    }
    crate::opcache_preload_sources::parse_physical_file(file, defines)
}

/// The literal text a path expression starts with, for each value it can take.
fn literal_prefixes(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Ternary { then_expr, else_expr, .. } => {
            literal_prefixes(then_expr, out);
            literal_prefixes(else_expr, out);
        }
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            literal_prefixes(value, out);
            literal_prefixes(default, out);
        }
        _ => {
            let mut parts = Vec::new();
            concat_parts(expr, &mut parts);
            let mut prefix = String::new();
            for part in parts {
                match fold_literal(part) {
                    Some(text) => prefix.push_str(&text),
                    None => break,
                }
            }
            if !prefix.is_empty() {
                out.push(prefix);
            }
        }
    }
}

fn concat_parts<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    match &expr.kind {
        ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
            concat_parts(left, out);
            concat_parts(right, out);
        }
        _ => out.push(expr),
    }
}

/// A string literal, a concatenation of them, or `dirname()` of one (`dirname(__DIR__)`).
fn fold_literal(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(text) => Some(text.clone()),
        ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
            Some(fold_literal(left)? + &fold_literal(right)?)
        }
        ExprKind::FunctionCall { name, args }
            if name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("dirname")
                && args.len() == 1 =>
        {
            let path = fold_literal(&args[0])?;
            Path::new(&path).parent().map(|parent| parent.display().to_string())
        }
        _ => None,
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|entry| entry.ok().map(|entry| entry.path())).collect();
    entries.sort();
    for path in entries {
        if out.len() > MAX_TEMPLATES_PER_SITE {
            return;
        }
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

/// A template runs the same as a closure body: it declares nothing, includes nothing, binds no
/// reference or global, and reaches its scope by no name-based function.
fn template_is_eligible(program: &[Stmt]) -> bool {
    let mut eligible = true;
    walk_nodes(program, &mut |node| match node {
        Node::Stmt(stmt) => {
            eligible &= matches!(
                stmt.kind,
                StmtKind::Echo(_)
                    | StmtKind::ExprStmt(_)
                    | StmtKind::If { .. }
                    | StmtKind::While { .. }
                    | StmtKind::DoWhile { .. }
                    | StmtKind::For { .. }
                    | StmtKind::Foreach { value_by_ref: false, .. }
                    | StmtKind::Switch { .. }
                    | StmtKind::Try { .. }
                    | StmtKind::Assign { .. }
                    | StmtKind::TypedAssign { .. }
                    | StmtKind::ArrayAssign { .. }
                    | StmtKind::ArrayPush { .. }
                    | StmtKind::NestedArrayAssign { .. }
                    | StmtKind::ListUnpack { .. }
                    | StmtKind::PropertyAssign { .. }
                    | StmtKind::PropertyArrayPush { .. }
                    | StmtKind::PropertyArrayAssign { .. }
                    | StmtKind::Return(_)
                    | StmtKind::Break(_)
                    | StmtKind::Continue(_)
                    | StmtKind::Throw(_)
                    | StmtKind::Synthetic(_)
            );
        }
        Node::Expr(expr) => match &expr.kind {
            ExprKind::IncludeValue { .. } => eligible = false,
            ExprKind::FunctionCall { name, .. } if is_dynamic_include(name) || calls_scope_by_name(name) => {
                eligible = false
            }
            _ => {}
        },
    });
    eligible
}

/// Builds (once per template list) `function <name>(string $path): ?\Closure`, which maps a
/// run-time include path onto the scoped unit of the template it names.
fn lookup_function(templates: Vec<(PathBuf, Program, BTreeSet<String>)>, state: &mut State<'_>) -> String {
    use crate::synthetic_class::{closure, e_call, e_index, e_int, e_match, e_not, e_null, e_null_coalesce, e_str, e_var, s_assign, s_return, t_array, t_class, t_nullable};
    let key: Vec<PathBuf> = templates.iter().map(|(path, _, _)| path.clone()).collect();
    if let Some(name) = state.lookups.get(&key) {
        return name.clone();
    }
    let hash = key.iter().flat_map(|path| path.as_os_str().as_encoded_bytes().iter().copied().chain([0])).fold(
        0xcbf2_9ce4_8422_2325u64,
        |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3),
    );
    let name = format!("__elephc_scoped_units_{hash:016x}");
    let arms = templates
        .into_iter()
        .map(|(path, program, names)| {
            let mut body: Vec<Stmt> = names
                .iter()
                .map(|name| s_assign(name, e_null_coalesce(e_index(e_var(VARS_PARAM), e_str(name)), e_null())))
                .collect();
            body.extend(program);
            // Falling off the end of an included file evaluates the include to 1.
            body.push(s_return(e_int(1)));
            let unit = closure().param(VARS_PARAM, t_array()).body(body).build();
            (vec![e_str(&path.display().to_string())], unit)
        })
        .collect();
    let path_param = "path";
    let body = vec![
        Stmt::new(
            StmtKind::If {
                condition: e_not(e_call("str_starts_with", vec![e_var(path_param), e_str("/")])),
                then_body: vec![s_return(e_null())],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        ),
        s_return(e_match(e_call("realpath", vec![e_var(path_param)]), arms, Some(e_null()))),
    ];
    state.functions.push(Stmt::new(
        StmtKind::FunctionDecl {
            type_params: Vec::new(),
            name: name.clone(),
            params: vec![(path_param.to_string(), Some(crate::parser::ast::TypeExpr::Str), None, false)],
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: Some(t_nullable(t_class("\\Closure"))),
            by_ref_return: false,
            body,
        },
        Span::dummy(),
    ));
    state.lookups.insert(key, name.clone());
    name
}

/// `(($u = lookup($p = <path>)) !== null) ? <bound $u>(get_defined_vars()) : include $p`.
fn scoped_site(path: Expr, required: bool, lookup: &str, receiver: Receiver, span: Span) -> Expr {
    use crate::synthetic_class::{e_assign, e_binop, e_bool, e_null, e_self_class, e_static_call, e_ternary, e_this, e_var, fq_name};
    let unit = crate::names::generated_local_name("__elephc_scoped_unit");
    let staged = crate::names::generated_local_name("__elephc_scoped_path");
    let found = e_binop(
        e_assign(
            e_var(&unit),
            Expr::new(
                ExprKind::FunctionCall { name: fq_name(lookup), args: vec![e_assign(e_var(&staged), path)] },
                span,
            ),
        ),
        BinOp::StrictNotEq,
        e_null(),
    );
    let callee = match receiver {
        Receiver::Instance => e_static_call("\\Closure", "bind", vec![e_var(&unit), e_this(), e_self_class()]),
        Receiver::Static => e_static_call("\\Closure", "bind", vec![e_var(&unit), e_null(), e_self_class()]),
        Receiver::Function => e_var(&unit),
    };
    let scope = Expr::new(ExprKind::FunctionCall { name: fq_name("get_defined_vars"), args: Vec::new() }, span);
    let call = Expr::new(ExprKind::ExprCall { callee: Box::new(callee), args: vec![scope] }, span);
    let ordinary = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec![DYNAMIC_INCLUDE_FUNCTION.to_string()]),
            args: vec![e_var(&staged), e_bool(false), e_bool(required)],
        },
        span,
    );
    e_ternary(found, call, ordinary)
}
