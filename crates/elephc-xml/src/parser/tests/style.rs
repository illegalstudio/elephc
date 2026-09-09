//! Purpose:
//! The line-spelling flags of each PHP probe script: which event lines carry an
//! `@line:col/byte` position and whether probe1's `START`/`END`/`CDATA`/`DEFAULT`/`NSSTART`
//! words are used instead of `S`/`E`/`C`/`D`/`NS`.
//!
//! Called from:
//! - `crate::parser::tests::harness` (renders replayed events the way the scripts did);
//! - `crate::parser::tests::regen` (emits PHP handlers that print the same lines).
//!
//! Key details:
//! - Pure data with no libxml2 dependency, so the regenerator compiles without
//!   `cfg(elephc_xml_native)`; keeping one definition guarantees the harness and the
//!   generated scripts agree on every position suffix.

/// How the probe printed its lines.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Style {
    /// probe1's `dump_events` spelling (`START`/`END`/`CDATA`/`DEFAULT`/`NSSTART`).
    pub(crate) probe1: bool,
    /// Whether `NS` lines carry a position.
    pub(crate) ns_pos: bool,
    /// Whether S/E/C/D/PI/NS/NOTATION lines carry `@line:col/byte`.
    pub(crate) event_pos: bool,
    /// Whether `UNPARSED` lines carry a position.
    pub(crate) unparsed_pos: bool,
    /// Whether `EXTREF` lines carry a position.
    pub(crate) extref_pos: bool,
    /// Whether `PI` lines carry a position.
    pub(crate) pi_pos: bool,
}

impl Style {
    /// probe1's format.
    pub(crate) const PROBE1: Style = Style { probe1: true, ns_pos: false, event_pos: true, unparsed_pos: false, extref_pos: false, pi_pos: false };
    /// probe4/probe5's `run()` (no positions on events).
    pub(crate) const RUN_NOPOS: Style = Style { probe1: false, ns_pos: false, event_pos: false, unparsed_pos: false, extref_pos: false, pi_pos: false };
    /// probe6's `run()`.
    pub(crate) const RUN6: Style = Style { probe1: false, ns_pos: true, event_pos: true, unparsed_pos: false, extref_pos: true, pi_pos: true };
    /// probe8's `run()`.
    pub(crate) const RUN8: Style = Style { probe1: false, ns_pos: true, event_pos: true, unparsed_pos: true, extref_pos: false, pi_pos: true };
    /// probe9's `run()`.
    pub(crate) const RUN9: Style = Style { probe1: false, ns_pos: false, event_pos: true, unparsed_pos: false, extref_pos: false, pi_pos: false };
}
