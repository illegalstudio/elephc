use std::collections::HashMap;

pub struct DataSection {
    entries: Vec<(String, Vec<u8>)>,
    counter: usize,
    dedup: HashMap<Vec<u8>, String>,
}

impl DataSection {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            counter: 0,
            dedup: HashMap::new(),
        }
    }

    pub fn add_string(&mut self, bytes: &[u8]) -> (String, usize) {
        if let Some(label) = self.dedup.get(bytes) {
            return (label.clone(), bytes.len());
        }

        let label = format!("_str_{}", self.counter);
        self.counter += 1;
        self.entries.push((label.clone(), bytes.to_vec()));
        self.dedup.insert(bytes.to_vec(), label.clone());
        (label, bytes.len())
    }

    pub fn emit(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut out = String::from(".data\n");
        for (label, bytes) in &self.entries {
            out.push_str(&format!("{}:\n", label));
            out.push_str("    .ascii \"");
            for &b in bytes {
                match b {
                    b'\n' => out.push_str("\\n"),
                    b'\t' => out.push_str("\\t"),
                    b'\\' => out.push_str("\\\\"),
                    b'"' => out.push_str("\\\""),
                    0x20..=0x7e => out.push(b as char),
                    _ => out.push_str(&format!("\\x{:02x}", b)),
                }
            }
            out.push_str("\"\n");
        }
        out
    }
}
