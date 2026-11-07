//! Purpose:
//! Provides the assembler/linker facade used by the compiler pipeline.
//! Adapts legacy string options into a typed plan and orchestrates platform helpers.
//!
//! Called from:
//! - `crate::pipeline::compile()` after user and runtime object generation.
//! - `crate::cli` for table-driven bridge flag validation.
//!
//! Key details:
//! - The legacy `link()` API remains behavior-compatible while callers migrate to `LinkPlan`.
//! - Dependency resolution stays outside this module; the linker consumes typed paths only.

mod archive_dedup;
mod aarch64_encode;
mod asm_cache;
mod asm_split;

mod bridges;
mod inhouse;
mod command;
mod pdo;
mod sdk;

use std::path::{Path, PathBuf};
use std::process::{self, Command};

use crate::codegen::platform::{AppleVariant, Platform, Target};
use crate::codegen::Emit;
use crate::link_plan::{LinkItem, LinkPlan};

use self::command::{LinkPaths, MacSdk};

/// Structured failure produced while preparing typed linker inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// A requested Elephc bridge archive could not be resolved to a regular file.
    MissingBridge {
        /// Authoritative bridge linker name that could not be materialized.
        name: String,
        /// Archive filename or exact path this bridge resolves to, when known.
        ///
        /// An unknown named bridge has no archive path or discovery metadata and renders
        /// only the first line. An unknown bridge with an exact archive path reports that
        /// path in `needs:` without inventing fallback locations.
        archive: Option<String>,
        /// Per-bridge directory override that takes priority over every search location.
        env_var: Option<String>,
        /// Directories that were consulted, in the order they were tried.
        searched: Vec<String>,
        /// The `env_var` directory, when it is what short-circuited discovery.
        ///
        /// An override does not join the search — it REPLACES it, returning before any
        /// fallback is consulted. So its presence changes what the message may claim: the
        /// fallbacks were not tried, and the fix is to correct or unset the variable rather
        /// than to place an archive next to the binary, which the override would ignore.
        override_dir: Option<String>,
    },
}

impl std::fmt::Display for LinkError {
    /// Formats an actionable linker-preparation diagnostic.
    ///
    /// "Actionable" means naming the way out, which the bare first line never did: a binary
    /// copied out of its build tree fails here with nothing to go on, and the archives simply
    /// have to travel with it (issue #517). So the message reports the archive it wanted, the
    /// directories it actually looked in, and the environment override — a resolution order
    /// that is otherwise only discoverable by reading `linker::bridges`.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBridge {
                name,
                archive,
                env_var,
                searched,
                override_dir,
            } => {
                write!(formatter, "required Elephc bridge `{name}` could not be found")?;
                let Some(archive) = archive else {
                    return Ok(());
                };
                write!(formatter, "\n  needs: {archive}")?;
                if !searched.is_empty() {
                    write!(formatter, "\n  looked in:")?;
                    for directory in searched {
                        write!(formatter, "\n    {directory}")?;
                    }
                }
                let Some(env_var) = env_var else {
                    return Ok(());
                };
                // An override that short-circuited discovery gets the opposite advice: the
                // fallbacks were never consulted, so telling the user to put the archive next
                // to the binary would send them somewhere this run will not look.
                let invalid_file = invalid_archive_filename(archive);
                match (override_dir, invalid_file) {
                    (Some(override_dir), Some(filename)) => write!(
                        formatter,
                        "\n\n{env_var} is set to {override_dir}, which takes priority over every \
                         other location, so no other directory was consulted. Replace invalid \
                         archive {archive} with a non-empty regular file, point {env_var} to a \
                         directory containing a valid {filename}, or unset {env_var} to search \
                         the other locations again. `elephc --print-capabilities` lists every \
                         archive this binary can need."
                    ),
                    (Some(override_dir), None) => write!(
                        formatter,
                        "\n\n{env_var} is set to {override_dir}, which takes priority over every \
                         other location — the elephc binary's own directory, a sibling lib/ and \
                         the build tree were NOT consulted. Put {archive} in {override_dir}, \
                         point {env_var} somewhere that has it, or unset {env_var} to search \
                         those locations again. `elephc --print-capabilities` lists every \
                         archive this binary can need."
                    ),
                    (None, Some(filename)) => write!(
                        formatter,
                        "\n\nReplace invalid archive {archive} with a non-empty regular \
                         file, or set {env_var} to a directory containing a valid \
                         {filename}. `elephc --print-capabilities` lists every archive \
                         this binary can need."
                    ),
                    (None, None) => write!(
                        formatter,
                        "\n\nSet {env_var} to a directory containing {archive}, or keep the \
                         bridge archives next to the elephc binary (or in a sibling lib/). \
                         `elephc --print-capabilities` lists every archive this binary can need."
                    ),
                }
            }
        }
    }
}

