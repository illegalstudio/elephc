//! Purpose:
//! Bounds assembler memory for large ELF programs without splitting function bodies.
//!
//! Called from:
//! - `crate::linker::assemble` for large Linux assembly files.
//!
//! Key details:
//! - Streams complete function sections into serial assembler inputs.
//! - Promotes defined non-global symbols referenced across inputs, then combines all objects.
//! - Leaves Mach-O platform stamping and ordinary small-file assembly unchanged.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::codegen::platform::{Arch, Platform, Target};

const CHUNK_BYTES: u64 = 64 * 1024 * 1024;
const LARGE_ASSEMBLY_BYTES: u64 = 256 * 1024 * 1024;
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// Owns only a newly created staging directory and removes it on every return path.
struct Staging(PathBuf);

impl Staging {
    /// Creates private staging beside the output, without borrowing a shared directory.
    fn create(output: &Path) -> io::Result<Self> {
        let parent = output.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        loop {
            let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".elephc-assembly-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for Staging {
    /// Deletes only the directory successfully created by this invocation.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Records definitions and bindings before deciding which cross-input references need promotion.
#[derive(Default)]
struct Symbol {
    defined_in: Option<usize>,
    global: bool,
}

/// Reports whether this target and source size need the ELF bounded-memory path.
fn should_chunk(target: Target, bytes: u64) -> bool {
    target.platform == Platform::Linux && bytes >= LARGE_ASSEMBLY_BYTES
}

/// Visits symbol tokens while ignoring quoted data, numeric operands and target comments.
fn symbol_tokens(line: &str, arch: Arch, mut visit: impl FnMut(&str)) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' | b'\'' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index = (index + 2).min(bytes.len());
                    } else if bytes[index] == quote {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            b'#' if arch == Arch::X86_64 => break,
            b'/' if bytes.get(index + 1) == Some(&b'/') => break,
            byte if symbol_byte(byte) && !byte.is_ascii_digit()
                && (index == 0 || !symbol_byte(bytes[index - 1])) => {
                let start = index;
                index += 1;
                while index < bytes.len() && symbol_byte(bytes[index]) {
                    index += 1;
                }
                visit(&line[start..index]);
            }
            _ => index += 1,
        }
    }
}

/// Recognizes the ASCII symbol alphabet emitted by the native backend.
fn symbol_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$')
}

/// Starts an independent assembler input with the required target syntax state.
fn start_chunk(directory: &Path, index: usize, target: Target) -> io::Result<BufWriter<File>> {
    let mut writer = BufWriter::new(File::create(directory.join(format!("part-{index}.s")))?);
    if target.arch == Arch::X86_64 {
        writeln!(writer, ".intel_syntax noprefix")?;
    }
    Ok(writer)
}

/// Indexes actual definitions and explicit bindings without guessing from symbol prefixes.
fn record_symbols(line: &str, index: usize, symbols: &mut HashMap<String, Symbol>) {
    let line = line.trim_start();
    if let Some((label, _)) = line.split_once(':') {
        if !label.is_empty() && label.bytes().all(symbol_byte) && !label.as_bytes()[0].is_ascii_digit() {
            symbols.entry(label.to_owned()).or_default().defined_in = Some(index);
        }
    }
    for directive in [".globl", ".global", ".weak"] {
        if let Some(rest) = line.strip_prefix(directive).filter(|rest| rest.starts_with(char::is_whitespace)) {
            for symbol in rest.split(|c: char| c.is_whitespace() || c == ',') {
                if !symbol.is_empty() && symbol.bytes().all(symbol_byte) {
                    symbols.entry(symbol.to_owned()).or_default().global = true;
                }
            }
        }
    }
}

/// Streams the program in source order, splitting only before complete function sections.
fn partition(source: &Path, directory: &Path, target: Target, limit: u64)
    -> io::Result<(usize, HashMap<String, Symbol>)>
{
    let mut reader = BufReader::new(File::open(source)?);
    let mut writer = start_chunk(directory, 0, target)?;
    let mut index = 0;
    let mut size = 0;
    let mut symbols = HashMap::new();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if size >= limit && line.trim_start().starts_with(".section .text.") {
            writer.flush()?;
            index += 1;
            writer = start_chunk(directory, index, target)?;
            size = 0;
        }
        record_symbols(&line, index, &mut symbols);
        writer.write_all(line.as_bytes())?;
        size += line.len() as u64;
    }
    writer.flush()?;
    Ok((index + 1, symbols))
}

