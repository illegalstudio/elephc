//! Native runtime `include`/`require` for a closed-world build (`--closed-world`).
//!
//! One shared routine, `__rt_closed_include`, answers every runtime include in the program from a
//! table with one record per compiled source, so no include site carries code of its own and no
//! interpreter is linked. A record is `{path, length, guard cell, info}`:
//!
//! - `info == 0`: a declaration file the binary already carries (the compiler spliced it).
//!   Including it runs nothing and evaluates to `1`; an `_once` form that finds the guard raised
//!   answers `true`.
//! - `info == 1`: a source whose top level has effects the binary cannot replay from an include.
//!   The compiler already ran it at its static include site, so an `_once` form (or a discarded
//!   plain include of a compiler-included file) answers `true`; anything that needs it to run again
//!   is a fatal error naming the file.
//! - otherwise `info` is the address of a unit function (`__elephc_include_unit_*`) holding the
//!   file's top level, whose return value is the include's value.
//!
//! A path the table does not know is php's failed-open: two warnings and `false`, or for `require`
//! the fatal error php raises.
//!
//! Register contracts (a leaf of the include sites, not a C ABI):
//! - in: AArch64 `x1`/`x2` = path bytes/length, `x3` = flags; x86_64 `rax`/`rdx`/`rcx`;
//! - out: the include's value as an owned boxed `mixed` in the integer result register;
//! - flags: bit 0 `_once`, bit 1 `require`, bit 2 the value is discarded (a statement include).

use super::bridge::{emit_lookup, OnMatch};
use super::include_guard_symbol;
use crate::codegen::{
    abi,
    data_section::{DataSection, DataWord},
    emit::Emitter,
    emit_box_current_value_as_mixed,
    platform::Arch,
};
use crate::ir::{ClosedIncludeKind, Module};
use crate::types::PhpType;

/// Label of the shared runtime include routine.
pub(in crate::codegen) const CLOSED_INCLUDE: &str = "__rt_closed_include";
const LOOKUP: &str = "__rt_closed_include_lookup";
const FAILED: &str = "__rt_include_failed";

/// Include flag bits shared with `lower_dynamic_include`.
pub(in crate::codegen) const FLAG_ONCE: i64 = 1;
pub(in crate::codegen) const FLAG_REQUIRED: i64 = 2;
pub(in crate::codegen) const FLAG_DISCARDED: i64 = 4;

const FRAME: usize = 64;
const PATH: usize = 8;
const LEN: usize = 16;
const FLAGS: usize = 24;
const CANON: usize = 32;
const CANON_LEN: usize = 40;
const RECORD_SLOT: usize = 48;
/// Bytes per table record: path pointer, path length, guard cell, info.
const RECORD: usize = 32;

/// The activation cells each declaration file raises when it is included: its interfaces' and
/// its probe-only classes' (the cells `__elephc_eval_source_activate` raises where an interpreter
/// is linked).
///
/// A declaration file whose classes the compiler took from a run-time-only include (an
/// `include_once` in a method body, as Symfony's dumped container writes for service classes) is
/// declared in the binary, but php declares it only when that include -- or the autoloader's --
/// runs. Answering the autoloader's include with `1` and raising nothing left
/// `interface_exists()` false after a successful autoload.
pub(super) fn declaration_activation_cells(
    module: &Module,
) -> std::collections::HashMap<std::path::PathBuf, Vec<String>> {
    let mut cells: std::collections::HashMap<std::path::PathBuf, Vec<String>> = Default::default();
    for (path, cell) in super::bridge::preincluded_interface_activation_cells(module) {
        cells.entry(path).or_default().push(cell);
    }
    let mut declared = module.declared_class_source_files.iter().collect::<Vec<_>>();
    declared.sort();
    for (name, file) in declared {
        let folded = name.trim_start_matches('\\').to_ascii_lowercase();
        if !module.deferred_class_loads.contains(&folded) {
            continue;
        }
        let declared = std::path::PathBuf::from(file);
        let declared = declared.canonicalize().unwrap_or(declared);
        if module.preincluded_sources.contains(&declared) {
            cells.entry(declared).or_default().push(super::bridge::deferred_class_symbol(&folded));
        }
    }
    cells
}

