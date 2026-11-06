//! Purpose:
//! Turns raw program counters read out of another process into the display
//! stacks the rest of `monitor` already renders, exports and diffs.
//!
//! Called from:
//! - `crate::monitor` on Linux, for `--attach`, which is handed a pid already
//!   running under someone else's control and has no channel to ask over.
//!
//! Key details:
//! - Naming only. The syscalls that produce the addresses live next to this and
//!   are the one part a host without `ptrace` cannot exercise; everything here
//!   takes numbers and returns names, and is tested on any host.
//! - The frame chain is walked innermost-first, which is the order a sampler
//!   collects it and the reverse of the order a profile displays it.

use super::elf::{symbolize, FuncSymbol};
use super::Kind;

/// The name given to a frame no symbol claims.
///
/// One bucket rather than an address, because an address is not a name: it says
/// nothing a reader can act on, and a hundred distinct ones would split a single
/// runtime helper into a hundred rows that each look insignificant.
pub(crate) const UNNAMED: &str = "<native>";

/// Turns one sampled frame chain into a display stack, outermost first.
///
/// The addresses arrive innermost-first — a sampler reads the interrupted `pc`
/// and then climbs — and every consumer downstream reads a stack the other way,
/// from the root inwards. Reversing here rather than at each of them is what
/// keeps `{main}` at the top of a table instead of the bottom.
///
/// Names are demangled on the way out, to the same spelling the in-process
/// paths print: an operator comparing an attached profile against an asked one
/// is looking at one program, and `_fn_hot_u_leaf` beside `hot_leaf` reads as
/// two different functions.
///
/// A run of consecutive unnamed frames collapses to one. A profile is read to
/// find the code that costs, and six rows of `<native>` between two PHP
/// functions tell a reader nothing while pushing what they came for off the
/// screen — which is the same reason the exact profiler folds inlined bodies
/// into their caller.
pub(crate) fn display_stack(
    frames: &[u64],
    symbols: &[FuncSymbol],
    bias: u64,
) -> Vec<(String, Kind)> {
    let mut out: Vec<(String, Kind)> = Vec::new();
    for address in frames.iter().rev() {
        match symbolize(symbols, bias, *address) {
            // Classified on the MANGLED name and displayed as the SOURCE one.
            // The prefix is what says which kind of time this is, and it is
            // exactly what demangling removes — so the order is not a
            // preference. Reading the other way round leaves every PHP function
            // classified as native, and a profile that says a PHP program spends
            // none of its time in PHP.
            Some(name) => out.push((super::render::demangle(name), kind_of(name))),
            None => {
                if out.last().map(|(name, _)| name.as_str()) != Some(UNNAMED) {
                    out.push((UNNAMED.to_string(), Kind::Native));
                }
            }
        }
    }
    out
}

/// What kind of time a named frame represents.
///
/// The compiler emits PHP functions under `_php_`/`_fn_` prefixes, methods
/// under `_method_`, and its own helpers under `__rt_`, so the name itself
/// says which is which — and the distinction is what lets a reader tell their
/// own hot function from the runtime doing work on its behalf. The prefixes
/// here must stay aligned with `is_php_symbol` and `demangle`: classifying a
/// method as native is how an OOP profile becomes a table of `<non-PHP>`.
fn kind_of(symbol: &str) -> Kind {
    let bare = symbol.strip_prefix('_').unwrap_or(symbol);
    if bare.starts_with("_rt_") || bare.starts_with("rt_") {
        Kind::Helper
    } else if bare.starts_with("php_") || super::is_php_symbol(symbol) {
        Kind::Php
    } else {
        Kind::Native
    }
}

/// Counts identical stacks, which is what a sample count IS.
///
/// A sampler produces one stack per interrupt and says nothing about time; the
/// weight of a stack is how many times it was seen. Folding here rather than
/// storing every sample keeps a long window bounded by the program's shapes
/// rather than by its duration.
pub(crate) fn fold(stacks: Vec<Vec<(String, Kind)>>) -> Vec<(Vec<(String, Kind)>, u64)> {
    let mut counted: std::collections::BTreeMap<Vec<(String, Kind)>, u64> =
        std::collections::BTreeMap::new();
    for stack in stacks {
        if stack.is_empty() {
            continue;
        }
        *counted.entry(stack).or_default() += 1;
    }
    counted.into_iter().collect()
}