/// Promotes crossing local definitions while preserving exported and unresolved external bindings.
fn crossing_symbols(directory: &Path, count: usize, target: Target, symbols: &HashMap<String, Symbol>)
    -> io::Result<Vec<Vec<String>>>
{
    let mut promotions = vec![std::collections::HashSet::new(); count];
    let mut line = String::new();
    for index in 0..count {
        let mut reader = BufReader::new(File::open(directory.join(format!("part-{index}.s")))?);
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            symbol_tokens(&line, target.arch, |name| {
                if let Some(symbol) = symbols.get(name).filter(|symbol| !symbol.global) {
                    if let Some(definition) = symbol.defined_in.filter(|definition| *definition != index) {
                        promotions[index].insert(name.to_owned());
                        promotions[definition].insert(name.to_owned());
                    }
                }
            });
        }
    }
    Ok(promotions.into_iter().map(|symbols| symbols.into_iter().collect()).collect())
}

/// Runs one tool without exiting the process, so staging cleanup also covers failures.
fn run(name: &str, command: &mut Command) -> Result<(), String> {
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{name} failed with {status}")),
        Err(error) => Err(format!("could not run {name}: {error}")),
    }
}

/// Assembles all sections and preserves cross-input local addresses as hidden symbols.
fn assemble_partitioned(target: Target, source: &Path, output: &Path, limit: u64)
    -> Result<(), String>
{
    let staging = Staging::create(output).map_err(|error| error.to_string())?;
    let (count, symbols) = partition(source, &staging.0, target, limit)
        .map_err(|error| error.to_string())?;
    let mut promotions = crossing_symbols(&staging.0, count, target, &symbols)
        .map_err(|error| error.to_string())?;
    drop(symbols);
    let mut objects = Vec::with_capacity(count);
    for (index, symbols) in promotions.iter_mut().enumerate() {
        symbols.sort_unstable();
        let input = staging.0.join(format!("input-{index}.s"));
        let object = staging.0.join(format!("part-{index}.o"));
        let mut writer = BufWriter::new(File::create(&input).map_err(|error| error.to_string())?);
        for symbol in symbols {
            writeln!(writer, ".globl {symbol}\n.hidden {symbol}").map_err(|error| error.to_string())?;
        }
        let body = staging.0.join(format!("part-{index}.s"));
        io::copy(&mut File::open(&body).map_err(|error| error.to_string())?, &mut writer)
            .map_err(|error| error.to_string())?;
        writer.flush().map_err(|error| error.to_string())?;
        drop(writer);
        fs::remove_file(body).map_err(|error| error.to_string())?;
        let mut assembler = super::assembler_command(target);
        assembler.arg("-o").arg(&object).arg(&input);
        run("Assembler", &mut assembler)?;
        fs::remove_file(input).map_err(|error| error.to_string())?;
        objects.push(object);
    }
    let combined = staging.0.join("combined.o");
    let mut linker = Command::new(target.linker_cmd());
    linker.args(["-r", "-nostdlib", "-no-pie", "-o"]).arg(&combined).args(&objects);
    run("Relocatable linker", &mut linker)?;
    fs::rename(combined, output).map_err(|error| error.to_string())
}

