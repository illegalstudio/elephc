//! Purpose:
//! Extracts and classifies mutable global symbols the elephc compiler/runtime/bridges
//! emit, and checks them against a versioned baseline. The audit is the structural
//! guard against a future contributor silently adding a mutable process-global that
//! would break a per-thread invariant under load.
//!
//! Called from:
//! - `crate::bin` (the `elephc-globals-audit` CLI) for `--check` and table emission.
//! - `tests/` integration tests that assert `--check` passes against the committed
//!   baseline and fails when a deliberate unclassified symbol is introduced.
//!
//! Key details:
//! - Extraction is text-driven: it walks the compiler/runtime Rust source for
//!   `.comm <sym>` / `.globl <sym>` emission format strings and greps the bridge
//!   crates for `static mut` items. It never links the compiler.
//! - Classification buckets: CONST, RO_BOOT, SHARED_LOCK, TLS, UNKNOWN. UNKNOWN is
//!   a failure for symbols introduced AFTER the baseline; existing UNKNOWN entries
//!   are grandfathered in the baseline.
//! - `--check` exits non-zero on (a) a new mutable symbol missing from the baseline,
//!   or (b) an existing symbol whose classification changed. Both force a deliberate
//!   baseline bump.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Classification bucket for a mutable (or potentially mutable) global symbol.
///
/// The buckets are deliberately coarse: the audit's job is to force every new
/// global to be explicitly bucketed, not to encode the full per-symbol semantics.
/// See `docs/internals/global-state.md` for the full scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// Read-only after link (rodata, vtables, const tables, literal strings). Zero cost.
    Const,
    /// Written exactly once at first request / process boot, read-only afterward.
    RoBoot,
    /// A process resource that must be locked or re-designed for any concurrency.
    SharedLock,
    /// Must be per-thread. The audit labels it; it does NOT migrate it.
    Tls,
    /// The extractor could not classify. CI treats UNKNOWN as a failure for
    /// symbols introduced after the baseline; existing UNKNOWN entries are
    /// grandfathered in the baseline.
    Unknown,
}

impl Class {
    /// Parses a classification token from the baseline TOML.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "CONST" => Some(Self::Const),
            "RO_BOOT" => Some(Self::RoBoot),
            "SHARED_LOCK" => Some(Self::SharedLock),
            "TLS" => Some(Self::Tls),
            "UNKNOWN" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// Renders the classification as the baseline TOML token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Const => "CONST",
            Self::RoBoot => "RO_BOOT",
            Self::SharedLock => "SHARED_LOCK",
            Self::Tls => "TLS",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// A single extracted global symbol with its classification and provenance.
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    /// The symbol name as it appears in `.comm`/`.globl`/`static mut` form.
    pub symbol: String,
    /// Best-known classification (from the baseline, or UNKNOWN if unseen).
    pub class: Class,
    /// Where the symbol is emitted: `codegen:<file>:<line>` or `bridge:<file>:<line>`.
    pub source: String,
    /// Free-form notes carried from the baseline (family, mutability rationale).
    pub notes: String,
}

/// Result of a `--check` run: the drift between live extraction and the baseline.
#[derive(Debug, Default)]
pub struct CheckReport {
    /// Symbols present in the live extraction but missing from the baseline.
    pub new_symbols: Vec<SymbolEntry>,
    /// Symbols whose classification in the baseline differs from the live one.
    /// (Only possible when the extractor itself re-classifies; today the
    /// extractor defers to the baseline, so this is empty unless a baseline
    /// entry is removed.)
    pub changed: Vec<(String, Class, Class)>,
    /// Symbols in the baseline that no longer appear in the live extraction.
    pub removed: Vec<String>,
    /// Total symbols seen live.
    pub live_count: usize,
    /// Total symbols in the baseline.
    pub baseline_count: usize,
}

