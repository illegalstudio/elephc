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

use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};

use super::rule::AutoloadRule;

/// Subset of PHP values the interpreter can represent.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Str {
        value: String,
        is_class_name: bool,
    },
    Bool(bool),
    Int(i64),
    Null,
}

impl Value {
    /// Convert a Value to a string slice if it is a string variant.
    fn as_str(&self) -> Option<&str> {
        if let Value::Str { value, .. } = self {
            Some(value.as_str())
        } else {
            None
        }
    }

    /// Return true when the string originated from the SPL autoload class-name parameter.
    fn is_class_name(&self) -> bool {
        matches!(
            self,
            Value::Str {
                is_class_name: true,
                ..
            }
        )
    }

    /// Return true if the value is truthy according to PHP rules.
    fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Str { value, .. } => !value.is_empty() && value != "0",
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

/// Interpreter for symbolically evaluating autoload closure bodies.
struct Interpreter {
    vars: HashMap<String, Value>,
}

impl Interpreter {
    /// Create a new interpreter with an empty variable map.
    fn new() -> Self {
        Interpreter {
            vars: HashMap::new(),
        }
    }

    /// Execute a block of statements, returning the flow control result.
    fn exec_block(&mut self, stmts: &[Stmt]) -> Flow {
        for stmt in stmts {
            match self.exec_stmt(stmt) {
                Flow::Continue => {}
                other => return other,
            }
        }
        Flow::Continue
    }

    /// Execute a single statement and return the resulting flow control.
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
                Some(Value::Str { value, .. }) => Flow::Include(PathBuf::from(value)),
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

