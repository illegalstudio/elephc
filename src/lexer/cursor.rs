use crate::span::Span;

pub struct Cursor<'a> {
    source: &'a str,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn span(&self) -> Span {
        Span::new(self.line, self.col)
    }

    pub fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    pub fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    pub fn remaining(&self) -> &'a str {
        &self.source[self.pos..]
    }
}