/// Emits the routine a declaration file's record runs in place of "evaluate to `1`": it raises
/// the file's activation cells, then answers `1` like any declaration file. It takes the record's
/// unit slot, so the include routine's guard and `_once` handling stay exactly a unit's.
fn emit_declaration_activation(
    emitter: &mut Emitter,
    data: &mut DataSection,
    index: usize,
    cells: &[String],
) -> String {
    let label = format!("__rt_closed_include_activate_{index}");
    emitter.blank();
    if emitter.target.arch == Arch::AArch64 { emitter.raw(".align 2"); }
    emitter.label_global(&label);
    abi::emit_frame_prologue(emitter, 16);
    let value = abi::temp_int_reg(emitter.target);
    for cell in cells {
        let cell = data.add_comm(cell.clone(), 8);
        abi::emit_load_int_immediate(emitter, value, 1);
        abi::emit_store_reg_to_symbol(emitter, value, &cell, 0);
    }
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1); // a declaration file evaluates to int 1
    emit_box_current_value_as_mixed(emitter, &PhpType::Int);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
    label
}

/// Emits the include table, its lookup, `__rt_closed_include` and `__rt_include_failed`.
pub(super) fn emit_closed_include(module: &Module, emitter: &mut Emitter, data: &mut DataSection) {
    let Some(includes) = module.closed_includes.as_ref() else { return };
    let activations = declaration_activation_cells(module);
    let mut words = Vec::new();
    let mut keys = Vec::new();
    let aliases = spelled_aliases(&program_literals(module), includes);
    for (include, spelling) in includes
        .iter()
        .map(|include| (include, None))
        .chain(aliases.iter().map(|(index, spelling)| (&includes[*index], Some(spelling))))
    {
        let key = match spelling {
            Some(spelling) => spelling.as_bytes().to_vec(),
            None => include.canonical_path.as_os_str().as_encoded_bytes().to_vec(),
        };
        let (path, length) = data.add_string(&key);
        keys.push(key);
        let cell = data.add_comm(include_guard_symbol(&include.canonical_path), 8);
        let info = match &include.kind {
            ClosedIncludeKind::Declarations => match activations.get(&include.canonical_path) {
                Some(cells) => {
                    DataWord::Symbol(emit_declaration_activation(emitter, data, keys.len(), cells))
                }
                None => DataWord::U64(0),
            },
            ClosedIncludeKind::NotCompiled => DataWord::U64(1),
            ClosedIncludeKind::Unit(function) => {
                DataWord::Symbol(crate::names::function_symbol(function))
            }
        };
        words.extend([
            DataWord::Symbol(path),
            DataWord::U64(length as u64),
            DataWord::Symbol(cell),
            info,
        ]);
    }
    debug_assert_eq!(words.len(), (includes.len() + aliases.len()) * RECORD / 8);
    let table = data.add_words(words);
    emit_lookup(emitter, data, LOOKUP, "closed_include_lookup", &table, &keys, OnMatch::Record);
    let words_for = |data: &mut DataSection, texts: [&str; 4]| {
        let mut words = Vec::new();
        for text in texts {
            let (label, length) = data.add_string(text.as_bytes());
            words.extend([DataWord::Symbol(label), DataWord::U64(length as u64)]);
        }
        data.add_words(words)
    };
    let first = words_for(
        data,
        ["Warning: include(", "Warning: include_once(", "Warning: require(", "Warning: require_once("],
    );
    let second = words_for(
        data,
        [
            "Warning: include(): Failed opening '",
            "Warning: include_once(): Failed opening '",
            "Warning: require(): Failed opening '",
            "Warning: require_once(): Failed opening '",
        ],
    );
    let middle = data.add_string(b"): Failed to open stream: No such file or directory\n");
    let tail = data.add_string(b"' for inclusion\n");
    let fatal_head = data.add_string(b"Fatal error: Uncaught Error: Failed opening required '");
    let fatal_tail = data.add_string(b"'\n");
    let not_compiled_head = data.add_string(b"Fatal error: Uncaught Error: Cannot include '");
    let not_compiled_tail = data.add_string(
        b"': it is part of this closed-world build but its top level cannot be run from an include\n",
    );
    // Under a usage manifest, a file that exists but was not compiled is one php would include and
    // no recorded run did: a closed-world violation, not php's failed-open.
    let escaped = module.usage_manifest.then(|| {
        (
            data.add_string(b"Fatal error: closed-world violation: '"),
            data.add_string(
                b"' was not recorded in the usage manifest, so this build did not compile it; record a run that reaches it and rebuild\n",
            ),
        )
    });
    emit_runtime_include(emitter, &not_compiled_head, &not_compiled_tail, escaped.as_ref());
    emit_include_failed(emitter, &first, &second, &middle, &tail, &fatal_head, &fatal_tail);
}