    /// Evaluate an expression to a Value, or return None if it cannot be folded.
    fn eval(&mut self, expr: &Expr) -> Option<Value> {
        match &expr.kind {
            ExprKind::StringLiteral(s) => Some(Value::Str {
                value: s.clone(),
                is_class_name: false,
            }),
            ExprKind::IntLiteral(n) => Some(Value::Int(*n)),
            ExprKind::BoolLiteral(b) => Some(Value::Bool(*b)),
            ExprKind::Null => Some(Value::Null),
            ExprKind::Variable(name) => self.vars.get(name).cloned(),
            ExprKind::ConstRef(name) => match name.as_canonical().trim_start_matches('\\') {
                "PATHINFO_DIRNAME" => Some(Value::Int(PATHINFO_DIRNAME)),
                "PATHINFO_BASENAME" => Some(Value::Int(PATHINFO_BASENAME)),
                "PATHINFO_EXTENSION" => Some(Value::Int(PATHINFO_EXTENSION)),
                "PATHINFO_FILENAME" => Some(Value::Int(PATHINFO_FILENAME)),
                "PHP_URL_SCHEME" => Some(Value::Int(0)),
                "PHP_URL_HOST" => Some(Value::Int(1)),
                "PHP_URL_PORT" => Some(Value::Int(2)),
                "PHP_URL_USER" => Some(Value::Int(3)),
                "PHP_URL_PASS" => Some(Value::Int(4)),
                "PHP_URL_PATH" => Some(Value::Int(5)),
                "PHP_URL_QUERY" => Some(Value::Int(6)),
                "PHP_URL_FRAGMENT" => Some(Value::Int(7)),
                _ => None,
            },
            ExprKind::BinaryOp { left, op, right } => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                match op {
                    BinOp::Concat => {
                        let ls = value_to_string(&l)?;
                        let rs = value_to_string(&r)?;
                        Some(Value::Str {
                            value: format!("{}{}", ls, rs),
                            is_class_name: false,
                        })
                    }
                    // Integer addition exists for one reason: the canonical PSR-4 leaf
                    // expression `substr($class, strrpos($class, '\\') + 1)`. `strrpos`
                    // returns `int|false`, and PHP's `false + 1` is `1` — so the operand
                    // conversion must accept booleans, not just ints, or a global-namespace
                    // class would abort the rule instead of matching PHP.
                    BinOp::Add => {
                        let ls = value_to_arith_int(&l)?;
                        let rs = value_to_arith_int(&r)?;
                        ls.checked_add(rs).map(Value::Int)
                    }
                    BinOp::Eq | BinOp::StrictEq => Some(Value::Bool(values_equal(&l, &r))),
                    BinOp::NotEq | BinOp::StrictNotEq => Some(Value::Bool(!values_equal(&l, &r))),
                    BinOp::And => Some(Value::Bool(l.truthy() && r.truthy())),
                    BinOp::Or => Some(Value::Bool(l.truthy() || r.truthy())),
                    _ => None,
                }
            }
            ExprKind::Not(inner) => self.eval(inner).map(|v| Value::Bool(!v.truthy())),
            // The parser keeps `-6` as `Negate(IntLiteral(6))`, so without this arm no
            // negative literal can be written at all and `substr`'s negative offset and
            // negative length — both ordinary PHP — would abort every rule that used one.
            // Integers only: no other unary operator folds.
            ExprKind::Negate(inner) => match self.eval(inner)? {
                Value::Int(n) => n.checked_neg().map(Value::Int),
                _ => None,
            },
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
            } => self.eval(inner).and_then(|v| {
                value_to_string(&v).map(|value| Value::Str {
                    value,
                    is_class_name: false,
                })
            }),
            _ => None,
        }
    }

    /// Evaluate a builtin function call, returning a Value if the builtin is supported.
    fn eval_builtin(&mut self, name: &str, args: &[Expr]) -> Option<Value> {
        match name {
            "str_replace" => {
                let from = self.eval(args.first()?)?;
                let to = self.eval(args.get(1)?)?;
                let hay = self.eval(args.get(2)?)?;
                let from_s = from.as_str()?;
                let to_s = to.as_str()?;
                let hay_s = hay.as_str()?;
                Some(Value::Str {
                    value: hay_s.replace(from_s, to_s),
                    is_class_name: false,
                })
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
            // `substr` and `strrpos` are the pair behind the ordinary hand-written
            // PSR-4 loader: `substr($class, strrpos($class, '\\') + 1)`.
            "substr" => {
                if args.len() > 3 {
                    return None;
                }
                let subject = self.eval(args.first()?)?;
                // PHP would coerce an int/bool/null subject to string here. The
                // interpreter aborts instead: a coerced subject never appears in a
                // real autoloader, and a wrong guess picks a wrong file.
                let subject_s = subject.as_str()?.to_string();
                let offset = match self.eval(args.get(1)?)? {
                    Value::Int(n) => n,
                    _ => return None,
                };
                let length = match args.get(2) {
                    None => None,
                    // An explicit `null` length means "to the end", same as omitting it.
                    Some(arg) => match self.eval(arg)? {
                        Value::Null => None,
                        Value::Int(n) => Some(n),
                        _ => return None,
                    },
                };
                fold_substr(&subject_s, offset, length).map(|value| Value::Str {
                    value,
                    // Every derived string in this interpreter clears `is_class_name`,
                    // and `substr` follows that rule even though a leaf name arguably
                    // still *is* a class name. `is_class_name` only widens `==` to
                    // PHP's case-insensitive class lookup; a substring is no longer a
                    // whole class name, so comparing it case-insensitively would claim
                    // a PHP semantic that does not exist for it.
                    is_class_name: false,
                })
            }
            "strrpos" => {
                if args.len() > 3 {
                    return None;
                }
                let haystack = self.eval(args.first()?)?;
                let haystack_s = haystack.as_str()?.to_string();
                let needle = self.eval(args.get(1)?)?;
                // PHP 8 stringifies a non-string needle; aborting is the safe read.
                let needle_s = needle.as_str()?.to_string();
                let offset = match args.get(2) {
                    None => 0,
                    Some(arg) => match self.eval(arg)? {
                        Value::Int(n) => n,
                        _ => return None,
                    },
                };
                fold_strrpos(&haystack_s, &needle_s, offset)
            }
            "strtolower" => self
                .eval(args.first()?)
                .and_then(|v| v.as_str().map(|s| s.to_lowercase()))
                .map(|value| Value::Str {
                    value,
                    is_class_name: false,
                }),
            "strtoupper" => self
                .eval(args.first()?)
                .and_then(|v| v.as_str().map(|s| s.to_uppercase()))
                .map(|value| Value::Str {
                    value,
                    is_class_name: false,
                }),
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
                fold_sprintf(&format_s, &substitutions).map(|value| Value::Str {
                    value,
                    is_class_name: false,
                })
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
                fold_dirname(path_str, levels).map(|value| Value::Str {
                    value,
                    is_class_name: false,
                })
            }
            "basename" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                let p = Path::new(path_str);
                p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| Value::Str {
                        value: s.to_string(),
                        is_class_name: false,
                    })
            }
            "realpath" => {
                let path = self.eval(args.first()?)?;
                let path_str = path.as_str()?;
                Some(match Path::new(path_str).canonicalize() {
                    Ok(c) => Value::Str {
                        value: c.to_string_lossy().into_owned(),
                        is_class_name: false,
                    },
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
                Some(Value::Str {
                    value: component,
                    is_class_name: false,
                })
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

/// `substr($string, $offset, $length)` over bytes, matching PHP 8's clamping:
///
/// - a negative `offset` counts from the end and clamps at 0;
/// - an `offset` past the end yields `""` (PHP 8 no longer returns `false`);
/// - a negative `length` drops that many bytes from the end and clamps at 0;
/// - `length` is clamped to the bytes remaining after `offset`.
///
/// Returns `None` when the byte window would split a multi-byte UTF-8 sequence.
/// PHP is byte-oriented and would happily return the broken bytes, but the rest
/// of the pass threads Rust `String`s, so the rule aborts rather than guess.
fn fold_substr(subject: &str, offset: i64, length: Option<i64>) -> Option<String> {
    let bytes = subject.as_bytes();
    let len = bytes.len() as i64;

    // Saturating throughout: PHP accepts `PHP_INT_MIN` here and clamps, so the
    // fold must not overflow on the way to the same answer.
    let start = if offset < 0 {
        len.saturating_add(offset).max(0)
    } else {
        offset.min(len)
    };
    let remaining = len - start;

    let take = match length {
        None => remaining,
        Some(n) if n < 0 => remaining.saturating_add(n).max(0),
        Some(n) => n.min(remaining),
    };

    let from = start as usize;
    let to = from + take as usize;
    std::str::from_utf8(bytes.get(from..to)?)
        .ok()
        .map(|s| s.to_string())
}

/// `strrpos($haystack, $needle, $offset)` over bytes.
///
/// Returns `Value::Int(byte_index)` on a hit and `Value::Bool(false)` on a miss —
/// the `false`-versus-`0` distinction callers depend on. Returns `None` (aborting
/// the rule) when PHP would throw: an `$offset` outside the haystack raises
/// `ValueError`, and the interpreter has no way to represent a thrown exception.
///
/// Offset semantics, measured against php 8.5.10:
/// - `$offset >= 0`: the match must *start* at or after `$offset`.
/// - `$offset < 0`: the match must *start* at or before `strlen + $offset`; the
///   needle itself is allowed to extend past that point.
fn fold_strrpos(haystack: &str, needle: &str, offset: i64) -> Option<Value> {
    let hay = haystack.as_bytes();
    let nee = needle.as_bytes();
    let len = hay.len() as i64;

    // PHP: "Argument #3 ($offset) must be contained in argument #1 ($haystack)".
    if offset > len || offset < -len {
        return None;
    }

    if nee.len() > hay.len() {
        return Some(Value::Bool(false));
    }

    // Window of permitted *start* positions for the needle.
    let min_start = if offset >= 0 { offset } else { 0 };
    let max_start = if offset >= 0 {
        len - nee.len() as i64
    } else {
        (len + offset).min(len - nee.len() as i64)
    };

    let mut candidate = max_start;
    while candidate >= min_start {
        let from = candidate as usize;
        if &hay[from..from + nee.len()] == nee {
            return Some(Value::Int(candidate));
        }
        candidate -= 1;
    }
    Some(Value::Bool(false))
}

/// Convert a Value to an integer for `+`, following PHP's scalar rules for the
/// only operand kinds this subset can produce: `int` stays, `false` is `0` and
/// `true` is `1`. Strings and null abort — a numeric string would need PHP's
/// full numeric-string parse and a non-numeric one is a `TypeError`.
fn value_to_arith_int(value: &Value) -> Option<i64> {
    match value {
        Value::Int(n) => Some(*n),
        Value::Bool(true) => Some(1),
        Value::Bool(false) => Some(0),
        _ => None,
    }
}

/// Check if a path is readable (file or directory).
fn is_readable_path(path: &Path) -> bool {
    fs::File::open(path).is_ok() || fs::read_dir(path).is_ok()
}

/// Convert a Value to a String representation.
fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Str { value, .. } => Some(value.clone()),
        Value::Int(n) => Some(n.to_string()),
        Value::Bool(true) => Some("1".to_string()),
        Value::Bool(false) => Some(String::new()),
        Value::Null => Some(String::new()),
    }
}

/// Compare two interpreter values, using PHP's case-insensitive class-name lookup when one side
/// is the SPL autoload parameter.
fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (
            Value::Str {
                value: left_value, ..
            },
            Value::Str {
                value: right_value, ..
            },
        ) => {
            if (left.is_class_name() || right.is_class_name())
                && !left_value.is_empty()
                && !right_value.is_empty()
            {
                php_symbol_key(left_value) == php_symbol_key(right_value)
            } else {
                left_value == right_value
            }
        }
        _ => left == right,
    }
}

/// Try to resolve `class_name` through `rule`. Returns the include path
/// produced by the closure, or `None` if the rule doesn't yield one (no
/// matching require_once, condition rejected the candidate, or an
/// unsupported operation aborted evaluation).
pub fn resolve(rule: &AutoloadRule, class_name: &str) -> Option<PathBuf> {
    let mut interp = Interpreter::new();
    interp.vars.insert(
        rule.param_name.clone(),
        Value::Str {
            value: class_name.to_string(),
            is_class_name: true,
        },
    );
    match interp.exec_block(&rule.body) {
        Flow::Include(path) => Some(path),
        _ => None,
    }
}