impl CheckReport {
    /// `--check` passes when there are no new, changed, or removed symbols.
    pub fn ok(&self) -> bool {
        self.new_symbols.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// Walks the compiler/runtime source tree and extracts every `.comm`/`.globl`
/// symbol the emitters produce, plus every `static mut` in the bridge crates.
/// Returns entries in stable (sorted) order so baseline diffs are reproducible.
pub fn extract_all(repo_root: &Path) -> Vec<SymbolEntry> {
    let mut entries: BTreeMap<String, SymbolEntry> = BTreeMap::new();

    // Runtime + data-section emitters: the `.comm`/`.globl` format strings live
    // in `src/codegen/runtime/data/` and `src/codegen/data_section.rs`. We also
    // scan the per-target runtime emitters (`src/codegen/runtime/**`) because a
    // few `.globl`/`.comm` lines appear outside the data module.
    let codegen_dirs = [
        "src/codegen/runtime/data",
        "src/codegen/data_section.rs",
        "src/codegen/runtime",
    ];
    for dir in codegen_dirs {
        let path = repo_root.join(dir);
        if path.is_dir() {
            walk_rs(&path, &mut |file, line_no, line| {
                extract_comm_globl(line, file, line_no, &mut entries);
            });
        } else if path.is_file() {
            scan_file(&path, &mut |file, line_no, line| {
                extract_comm_globl(line, file, line_no, &mut entries);
            });
        }
    }

    // Bridge crates: `static mut` items in `crates/elephc-*/src/**/*.rs`.
    let crates_dir = repo_root.join("crates");
    if crates_dir.is_dir() {
        let mut bridge_dirs: Vec<PathBuf> = Vec::new();
        if let Ok(read) = fs::read_dir(&crates_dir) {
            for ent in read.flatten() {
                let p = ent.path();
                if p.is_dir() && p.join("Cargo.toml").exists() {
                    bridge_dirs.push(p.join("src"));
                }
            }
        }
        for bd in bridge_dirs {
            walk_rs(&bd, &mut |file, line_no, line| {
                extract_static_mut(line, file, line_no, &mut entries);
            });
        }
    }

    let mut out: Vec<SymbolEntry> = entries.into_values().collect();
    out.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    out
}

/// Recursively walks `.rs` files under `root`, invoking `f(file, line_no, line)`.
fn walk_rs(root: &Path, f: &mut impl FnMut(&str, usize, &str)) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else { continue };
        for ent in read.flatten() {
            let p = ent.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                scan_file(&p, f);
            }
        }
    }
}

/// Reads a single `.rs` file and invokes `f(relative_file, line_no, line)` per line.
fn scan_file(path: &Path, f: &mut impl FnMut(&str, usize, &str)) {
    let Ok(text) = fs::read_to_string(path) else { return };
    for (i, line) in text.lines().enumerate() {
        let display = path.to_string_lossy().into_owned();
        f(&display, i + 1, line);
    }
}

/// Extracts `.comm <sym>` and `.globl <sym>` targets from a source line. The
/// emitters build these as format strings (`.comm _foo, 8, 3`) or literal
/// strings; both forms are matched. Symbols starting with `_ro_`/`_const_` or
/// known read-only suffixes are still extracted (the classifier labels them
/// CONST) so the baseline records them explicitly.
fn extract_comm_globl(line: &str, file: &str, line_no: usize, out: &mut BTreeMap<String, SymbolEntry>) {
    // `.comm <sym>,` — the symbol is the first token after `.comm`.
    if let Some(sym) = match_after(line, ".comm ") {
        if let Some(clean) = clean_symbol(sym) {
            // Skip runtime helper code labels (`__rt_*`) — they are function
            // entry points, not mutable data state, and do not belong in the
            // global-state audit.
            if clean.starts_with("__rt_") {
                return;
            }
            let notes = String::from(".comm");
            let class = classify(&clean, file, &notes);
            out.entry(clean.clone()).or_insert_with(|| SymbolEntry {
                symbol: clean,
                class,
                source: format!("codegen:{}:{}", file, line_no),
                notes,
            });
        }
    }
    // `.globl <sym>` — may be followed by `\n<sym>:` or a format-string closer.
    if let Some(sym) = match_after(line, ".globl ") {
        if let Some(clean) = clean_symbol(sym) {
            if clean.starts_with("__rt_") {
                return;
            }
            let notes = String::from(".globl");
            let class = classify(&clean, file, &notes);
            out.entry(clean.clone()).or_insert_with(|| SymbolEntry {
                symbol: clean,
                class,
                source: format!("codegen:{}:{}", file, line_no),
                notes,
            });
        }
    }
}

