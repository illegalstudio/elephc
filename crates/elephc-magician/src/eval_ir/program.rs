//! Purpose:
//! Defines parsed EvalIR programs and source-location metadata.
//!
//! Called from:
//! - Parser entry points, declaration metadata, Reflection, and interpreter execution.
//!
//! Key details:
//! - Source offsets and file/line ranges remain syntax metadata, not runtime cells.
//! - The CLI superglobals a fragment names are recorded at parse time, the moment PHP's
//!   `auto_globals_jit` arms an auto-global for the code that mentions it.

use super::*;

/// The superglobals PHP's CLI SAPI has populated when a script starts, in the order the
/// compiler's `superglobals::CLI_POPULATED_SUPERGLOBALS` lists them. `$_SESSION` is absent on
/// purpose: it does not exist until `session_start()`.
pub const EVAL_CLI_POPULATED_SUPERGLOBALS: [&str; 7] =
    ["_SERVER", "_GET", "_POST", "_COOKIE", "_FILES", "_ENV", "_REQUEST"];

/// Selects every caller-scope name when a fragment can inspect or mutate names dynamically.
pub const EVAL_SCOPE_ACCESS_ALL: u64 = u64::MAX;

/// Maps one case-sensitive PHP variable name onto its conservative access-mask bit.
pub fn eval_scope_name_mask(name: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    1_u64 << (hash & 63)
}

/// Parsed eval fragment lowered into dynamic by-name statements.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct EvalProgram {
    native_generic_syntax: bool,
    source_len: usize,
    statements: Vec<EvalStmt>,
    scope_access_mask: u64,
    #[serde(with = "cli_superglobal_names")]
    cli_superglobals: Vec<&'static str>,
}

/// Carries `cli_superglobals` through the `opcache.file_cache` store, which needs
/// `Deserialize`: a `&'static str` cannot be read back from bytes, so the names are written
/// as strings and mapped back onto [`EVAL_CLI_POPULATED_SUPERGLOBALS`] on read. A name
/// outside that list is a decode error, so a foreign entry is refused rather than replayed
/// without the superglobals its code expects.
mod cli_superglobal_names {
    /// Writes the names as a plain string sequence.
    pub fn serialize<S: serde::Serializer>(
        names: &Vec<&'static str>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(names, serializer)
    }

    /// Reads the names back as the static entries of the canonical list.
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<&'static str>, D::Error> {
        let names: Vec<String> = serde::Deserialize::deserialize(deserializer)?;
        names
            .iter()
            .map(|name| {
                super::EVAL_CLI_POPULATED_SUPERGLOBALS
                    .iter()
                    .copied()
                    .find(|known| known == name)
                    .ok_or_else(|| serde::de::Error::custom("unknown CLI superglobal"))
            })
            .collect()
    }
}

impl EvalProgram {
    /// Creates an EvalIR program for a source fragment and statement list.
    pub fn new(source_len: usize, statements: Vec<EvalStmt>) -> Self {
        Self {
            native_generic_syntax: false,
            source_len,
            statements,
            scope_access_mask: EVAL_SCOPE_ACCESS_ALL,
            cli_superglobals: Vec::new(),
        }
    }

    /// Records the conservative caller-scope names used by this parsed fragment.
    pub(crate) fn with_scope_access_mask(mut self, mask: u64) -> Self {
        self.scope_access_mask = mask;
        self
    }

    /// Returns the conservative caller-scope access mask for dynamic eval synchronization.
    pub const fn scope_access_mask(&self) -> u64 {
        self.scope_access_mask
    }

    /// Records whether source used native generic syntax forbidden in strict PHP mode.
    pub(crate) fn with_native_generic_syntax(mut self, enabled: bool) -> Self { self.native_generic_syntax = enabled; self }

    /// Returns whether source used native generic syntax.
    pub(crate) fn native_generic_syntax(&self) -> bool { self.native_generic_syntax }

    /// Records the CLI-populated superglobals (without `$`) this fragment's code names.
    pub fn with_cli_superglobals(mut self, names: Vec<&'static str>) -> Self {
        self.cli_superglobals = names;
        self
    }

    /// Returns the CLI-populated superglobals this fragment's code names, in canonical order.
    pub fn cli_superglobals(&self) -> &[&'static str] {
        &self.cli_superglobals
    }

    /// Returns the byte length of the parsed eval fragment.
    pub const fn source_len(&self) -> usize {
        self.source_len
    }

    /// Returns the ordered EvalIR statements in source order.
    pub fn statements(&self) -> &[EvalStmt] {
        &self.statements
    }

    /// Consumes the program and returns its statement list.
    pub fn into_statements(self) -> Vec<EvalStmt> {
        self.statements
    }
}

/// One source range inside the current eval fragment.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalSourceLocation {
    start_line: i64,
    end_line: i64,
}

impl EvalSourceLocation {
    /// Creates a source range using one-based eval-fragment line numbers.
    pub const fn new(start_line: i64, end_line: i64) -> Self {
        Self {
            start_line,
            end_line,
        }
    }

    /// Creates a single-line source range.
    pub const fn single_line(line: i64) -> Self {
        Self::new(line, line)
    }

    /// Returns the one-based line where the declaration starts.
    pub const fn start_line(&self) -> i64 {
        self.start_line
    }

    /// Returns the one-based line where the declaration ends.
    pub const fn end_line(&self) -> i64 {
        self.end_line
    }
}
