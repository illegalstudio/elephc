//! Purpose:
//! Symbolically evaluates supported `spl_autoload_register` closure bodies.
//! Derives require/include paths for candidate class names at compile time.
//!
//! Called from:
//! - `crate::autoload::rule::AutoloadRule::resolve()`
//!
//! Key details:
//! - Only a deliberate subset of PHP is foldable; unsupported constructs return `Unfoldable`.
//! - Filesystem predicates read the real compile-time filesystem, matching AOT autoload needs.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};

use super::rule::AutoloadRule;

/// Subset of PHP values the interpreter can represent.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Str(String),
    Bool(bool),
    Int(i64),
    Null,
}

impl Value {
    fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self {
            Some(s.as_str())
        } else {
            None
        }
    }

    fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Str(s) => !s.is_empty() && s != "0",
            Value::Null => false,
        }
    }
}

/// Tri-state result of executing a single statement (or block).
enum Flow {
    /// Continue executing the next statement.
    Continue,
    /// `return` was encountered. Halts the current block.
    Return,
    /// A require/include succeeded with a foldable path. Halts the closure.
    Include(PathBuf),
    /// An unsupported operation was encountered. The rule yields no path
    /// for this candidate; the caller falls back to the next rule.
    Unfoldable,
}

struct Interpreter {
    vars: HashMap<String, Value>,
}

impl Interpreter {
    fn new() -> Self {
        Interpreter {
            vars: HashMap::new(),
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> Flow {
        for stmt in stmts {
            match self.exec_stmt(stmt) {
                Flow::Continue => {}
                other => return other,
            }
        }
        Flow::Continue
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Flow {
        match &stmt.kind {
            StmtKind::Assign { name, value } => match self.eval(value) {
                Some(v) => {
                    self.vars.insert(name.clone(), v);
                    Flow::Continue
                }
                None => Flow::Unfoldable,
            },
            StmtKind::ExprStmt(expr) => match self.eval(expr) {
                Some(_) => Flow::Continue,
                None => Flow::Unfoldable,
            },
            StmtKind::Include {
                path,
                once: _,
                required: _,
            } => match self.eval(path) {
                Some(Value::Str(s)) => Flow::Include(PathBuf::from(s)),
                _ => Flow::Unfoldable,
            },
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                match self.eval(condition) {
                    Some(v) if v.truthy() => self.exec_block(then_body),
                    Some(_) => {
                        for (cond, body) in elseif_clauses {
                            match self.eval(cond) {
                                Some(v) if v.truthy() => return self.exec_block(body),
                                Some(_) => continue,
                                None => return Flow::Unfoldable,
                            }
                        }
                        match else_body {
                            Some(body) => self.exec_block(body),
                            None => Flow::Continue,
                        }
                    }
                    None => Flow::Unfoldable,
                }
            }
            StmtKind::Return(_) => Flow::Return,
            // Synthetic blocks (e.g. produced by the resolver wrapping
            // included files) are transparent.
            StmtKind::Synthetic(stmts) => self.exec_block(stmts),
            // Anything else (loops, throw, namespace, function decl, etc.)
            // is unsupported.
            _ => Flow::Unfoldable,
        }
    }

    fn eval(&mut self, expr: &Expr) -> Option<Value> {
        match &expr.kind {
            ExprKind::StringLiteral(s) => Some(Value::Str(s.clone())),
            ExprKind::IntLiteral(n) => Some(Value::Int(*n)),
            ExprKind::BoolLiteral(b) => Some(Value::Bool(*b)),
            ExprKind::Null => Some(Value::Null),
            ExprKind::Variable(name) => self.vars.get(name).cloned(),
            ExprKind::ConstRef(name) => match name.as_canonical().trim_start_matches('\\') {
                "PATHINFO_DIRNAME" => Some(Value::Int(PATHINFO_DIRNAME)),
                "PATHINFO_BASENAME" => Some(Value::Int(PATHINFO_BASENAME)),
                "PATHINFO_EXTENSION" => Some(Value::Int(PATHINFO_EXTENSION)),
                "PATHINFO_FILENAME" => Some(Value::Int(PATHINFO_FILENAME)),
                _ => None,
            },
            ExprKind::BinaryOp { left, op, right } => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                match op {
                    BinOp::Concat => {
                        let ls = value_to_string(&l)?;
                        let rs = value_to_string(&r)?;
                        Some(Value::Str(format!("{}{}", ls, rs)))
                    }
                    BinOp::Eq | BinOp::StrictEq => Some(Value::Bool(l == r)),
                    BinOp::NotEq | BinOp::StrictNotEq => Some(Value::Bool(l != r)),
                    BinOp::And => Some(Value::Bool(l.truthy() && r.truthy())),
                    BinOp::Or => Some(Value::Bool(l.truthy() || r.truthy())),
                    _ => None,
                }
            }
            ExprKind::Not(inner) => self.eval(inner).map(|v| Value::Bool(!v.truthy())),
            ExprKind::FunctionCall { name, args } => {
                let canonical = name.as_canonical();
                let trimmed = canonical.trim_start_matches('\\');
                self.eval_builtin(trimmed, args)
            }
            // Cast { String, ... } is the typical idiom for forcing a value
            // to string; route through value_to_string.
            ExprKind::Cast {
                target: crate::parser::ast::CastType::String,
                expr: inner,
            } => self.eval(inner).and_then(|v| value_to_string(&v).map(Value::Str)),
            _ => None,
        }
    }

    fn eval_builtin(&mut self, name: &str, args: &[Expr]) -> Option<Value> {
        match name {
            "str_replace" => {
                let from = self.eval(args.first()?)?;
                let to = self.eval(args.get(1)?)?;
                let hay = self.eval(args.get(2)?)?;
                let from_s = from.as_str()?;
                let to_s = to.as_str()?;
                let hay_s = hay.as_str()?;
                Some(Value::Str(hay_s.replace(from_s, to_s)))
            }
            "str_starts_with" => {
                let hay = self.eval(args.first()?)?;
                let needle = self.eval(args.get(1)?)?;
                Some(Value::Bool(
                    hay.as_str()?.starts_with(needle.as_str()?),
                ))
            }
            "str_ends_with" => {
                let hay = self.eval(args.first()?)?;
                let needle = self.eval(args.get(1)?)?;
                Some(Value::Bool(
                    hay.as_str()?.ends_with(needle.as_str()?),
                ))
            }
            "strtolower" => self
                .eval(args.first()?)
                .and_then(|v| v.as_str().map(|s| s.to_lowercase()))
                .map(Value::Str),
            "strtoupper" => self
                .eval(args.first()?)
                .and_then(|v| v.as_str().map(|s| s.to_uppercase()))
                .map(Value::Str),
            "file_exists" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                Some(Value::Bool(Path::new(path_str).exists()))
            }
            "is_file" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                Some(Value::Bool(Path::new(path_str).is_file()))
            }
            "is_readable" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                Some(Value::Bool(is_readable_path(Path::new(path_str))))
            }
            "is_dir" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                Some(Value::Bool(Path::new(path_str).is_dir()))
            }
            "sprintf" => {
                let format = self.eval(args.first()?)?;
                let format_s = format.as_str()?.to_string();
                let mut substitutions: Vec<String> = Vec::new();
                for arg in &args[1..] {
                    let v = self.eval(arg)?;
                    substitutions.push(value_to_string(&v)?);
                }
                fold_sprintf(&format_s, &substitutions).map(Value::Str)
            }
            "dirname" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                let levels = match args.get(1) {
                    None => 1,
                    Some(arg) => match self.eval(arg)? {
                        Value::Int(n) if n >= 1 => n,
                        _ => return None,
                    },
                };
                fold_dirname(path_str, levels).map(Value::Str)
            }
            "basename" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                let p = Path::new(path_str);
                p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| Value::Str(s.to_string()))
            }
            "realpath" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                Some(match Path::new(path_str).canonicalize() {
                    Ok(c) => Value::Str(c.to_string_lossy().into_owned()),
                    Err(_) => Value::Bool(false),
                })
            }
            "pathinfo" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                let flag_arg = self.eval(args.get(1)?)?;
                let flag = match flag_arg {
                    Value::Int(n) => n,
                    _ => return None,
                };
                let p = Path::new(path_str);
                let component = match flag {
                    PATHINFO_DIRNAME => p
                        .parent()
                        .and_then(|d| d.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    PATHINFO_BASENAME => p
                        .file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    PATHINFO_EXTENSION => p
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    PATHINFO_FILENAME => p
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    _ => return None,
                };
                Some(Value::Str(component))
            }
            _ => None,
        }
    }
}

