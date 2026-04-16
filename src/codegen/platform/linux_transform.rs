/// macOS syscall number → Linux aarch64 syscall number.
#[allow(dead_code)]
pub(super) fn map_syscall(macos_num: u32) -> u32 {
    match macos_num {
        1 => 93,
        3 => 63,
        4 => 64,
        5 => 56,
        6 => 57,
        10 => 35,
        12 => 49,
        15 => 52,
        33 => 48,
        116 => 169,
        128 => 38,
        136 => 34,
        137 => 35,
        199 => 62,
        338 => 79,
        _ => panic!(
            "unknown macOS syscall number {} — cannot map to Linux",
            macos_num
        ),
    }
}

#[allow(dead_code)]
const C_SYMBOLS: &[&str] = &[
    "abs",
    "acos",
    "arc4random",
    "arc4random_uniform",
    "asin",
    "atan",
    "atan2",
    "atof",
    "atoi",
    "closedir",
    "cos",
    "cosh",
    "exp",
    "fgetc",
    "free",
    "getcwd",
    "getenv",
    "glob",
    "globfree",
    "hypot",
    "localtime",
    "log",
    "log10",
    "log2",
    "longjmp",
    "malloc",
    "memcpy",
    "memset",
    "mkstemp",
    "mktime",
    "opendir",
    "pclose",
    "popen",
    "pow",
    "putenv",
    "readdir",
    "regcomp",
    "regexec",
    "regfree",
    "setjmp",
    "sin",
    "sinh",
    "sleep",
    "snprintf",
    "system",
    "tan",
    "tanh",
    "time",
    "usleep",
];

#[allow(dead_code)]
fn is_c_symbol(name: &str) -> bool {
    C_SYMBOLS.binary_search(&name).is_ok()
}

#[allow(dead_code)]
pub(super) fn needs_at_fdcwd(macos_num: u32) -> bool {
    matches!(macos_num, 5 | 10 | 33 | 128 | 136 | 137 | 338)
}

#[allow(dead_code)]
pub(super) fn transform_for_linux(asm: &str) -> String {
    let mut result = String::with_capacity(asm.len());
    let lines: Vec<&str> = asm.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if let Some(transformed) = transform_relocation(line) {
            result.push_str(&transformed);
            result.push('\n');
            i += 1;
            continue;
        }

        if let Some(macos_num) = parse_syscall_mov(trimmed) {
            let next_line = lines.get(i + 1).map(|next| next.trim()).unwrap_or("");
            if !next_line.starts_with("svc #0x80") {
                result.push_str(line);
                result.push('\n');
                i += 1;
                continue;
            }

            let linux_num = map_syscall(macos_num);
            let indent = &line[..line.len() - trimmed.len()];

            if needs_at_fdcwd(macos_num) {
                match macos_num {
                    128 => {
                        result.push_str(&format!("{}mov x3, x1\n", indent));
                        result.push_str(&format!("{}mov x1, x0\n", indent));
                        result.push_str(&format!("{}mov x2, #-100\n", indent));
                        result.push_str(&format!("{}mov x0, #-100\n", indent));
                    }
                    338 => {
                        result.push_str(&format!("{}mov x2, x1\n", indent));
                        result.push_str(&format!("{}mov x1, x0\n", indent));
                        result.push_str(&format!("{}mov x0, #-100\n", indent));
                        result.push_str(&format!("{}mov x3, #0\n", indent));
                    }
                    5 => {
                        result.push_str(&format!("{}mov x3, x2\n", indent));
                        result.push_str(&format!("{}mov x2, x1\n", indent));
                        result.push_str(&format!("{}mov x1, x0\n", indent));
                        result.push_str(&format!("{}mov x0, #-100\n", indent));
                    }
                    136 => {
                        result.push_str(&format!("{}mov x2, x1\n", indent));
                        result.push_str(&format!("{}mov x1, x0\n", indent));
                        result.push_str(&format!("{}mov x0, #-100\n", indent));
                    }
                    10 => {
                        result.push_str(&format!("{}mov x1, x0\n", indent));
                        result.push_str(&format!("{}mov x0, #-100\n", indent));
                        result.push_str(&format!("{}mov x2, #0\n", indent));
                    }
                    137 => {
                        result.push_str(&format!("{}mov x1, x0\n", indent));
                        result.push_str(&format!("{}mov x0, #-100\n", indent));
                        result.push_str(&format!("{}mov x2, #0x200\n", indent));
                    }
                    33 => {
                        result.push_str(&format!("{}mov x2, x1\n", indent));
                        result.push_str(&format!("{}mov x1, x0\n", indent));
                        result.push_str(&format!("{}mov x0, #-100\n", indent));
                        result.push_str(&format!("{}mov x3, #0\n", indent));
                    }
                    _ => unreachable!(),
                }
            }

            result.push_str(&format!("{}mov x8, #{}\n", indent, linux_num));
            i += 1;
            continue;
        }

        if trimmed == "svc #0x80"
            || trimmed.starts_with("svc #0x80 ")
            || trimmed.starts_with("svc #0x80\t")
        {
            let indent = &line[..line.len() - trimmed.len()];
            result.push_str(indent);
            result.push_str("svc #0\n");
            i += 1;
            continue;
        }

        if trimmed == "_main:" {
            result.push_str("main:\n");
            i += 1;
            continue;
        }
        if trimmed == ".globl _main" {
            result.push_str(".globl main\n");
            i += 1;
            continue;
        }

        if let Some(transformed) = transform_c_call(trimmed) {
            let indent = &line[..line.len() - trimmed.len()];
            result.push_str(indent);
            result.push_str(&transformed);
            result.push('\n');
            i += 1;
            continue;
        }

        if let Some(pos) = trimmed.find("; ") {
            let indent = &line[..line.len() - trimmed.len()];
            let before = &trimmed[..pos];
            let after = &trimmed[pos + 2..];
            result.push_str(indent);
            result.push_str(before);
            result.push_str("// ");
            result.push_str(after);
            result.push('\n');
            i += 1;
            continue;
        }

        result.push_str(line);
        result.push('\n');
        i += 1;
    }

    result
}

