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
/// The compiler emits PHP functions under `_php_`/`_fn_` prefixes and its own
/// helpers under `__rt_`, so the name itself says which is which — and the
/// distinction is what lets a reader tell their own hot function from the
/// runtime doing work on its behalf.
fn kind_of(symbol: &str) -> Kind {
    let bare = symbol.strip_prefix('_').unwrap_or(symbol);
    if bare.starts_with("_rt_") || bare.starts_with("rt_") {
        Kind::Helper
    } else if bare.starts_with("php_") || bare.starts_with("fn_") || bare == "main" {
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

/// which of the four learns what to do about it.
#[cfg(target_os = "linux")]
pub(crate) fn image_for(pid: u32) -> Result<Image, ImageError> {
    use super::elf;
    use super::ptrace;

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
    Ok(Image { symbols, exe: exe.to_string_lossy().into_owned(), first_vaddr })
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

    fn symbols() -> Vec<FuncSymbol> {
        // Sorted by address, which is what `symbolize` binary-searches, and
        // spelled the way the compiler actually emits them: `fn_`-prefixed with
        // `_u_` standing in for an underscore. Reading an attached profile of a
        // real program is what these names have to survive.
        vec![
            FuncSymbol { value: 0x1000, size: 0x100, name: "_fn_spin".into() },
            FuncSymbol { value: 0x2000, size: 0x100, name: "_fn_hot_u_leaf".into() },
            FuncSymbol { value: 0x3000, size: 0x100, name: "__rt_mixed_add".into() },
            FuncSymbol { value: 0x4000, size: 0x100, name: "main".into() },
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