/// What a running process's addresses have to be read against: the function
/// symbols of the image behind it, and where that image actually landed.
///
/// Built once per `--attach` rather than per window. The symbols do not change
/// while the program runs, and re-reading a binary's symbol table every two
/// seconds to answer the same question is work charged to a live view for
/// nothing.
pub(crate) struct Image {
    pub(crate) symbols: Vec<FuncSymbol>,
    /// The executable's path as the kernel reports it, and the first address its
    /// own headers ask to be loaded at.
    ///
    /// Kept because the symbols are shared by a whole process tree but the BIAS
    /// is not: a prefork server's workers run the same image, so they resolve
    /// against the same table, and each is mapped where its own `/proc` says.
    /// Deriving a worker's bias needs these two and not the file again.
    pub(crate) exe: String,
    pub(crate) first_vaddr: u64,
    /// Who this image was built for. A later window that finds a different
    /// starttime at the same pid is looking at a reused number, not the
    /// program these symbols describe.
    pub(crate) identity: super::process_id::ProcessIdentity,
    /// Tids this process is still tracing because the last window could not
    /// stop them. `PTRACE_DETACH` needs a stopped tracee; a D-state one
    /// leaves the relationship in place. The next window still tries to
    /// seize: success is a new thread (including a recycled tid). A failed
    /// seize is adopted only when `TracerPid` is this process.
    pub(crate) held: Vec<u32>,
}

/// Reads the image behind a running pid, or says which part was not readable.
///
/// Four things can be missing and they are not interchangeable — a stripped
/// binary, an unreadable `/proc`, an image that is not ELF, a mapping that is
/// not there. An operator who is told "cannot attach" learns nothing; one told
/// Why an image could not be built, and whether the kernel REFUSED it.
///
/// The distinction exists to keep one sentence off the other three. The
/// `yama/ptrace_scope` hint used to be appended to every failure here, so an
/// absent pid answered `cannot read /proc/999999/exe: No such file or directory
/// (the kernel's yama/ptrace_scope is 1, ...)` and a stripped binary ended the
/// same way — a hint naming a cause that had not occurred, which is the same
/// class of wrong answer as the refusal that used to read as a target that had
/// exited.
pub(crate) struct ImageError {
    /// What went wrong, in the operator's terms.
    pub(crate) reason: String,
    /// Whether the kernel refused the read, as opposed to there being nothing
    /// to read. Only this earns the hint.
    pub(crate) denied: bool,
}

impl ImageError {
    /// A failure that has nothing to do with permission: an absent pid, a
    /// binary that is not ELF64, a program with no symbols left.
    fn plain(reason: String) -> Self {
        Self { reason, denied: false }
    }

    /// A failed read, which the kernel may or may not have refused.
    fn from_read(what: String, error: &std::io::Error) -> Self {
        Self {
            reason: format!("{what}: {error}"),
            denied: error.kind() == std::io::ErrorKind::PermissionDenied,
        }
    }
}

/// Loads symbols only after identifying the target, then rechecks its identity.
#[cfg(target_os = "linux")]
pub(crate) fn image_for(pid: u32) -> Result<Image, ImageError> {
    image_for_checked(pid, super::process_id::identity_of_pid)
}