/// Returns the filename of `archive` when it is a full path to an entry that exists but is
/// not a usable archive, which is how the errors distinguish an invalid file from a missing
/// one: a missing archive is reported by its bare filename.
fn invalid_archive_filename(archive: &str) -> Option<&str> {
    Path::new(archive)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| *name != archive)
}

impl std::error::Error for LinkError {}

/// Resolves a `--with-<flag>` suffix to its bridge linker library name.
pub(crate) fn bridge_lib_for_flag(flag: &str) -> Option<&'static str> {
    bridges::bridge_lib_for_flag(flag)
}

/// Returns every accepted `--with-<flag>` suffix in bridge table order.
pub(crate) fn crate_flag_names() -> Vec<&'static str> {
    bridges::crate_flag_names()
}

/// Maps a `--with-<flag>` suffix to the archive filename it resolves to.
pub(crate) fn archive_filename_for_flag(flag: &str) -> Option<String> {
    bridges::archive_filename_for_flag(flag)
}

/// Returns bridge library/flag pairs present in one planned named-library set.
pub(crate) fn bridges_in(
    link_libraries: &[String],
) -> Vec<(&'static str, &'static str)> {
    bridges::bridges_in(link_libraries)
}

/// The archive of the bridge named `lib_name` (`libelephc_<name>.a`), auto-built in a
/// source checkout. Hosted PHP extensions link the `elephc_zend` engine through it, after
/// their own archives.
pub(crate) fn bridge_archive(lib_name: &str) -> Result<std::path::PathBuf, LinkError> {
    bridges::locate_archive(lib_name)
}

/// Maps one bridge library name to the PHP extensions identified by linking that bridge.
pub(crate) fn php_extensions_for_lib(lib_name: &str) -> &'static [&'static str] {
    bridges::php_extensions_for_lib(lib_name)
}

/// Returns native libraries required by the selected optional PDO bridge profile.
pub(crate) fn pdo_system_libraries() -> Vec<&'static str> {
    pdo::system_libraries()
}

/// Builds the assembler invocation for a target, minus the input and output.
///
/// A Mach-O object records the platform it was built for, and `ld` refuses to
/// link a macOS-tagged object into an iOS image — the failure reads
/// *"building for 'iOS-simulator', but linking in object file built for
/// 'macOS'"*. Plain `as` has no way to say "iOS", so non-macOS Apple targets
/// assemble through `clang`, which stamps the platform from `-target` and needs
/// the matching SDK to resolve it.
///
/// Shared by the user object and the cached runtime object. Both must carry the
/// same platform or the link fails on whichever one disagrees, and they used to
/// build this command separately.
pub(crate) fn assembler_command(target: Target) -> Command {
    if target.platform == Platform::MacOS && target.apple_variant != AppleVariant::MacOS {
        let sdk_path = sdk::macos_sdk_path(target.apple_sdk_name());
        let mut assembler = Command::new("clang");
        assembler
            .arg("-c")
            .args(["-target", &target.apple_clang_triple()])
            .args(["-isysroot", &sdk_path]);
        return assembler;
    }
    let mut assembler = Command::new(target.assembler_cmd());
    if target.platform == Platform::MacOS {
        assembler.args(["-arch", target.darwin_arch_name()]);
    }
    assembler
}

/// Whether the in-tree assembler (`crate::linker::inhouse`) handles this target: macOS arm64,
/// unless `ELEPHC_ASM=system` asks for `as`.
///
/// It runs in the compiler's own process, so `as` and its process start go away. A Symfony build
/// on one core spent 747 s of CPU, children included, against 546 s through this path.
fn inhouse_enabled(target: Target) -> bool {
    target.platform == Platform::MacOS
        && target.apple_variant == AppleVariant::MacOS
        && target.arch == crate::codegen::platform::Arch::AArch64
        && !matches!(std::env::var("ELEPHC_ASM").as_deref(), Ok("system"))
}

