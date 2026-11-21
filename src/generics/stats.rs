//! Purpose:
//! Internal diagnostics for monomorphization: how many generic templates a program declares,
//! how many instantiations they produced, and where the time of a round went.
//!
//! Called from:
//! - `crate::generics::monomorphize_recovering` (one report per monomorphization)
//! - `crate::generics::classes` (instantiation counts)
//!
//! Key details:
//! - Enabled by the environment variable `ELEPHC_GENERICS_STATS`; it is a diagnostic for
//!   regression tests and cost hunts, not a user option, and changes nothing when set.
//! - A template is a "doc" template when every one of its type parameters came from PHPDoc
//!   (`TypeParam::from_docblock`): php never reads those, so such a template only exists
//!   because elephc adopted the annotation.
//! - Counters are process-wide and cumulative, because the autoload rounds monomorphize the
//!   program again from scratch; `erasure_reruns` is the docblock-erasure round of this process
//!   (a re-run is a new process, so it does not accumulate here).
//! - The line is `[elephc-generics] templates= doc_templates= instances= doc_class_instances=
//!   rounds= checks= erasure_reruns= erasure_rerun_skipped= check_ms= instantiate_ms=`, and each instantiation round
//!   prints a `[elephc-generics] round N check_ms= instantiate_ms=` line.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static TEMPLATES: AtomicU64 = AtomicU64::new(0);
static DOC_TEMPLATES: AtomicU64 = AtomicU64::new(0);
static INSTANCES: AtomicU64 = AtomicU64::new(0);
static DOC_INSTANCES: AtomicU64 = AtomicU64::new(0);
static ROUNDS: AtomicU64 = AtomicU64::new(0);
static CHECKS: AtomicU64 = AtomicU64::new(0);
static ERASURE_SKIPPED: AtomicU64 = AtomicU64::new(0);
static CHECK_US: AtomicU64 = AtomicU64::new(0);
static INSTANTIATE_US: AtomicU64 = AtomicU64::new(0);

/// Records a refusal no adopted annotation can have caused, so no erasure re-run was started.
pub(crate) fn record_erasure_skipped() {
    ERASURE_SKIPPED.fetch_add(1, Ordering::Relaxed);
}

/// Whether the diagnostic is switched on.
pub(crate) fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("ELEPHC_GENERICS_STATS").is_some())
}

/// Records the class templates one monomorphization collected (the last collection wins:
/// every autoload round collects the whole program again).
pub(crate) fn record_templates(total: u64, doc: u64) {
    TEMPLATES.store(total, Ordering::Relaxed);
    DOC_TEMPLATES.store(doc, Ordering::Relaxed);
}

/// Records one class instantiation, and whether its template exists only through PHPDoc.
pub(crate) fn record_instance(doc: bool) {
    INSTANCES.fetch_add(1, Ordering::Relaxed);
    if doc {
        DOC_INSTANCES.fetch_add(1, Ordering::Relaxed);
    }
}

/// A stopwatch for one phase of a round.
pub(crate) struct Phase(Instant);

impl Phase {
    pub(crate) fn start() -> Self {
        Phase(Instant::now())
    }

    fn micros(&self) -> u64 {
        self.0.elapsed().as_micros() as u64
    }

    /// Ends a whole-program check, returning its duration in microseconds.
    pub(crate) fn finish_check(self) -> u64 {
        let micros = self.micros();
        CHECKS.fetch_add(1, Ordering::Relaxed);
        CHECK_US.fetch_add(micros, Ordering::Relaxed);
        micros
    }

    /// Ends the instantiation and splicing that follows a check.
    pub(crate) fn finish_instantiate(self) -> u64 {
        let micros = self.micros();
        INSTANTIATE_US.fetch_add(micros, Ordering::Relaxed);
        micros
    }
}

/// Counts one instantiation round and prints its timing.
pub(crate) fn round(check_us: u64, instantiate_us: u64) {
    let round = ROUNDS.fetch_add(1, Ordering::Relaxed) + 1;
    if enabled() {
        eprintln!(
            "[elephc-generics] round {round} check_ms={} instantiate_ms={}",
            check_us / 1000,
            instantiate_us / 1000
        );
    }
}

/// Prints the cumulative counters.
pub(crate) fn report() {
    if !enabled() {
        return;
    }
    let reruns = std::env::var("ELEPHC_DOCBLOCK_ERASURE_ROUND")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    eprintln!(
        "[elephc-generics] templates={} doc_templates={} instances={} doc_class_instances={} rounds={} checks={} erasure_reruns={} erasure_rerun_skipped={} check_ms={} instantiate_ms={}",
        TEMPLATES.load(Ordering::Relaxed),
        DOC_TEMPLATES.load(Ordering::Relaxed),
        INSTANCES.load(Ordering::Relaxed),
        DOC_INSTANCES.load(Ordering::Relaxed),
        ROUNDS.load(Ordering::Relaxed),
        CHECKS.load(Ordering::Relaxed),
        reruns,
        ERASURE_SKIPPED.load(Ordering::Relaxed),
        CHECK_US.load(Ordering::Relaxed) / 1000,
        INSTANTIATE_US.load(Ordering::Relaxed) / 1000,
    );
}