/// Loads the target image with a final identity read that tests can synchronize.
#[cfg(target_os = "linux")]
fn image_for_checked(
    pid: u32,
    check_identity: impl FnOnce(super::process_id::ProcessIdentity) -> super::process_id::Identity,
) -> Result<Image, ImageError> {
    use super::elf;
    use super::ptrace;

    let identity = super::process_id::identity_of(pid).ok_or_else(|| {
        ImageError::plain(format!("cannot identify the target from /proc/{pid}/stat"))
    })?;
    let exe = ptrace::executable_path(pid)
        .map_err(|error| ImageError::from_read(format!("cannot read /proc/{pid}/exe"), &error))?;
    let bytes = std::fs::read(&exe).map_err(|error| {
        ImageError::from_read(format!("cannot read the target's binary {}", exe.display()), &error)
    })?;
    let first_vaddr = elf::first_load_vaddr(&bytes)
        .ok_or_else(|| ImageError::plain(format!("{} is not an ELF64 image this can read", exe.display())))?;
    let maps = ptrace::memory_maps(pid)
        .map_err(|error| ImageError::from_read(format!("cannot read /proc/{pid}/maps"), &error))?;
    // Read and discarded: what it answers for the ROOT is not reused, because
    // every process of the tree derives its own. It is read anyway because
    // failing HERE is the difference between one clear sentence and a window
    // that silently comes back empty.
    elf::load_bias(&maps, &exe.to_string_lossy(), first_vaddr).ok_or_else(|| {
        ImageError::plain(format!(
            "{} is running but is not mapped in /proc/{pid}/maps",
            exe.display()
        ))
    })?;
    let symbols = elf::function_symbols(&bytes);
    if symbols.is_empty() {
        return Err(ImageError::plain(format!(
            "{} carries no function symbols, so an attached sampler has nothing to name its \
             addresses with. elephc strips them by default because nothing INSIDE a program \
             reads them; a sampler reads them from the outside. Rebuild it with \
             --keep-symbols (or --debug-info), or read it through its endpoint instead: \
             ELEPHC_PROBE_ADDR=127.0.0.1:9411, then `elephc monitor 127.0.0.1:9411`.",
            exe.display()
        )));
    }
    match check_identity(identity) {
        super::process_id::Identity::Same => {}
        super::process_id::Identity::Gone => {
            return Err(ImageError::plain(format!("pid {pid} disappeared while reading its image")));
        }
        super::process_id::Identity::Replaced => {
            return Err(ImageError::plain(format!(
                "pid {pid} changed while reading its image: the kernel reused the pid"
            )));
        }
    }
    Ok(Image {
        symbols,
        exe: exe.to_string_lossy().into_owned(),
        first_vaddr,
        identity,
        held: Vec::new(),
    })
}