/// Assembles `asm_path` with the in-tree assembler when it is enabled and handles the text,
/// reporting whether `obj_path` was written. A slice it does not handle is left to `as`.
fn assemble_inhouse(target: Target, asm_path: &Path, obj_path: &Path) -> bool {
    if !inhouse_enabled(target) {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(asm_path) else {
        return false;
    };
    match inhouse::assemble(&text) {
        Ok(object) => std::fs::write(obj_path, object).is_ok(),
        Err(inhouse::Unsupported(reason)) => {
            if trace_enabled() {
                eprintln!("asm-trace inhouse fallback {}: {reason}", asm_path.display());
            }
            false
        }
    }
}

/// Invokes the target assembler for one generated assembly source file.
pub(crate) fn assemble(target: Target, asm_path: &Path, obj_path: &Path) {
    if assemble_inhouse(target, asm_path, obj_path) {
        return;
    }
    let mut assembler = assembler_command(target);
    assembler.arg("-o").arg(obj_path).arg(asm_path);
    command::run_tool("Assembler", &mut assembler);
}

/// Ceiling on concurrent `as` processes for one user object.
///
/// The default is one per core. It used to stop at eight, where the curve had flattened while
/// the slices were rendered one after another before any assembler started; with every slice
/// rendered and assembled by its own worker, the Symfony `--web` assembly phase measured 11.7 s
/// at 12 jobs, 8.5 s at 18 (this machine's core count) and 8.4 s at 24 and 36. Assembler memory
/// is proportional to the slice, so more, smaller slices do not add to it.
const MAX_ASSEMBLER_JOBS: usize = 64;

/// Largest slice of assembly text one `as` run is handed, whatever the job count.
///
/// An AArch64 `bl` reaches +-128 MB, and the assembler resolves a call to a symbol defined in the
/// SAME object itself instead of leaving the linker a relocation it could route through a branch
/// island. The Symfony demo's preload build wrote 27 GB of assembly, which 18 jobs cut into
/// 1.5 GB slices of about 150 MB of code each, and every far call inside one failed with "fixup
/// value out of range".
///
/// The bound must hold for the DENSEST text, not the average. Commented code runs about ten
/// source bytes per code byte, but the eval-bridge dispatch ladders carry no comments: the demo
/// console's 18 GB of assembly ran 2.6 bytes per code byte, so 768 MB slices held ~300 MB of code
/// each. A slice is also one Mach-O atom (user objects carry no `.subsections_via_symbols`), and
/// the linker cannot put a branch island inside an atom: `ld` failed with "atom ... is larger than
/// the max code size between branch island clusters" and an `arm64_b26` fixup out of range. At
/// 192 MB a slice stays under ~77 MB of code even at that density.
const MAX_SLICE_SOURCE_BYTES: usize = 192 << 20;

/// Returns how many slices to cut `source_len` bytes of assembly into: one per job, and more when
/// the text is large enough that a job-sized slice would exceed [`MAX_SLICE_SOURCE_BYTES`].
fn slice_parts(jobs: usize, source_len: usize) -> usize {
    jobs.max(source_len.div_ceil(MAX_SLICE_SOURCE_BYTES))
}

/// Large inputs need safe object boundaries even when only one assembler may run at a time.
fn needs_slices(jobs: usize, source_len: usize) -> bool {
    jobs > 1 || source_len > MAX_SLICE_SOURCE_BYTES
}

/// Processes slices with a bounded worker pool, preserving per-slice order and panic outcomes.
///
/// Slice count is a size-safety decision, not permission to spawn that many assemblers. Catching
/// each slice's panic preserves the caller's existing fail-closed policy without losing the other
/// queued outcomes. Results are published only after work completes, outside the work closure.
fn run_slice_workers<T: Send>(
    count: usize,
    jobs: usize,
    work: impl Fn(usize) -> T + Sync,
) -> Vec<std::thread::Result<T>> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let outcomes: std::sync::Mutex<Vec<Option<std::thread::Result<T>>>> =
        std::sync::Mutex::new((0..count).map(|_| None).collect());
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..count.min(jobs.max(1)))
            .map(|_| {
                let (next, outcomes, work) = (&next, &outcomes, &work);
                scope.spawn(move || loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if index >= count {
                        break;
                    }
                    let outcome =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(index)));
                    outcomes.lock().expect("assembler outcomes mutex poisoned")[index] = Some(outcome);
                })
            })
            .collect();
        for worker in workers {
            // An unexpected failure outside the slice's catch boundary leaves a missing outcome,
            // which is returned as a panic below rather than silently omitting an object.
            let _ = worker.join();
        }
    });
    outcomes
        .into_inner()
        .expect("assembler outcomes mutex poisoned")
        .into_iter()
        .map(|outcome| {
            outcome.unwrap_or_else(|| Err(Box::new("assembler worker did not publish its result")))
        })
        .collect()
}

