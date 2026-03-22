use crate::span::Span;

pub struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            bytes: source.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn span(&self) -> Span {
        Span::new(self.line, self.col)
    }

    #[inline]
    pub fn peek(&self) -> Option<char> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let b = self.bytes[self.pos];
        if b.is_ascii() {
            Some(b as char)
        } else {
            // Fallback for non-ASCII (rare in PHP source)
            std::str::from_utf8(&self.bytes[self.pos..])
                .ok()?
                .chars()
                .next()
        }
    }

    pub fn advance(&mut self) -> Option<char> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let b = self.bytes[self.pos];
        let ch = if b.is_ascii() {
            self.pos += 1;
            b as char
        } else {
            let s = std::str::from_utf8(&self.bytes[self.pos..]).ok()?;
            let ch = s.chars().next()?;
            self.pos += ch.len_utf8();
            ch
        };
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    pub fn remaining(&self) -> &'a str {
        std::str::from_utf8(&self.bytes[self.pos..]).unwrap_or("")
    }
}