#[allow(dead_code)]
pub(super) fn transform_relocation(line: &str) -> Option<String> {
    if !line.contains("@PAGE") && !line.contains("@GOT") {
        return None;
    }

    let mut result = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch == '@' {
            let rest: String = chars.clone().collect();
            if rest.starts_with("@GOTPAGEOFF") {
                let symbol_start = result
                    .rfind(|candidate: char| !candidate.is_alphanumeric() && candidate != '_')
                    .map(|index| index + 1)
                    .unwrap_or(0);
                let symbol = result[symbol_start..].to_string();
                result.truncate(symbol_start);
                result.push_str(&format!(":got_lo12:{}", symbol));
                for _ in 0..11 {
                    chars.next();
                }
            } else if rest.starts_with("@GOTPAGE") {
                let symbol_start = result
                    .rfind(|candidate: char| !candidate.is_alphanumeric() && candidate != '_')
                    .map(|index| index + 1)
                    .unwrap_or(0);
                let symbol = result[symbol_start..].to_string();
                result.truncate(symbol_start);
                result.push_str(&format!(":got:{}", symbol));
                for _ in 0..8 {
                    chars.next();
                }
            } else if rest.starts_with("@PAGEOFF") {
                let symbol_start = result
                    .rfind(|candidate: char| !candidate.is_alphanumeric() && candidate != '_')
                    .map(|index| index + 1)
                    .unwrap_or(0);
                let symbol = result[symbol_start..].to_string();
                result.truncate(symbol_start);
                result.push_str(&format!(":lo12:{}", symbol));
                for _ in 0..8 {
                    chars.next();
                }
            } else if rest.starts_with("@PAGE") {
                for _ in 0..5 {
                    chars.next();
                }
            } else {
                result.push(ch);
                chars.next();
            }
        } else {
            result.push(ch);
            chars.next();
        }
    }

    Some(result)
}

#[allow(dead_code)]
pub(super) fn parse_syscall_mov(trimmed: &str) -> Option<u32> {
    let rest = trimmed.strip_prefix("mov x16, #")?;
    let num_str = rest.split_whitespace().next().unwrap_or(rest);
    num_str.parse::<u32>().ok()
}

#[allow(dead_code)]
fn remap_symbol(name: &str) -> &str {
    match name {
        "CC_MD5" => "MD5",
        "CC_SHA1" => "SHA1",
        "CC_SHA256" => "SHA256",
        _ => name,
    }
}

#[allow(dead_code)]
pub(super) fn transform_c_call(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("bl _")?;
    if rest.starts_with('_') {
        return None;
    }
    let func_name = rest.split_whitespace().next().unwrap_or(rest);
    let remapped = remap_symbol(func_name);
    if remapped != func_name {
        return Some(format!("bl {}", remapped));
    }
    if is_c_symbol(func_name) {
        return Some(format!("bl {}", rest));
    }
    None
}