/// Objects produced from one user assembly file, in slice order, plus a note for `--timings`.
pub(crate) struct AssembledObjects {
    /// Object paths in the order the slices must reach the link line. The first
    /// entry is always the caller's `obj_path`, so every existing cleanup and
    /// debug-map path keeps working unchanged.
    pub(crate) objects: Vec<PathBuf>,
    /// What the split had to publish, when it split at all.
    pub(crate) note: Option<String>,
}

/// Returns how many `as` processes may assemble one user object.
///
/// `ELEPHC_ASM_JOBS` overrides the host-derived default. One job preserves the single-object
/// path for small inputs, but still processes large inputs as safe, sequential slices.
fn assembler_jobs() -> usize {
    if let Ok(value) = std::env::var("ELEPHC_ASM_JOBS") {
        return value.trim().parse::<usize>().unwrap_or(1).clamp(1, 64);
    }
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, MAX_ASSEMBLER_JOBS)
}

/// Assembles one user assembly file, splitting it across parallel `as` runs when
/// the target supports it, and returns the objects in slice order.
///
/// Every fallback path here produces exactly today's single object: an
/// unsupported target or artifact, a small input with one worker, `--debug-info` (whose
/// dSYM debug map has to name one object per compilation unit and is not worth
/// re-deriving for a build the developer is about to step through), an
/// unreadable `.s`, an assembly with no legal cut, or a failed slice write.
///
/// The source text is freed as soon as the split is done, and each slice once it is written, so
/// the assemblers run beside the compiler's residual heap instead of the text plus every slice.
pub(crate) fn assemble_parallel(
    target: Target,
    emit: Emit,
    debug_info: bool,
    asm_path: &Path,
    obj_path: &Path,
) -> AssembledObjects {
    let single = || AssembledObjects {
        objects: vec![obj_path.to_path_buf()],
        note: None,
    };
    let jobs = assembler_jobs();
    if debug_info || !asm_split::supports_split(target, emit) {
        assemble(target, asm_path, obj_path);
        return single();
    }
    let source_len = std::fs::metadata(asm_path)
        .map(|metadata| usize::try_from(metadata.len()).unwrap_or(usize::MAX))
        .unwrap_or(0);
    if !needs_slices(jobs, source_len) {
        assemble(target, asm_path, obj_path);
        return single();
    }
    // Map the `.s` instead of reading it: the pages are clean and file-backed, so the kernel can
    // drop them under pressure instead of the build holding a 1.5 GB private copy of the text.
    let Some(mapped) = crate::mapped_file::MappedFile::open(asm_path) else {
        assemble(target, asm_path, obj_path);
        return single();
    };
    let Ok(source) = std::str::from_utf8(mapped.bytes()) else {
        drop(mapped);
        assemble(target, asm_path, obj_path);
        return single();
    };

    let slice_asm = |index: usize| obj_path.with_extension(format!("slice{index}.s"));
    let slice_obj = |index: usize| match index {
        0 => obj_path.to_path_buf(),
        _ => obj_path.with_extension(format!("slice{index}.o")),
    };

    let parts = slice_parts(jobs, source.len());
    let plan_started = std::time::Instant::now();
    let Some(mut plan) = asm_split::plan_split(
        source,
        parts,
        target.platform.local_label_prefix(),
        // `as` matches an instruction slowly, so the split hands it encoded words. The in-tree
        // assembler encodes anyway: encoding here too, then writing and parsing the hex, took a
        // slice from 6 s to 15.
        target.arch == crate::codegen::platform::Arch::AArch64
            && encode_enabled()
            && !inhouse_enabled(target),
    ) else {
        drop(mapped);
        assemble(target, asm_path, obj_path);
        return single();
    };
    if compact_enabled() {
        plan.compact(target.line_comment_prefix());
    }
    let slice_count = plan.slices();
    if trace_enabled() {
        eprintln!("asm-trace plan: {:.2}s, {slice_count} slices", plan_started.elapsed().as_secs_f64());
    }

    // Every slice is rendered, keyed, written and -- unless the cache already holds its object --
    // assembled by a bounded worker pool. Rendering every slice before starting any assembler, as
    // this did, left the assemblers idle for the whole serial render; now the first `as` starts
    // as soon as its own slice is on disk, and no slice ever exists in memory whole: its cache
    // key is folded from the same chunks that are written.
    let caching = asm_cache::is_enabled();
    let identity = caching.then(|| asm_cache::assembler_identity(&assembler_command(target)));
    let outcomes = run_slice_workers(slice_count, jobs, |index| -> std::io::Result<SliceOutcome> {
        let asm = slice_asm(index);
        let started = std::time::Instant::now();
        let mut key = identity.as_deref().map(asm_cache::slice_key_seed);
        // The in-tree assembler takes the slice straight from memory: writing it to disk only to
        // read it back cost a copy of the whole listing. A slice it does not handle is written
        // out and handed to `as` as before.
        if inhouse_enabled(target) {
            let mut text = String::new();
            plan.render_slice(index, &mut |chunk| {
                if let Some(key) = key.as_mut() {
                    *key = asm_cache::extend_slice_key(*key, chunk.as_bytes());
                }
                text.push_str(chunk);
                Ok(())
            })?;
            if let Some(key) = key {
                if asm_cache::reuse(key, &slice_obj(index)) {
                    return Ok(SliceOutcome { key: Some(key), assembled: false });
                }
            }
            let rendered = started.elapsed();
            match inhouse::assemble(&text) {
                Ok(object) => {
                    std::fs::write(slice_obj(index), object)?;
                    if trace_enabled() {
                        eprintln!(
                            "asm-trace slice {index}: {} bytes, render {:.2}s, inhouse {:.2}s",
                            text.len(),
                            rendered.as_secs_f64(),
                            (started.elapsed() - rendered).as_secs_f64()
                        );
                    }
                    return Ok(SliceOutcome { key, assembled: true });
                }
                Err(inhouse::Unsupported(reason)) => {
                    if trace_enabled() {
                        eprintln!("asm-trace inhouse fallback slice {index}: {reason}");
                    }
                    std::fs::write(&asm, &text)?;
                    let mut assembler = assembler_command(target);
                    assembler.arg("-o").arg(slice_obj(index)).arg(&asm);
                    command::run_tool("Assembler", &mut assembler);
                    return Ok(SliceOutcome { key, assembled: true });
                }
            }
        }
        let file = std::fs::File::create(&asm)?;
        let mut writer = std::io::BufWriter::with_capacity(1 << 20, file);
        plan.render_slice(index, &mut |text| {
            if let Some(key) = key.as_mut() {
                *key = asm_cache::extend_slice_key(*key, text.as_bytes());
            }
            std::io::Write::write_all(&mut writer, text.as_bytes())
        })?;
        std::io::Write::flush(&mut writer)?;
        drop(writer);
        // A restored object is a HARDLINK to the cache entry, so anything that writes to it in
        // place would write through into the cache; `reuse` first unlinks the destination name.
        if let Some(key) = key {
            if asm_cache::reuse(key, &slice_obj(index)) {
                let _ = std::fs::remove_file(&asm);
                return Ok(SliceOutcome { key: Some(key), assembled: false });
            }
        }
        let rendered = started.elapsed();
        assemble(target, &asm, &slice_obj(index));
        if trace_enabled() {
            eprintln!(
                "asm-trace slice {index}: {} bytes, render {:.2}s, as {:.2}s",
                std::fs::metadata(&asm).map(|m| m.len()).unwrap_or(0),
                rendered.as_secs_f64(),
                (started.elapsed() - rendered).as_secs_f64()
            );
        }
        Ok(SliceOutcome { key, assembled: true })
    });
    let summary = plan.summary;
    drop(mapped);

    let mut keys: Vec<u64> = Vec::with_capacity(slice_count);
    let mut misses: Vec<usize> = Vec::new();
    let mut failed_write = false;
    for (index, outcome) in outcomes.into_iter().enumerate() {
        match outcome {
            Ok(Ok(outcome)) => {
                keys.extend(outcome.key);
                if outcome.assembled {
                    misses.push(index);
                }
            }
            Ok(Err(_)) => failed_write = true,
            Err(_) => {
                // A worker that failed the tool call already exited the process; a
                // panicking one has to be reported here rather than linked around.
                eprintln!("Assembler: a parallel slice panicked");
                process::exit(1);
            }
        }
    }
    if failed_write {
        // A slice that could not be written leaves nothing to link; assemble the file whole.
        for index in 0..slice_count {
            let _ = std::fs::remove_file(slice_asm(index));
            let _ = std::fs::remove_file(slice_obj(index));
        }
        assemble(target, asm_path, obj_path);
        return single();
    }
    let cached = slice_count - misses.len();

    // Publish only what this build actually assembled. A hit was already published
    // by whoever produced it, and re-publishing it would only move its mtime.
    if caching {
        for &index in &misses {
            asm_cache::publish(keys[index], &slice_obj(index));
        }
        asm_cache::prune(&keys);
    }

    // The slice sources are derived files: the `.s` the developer asked for is
    // still on disk untouched. A failing `as` exits before this point, so a
    // slice that could not be assembled is always left behind for inspection.
    let objects: Vec<PathBuf> = (0..slice_count).map(|index| slice_obj(index)).collect();
    for &index in &misses {
        let _ = std::fs::remove_file(slice_asm(index));
    }
    let note = format!(
        "Assembler: {} slices, {} workers ({} temporaries promoted, {} locals published, {} reused from cache)",
        objects.len(),
        jobs.min(slice_count),
        summary.promoted.len(),
        summary.published,
        cached
    );
    AssembledObjects {
        objects,
        note: Some(note),
    }
}