/// Extracts `static mut <name>` items from a bridge crate source line.
fn extract_static_mut(line: &str, file: &str, line_no: usize, out: &mut BTreeMap<String, SymbolEntry>) {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("static mut ") {
        let name = rest.trim_start().split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or("");
        if !name.is_empty() {
            let notes = String::from("static mut");
            let class = classify(name, file, &notes);
            out.entry(name.to_string()).or_insert_with(|| SymbolEntry {
                symbol: name.to_string(),
                class,
                source: format!("bridge:{}:{}", file, line_no),
                notes,
            });
        }
    }
}

/// Returns the slice after the first occurrence of `needle` in `line`, trimmed.
fn match_after<'a>(line: &'a str, needle: &str) -> Option<&'a str> {
    let idx = line.find(needle)?;
    let rest = &line[idx + needle.len()..];
    let rest = rest.trim_start();
    Some(rest)
}

/// Cleans a raw symbol token pulled from a format string. Accepts the leading
/// underscore convention, strips `{}`/`{name}` placeholders (runtime builds
/// these from format args, but the audit names the template), and rejects tokens
/// that are clearly not symbols (literals, labels with colons).
fn clean_symbol(raw: &str) -> Option<String> {
    // Stop at the first comma, brace, quote, or backslash — these delimit the
    // symbol inside a format-string literal.
    let end = raw.find(|c: char| matches!(c, ',' | '{' | '"' | '\\' | '\n' | ' ' | '`')).unwrap_or(raw.len());
    let mut s = raw[..end].trim().trim_end_matches(':').to_string();
    // Strip a trailing format placeholder like `{}` entirely (e.g. `.comm {}, 8`).
    if s.contains('{') {
        return None;
    }
    // Reject empty and clearly non-symbol tokens.
    if s.is_empty() || !s.starts_with('_') && !s.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
        // Allow bridge `static mut` uppercased names too; here we only reject
        // tokens that are purely numeric or punctuation.
        if s.is_empty() || s.chars().all(|c| !c.is_alphanumeric()) {
            return None;
        }
    }
    // Drop a leading Mach-O underscore for stable cross-target matching? No —
    // keep it; the baseline uses the emitted symbol name verbatim.
    s.make_ascii_lowercase();
    Some(s)
}

/// Parses a baseline TOML file into a map of `symbol -> (Class, notes)`.
/// The baseline format is the simple tabular form emitted by `render_baseline`.
pub fn load_baseline(path: &Path) -> BTreeMap<String, (Class, String)> {
    let Ok(text) = fs::read_to_string(path) else { return BTreeMap::new() };
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        // Format: `symbol = "CLASS"  # notes`
        if let Some(eq) = line.find('=') {
            let sym = line[..eq].trim().to_string();
            let rest = line[eq + 1..].trim();
            // Pull the quoted class token.
            let class_str = rest.trim_start_matches('"');
            let end = class_str.find('"').unwrap_or(class_str.len());
            let class = Class::parse(&class_str[..end]).unwrap_or(Class::Unknown);
            let notes = rest.find('#').map(|i| rest[i + 1..].trim().to_string()).unwrap_or_default();
            if !sym.is_empty() {
                map.insert(sym, (class, notes));
            }
        }
    }
    map
}

/// Renders a baseline TOML body from a sorted iterator of entries. Stable order
/// keeps baseline diffs reviewable.
pub fn render_baseline<'a, I>(entries: I) -> String
where
    I: IntoIterator<Item = (&'a String, Class, &'a String)>,
{
    let mut out = String::from("# elephc global-state audit baseline.\n");
    out.push_str("# One line per symbol: symbol = \"CLASS\"  # notes\n");
    out.push_str("# Classes: CONST | RO_BOOT | SHARED_LOCK | TLS | UNKNOWN\n");
    out.push_str("# Bump this file deliberately when a symbol is added or re-classified.\n\n");
    for (sym, class, notes) in entries {
        out.push_str(&format!("{} = \"{}\"  # {}\n", sym, class.as_str(), notes));
    }
    out
}

