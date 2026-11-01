//! Compact native include-state lookup/reset callbacks over a table of physical sources.
//! Callbacks touch only static guard cells and never execute PHP or allocate PHP values.

use super::include_guard_symbol;
use crate::codegen::{abi, context::FunctionContext, data_section::{DataSection, DataWord}, emit::Emitter, platform::Arch};
use crate::ir::Module;

const LOOKUP: &str = "__rt_source_include_lookup";
const RESET: &str = "__rt_source_include_reset";
const PRIME: &str = "__rt_source_include_prime";
/// Non-zero while every compiler-included guard is known raised, so a repeated prime is a load
/// and a branch. The prime sets it; `__rt_source_include_reset` and the `--web` request reset,
/// the only writers that ever lower a guard, clear it.
pub(in crate::codegen) const PRIMED: &str = "_source_include_primed";
/// C-ABI name: Magician calls this one by symbol, so it carries the platform prefix.
const CLASS_LOOKUP: &str = "__elephc_eval_class_deferred_lookup";
const CLASS_RESET: &str = "__rt_class_deferred_reset";
/// C-ABI name: Magician calls it when it skips an include on the compiler's mark, so it carries
/// the platform prefix too.
const SOURCE_ACTIVATE: &str = "__elephc_eval_source_activate";
/// Bytes per table record: path pointer, path length, guard cell, compiler-included flag.
const RECORD: usize = 32;
/// Guard-cell value meaning "the COMPILER already included this source", as opposed to the `1` a
/// RUNTIME include writes. Every reader that only asks "has this been included" still tests for
/// non-zero, so the distinction costs nothing; what it buys is the interpreter being able to tell
/// an inclusion the compiler performed at program start from one this request performed itself.
/// Mirrored by `COMPILER_INCLUDED` in `crates/elephc-magician/src/context/request_includes.rs` —
/// the two constants are one ABI and must agree.
const COMPILER_INCLUDED: u64 = 2;
const FRAME: usize = 64;
const PATH: usize = 8;
const LENGTH: usize = 16;
const CURSOR: usize = 24;
const REMAINING: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{AppleVariant, Platform, Target};

    fn helper_asm(target: Target, count: usize) -> String {
        let catalog = crate::ir::SourceCatalog::from_units((0..count).map(|index| crate::resolver::SourceUnit {
            canonical_path: format!("/sources/{index}.php").into(),
            mode: crate::source::SourceMode::Php, source: "<?php".into(),
        })).unwrap();
        let mut module = Module::with_source_catalog(target, catalog);
        module.required_runtime_features.eval_bridge = true;
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        emit_state_helpers(&module, &mut emitter, &mut data);
        for (_, source) in module.source_catalog().unwrap().iter() {
            assert!(data.has_comm(&include_guard_symbol(&source.canonical_path)));
        }
        emitter.output()
    }

    #[test]
    fn native_include_lookup_is_compact_on_every_supported_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
            Target { apple_variant: AppleVariant::IOS, ..Target::new(Platform::MacOS, Arch::AArch64) },
            Target { apple_variant: AppleVariant::IOSSimulator, ..Target::new(Platform::MacOS, Arch::AArch64) },
        ] {
            let small = helper_asm(target, 1);
            let large = helper_asm(target, 100);
            assert!(large.len() < small.len() + 128, "lookup code must not unroll per source");
            assert!(small.contains(LOOKUP) && small.contains(RESET));
            match target.arch {
                Arch::AArch64 => assert!(small.contains("cbnz w0")),
                Arch::X86_64 => assert!(small.contains("test eax, eax")),
            }
            assert!(helper_asm(target, 0).contains(LOOKUP));
        }
    }

    #[cfg(unix)]
    #[test]
    fn native_include_callbacks_execute_on_host() {
        use std::os::unix::ffi::OsStringExt;
        struct Fixture { path: std::path::PathBuf, temporary: bool }
        impl Drop for Fixture {
            fn drop(&mut self) { if self.temporary { let _ = std::fs::remove_dir_all(&self.path); } }
        }
        let (path, temporary): (std::path::PathBuf, bool) = match std::env::var_os("ELEPHC_INCLUDE_ABI_FIXTURE_DIR") {
            Some(path) => (path.into(), false),
            None => (std::env::temp_dir().join(format!("elephc-include-abi-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())), true),
        };
        // Acquire ownership before installing cleanup, never delete a pre-existing directory.
        if temporary { std::fs::create_dir(&path).unwrap(); }
        else { std::fs::create_dir_all(&path).unwrap(); }
        let fixture = Fixture { path, temporary };
        let c = fixture.path.join("check.c");
        std::fs::write(&c, r#"
typedef unsigned long long u64;
extern u64 *lookup(const unsigned char *, u64) __asm__("__rt_source_include_lookup");
extern void reset(void) __asm__("__rt_source_include_reset");
extern void prime(void) __asm__("__rt_source_include_prime");
int main(void) {
    const unsigned char a[] = "/sources/0.php";
    const unsigned char b[] = "/sources/1.php";
    const unsigned char absent[] = "/sources/x.php";
    const unsigned char raw[] = {'/', 's', 255};
    u64 *first = lookup(a, sizeof(a)-1), *second = lookup(b, sizeof(b)-1);
    u64 *third = lookup(raw, sizeof(raw));
    if (!first || !second || !third || first == second || second == third || first == third) return 1;
    if (*first || *second || *third) return 2;
    if (lookup(absent, sizeof(absent)-1) || lookup(a, 0)) return 3;
    *first = 1; *second = 1; *third = 1;
    if (lookup(a, sizeof(a)-1) != first || *lookup(a, sizeof(a)-1) != 1) return 4;
    reset();
    if (*first || *second || *third) return 5;
    /* `/sources/1.php` is the only compiler-included source, and prime must raise ITS cell to
       the distinguishing value, leaving every runtime-only source at zero. */
    prime();
    if (*second != 2) return 6;
    if (*first || *third) return 7;
    reset();
    return (*first || *second || *third) ? 8 : 0;
}
"#).unwrap();
        let host = Target::detect_host();
        for target in [host, Target::new(Platform::Linux, Arch::AArch64), Target::new(Platform::Linux, Arch::X86_64)] {
            let paths = [std::path::PathBuf::from("/sources/0.php"), "/sources/1.php".into(),
                std::ffi::OsString::from_vec(vec![b'/', b's', 255]).into()];
            let catalog = crate::ir::SourceCatalog::from_units(paths.map(|path| crate::resolver::SourceUnit {
                canonical_path: path, mode: crate::source::SourceMode::Php, source: "<?php".into(),
            })).unwrap();
            let mut module = Module::with_source_catalog(target, catalog);
            module.required_runtime_features.eval_bridge = true;
            module.preincluded_sources.insert(std::path::PathBuf::from("/sources/1.php"));
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            emit_state_helpers(&module, &mut emitter, &mut data);
            let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let asm = fixture.path.join(format!("{target}.s"));
            std::fs::write(&asm, format!(".text\n{syntax}{}{}", emitter.output(), data.emit(target))).unwrap();
        }
        let executable = fixture.path.join("check");
        let compile = std::process::Command::new("cc").arg(&c).arg(fixture.path.join(format!("{host}.s")))
            .arg("-o").arg(&executable).output().unwrap();
        assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
        let run = std::process::Command::new(&executable).output().unwrap();
        assert!(run.status.success(), "callback ABI failed: {:?}, {}", run.status.code(), String::from_utf8_lossy(&run.stderr));
    }

    /// Records sharing a key lie on one probe path in table order, so a lookup's first match is
    /// the record the linear scan used to find, and a raise-all walk meets every one of them.
    #[test]
    fn key_index_keeps_table_order_on_each_probe_path() {
        let keys = ["a", "b", "a", "c", "a", "", "b"].map(|key| key.as_bytes().to_vec());
        let slots = key_index_slots(&keys);
        assert!(slots.len().is_power_of_two() && slots.len() >= keys.len() * 2);
        let mask = slots.len() - 1;
        for key in &keys {
            let mut slot = key_hash(key) as usize & mask;
            let mut seen = Vec::new();
            while slots[slot] != 0 {
                let record = slots[slot] as usize - 1;
                if keys[record] == *key { seen.push(record); }
                slot = (slot + 1) & mask;
            }
            let expected = keys.iter().enumerate().filter(|(_, other)| *other == key).map(|(index, _)| index).collect::<Vec<_>>();
            assert_eq!(seen, expected, "key {key:?}");
        }
        assert_eq!(key_index_slots(&[]), vec![0], "an empty table still ends every probe");
    }

    /// Runs the lookup over enough sources that home slots collide, on the host.
    #[cfg(unix)]
    #[test]
    fn native_include_lookup_finds_every_source_of_a_large_table() {
        const COUNT: usize = 500;
        let dir = std::env::temp_dir().join(format!("elephc-include-index-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        let c = dir.join("check.c");
        std::fs::write(&c, format!(r#"
#include <stdio.h>
#include <string.h>
typedef unsigned long long u64;
extern u64 *lookup(const char *, u64) __asm__("__rt_source_include_lookup");
int main(void) {{
    static u64 *cells[{COUNT}];
    char path[64];
    for (int i = 0; i < {COUNT}; i++) {{
        snprintf(path, sizeof(path), "/sources/%d.php", i);
        cells[i] = lookup(path, strlen(path));
        if (!cells[i]) return 1;
        *cells[i] = (u64)i + 10;
    }}
    for (int i = 0; i < {COUNT}; i++) {{
        snprintf(path, sizeof(path), "/sources/%d.php", i);
        if (lookup(path, strlen(path)) != cells[i] || *cells[i] != (u64)i + 10) return 2;
    }}
    if (lookup("/sources/{COUNT}.php", strlen("/sources/{COUNT}.php"))) return 3;
    if (lookup("/sources/1.ph", strlen("/sources/1.ph")) || lookup("/sources/1.phpx", strlen("/sources/1.phpx"))) return 4;
    return 0;
}}
"#)).unwrap();
        let host = Target::detect_host();
        let mut emitter = Emitter::new(host);
        let mut data = DataSection::new();
        let catalog = crate::ir::SourceCatalog::from_units((0..COUNT).map(|index| crate::resolver::SourceUnit {
            canonical_path: format!("/sources/{index}.php").into(),
            mode: crate::source::SourceMode::Php, source: "<?php".into(),
        })).unwrap();
        let mut module = Module::with_source_catalog(host, catalog);
        module.required_runtime_features.eval_bridge = true;
        emit_state_helpers(&module, &mut emitter, &mut data);
        let syntax = if host.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
        let source = dir.join("helpers.s");
        std::fs::write(&source, format!(".text\n{syntax}{}{}", emitter.output(), data.emit(host))).unwrap();
        let executable = dir.join("check");
        let compile = std::process::Command::new("cc").arg(&c).arg(&source).arg("-o").arg(&executable).output().unwrap();
        let run = compile.status.success().then(|| std::process::Command::new(&executable).output().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
        let run = run.unwrap();
        assert!(run.status.success(), "lookup over {COUNT} sources failed: {:?}", run.status.code());
    }
}

/// Emits one compact table and two callbacks, independent of the number of include sites.
pub(in crate::codegen) fn emit_state_helpers(module: &Module, emitter: &mut Emitter, data: &mut DataSection) {
    // A closed-world build with runtime includes keeps the guard cells, the prime and its own
    // lookup, and none of the callbacks only the interpreter calls.
    let closed = module.closed_world && module.closed_includes.is_some();
    if !module.required_runtime_features.eval_bridge && !closed { return; }
    let mut words = Vec::new();
    let mut keys = Vec::new();
    // With no interpreter, a declaration file that still has activation cells to raise is not
    // primed as "the compiler ran it": its first include -- the autoloader's, or the compiled
    // `include_once` it came from -- must run the routine that raises them
    // (`closed_include::declaration_activation_cells`).
    let activated = if closed && !module.required_runtime_features.eval_bridge {
        super::closed_include::declaration_activation_cells(module)
    } else {
        Default::default()
    };
    if let Some(catalog) = module.source_catalog() {
        for (_, source) in catalog.iter() {
            let key = source.canonical_path.as_os_str().as_encoded_bytes().to_vec();
            let (path, length) = data.add_string(&key);
            keys.push(key);
            let cell = data.add_comm(include_guard_symbol(&source.canonical_path), 8);
            let preincluded = if module.preincluded_sources.contains(&source.canonical_path)
                && !activated.contains_key(&source.canonical_path)
            {
                COMPILER_INCLUDED
            } else {
                0
            };
            words.extend([
                DataWord::Symbol(path),
                DataWord::U64(length as u64),
                DataWord::Symbol(cell),
                DataWord::U64(preincluded),
            ]);
        }
    }
    let count = words.len() / 4;
    let table = data.add_words(words);
    data.add_comm(PRIMED.to_string(), 8);
    if closed && !module.required_runtime_features.eval_bridge {
        emit_prime(emitter, &table, count);
        super::closed_include::emit_closed_include(module, emitter, data);
        return;
    }
    emit_lookup(emitter, data, LOOKUP, "source_lookup", &table, &keys, OnMatch::Return);
    emit_reset(emitter, RESET, "source_reset", &table, count, Some(PRIMED));
    emit_prime(emitter, &table, count);
    emit_deferred_class_helpers(module, emitter, data);
    emit_source_activate(module, emitter, data);
    emit_include_unit_lookup(module, emitter, data);
}

/// Label of the include-unit lookup: string path in -> unit record or 0 out.
pub(in crate::codegen) const INCLUDE_UNIT_LOOKUP: &str = "__rt_include_unit_lookup";

/// Byte offset of a unit record's function address.
pub(in crate::codegen) const INCLUDE_UNIT_FUNCTION_OFFSET: usize = 16;

/// Byte offset of a unit record's guard-cell address.
pub(in crate::codegen) const INCLUDE_UNIT_GUARD_OFFSET: usize = 24;

/// Label of the include-unit pre-filter: string path in (preserved) -> 1 when its last component
/// names a unit, 0 otherwise.
pub(in crate::codegen) const INCLUDE_UNIT_CANDIDATE: &str = "__rt_include_unit_candidate";

/// Bytes per include-unit record.
const UNIT_RECORD: usize = 48;

/// Emits the include-unit table, `__rt_include_unit_candidate` and `__rt_include_unit_lookup`.
///
/// One 48-byte record per unit: canonical path, its length, the unit function, the guard cell the
/// source table already owns for that path (so a unit run and an interpreted include of the same
/// file mark ONE cell), and where the path's last component starts. The lookup takes the
/// canonical path as a string (x1/x2, rax/rdx) and answers the record or 0. The table holds only
/// the few files the preload script named, so it is scanned rather than hashed.
///
/// The candidate test exists because canonicalizing is the expensive half: a request's other
/// dynamic includes -- Composer's `files`, autoloaded classes -- would each pay a `realpath` only to
/// miss. A file keeps its last component through canonicalization unless that component is itself
/// a symlink, and such a file simply is not recognized as a unit: it is interpreted, as before.
fn emit_include_unit_lookup(module: &Module, emitter: &mut Emitter, data: &mut DataSection) {
    if module.include_units.is_empty() {
        return;
    }
    let cataloged = |path: &std::path::Path| {
        module
            .source_catalog()
            .is_some_and(|catalog| catalog.iter().any(|(_, source)| source.canonical_path == path))
    };
    let mut words = Vec::new();
    for unit in &module.include_units {
        if !cataloged(&unit.canonical_path) {
            continue;
        }
        let key = unit.canonical_path.as_os_str().as_encoded_bytes().to_vec();
        let (path, length) = data.add_string(&key);
        let basename_offset = key.iter().rposition(|byte| *byte == b'/').map_or(0, |slash| slash + 1);
        words.extend([
            DataWord::Symbol(path),
            DataWord::U64(length as u64),
            DataWord::Symbol(crate::names::function_symbol(&unit.function)),
            DataWord::Symbol(include_guard_symbol(&unit.canonical_path)),
            DataWord::U64(basename_offset as u64),
            DataWord::U64(0),
        ]);
    }
    let count = words.len() / 6;
    let table = if count == 0 { None } else { Some(data.add_words(words)) };
    emit_include_unit_candidate(emitter, table.as_deref(), count);
    emitter.blank();
    emitter.comment("--- runtime: include unit lookup ---");
    if emitter.target.arch == Arch::AArch64 { emitter.raw(".align 2"); }
    emitter.label_global(INCLUDE_UNIT_LOOKUP);
    let Some(table) = table else {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #0"),                 // no unit reachable
            Arch::X86_64 => emitter.instruction("xor eax, eax"),                // no unit reachable
        }
        abi::emit_return(emitter);
        return;
    };
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", &table);
            emitter.instruction(&format!("mov x10, #{count}"));                 // records left to inspect
            emitter.label("__rt_include_unit_lookup_record");
            emitter.instruction("cbz x10, __rt_include_unit_lookup_miss");      // every record inspected
            emitter.instruction("ldr x11, [x9, #8]");                           // this record's path length
            emitter.instruction("cmp x11, x2");                                 // same length as the requested path?
            emitter.instruction("b.ne __rt_include_unit_lookup_next");
            emitter.instruction("ldr x12, [x9]");                               // this record's path bytes
            emitter.instruction("mov x13, #0");                                 // byte index
            emitter.label("__rt_include_unit_lookup_byte");
            emitter.instruction("cmp x13, x2");                                 // every byte matched?
            emitter.instruction("b.eq __rt_include_unit_lookup_hit");
            emitter.instruction("ldrb w14, [x12, x13]");                        // record byte
            emitter.instruction("ldrb w15, [x1, x13]");                         // requested byte
            emitter.instruction("cmp w14, w15");
            emitter.instruction("b.ne __rt_include_unit_lookup_next");
            emitter.instruction("add x13, x13, #1");
            emitter.instruction("b __rt_include_unit_lookup_byte");
            emitter.label("__rt_include_unit_lookup_next");
            emitter.instruction(&format!("add x9, x9, #{UNIT_RECORD}"));        // next record
            emitter.instruction("sub x10, x10, #1");
            emitter.instruction("b __rt_include_unit_lookup_record");
            emitter.label("__rt_include_unit_lookup_hit");
            emitter.instruction("mov x0, x9");                                  // answer the matching record
            emitter.instruction("ret");
            emitter.label("__rt_include_unit_lookup_miss");
            emitter.instruction("mov x0, #0");                                  // no unit for this path
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r9", &table);
            emitter.instruction(&format!("mov r10, {count}"));                  // records left to inspect
            emitter.label("__rt_include_unit_lookup_record");
            emitter.instruction("test r10, r10");                               // every record inspected?
            emitter.instruction("jz __rt_include_unit_lookup_miss");
            emitter.instruction("cmp QWORD PTR [r9 + 8], rdx");                 // same length as the requested path?
            emitter.instruction("jne __rt_include_unit_lookup_next");
            emitter.instruction("mov r11, QWORD PTR [r9]");                     // this record's path bytes
            emitter.instruction("xor ecx, ecx");                                // byte index
            emitter.label("__rt_include_unit_lookup_byte");
            emitter.instruction("cmp rcx, rdx");                                // every byte matched?
            emitter.instruction("je __rt_include_unit_lookup_hit");
            emitter.instruction("mov r8b, BYTE PTR [r11 + rcx]");               // record byte
            emitter.instruction("cmp r8b, BYTE PTR [rax + rcx]");               // against the requested byte
            emitter.instruction("jne __rt_include_unit_lookup_next");
            emitter.instruction("inc rcx");
            emitter.instruction("jmp __rt_include_unit_lookup_byte");
            emitter.label("__rt_include_unit_lookup_next");
            emitter.instruction(&format!("add r9, {UNIT_RECORD}"));             // next record
            emitter.instruction("dec r10");
            emitter.instruction("jmp __rt_include_unit_lookup_record");
            emitter.label("__rt_include_unit_lookup_hit");
            emitter.instruction("mov rax, r9");                                 // answer the matching record
            emitter.instruction("ret");
            emitter.label("__rt_include_unit_lookup_miss");
            emitter.instruction("xor eax, eax");                                // no unit for this path
            emitter.instruction("ret");
        }
    }
}

/// What a table scan does with a record whose path matches.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::codegen) enum OnMatch {
    /// Return that record's cell address, or null when none matched.
    Return,
    /// Return the matching RECORD's address (so the caller reads more than the cell), or null.
    Record,
    /// Write 1 into the cell of EVERY matching record, then return.
    RaiseAll,
}

/// Emits `__elephc_eval_source_activate`, which does to class-like LOAD state what including one
/// compiler-included source does.
///
/// The autoload pass splices a file's declarations where the compiled program first names one of
/// them, and that is where its `ClassLikeActivate` event runs. PHP loads the file earlier whenever
/// something dynamic asks first -- an interpreted `interface X extends Y`, an `implements`, a
/// `class_exists()` -- and the autoloader's `include` of it is then skipped on the compiler's
/// mark. That skip is the moment php declares the file's symbols, so it raises the cells the
/// spliced event would: each interface's activation cell and each probe-only class's load flag.
/// The spliced event later finds its cell raised and, for a pre-included source, accepts that.
fn emit_source_activate(module: &Module, emitter: &mut Emitter, data: &mut DataSection) {
    let mut words = Vec::new();
    let mut keys = Vec::new();
    let mut add = |data: &mut DataSection, path: &std::path::Path, cell: String| {
        let key = path.as_os_str().as_encoded_bytes().to_vec();
        let (label, length) = data.add_string(&key);
        keys.push(key);
        let cell = data.add_comm(cell, 8);
        words.extend([
            DataWord::Symbol(label),
            DataWord::U64(length as u64),
            DataWord::Symbol(cell),
            DataWord::U64(0),
        ]);
    };
    for (path, cell) in preincluded_interface_activation_cells(module) {
        add(data, &path, cell);
    }
    // Name order: the map's own order changes with every process's hash seed, and so did the table.
    let mut declared = module.declared_class_source_files.iter().collect::<Vec<_>>();
    declared.sort();
    for (name, file) in declared {
        let folded = name.trim_start_matches('\\').to_ascii_lowercase();
        if !module.deferred_class_loads.contains(&folded) {
            continue;
        }
        // Same canonicalization as `autoload::record_compile_time_inclusions`, which chose these.
        let declared = std::path::PathBuf::from(file);
        let declared = declared.canonicalize().unwrap_or(declared);
        if module.preincluded_sources.contains(&declared) {
            add(data, &declared, deferred_class_symbol(&folded));
        }
    }
    let table = data.add_words(words);
    let symbol = emitter.target.extern_symbol(SOURCE_ACTIVATE);
    emit_lookup(emitter, data, &symbol, "source_activate", &table, &keys, OnMatch::RaiseAll);
}

/// Every interface activation event belonging to a source the autoload pass pre-included, as
/// (canonical source path, activation cell symbol), deduplicated.
pub(super) fn preincluded_interface_activation_cells(module: &Module) -> Vec<(std::path::PathBuf, String)> {
    let Some(catalog) = module.source_catalog() else { return Vec::new() };
    let mut cells = std::collections::BTreeSet::new();
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        for instruction in &function.instructions {
            let Some(crate::ir::Immediate::ClassLikeActivation(activation)) =
                &instruction.immediate
            else {
                continue;
            };
            let (source, kind, name) = (&activation.source, &activation.kind, &activation.name);
            if *kind != crate::parser::ast::ClassLikeKind::Interface {
                continue;
            }
            let Some(unit) = catalog.get(*source) else { continue };
            if !module.preincluded_sources.contains(&unit.canonical_path) {
                continue;
            }
            let Some(name) = module.data.strings.get(name.as_raw() as usize) else { continue };
            cells.insert((
                unit.canonical_path.clone(),
                crate::names::classlike_activation_symbol(*kind, name),
            ));
        }
    }
    cells.into_iter().collect()
}

/// Emits the load-state table for classes the closed world carries only to answer a probe.
///
/// Same shape as the source table: one record of {name pointer, name length, flag cell,
/// placeholder} so both share the compact lookup and reset loops. Names are LOWERCASE, because
/// php class names are case-insensitive and the caller lowercases before asking.
fn emit_deferred_class_helpers(module: &Module, emitter: &mut Emitter, data: &mut DataSection) {
    let mut words = Vec::new();
    let mut keys = Vec::new();
    for name in &module.deferred_class_loads {
        let (label, length) = data.add_string(name.as_bytes());
        keys.push(name.as_bytes().to_vec());
        let cell = data.add_comm(deferred_class_symbol(name), 8);
        words.extend([
            DataWord::Symbol(label),
            DataWord::U64(length as u64),
            DataWord::Symbol(cell),
            DataWord::U64(0),
        ]);
    }
    let count = words.len() / 4;
    let table = data.add_words(words);
    let lookup_symbol = emitter.target.extern_symbol(CLASS_LOOKUP);
    emit_lookup(emitter, data, &lookup_symbol, "class_lookup", &table, &keys, OnMatch::Return);
    emit_reset(emitter, CLASS_RESET, "class_reset", &table, count, None);
}

/// Encodes one lowercase class name into an assembly-safe, collision-free flag symbol.
pub(in crate::codegen) fn deferred_class_symbol(name: &str) -> String {
    use std::fmt::Write;
    let mut symbol = String::from("_class_deferred_");
    for byte in name.as_bytes() {
        let _ = write!(symbol, "{byte:02x}");
    }
    symbol
}

/// Installs the program's state access before PHP execution; also safe to repeat at eval entry.
pub(in crate::codegen) fn emit_state_install(ctx: &mut FunctionContext<'_>) {
    if !ctx.module.required_runtime_features.eval_bridge {
        // No interpreter to register the table with: a closed-world build only raises the
        // compiler-included guards its own runtime include consults.
        if ctx.module.closed_world && ctx.module.closed_includes.is_some() {
            abi::emit_call_label(ctx.emitter, PRIME);
        }
        return;
    }
    let a0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let a1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let a2 = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_int_immediate(ctx.emitter, a0, 0);
    abi::emit_symbol_address(ctx.emitter, a1, LOOKUP);
    abi::emit_symbol_address(ctx.emitter, a2, RESET);
    let install = ctx.emitter.target.extern_symbol("__elephc_eval_register_native_include_state");
    abi::emit_call_label(ctx.emitter, &install);
    let ready = ctx.next_label("include_state_ready");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &ready);
    abi::emit_exit(ctx.emitter, 1);
    ctx.emitter.label(&ready);
    // Re-assert the inclusions the COMPILER performed. This runs at CLI entry, at every `--web`
    // request (after `__rt_web_reset` cleared the table) and at eval entry, and only ever raises
    // a flag the request reset lowered, so repeating it is exactly a no-op.
    abi::emit_call_label(ctx.emitter, PRIME);
}

/// FNV-1a offset basis of the key hash; the emitted lookup computes the same hash at run time.
const KEY_HASH_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a prime of the key hash.
const KEY_HASH_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Hashes one record key (a canonical path or a lowercase class name) byte by byte.
fn key_hash(key: &[u8]) -> u64 {
    key.iter()
        .fold(KEY_HASH_BASIS, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(KEY_HASH_PRIME))
}

/// Builds the open-addressing index over a table's record keys: a power-of-two number of slots,
/// each holding a record index plus one, or 0 when empty.
///
/// The index is at most half full, so a probe always reaches an empty slot. Records are inserted
/// in table order with linear probing, so records sharing a key lie on one probe path in table
/// order: the first match is the one the linear scan used to find, and a raise-all walk meets
/// every one of them before the empty slot that ends it.
pub(super) fn key_index_slots(keys: &[Vec<u8>]) -> Vec<u64> {
    let size = (keys.len() * 2).max(1).next_power_of_two();
    let mask = size - 1;
    let mut slots = vec![0u64; size];
    for (index, key) in keys.iter().enumerate() {
        let mut slot = (key_hash(key) as usize) & mask;
        while slots[slot] != 0 {
            slot = (slot + 1) & mask;
        }
        slots[slot] = index as u64 + 1;
    }
    slots
}

/// Emits a lookup callback over `table`, whose record `i` has the key `keys[i]`.
///
/// The key is hashed once and the index probed from its home slot; each candidate record is
/// compared by length and then `memcmp`. The table used to be scanned from its first record,
/// which for the source table meant every source of the closed world (thousands for Symfony) on
/// every interpreted include.
pub(in crate::codegen) fn emit_lookup(
    emitter: &mut Emitter,
    data: &mut DataSection,
    symbol: &str,
    tag: &str,
    table: &str,
    keys: &[Vec<u8>],
    on_match: OnMatch,
) {
    let slots = key_index_slots(keys);
    let mask = (slots.len() - 1) as i64;
    let index = data.add_words(slots.into_iter().map(DataWord::U64).collect());
    let prefix = emitter.target.platform.local_label_prefix();
    let hash_loop = format!("{prefix}{tag}_hash");
    let hashed = format!("{prefix}{tag}_hashed");
    let probe = format!("{prefix}{tag}_probe");
    let next = format!("{prefix}{tag}_next");
    let missing = format!("{prefix}{tag}_missing");
    let done = format!("{prefix}{tag}_done");
    if emitter.target.arch == Arch::AArch64 { emitter.raw(".align 2"); }
    emitter.label_global(symbol);
    abi::emit_frame_prologue(emitter, FRAME);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::store_at_offset(emitter, "x0", PATH);
            abi::store_at_offset(emitter, "x1", LENGTH);
            abi::emit_load_int_immediate(emitter, "x11", KEY_HASH_BASIS as i64);
            abi::emit_load_int_immediate(emitter, "x12", KEY_HASH_PRIME as i64);
            emitter.instruction("mov x13, #0");                                 // x13 = byte index within the requested key
            emitter.label(&hash_loop);
            emitter.instruction("cmp x13, x1");                                 // has every byte of the key been hashed?
            emitter.instruction(&format!("b.hs {hashed}"));                     // yes — probe the index
            emitter.instruction("ldrb w10, [x0, x13]");                         // load the next key byte
            emitter.instruction("eor x11, x11, x10");                           // mix the byte into the hash
            emitter.instruction("mul x11, x11, x12");                           // multiply by the FNV prime
            emitter.instruction("add x13, x13, #1");                            // advance to the next byte
            emitter.instruction(&format!("b {hash_loop}"));                     // keep hashing
            emitter.label(&hashed);
            abi::emit_load_int_immediate(emitter, "x12", mask);
            emitter.instruction("and x11, x11, x12");                           // x11 = the key's home slot
            abi::store_at_offset(emitter, "x11", CURSOR);
            emitter.label(&probe);
            abi::load_at_offset(emitter, "x11", CURSOR);
            abi::emit_symbol_address(emitter, "x12", &index);
            emitter.instruction("ldr x13, [x12, x11, lsl #3]");                 // x13 = record index + 1 in this slot, or 0 when empty
            emitter.instruction(&format!("cbz x13, {missing}"));                // an empty slot ends the probe
            emitter.instruction("sub x13, x13, #1");                            // x13 = record index
            abi::emit_symbol_address(emitter, "x12", table);
            emitter.instruction(&format!("add x12, x12, x13, lsl #{}", RECORD.trailing_zeros())); // x12 = the candidate record
            abi::store_at_offset(emitter, "x12", REMAINING);
            emitter.instruction("ldr x2, [x12, #8]");                           // x2 = candidate key length
            abi::load_at_offset(emitter, "x10", LENGTH);
            emitter.instruction("cmp x2, x10");                                 // compare the requested and candidate lengths
            emitter.instruction(&format!("b.ne {next}"));                       // different lengths cannot match
            emitter.instruction("ldr x1, [x12]");                               // x1 = candidate key bytes
            abi::load_at_offset(emitter, "x0", PATH);
        }
        Arch::X86_64 => {
            abi::store_at_offset(emitter, "rdi", PATH);
            abi::store_at_offset(emitter, "rsi", LENGTH);
            emitter.instruction(&format!("mov r11, {KEY_HASH_BASIS:#x}"));      // r11 = FNV-1a offset basis
            emitter.instruction(&format!("mov rcx, {KEY_HASH_PRIME:#x}"));      // rcx = FNV-1a prime
            emitter.instruction("xor edx, edx");                                // rdx = byte index within the requested key
            emitter.label(&hash_loop);
            emitter.instruction("cmp rdx, rsi");                                // has every byte of the key been hashed?
            emitter.instruction(&format!("jae {hashed}"));                      // yes — probe the index
            emitter.instruction("movzx eax, BYTE PTR [rdi + rdx]");             // load the next key byte
            emitter.instruction("xor r11, rax");                                // mix the byte into the hash
            emitter.instruction("imul r11, rcx");                               // multiply by the FNV prime
            emitter.instruction("add rdx, 1");                                  // advance to the next byte
            emitter.instruction(&format!("jmp {hash_loop}"));                   // keep hashing
            emitter.label(&hashed);
            emitter.instruction(&format!("mov rcx, {mask}"));                   // rcx = index size - 1
            emitter.instruction("and r11, rcx");                                // r11 = the key's home slot
            abi::store_at_offset(emitter, "r11", CURSOR);
            emitter.label(&probe);
            abi::load_at_offset(emitter, "r11", CURSOR);
            abi::emit_symbol_address(emitter, "r10", &index);
            emitter.instruction("mov rax, QWORD PTR [r10 + r11 * 8]");          // rax = record index + 1 in this slot, or 0 when empty
            emitter.instruction("test rax, rax");                               // is the slot empty?
            emitter.instruction(&format!("jz {missing}"));                      // an empty slot ends the probe
            emitter.instruction("sub rax, 1");                                  // rax = record index
            emitter.instruction(&format!("shl rax, {}", RECORD.trailing_zeros())); // rax = byte offset of the record
            abi::emit_symbol_address(emitter, "r10", table);
            emitter.instruction("add r10, rax");                                // r10 = the candidate record
            abi::store_at_offset(emitter, "r10", REMAINING);
            emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                // rdx = candidate key length
            abi::load_at_offset(emitter, "rax", LENGTH);
            emitter.instruction("cmp rdx, rax");                                // compare the requested and candidate lengths
            emitter.instruction(&format!("jne {next}"));                        // different lengths cannot match
            emitter.instruction("mov rsi, QWORD PTR [r10]");                    // rsi = candidate key bytes
            abi::load_at_offset(emitter, "rdi", PATH);
        }
    }
    let compare = emitter.target.extern_symbol("memcmp");
    abi::emit_call_label(emitter, &compare);
    let result = abi::int_result_reg(emitter);
    let a2 = abi::int_arg_reg_name(emitter.target, 2);
    let scratch = abi::temp_int_reg(emitter.target);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cbnz w0, {next}")),      // test the C int result without depending on upper bits
        Arch::X86_64 => {
            emitter.instruction("test eax, eax");                               // test the C int result without depending on upper bits
            emitter.instruction(&format!("jnz {next}"));                        // continue when the key bytes differ
        }
    }
    abi::load_at_offset(emitter, result, REMAINING);
    if on_match == OnMatch::Return {
        abi::emit_load_from_address(emitter, result, result, 16);
        abi::emit_jump(emitter, &done);
    } else if on_match == OnMatch::Record {
        abi::emit_jump(emitter, &done);
    } else {
        abi::emit_load_from_address(emitter, a2, result, 16);
        abi::emit_load_int_immediate(emitter, scratch, 1);
        abi::emit_store_to_address(emitter, scratch, a2, 0);
    }
    emitter.label(&next);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x11", CURSOR);
            emitter.instruction("add x11, x11, #1");                            // step to the next slot on the probe path
            abi::emit_load_int_immediate(emitter, "x12", mask);
            emitter.instruction("and x11, x11, x12");                           // wrap around the end of the index
            abi::store_at_offset(emitter, "x11", CURSOR);
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r11", CURSOR);
            emitter.instruction("add r11, 1");                                  // step to the next slot on the probe path
            emitter.instruction(&format!("mov rcx, {mask}"));                   // rcx = index size - 1
            emitter.instruction("and r11, rcx");                                // wrap around the end of the index
            abi::store_at_offset(emitter, "r11", CURSOR);
        }
    }
    abi::emit_jump(emitter, &probe);
    emitter.label(&missing);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(&done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Emits a reset callback clearing every guard of `table`, and `primed_flag` too when given (a
/// lowered guard makes the next prime do real work again).
fn emit_reset(
    emitter: &mut Emitter,
    symbol: &str,
    tag: &str,
    table: &str,
    count: usize,
    primed_flag: Option<&str>,
) {
    let prefix = emitter.target.platform.local_label_prefix();
    let again = format!("{prefix}{tag}_again");
    let done = format!("{prefix}{tag}_done");
    let cursor = abi::int_result_reg(emitter);
    let remaining = abi::int_arg_reg_name(emitter.target, 1);
    let cell = abi::temp_int_reg(emitter.target);
    if emitter.target.arch == Arch::AArch64 { emitter.raw(".align 2"); }
    emitter.label_global(symbol);
    abi::emit_frame_prologue(emitter, 16);
    if let Some(flag) = primed_flag {
        abi::emit_store_zero_to_symbol(emitter, flag, 0);
    }
    abi::emit_symbol_address(emitter, cursor, table);
    abi::emit_load_int_immediate(emitter, remaining, count as i64);
    emitter.label(&again);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {remaining}, {done}"));           // stop after every native source cell was reset
            emitter.instruction(&format!("ldr {cell}, [{cursor}, #16]"));       // load the current record's guard address
            emitter.instruction(&format!("str xzr, [{cell}]"));                 // clear actual native inclusion state
            emitter.instruction(&format!("add {cursor}, {cursor}, #{RECORD}")); // advance one source record
            emitter.instruction(&format!("sub {remaining}, {remaining}, #1"));  // consume one source record
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {remaining}, {remaining}"));     // test whether any source records remain
            emitter.instruction(&format!("jz {done}"));                         // stop after every native source cell was reset
            emitter.instruction(&format!("mov {cell}, QWORD PTR [{cursor} + 16]")); // load the current record's guard address
            emitter.instruction(&format!("mov QWORD PTR [{cell}], 0"));         // clear actual native inclusion state
            emitter.instruction(&format!("add {cursor}, {RECORD}"));            // advance one source record
            emitter.instruction(&format!("sub {remaining}, 1"));                // consume one source record
        }
    }
    abi::emit_jump(emitter, &again);
    emitter.label(&done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

/// Emits `__rt_source_include_prime`, which raises the guard of every source the COMPILER
/// already included.
///
/// The autoload pass opened those files at compile time and spliced their declarations into the
/// program, which is the same observable outcome PHP reaches by running the autoloader: the
/// symbols exist and the file counts as included. Without this, an interpreted `include_once`
/// of such a path found its guard clear, re-ran the file, and died redeclaring a class the
/// binary already carries.
///
/// Only raises flags — never clears one — so it is safe to repeat at every entry that installs
/// the include state.
///
/// It runs at every eval entry, and the table holds every source of the closed world (thousands
/// for Symfony), so a repeat walked it all to raise guards that were already raised: 2.4% of a
/// Symfony request. `PRIMED` records that nothing lowered a guard since the last walk.
fn emit_prime(emitter: &mut Emitter, table: &str, count: usize) {
    let prefix = emitter.target.platform.local_label_prefix();
    let again = format!("{prefix}source_prime_again");
    let next = format!("{prefix}source_prime_next");
    let done = format!("{prefix}source_prime_done");
    let already = format!("{prefix}source_prime_already");
    let cursor = abi::int_result_reg(emitter);
    let remaining = abi::int_arg_reg_name(emitter.target, 1);
    let cell = abi::temp_int_reg(emitter.target);
    let flag = abi::int_arg_reg_name(emitter.target, 2);
    if emitter.target.arch == Arch::AArch64 { emitter.raw(".align 2"); }
    emitter.label_global(PRIME);
    abi::emit_load_symbol_to_reg(emitter, cell, PRIMED, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {cell}, {already}"));            // every compiler-included guard is still raised
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {cell}, {cell}"));               // has a reset lowered a guard since the last walk?
            emitter.instruction(&format!("jnz {already}"));                     // no — every compiler-included guard is still raised
        }
    }
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_symbol_address(emitter, cursor, table);
    abi::emit_load_int_immediate(emitter, remaining, count as i64);
    emitter.label(&again);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {remaining}, {done}"));           // stop after every source record was inspected
            emitter.instruction(&format!("ldr {flag}, [{cursor}, #24]"));       // load the compiler-included flag
            emitter.instruction(&format!("cbz {flag}, {next}"));                // leave a runtime-only source untouched
            emitter.instruction(&format!("ldr {cell}, [{cursor}, #16]"));       // load the current record's guard address
            emitter.instruction(&format!("str {flag}, [{cell}]"));              // record the compile-time inclusion
            emitter.label(&next);
            emitter.instruction(&format!("add {cursor}, {cursor}, #{RECORD}")); // advance one source record
            emitter.instruction(&format!("sub {remaining}, {remaining}, #1"));  // consume one source record
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {remaining}, {remaining}"));     // test whether any source records remain
            emitter.instruction(&format!("jz {done}"));                         // stop after every source record was inspected
            emitter.instruction(&format!("mov {flag}, QWORD PTR [{cursor} + 24]")); // load the compiler-included flag
            emitter.instruction(&format!("test {flag}, {flag}"));               // test the compiler-included flag
            emitter.instruction(&format!("jz {next}"));                         // leave a runtime-only source untouched
            emitter.instruction(&format!("mov {cell}, QWORD PTR [{cursor} + 16]")); // load the current record's guard address
            emitter.instruction(&format!("mov QWORD PTR [{cell}], {flag}"));    // record the compile-time inclusion
            emitter.label(&next);
            emitter.instruction(&format!("add {cursor}, {RECORD}"));            // advance one source record
            emitter.instruction(&format!("sub {remaining}, 1"));                // consume one source record
        }
    }
    abi::emit_jump(emitter, &again);
    emitter.label(&done);
    abi::emit_store_imm_to_symbol(emitter, PRIMED, 0, 1);
    abi::emit_frame_restore(emitter, 16);
    emitter.label(&already);
    abi::emit_return(emitter);
}