/// Whether slices may carry pre-encoded instructions. `ELEPHC_ASM_ENCODE=0` hands `as` the
/// generated text unchanged, which is also what the object-identity check compares against.
/// Whether slices are written compact (`asm_split::SplitPlan::compact`). `ELEPHC_ASM_COMPACT=0`
/// writes them as the `.s` has them, which keeps a failing slice readable.
fn compact_enabled() -> bool {
    !matches!(std::env::var("ELEPHC_ASM_COMPACT").as_deref(), Ok("0") | Ok("off"))
}

/// `ELEPHC_ASM_TRACE=1` prints each slice's size and render/assemble times to stderr.
fn trace_enabled() -> bool {
    std::env::var_os("ELEPHC_ASM_TRACE").is_some()
}

fn encode_enabled() -> bool {
    !matches!(std::env::var("ELEPHC_ASM_ENCODE").as_deref(), Ok("0") | Ok("off"))
}

/// What one slice worker did.
struct SliceOutcome {
    /// The slice's cache key, when the cache is on.
    key: Option<u64>,
    /// Whether the slice went through `as` (a miss) rather than being restored from the cache.
    assembled: bool,
}

/// Packs the compiled objects into a static library with `ar`.
///
/// This deliberately does not go through the link plan. An archive is not a
/// link: bridge staticlibs and managed native packages stay separate `.a` files
/// for the consuming project to link alongside this one, exactly as a C library
/// would leave its own dependencies to its consumer. Rolling them in would
/// duplicate every symbol the host also links directly.
///
/// `rcs` replaces the archive, creates it when absent, and writes the symbol
/// index in one step -- no separate `ranlib` pass is needed on either platform.
pub(crate) fn archive(archive_path: &Path, object: &Path, runtime_object: &Path) {
    // A stale archive would otherwise keep members from the previous build,
    // because `r` replaces matching names but never removes vanished ones.
    let _ = std::fs::remove_file(archive_path);

    let mut ar = Command::new("ar");
    ar.arg("rcs").arg(archive_path).arg(object).arg(runtime_object);
    command::run_tool("ar", &mut ar);
}