/// Uses bounded-memory ELF assembly when needed and reports whether it handled the input.
pub(super) fn try_assemble(target: Target, source: &Path, output: &Path) -> Result<bool, String> {
    let bytes = fs::metadata(source).map_err(|error| error.to_string())?.len();
    if !should_chunk(target, bytes) {
        return Ok(false);
    }
    assemble_partitioned(target, source, output, CHUNK_BYTES)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only ELF targets use partitioning, with the exact same size threshold on both CPUs.
    #[test]
    fn chunked_assembly_target_matrix() {
        for name in ["linux-x86_64", "linux-aarch64", "macos-aarch64", "ios-arm64", "ios-sim-arm64"] {
            let target = Target::parse(name).unwrap();
            assert!(!should_chunk(target, LARGE_ASSEMBLY_BYTES - 1));
            assert_eq!(should_chunk(target, LARGE_ASSEMBLY_BYTES), name.starts_with("linux-"));
        }
    }

    /// String payloads, escaped quotes and comments do not invent cross-input references.
    #[test]
    fn chunked_assembly_local_token_boundaries() {
        let mut symbols = Vec::new();
        symbol_tokens(".quad .Lreal, prefix.Lnot, .Lother # .Lcomment", Arch::X86_64, |s| if s.starts_with(".L") { symbols.push(s.to_owned()); });
        symbol_tokens(r#".ascii ".Lstring\".Lquoted" // .Lcomment"#, Arch::AArch64, |s| if s.starts_with(".L") { symbols.push(s.to_owned()); });
        symbol_tokens("add x0, x0, #:lo12:.Larm // .Lcomment", Arch::AArch64, |s| if s.starts_with(".L") { symbols.push(s.to_owned()); });
        assert_eq!(symbols, [".Lreal", ".Lother", ".Larm"]);
    }

    /// Separate function inputs retain local-label addresses and their original data bytes.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn chunked_assembly_links_cross_section_locals_and_cleans_staging() {
        let owner = Staging::create(Path::new("test.o")).unwrap();
        let source = owner.0.join("source.s");
        fs::write(&source, concat!(
            ".intel_syntax noprefix\n.section .text.main,\"ax\",@progbits\n.globl main\nmain:\n",
            "push rbx\nlea rdi, [rip + message]\ncall puts\ncall qword ptr [rip + pointer]\nmov ebx, eax\n",
            "call qword ptr [rip + named_pointer]\nadd eax, ebx\npop rbx\nret\n",
            ".data\npointer: .quad .Lanswer\nnamed_pointer: .quad named_answer\n",
            "answer_number: .long 17\nmessage: .asciz \"chunked\"\n.ascii \".Lstring\"\n",
            ".section .text.answer,\"ax\",@progbits\n.Lanswer:\nmov eax, 20\nret\n",
            ".section .text.named_answer,\"ax\",@progbits\nnamed_answer:\nmov eax, dword ptr [rip + answer_number]\nret\n",
            ".section .note.GNU-stack,\"\",@progbits\n",
        )).unwrap();
        let target = Target::parse("linux-x86_64").unwrap();
        let object = owner.0.join("program.o");
        assemble_partitioned(target, &source, &object, 1).unwrap();
        let executable = owner.0.join("program");
        run("test linker", Command::new("gcc").arg("-no-pie").arg(&object).arg("-o").arg(&executable)).unwrap();
        let result = Command::new(executable).output().unwrap();
        assert_eq!(result.status.code(), Some(37));
        assert_eq!(result.stdout, b"chunked\n");
        let table = Command::new("readelf").arg("-Ws").arg(&object).output().unwrap();
        let table = String::from_utf8(table.stdout).unwrap();
        assert!(table.lines().any(|line| line.ends_with(" main") && line.contains("GLOBAL DEFAULT")));
        assert!(table.lines().any(|line| line.ends_with(" named_answer") && line.contains("GLOBAL HIDDEN")));
        assert_eq!(fs::read_dir(&owner.0).unwrap().count(), 3);
        let object_bytes = fs::read(object).unwrap();
        assert!(object_bytes.windows(8).any(|window| window == b".Lstring"));
    }

    /// Tool failures keep diagnostics and remove every newly owned intermediate input.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn chunked_assembly_failure_cleans_staging() {
        let owner = Staging::create(Path::new("test.o")).unwrap();
        let source = owner.0.join("bad.s");
        fs::write(&source, ".section .text.bad,\"ax\",@progbits\nthis_is_not_an_instruction\n").unwrap();
        let output = owner.0.join("bad.o");
        assert!(assemble_partitioned(Target::parse("linux-x86_64").unwrap(), &source, &output, 1).is_err());
        assert!(!output.exists());
        assert_eq!(fs::read_dir(&owner.0).unwrap().count(), 1);
    }
}
