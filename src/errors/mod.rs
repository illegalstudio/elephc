mod report;

use crate::span::Span;

#[derive(Debug, Clone)]
pub struct CompileError {
    pub span: Span,
    pub message: String,
}

impl CompileError {
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