/// Removes the symbol table from a linked executable.
///
/// Measured on this compiler's own output: 24 % of a `<?php echo 1;` binary and 28 % of a
/// realistic one, and the share grows with the program rather than shrinking — the symbol table
/// is roughly proportional to the number of declarations while `__text` is not. Nothing in a
/// compiled program reads its own symbols: `Throwable::getTrace()` is unimplemented and the
/// uncaught-exception report prints no stack trace, so the names are dead weight at run time.
///
/// NEVER strips a cdylib. Its exported symbols are its interface, and a host resolving one by
/// `dlsym` would get a null it may well read as "feature absent" rather than as an error.
///
/// Failure is not fatal: a missing or foreign `strip` leaves a larger binary, which is a worse
/// outcome than a failed build for something that is purely a size optimisation.
pub(crate) fn strip_symbols(target: Target, emit: Emit, bin_path: &Path) -> Result<(), String> {
    if emit != Emit::Executable {
        return Ok(());
    }
    let tool = target.strip_cmd();
    match Command::new(tool).arg(bin_path).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{tool} exited with {status}")),
        Err(error) => Err(format!("could not run {tool}: {error}")),
    }
}

/// Bakes macOS debug maps into a dSYM before temporary objects are removed.
pub(crate) fn bake_debug_info(target: Target, bin_path: &Path) -> bool {
    if target.platform != Platform::MacOS {
        return true;
    }
    let status = Command::new("dsymutil").arg(bin_path).status();
    matches!(status, Ok(status) if status.success())
}

