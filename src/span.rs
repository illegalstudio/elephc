#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }

    pub fn dummy() -> Self {
        Self { line: 0, col: 0 }
    }
}