/// The program's literal spellings of compiled files that are not their canonical path, each with
/// the index of the include it names: Composer's `autoload_static.php` lists its eager files as
/// `__DIR__ . '/..' . '/symfony/.../bootstrap.php'`, which folds to a `/composer/../` literal, and
/// every request of a compiled `--web` worker requires them again. Keyed under that spelling too,
/// the runtime lookup answers them without `realpath()`.
///
/// A literal joins only when the build's own `realpath()` maps it to that very file, which is the
/// same filesystem assumption the canonical table makes; the lexical normalization is just the
/// cheap filter that avoids a filesystem call per literal.
fn spelled_aliases(literals: &[String], includes: &[crate::ir::ClosedInclude]) -> Vec<(usize, String)> {
    let by_path: std::collections::HashMap<&std::path::Path, usize> = includes
        .iter()
        .enumerate()
        .map(|(index, include)| (include.canonical_path.as_path(), index))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut aliases = Vec::new();
    for literal in literals {
        if !literal.starts_with('/') || !(literal.contains("/../") || literal.contains("/./") || literal.contains("//")) {
            continue;
        }
        let Some(normalized) = lexically_normalized(literal) else { continue };
        let Some(&index) = by_path.get(std::path::Path::new(&normalized)) else { continue };
        let resolves_there = std::fs::canonicalize(literal)
            .is_ok_and(|resolved| resolved == includes[index].canonical_path);
        if resolves_there && seen.insert(literal.as_str()) {
            aliases.push((index, literal.clone()));
        }
    }
    aliases
}

/// Every string literal the program can hand an include: the module's interned strings plus the
/// literals inside class static defaults, which are emitted on their own (Composer's eager-file
/// list is `ComposerStaticInit*::$files`, a static array default).
fn program_literals(module: &Module) -> Vec<String> {
    let mut literals = module.data.strings.clone();
    for info in module.class_infos.values() {
        for default in info.static_defaults.iter().flatten() {
            let probe = [crate::parser::ast::Stmt::new(
                crate::parser::ast::StmtKind::Echo(default.clone()),
                default.span,
            )];
            crate::parser::ast::visit::walk_nodes(&probe, &mut |node| {
                if let crate::parser::ast::visit::Node::Expr(expr) = node {
                    if let crate::parser::ast::ExprKind::StringLiteral(text) = &expr.kind {
                        literals.push(text.clone());
                    }
                }
            });
        }
    }
    literals
}

/// `path` with `.` and `name/..` segments and repeated separators folded away, or `None` when a
/// `..` would climb above the root.
fn lexically_normalized(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    Some(format!("/{}", parts.join("/")))
}

/// Writes `text` to stderr (AArch64: pointer in `x1` length in `x2`; x86_64: `rsi`/`rdx` are set
/// from the same two arguments by the caller already).
fn write_stderr(emitter: &mut Emitter, label: &str, length: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // stderr
            abi::emit_symbol_address(emitter, "x1", label);
            emitter.instruction(&format!("mov x2, #{length}"));                 // byte count
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // stderr
            abi::emit_symbol_address(emitter, "rsi", label);
            emitter.instruction(&format!("mov edx, {length}"));                 // byte count
            emitter.instruction("mov eax, 1");                                  // write
            emitter.instruction("syscall");
        }
    }
}

/// Writes the path saved in the frame to stderr.
fn write_stderr_path(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // stderr
            abi::load_at_offset(emitter, "x1", PATH);
            abi::load_at_offset(emitter, "x2", LEN);
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // stderr
            abi::load_at_offset(emitter, "rsi", PATH);
            abi::load_at_offset(emitter, "rdx", LEN);
            emitter.instruction("mov eax, 1");                                  // write
            emitter.instruction("syscall");
        }
    }
}