/// Adapts the existing raw linker arguments into a typed plan and links the output.
///
/// Raw libraries retain their legacy dynamic behavior. New managed dependency
/// integration should call [`link_with_plan`] with exact archive items instead.
#[allow(dead_code)] // Retained as a compatibility adapter while non-pipeline callers migrate.
pub(crate) fn link(
    target: Target,
    emit: Emit,
    bin_path: &Path,
    obj_path: &Path,
    runtime_object_path: &Path,
    extra_link_libs: &[String],
    extra_link_paths: &[String],
    extra_frameworks: &[String],
    forced_whole_archive: &[String],
) {
    let mut plan = LinkPlan::new();
    for library in extra_link_libs {
        plan.push(LinkItem::named_user(library));
    }
    for path in extra_link_paths {
        plan.push(LinkItem::SearchPath(PathBuf::from(path)));
    }
    for framework in extra_frameworks {
        plan.push(LinkItem::Framework(framework.clone()));
    }
    if let Err(error) = link_with_plan(
        target,
        emit,
        bin_path,
        obj_path,
        &[],
        runtime_object_path,
        &plan,
        forced_whole_archive,
    ) {
        eprintln!("Linker error: {error}");
        process::exit(1);
    }
}

/// Resolves bridge inputs and executes a linker command from an already typed plan.
///
/// `extra_objects` carries the second and later slices of a parallel-assembled
/// user object. They are appended immediately after `obj_path`, in slice order,
/// which is the emission order: `ld` concatenates section contents in input
/// order, and an emitted `__data` record's meaning depends on its neighbours.
pub(crate) fn link_with_plan(
    target: Target,
    emit: Emit,
    bin_path: &Path,
    obj_path: &Path,
    extra_objects: &[PathBuf],
    runtime_object_path: &Path,
    plan: &LinkPlan,
    forced_whole_archive: &[String],
) -> Result<(), LinkError> {
    link_with_plan_and_inputs(target, emit, bin_path, obj_path, extra_objects,
        runtime_object_path, plan, forced_whole_archive).map(|_| ())
}

/// Links normally and returns exact archive paths plus bridge names for executable caches.
pub(crate) fn link_with_plan_and_inputs(
    target: Target,
    emit: Emit,
    bin_path: &Path,
    obj_path: &Path,
    extra_objects: &[PathBuf],
    runtime_object_path: &Path,
    plan: &LinkPlan,
    forced_whole_archive: &[String],
) -> Result<Vec<(PathBuf, Option<String>)>, LinkError> {
    let resolved = bridges::resolve(plan, forced_whole_archive, target.platform)?;
    let inputs = resolved.plan.items().iter().filter_map(|item| match item {
        LinkItem::StaticArchive { path, origin, .. } => {
            let bridge = match origin {
                crate::link_plan::LinkOrigin::Bridge { name } => Some(name.clone()),
                _ => None,
            };
            Some((path.clone(), bridge))
        }
        _ => None,
    }).collect();
    let prepared = (target.platform == Platform::MacOS)
        .then(|| archive_dedup::prepare(&resolved.plan));
    let render_plan = prepared
        .as_ref()
        .map(|prepared| &prepared.plan)
        .unwrap_or(&resolved.plan);

    // The SDK is selected by the target's Apple variant, not by the host: an
    // iOS build resolves the iOS SDK even though it runs on macOS.
    let apple_sdk = target.apple_sdk_name();
    let sdk_path = (target.platform == Platform::MacOS).then(|| sdk::macos_sdk_path(apple_sdk));
    let sdk_version =
        (target.platform == Platform::MacOS).then(|| sdk::macos_sdk_version(apple_sdk));
    let mac_sdk = sdk_path
        .as_deref()
        .zip(sdk_version.as_deref())
        .map(|(path, version)| MacSdk { path, version });
    let homebrew_paths = if target.platform == Platform::MacOS
        && render_plan.needs_default_macos_library_paths()
    {
        sdk::default_macos_library_paths()
    } else {
        Vec::new()
    };

    let rendered = command::render_link_command(
        target,
        emit,
        LinkPaths {
            bin: bin_path,
            object: obj_path,
            extra_objects,
            runtime: runtime_object_path,
        },
        render_plan,
        resolved.needs_libdl,
        mac_sdk,
        &homebrew_paths,
    );
    command::execute_link_command(rendered);

    if let Some(prepared) = prepared {
        prepared.cleanup();
    }
    Ok(inputs)
}

