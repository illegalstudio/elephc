mod report;

use crate::span::Span;

#[derive(Debug, Clone)]
pub struct CompileError {
    pub span: Span,
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
            message: message.to_string(),
            related: Vec::new(),
        }
    }

    pub fn from_many(mut errors: Vec<CompileError>) -> Self {
        let mut first = errors.remove(0);
        first.related = errors;
        first
    }

    pub fn flatten(&self) -> Vec<CompileError> {
        let mut all = vec![CompileError::new(self.span, &self.message)];
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
        if self.span.line > 0 {
            write!(f, "[{}:{}] {}", self.span.line, self.span.col, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

pub fn report(error: &CompileError) {
    report::print_error(error);
}

pub fn report_warning(warning: &CompileWarning) {
    report::print_warning(warning);
}