/// Diffs the live extraction against the baseline. Symbols present live but
/// missing from the baseline are returned as `new_symbols` (the CI failure).
/// Symbols present in the baseline but absent live are `removed` (informational;
/// a baseline bump should drop them).
pub fn check(live: &[SymbolEntry], baseline: &BTreeMap<String, (Class, String)>) -> CheckReport {
    let mut report = CheckReport {
        live_count: live.len(),
        baseline_count: baseline.len(),
        ..Default::default()
    };
    for e in live {
        match baseline.get(&e.symbol) {
            None => report.new_symbols.push(e.clone()),
            Some((bclass, _notes)) => {
                if *bclass != e.class && e.class != Class::Unknown {
                    report.changed.push((e.symbol.clone(), *bclass, e.class));
                }
            }
        }
    }
    for (sym, _) in baseline {
        if !live.iter().any(|e| &e.symbol == sym) {
            report.removed.push(sym.clone());
        }
    }
    report
}

/// Applies the baseline classifications to a live extraction in place. Symbols
/// not in the baseline keep UNKNOWN (which `check` then reports as new).
pub fn apply_baseline(live: &mut [SymbolEntry], baseline: &BTreeMap<String, (Class, String)>) {
    for e in live.iter_mut() {
        if let Some((class, notes)) = baseline.get(&e.symbol) {
            e.class = *class;
            if !notes.is_empty() {
                e.notes = notes.clone();
            }
        }
    }
}