/// Rejects a cached host when an auto-built bridge has newer local source inputs.
pub(crate) fn cached_bridge_is_current(name: &str, archive: &Path) -> bool {
    bridges::cached_archive_is_current(name, archive)
}

#[cfg(test)]
mod slice_parts_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Small text keeps one slice per job; text past the cap gets enough slices to stay under it,
    /// so no slice holds more code than an AArch64 `bl` reaches.
    #[test]
    fn large_assembly_gets_more_slices_than_jobs() {
        assert_eq!(slice_parts(18, 100 << 20), 18);
        assert_eq!(slice_parts(18, 18 * MAX_SLICE_SOURCE_BYTES), 18);
        let large = 27usize << 30;
        let parts = slice_parts(18, large);
        assert!(parts > 18);
        assert!(large.div_ceil(parts) <= MAX_SLICE_SOURCE_BYTES);
    }

    /// A worker limit must not bypass the object-size safety policy.
    #[test]
    fn one_worker_still_slices_large_inputs() {
        assert!(!needs_slices(1, MAX_SLICE_SOURCE_BYTES));
        assert!(needs_slices(1, MAX_SLICE_SOURCE_BYTES + 1));
        assert_eq!(slice_parts(1, MAX_SLICE_SOURCE_BYTES + 1), 2);
        assert!(needs_slices(2, 1));
    }

    /// More slices than workers must neither oversubscribe nor reorder the link inputs.
    #[test]
    fn slice_workers_respect_the_limit_and_preserve_order() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let first_workers = std::sync::Barrier::new(2);
        let outcomes = run_slice_workers(17, 2, |index| {
            let now = active.fetch_add(1, Ordering::Relaxed) + 1;
            peak.fetch_max(now, Ordering::Relaxed);
            if index < 2 {
                first_workers.wait();
            }
            std::thread::sleep(std::time::Duration::from_millis(4));
            active.fetch_sub(1, Ordering::Relaxed);
            index * 3
        });
        let values: Vec<_> = outcomes.into_iter().map(Result::unwrap).collect();
        assert_eq!(values, (0..17).map(|index| index * 3).collect::<Vec<_>>());
        assert_eq!(peak.load(Ordering::Relaxed), 2);
        assert_eq!(active.load(Ordering::Relaxed), 0);
    }

    /// Sequential work processes every slice, including fallible and panicking callbacks.
    #[test]
    fn slice_workers_keep_failures_at_their_original_indices() {
        let outcomes = run_slice_workers(7, 1, |index| -> Result<usize, &'static str> {
            match index {
                2 => panic!("slice worker panic fixture"),
                4 => Err("slice write fixture"),
                _ => Ok(index),
            }
        });
        assert_eq!(outcomes.len(), 7);
        for (index, outcome) in outcomes.into_iter().enumerate() {
            match index {
                2 => assert!(outcome.is_err()),
                4 => assert_eq!(outcome.unwrap(), Err("slice write fixture")),
                _ => assert_eq!(outcome.unwrap(), Ok(index)),
            }
        }
        assert!(run_slice_workers(0, 1, |_| 0usize).is_empty());
    }
}

#[cfg(test)]
mod probe {
    #[test]
    #[ignore]
    fn probe_assemble_existing_s() {
        let src = std::env::var("ELEPHC_PROBE_ASM").unwrap();
        let dir = std::env::var("ELEPHC_PROBE_DIR").unwrap();
        let s = std::path::Path::new(&dir).join("probe.s");
        let _ = std::fs::remove_file(&s);
        std::os::unix::fs::symlink(&src, &s).unwrap();
        let t = std::time::Instant::now();
        let r = super::assemble_parallel(
            crate::codegen::platform::Target::detect_host(),
            crate::codegen::Emit::Executable,
            false,
            &s,
            &std::path::Path::new(&dir).join("probe.o"),
        );
        eprintln!("total {:.2}s {:?}", t.elapsed().as_secs_f64(), r.note);
    }
}
