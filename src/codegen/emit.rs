use std::fmt::Write;

pub struct Emitter {
    buf: String,
}

impl Emitter {
    pub fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
        }
    }

    pub fn instruction(&mut self, instr: &str) {
        let _ = writeln!(self.buf, "    {}", instr);
    }

    pub fn label(&mut self, name: &str) {
        let _ = writeln!(self.buf, "{}:", name);
    }

    /// Emit a label that is visible across object files (for two-object linking).
    pub fn label_global(&mut self, name: &str) {
        let _ = writeln!(self.buf, ".globl {}", name);
        let _ = writeln!(self.buf, "{}:", name);
    }

    pub fn comment(&mut self, text: &str) {
        let _ = writeln!(self.buf, "    ; {}", text);
    }

    pub fn blank(&mut self) {
        self.buf.push('\n');
    }

    pub fn raw(&mut self, text: &str) {
        self.buf.push_str(text);
        self.buf.push('\n');
    }

    pub fn output(self) -> String {
        self.buf
    }
}
