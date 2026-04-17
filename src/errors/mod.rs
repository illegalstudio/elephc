mod report;

use crate::span::Span;

#[derive(Debug, Clone)]
pub struct CompileError {
    pub span: Span,
    pub file: Option<String>,
    pub message: String,
    pub related: Vec<CompileError>,
}

#[derive(Debug, Clone)]
pub struct CompileWarning {
    pub span: Span,
    pub message: String,
}

impl CompileError {
    pub fn new(span: Span, message: &str) -> Self {
        Self {
            span,
            file: None,
            message: message.to_string(),
            related: Vec::new(),
        }
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn from_many(mut errors: Vec<CompileError>) -> Self {
        let mut first = errors.remove(0);
        first.related = errors;
        first
    }

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
    pub fn new(span: Span, message: &str) -> Self {
        Self {
            span,
            message: message.to_string(),
        }
    }
}

impl std::error::Error for CompileError {}

impl std::fmt::Display for CompileError {
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

pub fn report(error: &CompileError) {
    report::print_error(error);
}

pub fn report_warning(warning: &CompileWarning) {
    report::print_warning(warning);
}