/// Heuristically classifies a symbol using the spec's family rules. Used to
/// seed the initial baseline; human review must confirm. The rules:
/// - `.globl` rodata (string literals, error messages, lookup tables, vtables,
///   const descriptors) = CONST.
/// - bridge function-pointer slots (`_elephc_*_fn`, `_*_close_fn`, `_phar_*_fn`,
///   `_bz2_*_fn`, `_iconv_*_fn`, `_zlib_close_fn`) = RO_BOOT (written once at
///   bridge init, read-only after).
/// - process write-once boot state (`_global_argc`, `_global_argv`,
///   `_enum_singletons_init`, `_empty_str`, `_heap_debug_enabled`) = RO_BOOT.
/// - atomic / shared process counters (bridge `SERVED`) = SHARED_LOCK.
/// - request/execution state (REQ_*, RESPONSE_*, MULTIPART_CACHE, TMP_FILES,
///   WORKER_*, exception chain, heap arena, concat scratch, serialize/json
///   caches, strtotime clock, tz save, stream handle tables, function statics,
///   globals) = TLS.
/// - everything else = UNKNOWN (forces a human to classify before CI passes).
pub fn classify(symbol: &str, _source: &str, notes: &str) -> Class {
    let s = symbol;
    // `.globl` rodata: const tables, error messages, vtables, class-id slots
    // written once at boot. Distinguish: class-id slots (`_*_class_id`) are
    // RO_BOOT; pure rodata labels are CONST.
    if notes == ".globl" {
        // Bridge function-pointer slots (RO_BOOT).
        if is_bridge_fn_slot(s) {
            return Class::RoBoot;
        }
        // Class/interface id slots and boot-computed count/ptr tables: RO_BOOT.
        if s.ends_with("_class_id") || s.ends_with("_class_id_") {
            return Class::RoBoot;
        }
        // Per-class descriptor storage (trailing `_` = one per class, written
        // once at class-registration boot): RO_BOOT. This covers `_class_*_`,
        // `_callable_*_`, `_interface_*_`, `_instanceof_name_*_`,
        // `_user_*_vtable_`, and `_hash_algo_` per-algo slots.
        if s.ends_with('_')
            && (s.starts_with("_class_")
                || s.starts_with("_callable_")
                || s.starts_with("_interface_")
                || s.starts_with("_instanceof_name_")
                || s.starts_with("_user_filter_vtable_")
                || s.starts_with("_user_wrapper_vtable_")
                || s.starts_with("_hash_algo_"))
        {
            return Class::RoBoot;
        }
        // Boot-built descriptor count/ptr/entries/missing tables.
        if s.ends_with("_count") || s.ends_with("_ptrs") || s.ends_with("_table")
            || s.ends_with("_entries") || s.ends_with("_missing")
        {
            if s.contains("_vtable") || s.contains("_method") || s.contains("_attribute")
                || s.contains("_interface") || s.contains("_parent") || s.contains("_gc_desc")
                || s.contains("_json_desc") || s.contains("_callable") || s.contains("_class")
                || s.contains("_name") || s.contains("_static")
        {
                return Class::RoBoot;
            }
            // Pure const lookup tables (b64, weekday names, month names, etc.).
            return Class::Const;
        }
        // Boot-built class registry and parent-id table (no trailing `_`, but
        // written once at class-registration boot).
        if s == "_classes_by_name" || s == "_class_parent_ids"
            || s == "_callable_invoke_name"
        {
            return Class::RoBoot;
        }
        // Pure rodata: string literals, key names, format strings, error
        // messages, debug labels, command strings, lookup tables. These are
        // never mutated after link.
        if s.ends_with("_msg") || s.ends_with("_str") || s.ends_with("_lit")
            || s.starts_with("_str_") || s.starts_with("_float_") || s.starts_with("_lt_k_")
            || s.starts_with("_lt_v_") || s.ends_with("_tab") || s.ends_with("_tbl")
            || s.ends_with("_fmt") || s.ends_with("_names") || s.ends_with("_algo_name")
            || s.ends_with("_key") || s.ends_with("_default") || s.ends_with("_template")
            || s.ends_with("_magic") || s.ends_with("_prefix") || s.ends_with("_cmd")
            || s.ends_with("_mode") || s.ends_with("_newline") || s.ends_with("_label")
            || s == "_php_tz_utc" || s == "_heap_max"
        {
            return Class::Const;
        }
        // Date format strings and calendar lookup tables.
        if s.starts_with("_date_fmt_") || s == "_days_in_month" || s == "_day_names"
            || s == "_month_names"
        {
            return Class::Const;
        }
        // Remaining string-literal singletons: file-get-contents slash, zlib
        // version string.
        if s == "_fgc_url_slash" || s == "_zlib_version" {
            return Class::Const;
        }
        // Prefix-based const-rodata families: error/message strings, key names,
        // format strings, debug labels, var_dump prefixes, stat/meta keys,
        // HTTP/string-literal prefixes, FTP command strings, stream-meta keys.
        if s.starts_with("_fiber_msg_")
            || s.starts_with("_filetype_")
            || s.starts_with("_fmt_")
            || s.starts_with("_heap_dbg_")
            || s.starts_with("_http_")
            || s.starts_with("_ftp_")
            || s.starts_with("_json_")
            || s.starts_with("_locale_")
            || s.starts_with("_meta_")
            || s.starts_with("_pathinfo_key_")
            || s.starts_with("_pr_")
            || s.starts_with("_vd_")
            || s.starts_with("_stat_key_")
            || s.starts_with("_etc_")
            || s.starts_with("_dirname_")
            || s.starts_with("_spl_autoload_exts_")
            || s.starts_with("_phar_")
            || s.starts_with("_diag_")
        {
            return Class::Const;
        }
        // Remaining `.globl` without a clear rule: UNKNOWN (the audit gate
        // forces a human classification decision before bumping the baseline).
        return Class::Unknown;
    }

    // `.comm` symbols.
    if notes == ".comm" {
        // Bridge function-pointer slots (RO_BOOT).
        if is_bridge_fn_slot(s) {
            return Class::RoBoot;
        }
        // Process write-once boot state.
        if matches!(s, "_global_argc" | "_global_argv" | "_enum_singletons_init"
            | "_empty_str" | "_heap_debug_enabled" | "_php_default_tz_len")
        {
            return Class::RoBoot;
        }
        // Heap arena + GC counters: TLS.
        if s.starts_with("_heap_") || s.starts_with("_gc_") {
            return Class::Tls;
        }
        // Exception / bailout / fiber chain: TLS.
        if s.starts_with("_exc_") || s.starts_with("_exit_") || s.starts_with("_fiber_") {
            return Class::Tls;
        }
        // Concat/cstr scratch: TLS.
        if s.starts_with("_concat_") || s.starts_with("_cstr_") {
            return Class::Tls;
        }
        // Serialize / JSON / date / tz caches: TLS.
        if s.starts_with("_ser_") || s.starts_with("_unser_") || s.starts_with("_json_")
            || s.starts_with("_php_tz_") || s == "_strtotime_clock"
        {
            return Class::Tls;
        }
        // Stream / network / handle tables: TLS.
        if s.starts_with("_stream_") || s.starts_with("_http_") || s.starts_with("_https_")
            || s.starts_with("_ftp_") || s.starts_with("_fsockopen_") || s.starts_with("_tls_")
            || s.starts_with("_dir_") || s.starts_with("_glob_") || s.starts_with("_popen_")
            || s.starts_with("_eof_") || s.starts_with("_bzstream_") || s.starts_with("_zstream_")
            || s.starts_with("_iconv_") || s.starts_with("_recvfrom_") || s.starts_with("_accept_")
            || s.starts_with("_servent_") || s.starts_with("_protoent_") || s.starts_with("_phar_write_")
            || s.starts_with("_url_") || s.starts_with("_fgc_") || s.starts_with("_user_wrapper_")
            || s.starts_with("_user_filter_") || s == "_user_wrappers"
            || s.starts_with("_stream_grow_") || s.starts_with("_http_redirect_")
            || s.starts_with("_principal_") || s == "_ssl_key_str" || s.starts_with("_ssl_local_")
            || s.starts_with("_tls_peer_")
        {
            return Class::Tls;
        }
        // Statics + globals: TLS.
        if s.starts_with("_static_") || s.starts_with("_eir_global_") || s.starts_with("_gvar_") {
            return Class::Tls;
        }
        // Diagnostic suppression (per-request @ state): TLS.
        if s == "_rt_diag_suppression" {
            return Class::Tls;
        }
        // Web capture mode flag: TLS.
        if s == "elephc_web_capture" {
            return Class::Tls;
        }
        // Unknown `.comm` — force human classification.
        return Class::Unknown;
    }

    // Bridge `static mut`.
    if notes == "static mut" {
        // Request/response state: TLS.
        if s.starts_with("REQ_") || s.starts_with("RESPONSE_") || s == "MULTIPART_CACHE"
            || s == "TMP_FILES"
        {
            return Class::Tls;
        }
        // Worker boot/handler/config: RO_BOOT (set once at boot).
        if matches!(s, "WORKER_BOOT" | "WORKER_BOOTED" | "WORKER_HANDLER" | "WORKER_CONFIG"
            | "WORKER_LISTEN" | "SCRIPT_HANDLER" | "BOOT_PIPE_WR" | "ENV_CACHE")
        {
            return Class::RoBoot;
        }
        // Shared process counters / cross-worker channels: SHARED_LOCK.
        if s == "SERVED" || s == "CHILD_DISPATCH_CHAN" {
            return Class::SharedLock;
        }
        // Web capture mode flag (per-process request capture toggle): TLS.
        if s == "elephc_web_capture" {
            return Class::Tls;
        }
        return Class::Unknown;
    }

    Class::Unknown
}