fn emit_runtime_include(
    emitter: &mut Emitter,
    not_compiled_head: &(String, usize),
    not_compiled_tail: &(String, usize),
    escaped: Option<&((String, usize), (String, usize))>,
) {
    let prefix = emitter.target.platform.local_label_prefix();
    let l = |name: &str| format!("{prefix}closed_include_{name}");
    // A table miss after `realpath()` resolved the path: the file exists.
    let exists_unknown = if escaped.is_some() { l("escaped") } else { l("unknown") };
    let (unknown, answer_true, dispatch, not_once, decl_done, not_decl, run, not_compiled) = (
        l("unknown"), l("true"), l("dispatch"), l("not_once"), l("decl_done"), l("not_decl"),
        l("run"), l("not_compiled"),
    );
    // The table holds the canonical paths the compiler resolved, so a path spelled exactly as one
    // of them names that file: it is answered without `realpath()`, whose filesystem walk was a
    // twentieth of a compiled Symfony request (php's realpath cache saves it the same walk). Only a
    // path the table does not spell (`..`, relative, a symlink) is canonicalized first.
    let record = l("record");
    emitter.blank();
    emitter.comment("--- runtime: closed-world include ---");
    if emitter.target.arch == Arch::AArch64 { emitter.raw(".align 2"); }
    emitter.label_global(CLOSED_INCLUDE);
    abi::emit_frame_prologue(emitter, FRAME);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::store_at_offset(emitter, "x1", PATH);
            abi::store_at_offset(emitter, "x2", LEN);
            abi::store_at_offset(emitter, "x3", FLAGS);
            emitter.instruction(&format!("cbz x2, {unknown}"));                 // an empty path names no file
            emitter.instruction("mov x0, x1");                                  // lookup(path as spelled, length)
            emitter.instruction("mov x1, x2");
            abi::emit_call_label(emitter, LOOKUP);                              // x0 = table record, or 0
            abi::store_at_offset(emitter, "x0", RECORD_SLOT);
            emitter.instruction(&format!("cbnz x0, {record}"));                 // a canonical spelling needs no filesystem walk
            abi::load_at_offset(emitter, "x1", PATH);
            abi::load_at_offset(emitter, "x2", LEN);
            abi::emit_call_label(emitter, "__rt_realpath");                     // x1/x2 = owned canonical path, or 0/0
            emitter.instruction(&format!("cbz x1, {unknown}"));                 // an unresolvable path names no compiled file
            abi::store_at_offset(emitter, "x1", CANON);
            abi::store_at_offset(emitter, "x2", CANON_LEN);
            emitter.instruction("mov x0, x1");                                  // lookup(path, length)
            emitter.instruction("mov x1, x2");
            abi::emit_call_label(emitter, LOOKUP);                              // x0 = table record, or 0
            abi::store_at_offset(emitter, "x0", RECORD_SLOT);
            abi::load_at_offset(emitter, "x0", CANON);
            abi::emit_call_label(emitter, "__rt_heap_free");                    // the canonical path was only needed for the lookup
            emitter.label(&record);
            abi::load_at_offset(emitter, "x9", RECORD_SLOT);
            emitter.instruction(&format!("cbz x9, {exists_unknown}"));          // not part of the closed world
            abi::load_at_offset(emitter, "x3", FLAGS);
            emitter.instruction("ldr x10, [x9, #16]");                          // the file's guard cell
            emitter.instruction("ldr x11, [x10]");                              // 0 until something includes the file
            emitter.instruction("ldr x12, [x9, #24]");                          // what a record's include does
            emitter.instruction(&format!("tbz x3, #0, {not_once}"));            // plain include?
            emitter.instruction(&format!("cbnz x11, {answer_true}"));           // `_once` of an included file: true
            emitter.instruction(&format!("b {dispatch}"));
            emitter.label(&not_once);
            emitter.instruction(&format!("tbz x3, #2, {dispatch}"));            // a value-using plain include always runs
            emitter.instruction("cmp x11, #2");                                 // 2 = the compiler already ran it
            emitter.instruction(&format!("b.eq {answer_true}"));                // a discarded include of that: nothing left to do
            emitter.label(&dispatch);
            emitter.instruction(&format!("cbnz x12, {not_decl}"));
            emitter.instruction(&format!("cbnz x11, {decl_done}"));
            emitter.instruction("mov x11, #1");                                 // included by this request
            emitter.instruction("str x11, [x10]");
            emitter.label(&decl_done);
            emitter.instruction("mov x0, #1");                                  // a declaration file evaluates to int 1
            emit_box_current_value_as_mixed(emitter, &PhpType::Int);
            abi::emit_frame_restore(emitter, FRAME);
            abi::emit_return(emitter);
            emitter.label(&not_decl);
            emitter.instruction("cmp x12, #1");
            emitter.instruction(&format!("b.eq {not_compiled}"));
            emitter.instruction(&format!("cbnz x11, {run}"));                   // keep a compiler mark as it is
            emitter.instruction("mov x11, #1");                                 // included by this request
            emitter.instruction("str x11, [x10]");
            emitter.label(&run);
            emitter.instruction("mov x0, #0");                                  // null func_get_args collector
            emitter.instruction("blr x12");                                     // x0 = the include's value
            abi::emit_frame_restore(emitter, FRAME);
            abi::emit_return(emitter);
            emitter.label(&answer_true);
            emitter.instruction("mov x0, #1");
            emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
            abi::emit_frame_restore(emitter, FRAME);
            abi::emit_return(emitter);
            emitter.label(&unknown);
            abi::load_at_offset(emitter, "x1", PATH);
            abi::load_at_offset(emitter, "x2", LEN);
            abi::load_at_offset(emitter, "x3", FLAGS);
            abi::emit_call_label(emitter, FAILED);                              // warnings, then false (or the require fatal)
            abi::emit_frame_restore(emitter, FRAME);
            abi::emit_return(emitter);
        }
        Arch::X86_64 => {
            abi::store_at_offset(emitter, "rax", PATH);
            abi::store_at_offset(emitter, "rdx", LEN);
            abi::store_at_offset(emitter, "rcx", FLAGS);
            emitter.instruction("test rdx, rdx");                               // an empty path names no file
            emitter.instruction(&format!("jz {unknown}"));
            emitter.instruction("mov rdi, rax");                                // lookup(path as spelled, length)
            emitter.instruction("mov rsi, rdx");
            abi::emit_call_label(emitter, LOOKUP);                              // rax = table record, or 0
            abi::store_at_offset(emitter, "rax", RECORD_SLOT);
            emitter.instruction("test rax, rax");                               // a canonical spelling needs no filesystem walk
            emitter.instruction(&format!("jnz {record}"));
            abi::load_at_offset(emitter, "rax", PATH);
            abi::load_at_offset(emitter, "rdx", LEN);
            abi::emit_call_label(emitter, "__rt_realpath");                     // rax/rdx = owned canonical path, or 0/0
            emitter.instruction("test rax, rax");                               // an unresolvable path names no compiled file
            emitter.instruction(&format!("jz {unknown}"));
            abi::store_at_offset(emitter, "rax", CANON);
            abi::store_at_offset(emitter, "rdx", CANON_LEN);
            emitter.instruction("mov rdi, rax");                                // lookup(path, length)
            emitter.instruction("mov rsi, rdx");
            abi::emit_call_label(emitter, LOOKUP);                              // rax = table record, or 0
            abi::store_at_offset(emitter, "rax", RECORD_SLOT);
            abi::load_at_offset(emitter, "rax", CANON);
            abi::emit_call_label(emitter, "__rt_heap_free");                    // the canonical path was only needed for the lookup
            emitter.label(&record);
            abi::load_at_offset(emitter, "r9", RECORD_SLOT);
            emitter.instruction("test r9, r9");                                 // not part of the closed world?
            emitter.instruction(&format!("jz {exists_unknown}"));
            abi::load_at_offset(emitter, "rcx", FLAGS);
            emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                // the file's guard cell
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // 0 until something includes the file
            emitter.instruction("mov r8, QWORD PTR [r9 + 24]");                 // what a record's include does
            emitter.instruction("test rcx, 1");                                 // `_once`?
            emitter.instruction(&format!("jz {not_once}"));
            emitter.instruction("test r11, r11");
            emitter.instruction(&format!("jnz {answer_true}"));                 // `_once` of an included file: true
            emitter.instruction(&format!("jmp {dispatch}"));
            emitter.label(&not_once);
            emitter.instruction("test rcx, 4");                                 // statement include (value discarded)?
            emitter.instruction(&format!("jz {dispatch}"));                     // a value-using plain include always runs
            emitter.instruction("cmp r11, 2");                                  // 2 = the compiler already ran it
            emitter.instruction(&format!("je {answer_true}"));
            emitter.label(&dispatch);
            emitter.instruction("test r8, r8");
            emitter.instruction(&format!("jnz {not_decl}"));
            emitter.instruction("test r11, r11");
            emitter.instruction(&format!("jnz {decl_done}"));
            emitter.instruction("mov QWORD PTR [r10], 1");                      // included by this request
            emitter.label(&decl_done);
            emitter.instruction("mov eax, 1");                                  // a declaration file evaluates to int 1
            emit_box_current_value_as_mixed(emitter, &PhpType::Int);
            abi::emit_frame_restore(emitter, FRAME);
            abi::emit_return(emitter);
            emitter.label(&not_decl);
            emitter.instruction("cmp r8, 1");
            emitter.instruction(&format!("je {not_compiled}"));
            emitter.instruction("test r11, r11");
            emitter.instruction(&format!("jnz {run}"));                         // keep a compiler mark as it is
            emitter.instruction("mov QWORD PTR [r10], 1");                      // included by this request
            emitter.label(&run);
            emitter.instruction("xor edi, edi");                                // null func_get_args collector
            emitter.instruction("call r8");                                     // rax = the include's value
            abi::emit_frame_restore(emitter, FRAME);
            abi::emit_return(emitter);
            emitter.label(&answer_true);
            emitter.instruction("mov eax, 1");
            emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
            abi::emit_frame_restore(emitter, FRAME);
            abi::emit_return(emitter);
            emitter.label(&unknown);
            abi::load_at_offset(emitter, "rax", PATH);
            abi::load_at_offset(emitter, "rdx", LEN);
            abi::load_at_offset(emitter, "rcx", FLAGS);
            abi::emit_call_label(emitter, FAILED);                              // warnings, then false (or the require fatal)
            abi::emit_frame_restore(emitter, FRAME);
            abi::emit_return(emitter);
        }
    }
    // Known to the closed world but not replayable: name the file and stop.
    emitter.label(&not_compiled);
    write_stderr(emitter, &not_compiled_head.0, not_compiled_head.1);
    write_stderr_path(emitter);
    write_stderr(emitter, &not_compiled_tail.0, not_compiled_tail.1);
    abi::emit_exit(emitter, 255);
    if let Some((head, tail)) = escaped {
        emitter.label(&exists_unknown);
        write_stderr(emitter, &head.0, head.1);
        write_stderr_path(emitter);
        write_stderr(emitter, &tail.0, tail.1);
        abi::emit_exit(emitter, 255);
    }
}

