mod report;

#[derive(Debug, Clone)]
pub struct CompileError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl CompileError {
    pub fn new(line: usize, col: usize, message: &str) -> Self {
        Self {
            line,
            col,
            message: message.to_string(),
        }
    }

    pub fn at(line: usize, col: usize, message: &str) -> Self {
        Self::new(line, col, message)
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "[{}:{}] {}", self.line, self.col, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

pub fn report(error: &CompileError) {
    report::print_error(error);
}