/// Returns true if the symbol looks like a bridge function-pointer slot
/// (`_elephc_*_fn`, `_*_close_fn`, etc.) written once at bridge init.
fn is_bridge_fn_slot(s: &str) -> bool {
    s.starts_with("_elephc_") && s.ends_with("_fn")
        || s.starts_with("_zlib_") && s.ends_with("_fn")
        || s.starts_with("_bz2_") && s.ends_with("_fn")
        || s.starts_with("_phar_") && s.ends_with("_fn")
        || s.starts_with("_iconv_") && s.ends_with("_fn")
}

/// Returns the path to the baseline file for a given target triple directory
/// inside the audit crate's `baseline/` folder.
pub fn baseline_path(crate_dir: &Path, target: &str) -> PathBuf {
    crate_dir.join("baseline").join(format!("{}.toml", target))
}

/// Lists the targets that have a committed baseline table.
pub fn baseline_targets(crate_dir: &Path) -> Vec<String> {
    let dir = crate_dir.join("baseline");
    let mut out = Vec::new();
    if let Ok(read) = fs::read_dir(&dir) {
        for ent in read.flatten() {
            if let Some(name) = ent.path().file_name().and_then(|s| s.to_str()) {
                if let Some(stem) = name.strip_suffix(".toml") {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit-level sanity: the cleaner accepts a normal underscore symbol and
    /// rejects a format placeholder.
    #[test]
    fn cleaner_accepts_real_symbol_rejects_placeholder() {
        assert_eq!(clean_symbol("_strtotime_clock,"), Some("_strtotime_clock".to_string()));
        assert_eq!(clean_symbol("{}, 8, 3"), None);
    }

    /// Classification round-trips through `parse`/`as_str` for every bucket.
    #[test]
    fn class_roundtrip() {
        for c in [Class::Const, Class::RoBoot, Class::SharedLock, Class::Tls, Class::Unknown] {
            assert_eq!(Class::parse(c.as_str()), Some(c));
        }
    }
}