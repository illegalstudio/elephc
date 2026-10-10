//! Purpose:
//! Models the visible ASCII terminal rows used by REPL regression tests.
//!
//! Called from:
//! - The `repl_terminal` PTY fixture when it reads terminal output.
//!
//! Key details:
//! - Handles cursor queries, movement, and line clearing across split reads.
//! - Assertions inspect rendered rows so output erased by a prompt cannot pass.

/// Minimal terminal state for the editor's cursor controls and ASCII test transcripts.
#[derive(Debug, Default)]
pub(super) struct Screen {
    lines: Vec<Vec<u8>>,
    row: usize,
    column: usize,
    escape: Vec<u8>,
}

impl Screen {
    /// Applies terminal output and returns replies to ANSI cursor-position requests.
    pub(super) fn observe(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut replies = Vec::new();
        for &byte in bytes {
            if !self.escape.is_empty() {
                self.escape.push(byte);
                if self.escape.len() >= 3 && (0x40..=0x7e).contains(&byte) {
                    let escape = std::mem::take(&mut self.escape);
                    self.control(&escape[2..escape.len() - 1], byte, &mut replies);
                }
                continue;
            }
            match byte {
                0x1b => self.escape.push(byte),
                b'\r' => self.column = 0,
                b'\n' => self.row += 1,
                8 => self.column = self.column.saturating_sub(1),
                32..=126 => {
                    let column = self.column;
                    let line = self.line();
                    line.resize(line.len().max(column + 1), b' ');
                    line[column] = byte;
                    self.column += 1;
                }
                _ => {}
            }
        }
        replies
    }

    /// Checks actual retained output, excluding text that a subsequent clear erased.
    pub(super) fn has_line(&self, text: &str) -> bool {
        self.lines.iter().any(|line| line == text.as_bytes())
    }

    /// Allocates the current row after a cursor movement or newline.
    fn line(&mut self) -> &mut Vec<u8> {
        self.lines.resize_with(self.lines.len().max(self.row + 1), Vec::new);
        &mut self.lines[self.row]
    }

    /// Interprets the cursor and erase subset emitted by the line editor.
    fn control(&mut self, parameters: &[u8], command: u8, replies: &mut Vec<u8>) {
        let count = std::str::from_utf8(parameters).ok()
            .and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
        match command {
            b'n' if parameters == b"6" => {
                replies.extend_from_slice(format!("\x1b[{};{}R", self.row + 1, self.column + 1).as_bytes());
            }
            b'A' => self.row = self.row.saturating_sub(count.max(1)),
            b'B' => self.row += count.max(1),
            b'C' => self.column += count.max(1),
            b'D' => self.column = self.column.saturating_sub(count.max(1)),
            b'G' => self.column = count.max(1) - 1,
            b'K' => {
                let column = self.column;
                if count == 2 { self.line().clear(); }
                else { self.line().truncate(column); }
            }
            _ => {}
        }
    }
}