// PHP `PATHINFO_*` constants. Listed here so the interpreter can fold
// `pathinfo($p, PATHINFO_EXTENSION)`-style calls without depending on a
// constants table at compile time.
const PATHINFO_DIRNAME: i64 = 1;
const PATHINFO_BASENAME: i64 = 2;
const PATHINFO_EXTENSION: i64 = 4;
const PATHINFO_FILENAME: i64 = 8;

/// Minimal `sprintf` for the autoloader use case. Supports `%s` (and the
/// `%%` literal escape) — enough for `sprintf("%s/%s.php", __DIR__, $name)`
/// patterns. Other directives (numeric width, %d, %f, …) yield None so
/// the rule falls back to the next autoload candidate.
fn fold_sprintf(format: &str, substitutions: &[String]) -> Option<String> {
    let mut out = String::new();
    let mut chars = format.chars();
    let mut next_sub = 0usize;
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '%' => out.push('%'),
            's' => {
                let sub = substitutions.get(next_sub)?;
                next_sub += 1;
                out.push_str(sub);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// `dirname(path, levels)` — strip `levels` trailing path components.
/// Levels is clamped at 1 when the input is missing or invalid (PHP
/// default).
fn fold_dirname(path: &str, levels: i64) -> Option<String> {
    let mut current = path.to_string();
    for _ in 0..levels {
        let parent = Path::new(&current).parent()?;
        let parent_str = parent.to_string_lossy().into_owned();
        if parent_str.is_empty() {
            // PHP returns "." for empty parents.
            current = ".".to_string();
        } else {
            current = parent_str;
        }
    }
    Some(current)
}

fn is_readable_path(path: &Path) -> bool {
    fs::File::open(path).is_ok() || fs::read_dir(path).is_ok()
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Str(s) => Some(s.clone()),
        Value::Int(n) => Some(n.to_string()),
        Value::Bool(true) => Some("1".to_string()),
        Value::Bool(false) => Some(String::new()),
        Value::Null => Some(String::new()),
    }
}

/// Try to resolve `class_name` through `rule`. Returns the include path
/// produced by the closure, or `None` if the rule doesn't yield one (no
/// matching require_once, condition rejected the candidate, or an
/// unsupported operation aborted evaluation).
pub fn resolve(rule: &AutoloadRule, class_name: &str) -> Option<PathBuf> {
    let mut interp = Interpreter::new();
    interp
        .vars
        .insert(rule.param_name.clone(), Value::Str(class_name.to_string()));
    match interp.exec_block(&rule.body) {
        Flow::Include(path) => Some(path),
        _ => None,
    }
}
