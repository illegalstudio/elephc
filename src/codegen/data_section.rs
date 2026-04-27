use std::collections::HashMap;

pub struct DataSection {
    entries: Vec<(String, Vec<u8>)>,
    float_entries: Vec<(String, u64)>,
    comm_entries: Vec<(String, usize)>,
    counter: usize,
    dedup: HashMap<Vec<u8>, String>,
    float_dedup: HashMap<u64, String>,
    comm_dedup: HashMap<String, String>,
}

impl DataSection {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            float_entries: Vec::new(),
            comm_entries: Vec::new(),
            counter: 0,
            dedup: HashMap::new(),
            float_dedup: HashMap::new(),
            comm_dedup: HashMap::new(),
        }
    }

    pub fn add_float(&mut self, value: f64) -> String {
        let bits = value.to_bits();
        if let Some(label) = self.float_dedup.get(&bits) {
            return label.clone();
        }
        let label = format!("_float_{}", self.counter);
        self.counter += 1;
        self.float_dedup.insert(bits, label.clone());
        self.float_entries.push((label.clone(), bits));
        label
    }

    pub fn add_string(&mut self, bytes: &[u8]) -> (String, usize) {
        if let Some(label) = self.dedup.get(bytes) {
            return (label.clone(), bytes.len());
        }

        let label = format!("_str_{}", self.counter);
        self.counter += 1;
        let owned = bytes.to_vec();
        self.dedup.insert(owned.clone(), label.clone());
        self.entries.push((label.clone(), owned));
        (label, bytes.len())
    }

    pub fn add_comm(&mut self, label: String, size: usize) -> String {
        if let Some(existing) = self.comm_dedup.get(&label) {
            return existing.clone();
        }

        self.comm_dedup.insert(label.clone(), label.clone());
        self.comm_entries.push((label.clone(), size));
        label
    }

    pub fn emit(&self) -> String {
        if self.entries.is_empty() && self.float_entries.is_empty() && self.comm_entries.is_empty() {
            return String::new();
        }

        let mut out = String::from(".data\n");
        for (label, size) in &self.comm_entries {
            out.push_str(&format!(".comm {}, {}, 3\n", label, size));
        }
        for (label, bytes) in &self.entries {
            out.push_str(&format!(".globl {}\n{}:\n", label, label));
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
        for (label, bits) in &self.float_entries {
            out.push_str(&format!(".p2align 3\n.globl {}\n{}:\n    .quad 0x{:016x}\n", label, label, bits));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::DataSection;

    #[test]
    fn test_float_constants_use_power_of_two_alignment_directive() {
        let mut data = DataSection::new();
        data.add_float(3.14);

        let asm = data.emit();

        assert!(asm.contains(".p2align 3\n"));
        assert!(!asm.contains(".align 3\n"));
    }
}