/// Where one process of the tree is mapped, for a tree that shares this image.
///
/// A prefork server's workers are forks: same executable, same symbol table,
/// their own mappings. Reusing the root's bias for a worker would resolve every
/// one of its addresses against the wrong offset — which does not fail, it
/// names the wrong functions, and a table that is confidently wrong is worse
/// than one that is short.
#[cfg(target_os = "linux")]
pub(crate) fn bias_of(image: &Image, pid: u32) -> Option<u64> {
    let maps = super::ptrace::memory_maps(pid).ok()?;
    super::elf::load_bias(&maps, &image.exe, image.first_vaddr)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A target reaped after its image loads is reported as gone, without a reuse claim.
    #[cfg(target_os = "linux")]
    #[test]
    fn image_loading_reports_a_target_that_disappears_during_the_load() {
        let mut tracee = super::super::ptrace::process_tests::Tracee::start();
        let pid = tracee.child.id();
        let result = image_for_checked(pid, |identity| {
            tracee.child.kill().unwrap();
            tracee.child.wait().unwrap();
            super::super::process_id::identity_of_pid(identity)
        });
        let error = result.err().expect("the vanished target must be rejected");
        assert_eq!(error.reason, format!("pid {pid} disappeared while reading its image"));
        assert!(!error.denied);
    }

    /// A different starttime at the final check keeps an explicit pid-reuse diagnostic.
    #[cfg(target_os = "linux")]
    #[test]
    fn image_loading_distinguishes_a_replaced_target_from_a_vanished_one() {
        let pid = std::process::id();
        let result = image_for_checked(pid, |mut identity| {
            identity.starttime = identity.starttime.wrapping_add(1);
            super::super::process_id::identity_of_pid(identity)
        });
        let error = result.err().expect("the replaced target must be rejected");
        assert_eq!(error.reason, format!(
            "pid {pid} changed while reading its image: the kernel reused the pid"
        ));
        assert!(!error.denied);
    }

    /// Static storage stays native even when classification receives an untyped name.
    #[test]
    fn static_storage_symbols_are_not_php_methods() {
        for (owner, member) in [
            ("Engine", "count"), ("My_Class", "hot_count"), ("prop", "local"), ("u", "u"),
        ] {
            for symbol in [
                elephc::names::static_property_symbol(owner, member),
                elephc::names::static_local_symbol(owner, member),
                elephc::names::static_local_init_symbol(owner, member),
            ] {
                assert!(!super::super::is_php_symbol(&symbol), "{symbol}");
                assert_eq!(kind_of(&symbol), Kind::Native, "{symbol}");
                assert_eq!(super::super::render::demangle(&symbol), symbol);
            }
        }
    }

    /// Classes named prop or local and escaped names remain valid static methods.
    #[test]
    fn static_method_classification_accepts_compiler_symbol_shapes() {
        for (class, method) in [
            ("Engine", "step"), ("prop", "count"), ("local", "count"),
            ("prop", "hot_count"), ("local", "hot_count"), ("My_Class", "_run_"),
            ("App\\Éngine", "run"), ("local", "init"), ("local", "init_u_a"), ("_", "_"),
        ] {
            let symbol = elephc::names::static_method_symbol(class, method);
            assert!(super::super::is_php_symbol(&symbol), "{symbol}");
            assert_eq!(kind_of(&symbol), Kind::Php, "{symbol}");
            assert_eq!(
                super::super::render::demangle(&symbol),
                format!("{class}::{method}"),
                "{symbol}"
            );
        }
    }

    /// Supplies sorted PHP function, helper and method symbols for stack fixtures.
    fn symbols() -> Vec<FuncSymbol> {
        // Sorted by address, which is what `symbolize` binary-searches, and
        // spelled the way the compiler actually emits them, by the compiler's own
        // symbol builders: an underscore in a name selects the escaped
        // `_method___My_u_Class___run` join. Reading an attached profile of a real
        // program is what these names have to survive.
        vec![
            FuncSymbol { value: 0x1000, size: 0x100, name: elephc::names::function_symbol("spin") },
            FuncSymbol { value: 0x2000, size: 0x100, name: elephc::names::function_symbol("hot_leaf") },
            FuncSymbol { value: 0x3000, size: 0x100, name: "__rt_mixed_add".into() },
            FuncSymbol { value: 0x4000, size: 0x100, name: "main".into() },
            FuncSymbol {
                value: 0x5000,
                size: 0x100,
                name: elephc::names::method_symbol("Engine", "step"),
            },
            FuncSymbol {
                value: 0x6000,
                size: 0x100,
                name: elephc::names::method_symbol("My_Class", "run"),
            },
        ]
    }

    /// The stack comes back outermost-first, whichever way the sampler collected
    /// it. Every consumer downstream reads a stack from the root inwards, and
    /// reversing at each of them instead is how `{main}` ends up at the bottom
    /// of a table it should head.
    #[test]
    fn a_chain_is_turned_around_so_the_root_comes_first() {
        let bias = 0xaaaa_0000_0000;
        // Innermost first, as a sampler reads it: spin called by hot_leaf
        // called by main.
        let frames = [bias + 0x1010, bias + 0x2010, bias + 0x4010];
        assert_eq!(
            display_stack(&frames, &symbols(), bias),
            vec![
                ("{main}".to_string(), Kind::Php),
                ("hot_leaf".to_string(), Kind::Php),
                ("spin".to_string(), Kind::Php),
            ]
        );
    }

    /// A runtime helper is named as one, so a reader can tell their own hot
    /// function from the runtime working on its behalf.
    #[test]
    fn a_helper_is_not_reported_as_php() {
        let bias = 0;
        assert_eq!(
            display_stack(&[0x3010], &symbols(), bias),
            vec![("__rt_mixed_add".to_string(), Kind::Helper)]
        );
    }

    /// Frames no symbol claims collapse into one row rather than one each.
    ///
    /// A profile is read to find the code that costs. Six rows of `<native>`
    /// between two PHP functions tell a reader nothing and push what they came
    /// for off the screen.
    #[test]
    fn a_run_of_unnamed_frames_is_one_row() {
        let bias = 0;
        // spin, then three addresses in nobody's range, then main.
        let frames = [0x1010, 0x9000, 0x9100, 0x9200, 0x4010];
        assert_eq!(
            display_stack(&frames, &symbols(), bias),
            vec![
                ("{main}".to_string(), Kind::Php),
                (UNNAMED.to_string(), Kind::Native),
                ("spin".to_string(), Kind::Php),
            ]
        );
    }

    /// Two unnamed runs SEPARATED by a named frame stay two rows: collapsing
    /// those would join stretches of the program that are not adjacent.
    #[test]
    fn unnamed_runs_on_either_side_of_a_name_stay_apart() {
        let bias = 0;
        let frames = [0x9000, 0x2010, 0x9100, 0x4010];
        assert_eq!(
            display_stack(&frames, &symbols(), bias),
            vec![
                ("{main}".to_string(), Kind::Php),
                (UNNAMED.to_string(), Kind::Native),
                ("hot_leaf".to_string(), Kind::Php),
                (UNNAMED.to_string(), Kind::Native),
            ]
        );
    }

    /// A frame is CLASSIFIED on its mangled name and DISPLAYED as its source
    /// one, and both halves have to hold at once.
    ///
    /// The prefix that says which kind of time a frame is — `fn_`, `__rt_` — is
    /// exactly what demangling removes, so doing them in the wrong order is not
    /// a matter of taste: classify after demangling and every PHP function comes
    /// out native, leaving a profile that says a PHP program spends none of its
    /// time in PHP.
    ///
    /// The spelling matters on its own too. An operator comparing an attached
    /// profile against an asked one is looking at one program, and
    /// `_fn_hot_u_leaf` beside `hot_leaf` reads as two different functions.
    #[test]
    fn a_frame_is_classified_mangled_and_displayed_demangled() {
        let named = display_stack(&[0x2010, 0x3010, 0x4010], &symbols(), 0);
        assert_eq!(
            named,
            vec![
                ("{main}".to_string(), Kind::Php),
                // A helper's name is not a PHP name, so demangling leaves it be.
                ("__rt_mixed_add".to_string(), Kind::Helper),
                ("hot_leaf".to_string(), Kind::Php),
            ]
        );
        assert!(
            !named.iter().any(|(name, _)| name.contains("_u_") || name.starts_with("_fn_")),
            "a mangled spelling reached the display: {named:?}"
        );
    }

    /// Compiler methods are `_method_<class>_<method>`. `is_php_symbol` and
    /// `demangle` already know that prefix; classification used to miss it, so
    /// an OOP hot path became `<non-PHP>` after `table_stats` dropped every
    /// frame that was not `Kind::Php`.
    #[test]
    fn a_method_frame_is_php_and_displayed_as_source() {
        let named = display_stack(&[0x5010, 0x6010, 0x4010], &symbols(), 0);
        assert_eq!(
            named,
            vec![
                ("{main}".to_string(), Kind::Php),
                ("My_Class::run".to_string(), Kind::Php),
                ("Engine::step".to_string(), Kind::Php),
            ]
        );
        let folded = super::super::php_folded_stacks(&[(named, 7)]);
        assert_eq!(
            folded,
            vec![(
                vec![
                    "{main}".to_string(),
                    "My_Class::run".to_string(),
                    "Engine::step".to_string(),
                ],
                7
            )]
        );
    }

    /// Static-only hot paths remain PHP through naming, display and folded exports.
    #[test]
    fn static_method_frames_are_php_and_exported_as_source() {
        let symbols = vec![FuncSymbol {
            value: 0x1000,
            size: 0x100,
            name: elephc::names::static_method_symbol("My_Class", "run_hot"),
        }];
        assert!(super::super::is_php_symbol(&symbols[0].name));
        let named = display_stack(&[0x1010], &symbols, 0);
        assert_eq!(named, vec![("My_Class::run_hot".to_string(), Kind::Php)]);
        assert_eq!(
            super::super::php_folded_stacks(&[(named, 7)]),
            vec![(vec!["My_Class::run_hot".to_string()], 7)]
        );
    }

    /// The weight of a stack is how many times it was seen, which is the only
    /// thing a sampler measures.
    #[test]
    fn identical_stacks_are_counted_rather_than_kept() {
        let one = vec![("main".to_string(), Kind::Php)];
        let two = vec![("main".to_string(), Kind::Php), ("_php_spin".to_string(), Kind::Php)];
        let folded = fold(vec![one.clone(), two.clone(), one.clone(), vec![]]);
        assert_eq!(folded.len(), 2, "an empty sample is not a shape: {folded:?}");
        assert!(folded.contains(&(one, 2)));
        assert!(folded.contains(&(two, 1)));
    }
}