/// Emits `__rt_include_unit_candidate`: whether the path's last component is some unit's.
/// Preserves the path registers (x1/x2, rax/rdx) for the caller's `realpath`.
fn emit_include_unit_candidate(emitter: &mut Emitter, table: Option<&str>, count: usize) {
    emitter.blank();
    emitter.comment("--- runtime: include unit candidate ---");
    if emitter.target.arch == Arch::AArch64 { emitter.raw(".align 2"); }
    emitter.label_global(INCLUDE_UNIT_CANDIDATE);
    let Some(table) = table else {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #0"),                 // no unit reachable
            Arch::X86_64 => emitter.instruction("xor eax, eax"),                // no unit reachable
        }
        abi::emit_return(emitter);
        return;
    };
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x3, x2");                                  // scan back from the end of the path
            emitter.label("__rt_include_unit_candidate_scan");
            emitter.instruction("cbz x3, __rt_include_unit_candidate_base");    // no slash: the whole path is the name
            emitter.instruction("sub x4, x3, #1");
            emitter.instruction("ldrb w5, [x1, x4]");
            emitter.instruction("cmp w5, #47");                                 // '/'
            emitter.instruction("b.eq __rt_include_unit_candidate_base");
            emitter.instruction("mov x3, x4");
            emitter.instruction("b __rt_include_unit_candidate_scan");
            emitter.label("__rt_include_unit_candidate_base");
            emitter.instruction("add x6, x1, x3");                              // x6 = the last component
            emitter.instruction("sub x7, x2, x3");                              // x7 = its length
            abi::emit_symbol_address(emitter, "x9", table);
            emitter.instruction(&format!("mov x10, #{count}"));                 // records left to inspect
            emitter.label("__rt_include_unit_candidate_record");
            emitter.instruction("cbz x10, __rt_include_unit_candidate_miss");
            emitter.instruction("ldr x11, [x9, #8]");                           // record path length
            emitter.instruction("ldr x12, [x9, #32]");                          // where its last component starts
            emitter.instruction("sub x13, x11, x12");                           // that component's length
            emitter.instruction("cmp x13, x7");
            emitter.instruction("b.ne __rt_include_unit_candidate_next");
            emitter.instruction("ldr x14, [x9]");
            emitter.instruction("add x14, x14, x12");                           // the record's last component
            emitter.instruction("mov x13, #0");
            emitter.label("__rt_include_unit_candidate_byte");
            emitter.instruction("cmp x13, x7");
            emitter.instruction("b.eq __rt_include_unit_candidate_hit");
            emitter.instruction("ldrb w15, [x14, x13]");
            emitter.instruction("ldrb w5, [x6, x13]");
            emitter.instruction("cmp w15, w5");
            emitter.instruction("b.ne __rt_include_unit_candidate_next");
            emitter.instruction("add x13, x13, #1");
            emitter.instruction("b __rt_include_unit_candidate_byte");
            emitter.label("__rt_include_unit_candidate_next");
            emitter.instruction(&format!("add x9, x9, #{UNIT_RECORD}"));
            emitter.instruction("sub x10, x10, #1");
            emitter.instruction("b __rt_include_unit_candidate_record");
            emitter.label("__rt_include_unit_candidate_hit");
            emitter.instruction("mov x0, #1");                                  // a unit has this file name
            emitter.instruction("ret");
            emitter.label("__rt_include_unit_candidate_miss");
            emitter.instruction("mov x0, #0");                                  // no unit can match this path
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("mov rcx, rdx");                                // scan back from the end of the path
            emitter.label("__rt_include_unit_candidate_scan");
            emitter.instruction("test rcx, rcx");
            emitter.instruction("jz __rt_include_unit_candidate_base");         // no slash: the whole path is the name
            emitter.instruction("cmp BYTE PTR [rax + rcx - 1], 47");            // '/'
            emitter.instruction("je __rt_include_unit_candidate_base");
            emitter.instruction("dec rcx");
            emitter.instruction("jmp __rt_include_unit_candidate_scan");
            emitter.label("__rt_include_unit_candidate_base");
            emitter.instruction("lea rsi, [rax + rcx]");                        // rsi = the last component
            emitter.instruction("mov rdi, rdx");
            emitter.instruction("sub rdi, rcx");                                // rdi = its length
            abi::emit_symbol_address(emitter, "r9", table);
            emitter.instruction(&format!("mov r10, {count}"));                  // records left to inspect
            emitter.label("__rt_include_unit_candidate_record");
            emitter.instruction("test r10, r10");
            emitter.instruction("jz __rt_include_unit_candidate_miss");
            emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                 // record path length
            emitter.instruction("sub r11, QWORD PTR [r9 + 32]");                // minus where its last component starts
            emitter.instruction("cmp r11, rdi");
            emitter.instruction("jne __rt_include_unit_candidate_next");
            emitter.instruction("mov r11, QWORD PTR [r9]");
            emitter.instruction("add r11, QWORD PTR [r9 + 32]");                // the record's last component
            emitter.instruction("xor ecx, ecx");
            emitter.label("__rt_include_unit_candidate_byte");
            emitter.instruction("cmp rcx, rdi");
            emitter.instruction("je __rt_include_unit_candidate_hit");
            emitter.instruction("mov r8b, BYTE PTR [r11 + rcx]");
            emitter.instruction("cmp r8b, BYTE PTR [rsi + rcx]");
            emitter.instruction("jne __rt_include_unit_candidate_next");
            emitter.instruction("inc rcx");
            emitter.instruction("jmp __rt_include_unit_candidate_byte");
            emitter.label("__rt_include_unit_candidate_next");
            emitter.instruction(&format!("add r9, {UNIT_RECORD}"));
            emitter.instruction("dec r10");
            emitter.instruction("jmp __rt_include_unit_candidate_record");
            emitter.label("__rt_include_unit_candidate_hit");
            emitter.instruction("mov r8d, 1");                                  // a unit has this file name
            emitter.instruction("jmp __rt_include_unit_candidate_done");
            emitter.label("__rt_include_unit_candidate_miss");
            emitter.instruction("xor r8d, r8d");                                // no unit can match this path
            emitter.label("__rt_include_unit_candidate_done");
            // The answer travels in r8: rax still holds the path for the caller's realpath.
            emitter.instruction("ret");
        }
    }
}
