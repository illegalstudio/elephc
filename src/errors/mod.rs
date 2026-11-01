//! Purpose:
//! Defines compiler diagnostic types and public reporting entry points.
//! Carries error, warning, file, and source-span data from all frontend and analysis passes.
//!
//! Called from:
//! - Compiler passes that construct diagnostics and `crate::pipeline::compile()` when reporting them.
//!
//! Key details:
//! - Diagnostics keep optional file data so included-file errors can point at their real source.

mod report;

use crate::span::Span;

#[derive(Debug, Clone)]
/// Compile error.
pub struct CompileError {
    pub span: Span,
    pub file: Option<String>,
    /// The class-like or function this error was raised inside, when a pass knew it.
    ///
    /// A `Span` carries only line and column — deliberately, so it stays 16 bytes in every token
    /// and AST node. That is enough while one file is being parsed, and not enough afterwards:
    /// the autoload pass splices hundreds of files into one program, and their line numbers
    /// collide. Naming the enclosing declaration is what lets `pipeline` recover the file, which
    /// it already maps per declaration for `Reflection*::getFileName()`.
    pub declaration: Option<String>,
    pub message: String,
    pub related: Vec<CompileError>,
    pub kind: CompileErrorKind,
    /// The run-time fault php raises for this construct, when the checker could name one.
    ///
    /// php binds a member or a class name when the expression EXECUTES, so a program whose
    /// never-taken branch calls a method nothing declares is loadable, and real vendor code
    /// relies on that (`if (method_exists(X::class, 'm')) { X::m(); }`). The checker still
    /// refuses the body, because a compiled call needs a target; this names what php would
    /// have raised there so `autoload::runtime_faults` can compile the construct into that
    /// fault instead of handing the whole class file to the interpreter. `None` is every error
    /// the compiler cannot vouch for: php's own compile-time refusals, and type gaps of the
    /// checker, where "compile it as a fault" would turn working code into a throw.
    pub runtime_fault: Option<RuntimeFault>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// What php raises when a construct the checker refused actually runs.
///
/// Every variant names the class the checker judged the construct against, so the pipeline can
/// tell a class the compiled world holds in full from one with a subclass or a definition
/// outside it, where php's answer depends on the run-time object and the fault is not certain.
pub enum RuntimeFault {
    /// `$o->m(...)`: no class the checker knows gives the receiver a method `m` (`Error`).
    UndefinedMethod { class: String, method: String },
    /// `$o->p` read: the receiver's class and its known descendants declare no `p` (a Warning
    /// and `null` in php, not an Error).
    UndefinedProperty { class: String, property: String },
    /// `X::m(...)` on a class the compiled world holds, which declares no `m` (`Error`).
    UndefinedStaticMethod { class: String, method: String },
    /// `X::$p` read on a class that declares no static `$p` (`Error`).
    UndefinedStaticProperty { class: String, property: String },
    /// `X::C` on a class that declares no constant `C` (`Error`).
    UndefinedClassConstant { class: String, constant: String },
    /// `new X`, `X::m()`, `X::$p` or `X::C` naming a class nothing declares (`Error`).
    UnknownClass { class: String },
    /// `$o->m(...)` passing fewer arguments than `X::m` requires (`ArgumentCountError`).
    /// `exactly` is php's wording: the method takes no optional or variadic parameter.
    TooFewArguments { class: String, method: String, passed: usize, required: usize, exactly: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// The class of a compile error, for consumers that must react to one class specifically.
pub enum CompileErrorKind {
    /// Every diagnostic without a more specific class.
    #[default]
    General,
    /// A builtin argument that AOT compilation needs as a compile-time literal was not one.
    ///
    /// The backend reads such an argument while lowering the call (a result-shape flag, a
    /// constant or class name), so no spelling of the call can defer it to run time.
    AotLiteralRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Severity of a compiler diagnostic that does not stop compilation.
pub enum WarningKind {
    /// Ordinary compiler warning.
    #[default]
    Warning,
    /// PHP deprecation notice raised while compiling a constant expression.
    Deprecation,
}

#[derive(Debug, Clone)]
/// Compile warning.
pub struct CompileWarning {
    pub span: Span,
    pub message: String,
    pub kind: WarningKind,
}

impl CompileError {
    /// Creates a new error with the given span and message, with no file and no related errors.
    pub fn new(span: Span, message: &str) -> Self {
        Self {
            span,
            file: None,
            declaration: None,
            message: message.to_string(),
            related: Vec::new(),
            kind: CompileErrorKind::General,
            runtime_fault: None,
        }
    }

    /// Names the run-time fault php raises for the refused construct; see `runtime_fault`.
    pub fn with_runtime_fault(mut self, fault: RuntimeFault) -> Self {
        self.runtime_fault = Some(fault);
        self
    }

    /// Creates the error for a builtin argument that must be a compile-time literal in AOT mode.
    pub fn aot_literal_required(span: Span, message: &str) -> Self {
        Self {
            kind: CompileErrorKind::AotLiteralRequired,
            ..Self::new(span, message)
        }
    }

    /// Sets the file name and returns self for chaining.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Records the class-like or function this error was raised inside, keeping any name already
    /// set: the innermost pass to know wins, and an outer one must not overwrite it.
    pub fn within_declaration(mut self, declaration: impl Into<String>) -> Self {
        if self.declaration.is_none() {
            self.declaration = Some(declaration.into());
        }
        for related in &mut self.related {
            if related.declaration.is_none() {
                related.declaration = self.declaration.clone();
            }
        }
        self
    }

    /// Fills in `file` from a declaration-name-to-path map, for this error and its related ones.
    ///
    /// The map is the one `autoload` already builds for reflection. An error whose declaration is
    /// unknown, or whose declaration has no recorded path, is left alone — the caller's own
    /// fallback (the entry file) still applies.
    pub fn resolve_declaration_files(
        mut self,
        paths: &std::collections::HashMap<String, String>,
    ) -> Self {
        if self.file.is_none() {
            if let Some(path) = self
                .declaration
                .as_deref()
                .and_then(|name| lookup_declaration_path(paths, name))
            {
                self.file = Some(path);
            }
        }
        self.related = self
            .related
            .into_iter()
            .map(|related| related.resolve_declaration_files(paths))
            .collect();
        self
    }
}

/// Looks a declaration name up in the source-path map.
///
/// The map is keyed the way every other symbol table in the compiler is: leading backslash
/// stripped and lowercased, because PHP matches class and function names case-insensitively.
/// A pass that recorded `App\Kernel` must still find the entry written as `app\kernel`.
fn lookup_declaration_path(
    paths: &std::collections::HashMap<String, String>,
    name: &str,
) -> Option<String> {
    let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    paths.get(&key).or_else(|| paths.get(name)).cloned()
}

impl CompileError {

    /// Combines a vector of errors into a single error, treating the first as primary and the rest as related.
    ///
    /// # Panics
    /// Panics if `errors` is empty. Callers must guarantee at least one error; the
    /// explicit assert turns a misuse into a clear message instead of an opaque
    /// `Vec::remove(0)` index-out-of-bounds panic.
    pub fn from_many(mut errors: Vec<CompileError>) -> Self {
        assert!(!errors.is_empty(), "from_many requires at least one error");
        let mut first = errors.remove(0);
        first.related = errors;
        first
    }

    /// Returns all errors including related ones as a flat vector, with the primary error first.
    pub fn flatten(&self) -> Vec<CompileError> {
        let mut first = self.clone();
        first.related.clear();
        let mut all = vec![first];
        for related in &self.related {
            all.extend(related.flatten());
        }
        all
    }
}

impl CompileWarning {
    /// Creates a new warning with the given span and message.
    pub fn new(span: Span, message: &str) -> Self {
        Self {
            span,
            message: message.to_string(),
            kind: WarningKind::Warning,
        }
    }

    /// Creates a deprecation diagnostic with the given span and message.
    pub fn deprecation(span: Span, message: &str) -> Self {
        Self {
            span,
            message: message.to_string(),
            kind: WarningKind::Deprecation,
        }
    }
}

impl std::error::Error for CompileError {}

impl std::fmt::Display for CompileError {
    /// Formats this value for display or debug output.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.file, self.span.line > 0) {
            (Some(file), true) => {
                write!(f, "[{}:{}:{}] {}", file, self.span.line, self.span.col, self.message)
            }
            (Some(file), false) => write!(f, "[{}] {}", file, self.message),
            (None, true) => write!(f, "[{}:{}] {}", self.span.line, self.span.col, self.message),
            (None, false) => write!(f, "{}", self.message),
        }
    }
}

/// Prints the error message to stderr with file:line:col formatting when available.
pub fn report(error: &CompileError) {
    report::print_error(error);
}

/// The plain text [`report`] prints for `error`, for replaying it from another process.
pub fn render_plain(error: &CompileError) -> String {
    report::render_plain(error)
}

/// Prints the warning message to stderr with file:line:col formatting when available.
pub fn report_warning(warning: &CompileWarning) {
    report::print_warning(warning);
}

/// Prints a `note:` line to stderr; see `report::print_note`.
pub fn report_note(message: &str) {
    report::print_note(message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    /// Verifies `from_many` panics with an explicit precondition message when given an
    /// empty error vector, instead of the opaque `Vec::remove(0)` index-out-of-bounds panic.
    #[test]
    #[should_panic(expected = "from_many requires at least one error")]
    fn from_many_empty_vec_panics_with_clear_message() {
        let _ = CompileError::from_many(Vec::new());
    }

    /// Verifies `from_many` keeps the first error as primary and attaches the rest as related.
    #[test]
    fn from_many_promotes_first_and_keeps_rest_related() {
        let errors = vec![
            CompileError::new(Span::new(1, 1), "first"),
            CompileError::new(Span::new(2, 2), "second"),
        ];
        let combined = CompileError::from_many(errors);
        assert_eq!(combined.message, "first");
        assert_eq!(combined.related.len(), 1);
        assert_eq!(combined.related[0].message, "second");
    }
}