/// `__rt_include_failed`: php's failed-open for a path nothing compiled answers.
///
/// Two warnings (`include(PATH): Failed to open stream: ...`, then `include(): Failed opening
/// 'PATH' for inclusion`), then `false`; a `require` raises the fatal php reports instead.
fn emit_include_failed(
    emitter: &mut Emitter,
    first: &str,
    second: &str,
    middle: &(String, usize),
    tail: &(String, usize),
    fatal_head: &(String, usize),
    fatal_tail: &(String, usize),
) {
    let prefix = emitter.target.platform.local_label_prefix();
    let returns_false = format!("{prefix}include_failed_false");
    let fatal = format!("{prefix}include_failed_fatal");
    emitter.blank();
    emitter.comment("--- runtime: include failed to open ---");
    if emitter.target.arch == Arch::AArch64 { emitter.raw(".align 2"); }
    emitter.label_global(FAILED);
    abi::emit_frame_prologue(emitter, FRAME);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::store_at_offset(emitter, "x1", PATH);
            abi::store_at_offset(emitter, "x2", LEN);
            abi::store_at_offset(emitter, "x3", FLAGS);
            emitter.instruction("and x9, x3, #3");                              // construct = (required << 1) | once
            emitter.instruction("lsl x9, x9, #4");                              // 16 bytes per {pointer, length} entry
            abi::store_at_offset(emitter, "x9", CANON);
            for table in [first, second] {
                abi::emit_symbol_address(emitter, "x10", table);
                abi::load_at_offset(emitter, "x9", CANON);
                emitter.instruction("add x10, x10, x9");
                emitter.instruction("ldr x1, [x10]");                           // "Warning: include(" and friends
                emitter.instruction("ldr x2, [x10, #8]");
                abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
                abi::load_at_offset(emitter, "x1", PATH);
                abi::load_at_offset(emitter, "x2", LEN);
                abi::emit_call_label(emitter, "__rt_diag_warning_fragment");    // the path as written
                let (label, length) = if table == first { middle } else { tail };
                abi::emit_symbol_address(emitter, "x1", label);
                emitter.instruction(&format!("mov x2, #{length}"));
                abi::emit_call_label(emitter, "__rt_diag_warning");             // complete the warning line
                if table == first {
                    // php reports a failed `require` as the Error alone after the stream warning.
                    abi::load_at_offset(emitter, "x3", FLAGS);
                    emitter.instruction(&format!("tbnz x3, #1, {fatal}"));
                }
            }
            emitter.instruction(&format!("b {returns_false}"));                 // include: false
        }
        Arch::X86_64 => {
            abi::store_at_offset(emitter, "rax", PATH);
            abi::store_at_offset(emitter, "rdx", LEN);
            abi::store_at_offset(emitter, "rcx", FLAGS);
            emitter.instruction("and rcx, 3");                                  // construct = (required << 1) | once
            emitter.instruction("shl rcx, 4");                                  // 16 bytes per {pointer, length} entry
            abi::store_at_offset(emitter, "rcx", CANON);
            for table in [first, second] {
                abi::emit_symbol_address(emitter, "r10", table);
                abi::load_at_offset(emitter, "rcx", CANON);
                emitter.instruction("add r10, rcx");
                emitter.instruction("mov rdi, QWORD PTR [r10]");                // "Warning: include(" and friends
                emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");
                abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
                abi::load_at_offset(emitter, "rdi", PATH);
                abi::load_at_offset(emitter, "rsi", LEN);
                abi::emit_call_label(emitter, "__rt_diag_warning_fragment");    // the path as written
                let (label, length) = if table == first { middle } else { tail };
                abi::emit_symbol_address(emitter, "rdi", label);
                emitter.instruction(&format!("mov esi, {length}"));
                abi::emit_call_label(emitter, "__rt_diag_warning");             // complete the warning line
                if table == first {
                    // php reports a failed `require` as the Error alone after the stream warning.
                    abi::load_at_offset(emitter, "rcx", FLAGS);
                    emitter.instruction("test rcx, 2");                         // require?
                    emitter.instruction(&format!("jnz {fatal}"));
                }
            }
            emitter.instruction(&format!("jmp {returns_false}"));               // include: false
        }
    }
    emitter.label(&fatal);
    write_stderr(emitter, &fatal_head.0, fatal_head.1);
    write_stderr_path(emitter);
    write_stderr(emitter, &fatal_tail.0, fatal_tail.1);
    abi::emit_exit(emitter, 255);
    emitter.label(&returns_false);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #0"),                     // the include evaluates to false
        Arch::X86_64 => emitter.instruction("xor eax, eax"),
    }
    emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// A literal that reaches a compiled file through `..` (Composer's eager-file list) is keyed
    /// under its spelling as well; one naming no compiled file, or climbing above the root, is not.
    #[test]
    fn composer_style_spellings_become_lookup_aliases() {
        let dir = std::env::temp_dir().join(format!("elephc-include-alias-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("vendor/composer")).unwrap();
        std::fs::create_dir_all(dir.join("vendor/poly")).unwrap();
        std::fs::write(dir.join("vendor/poly/bootstrap.php"), "<?php\n").unwrap();
        let canonical = std::fs::canonicalize(dir.join("vendor/poly/bootstrap.php")).unwrap();
        let root = canonical.parent().unwrap().parent().unwrap().display().to_string();
        let includes = vec![crate::ir::ClosedInclude {
            canonical_path: canonical.clone(),
            kind: ClosedIncludeKind::Declarations,
        }];
        let spelled = format!("{root}/composer/../poly/bootstrap.php");
        let literals = vec![
            spelled.clone(),
            format!("{root}/composer/../poly/missing.php"),
            canonical.display().to_string(),
            "/../../etc".to_string(),
        ];
        assert_eq!(spelled_aliases(&literals, &includes), vec![(0, spelled)]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A runtime include first looks its path up as spelled and only canonicalizes on a miss:
    /// calling `realpath()` first made every compiled include of a Symfony request walk the
    /// filesystem.
    #[test]
    fn closed_include_tries_the_spelled_path_before_realpath() {
        for name in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_runtime_include(&mut emitter, &("_h".to_string(), 1), &("_t".to_string(), 1));
            let assembly = emitter.output();
            let lookup = assembly.find(LOOKUP).unwrap_or_else(|| panic!("{name}: lookup call"));
            let realpath = assembly.find("__rt_realpath").unwrap_or_else(|| panic!("{name}: realpath call"));
            assert!(lookup < realpath, "{name}: the spelled path is looked up before realpath()");
            assert_eq!(assembly.matches(LOOKUP).count(), 2, "{name}: one lookup as spelled, one canonical");
        }
    }
}
