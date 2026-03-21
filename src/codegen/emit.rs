pub struct Emitter {
    lines: Vec<String>,
}

impl Emitter {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub fn instruction(&mut self, instr: &str) {
        self.lines.push(format!("    {}", instr));
    }

    pub fn label(&mut self, name: &str) {
        self.lines.push(format!("{}:", name));
    }

    pub fn comment(&mut self, text: &str) {
        self.lines.push(format!("    ; {}", text));
    }

    pub fn blank(&mut self) {
        self.lines.push(String::new());
    }

    pub fn raw(&mut self, text: &str) {
        self.lines.push(text.to_string());
    }

    pub fn output(&self) -> String {
        self.lines.join("\n") + "\n"
    }
}
