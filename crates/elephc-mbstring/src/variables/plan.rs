//! Purpose:
//! Resolves mb_convert_variables encodings and request policy before live host callbacks.
//!
//! Called from:
//! - Native and eval invocation adapters before traversing caller storage.
//!
//! Key details:
//! - Destination validation precedes source-list validation and any live mutation.
//! - Request state is not borrowed while a host callback can execute PHP.
//! - A single transfer encoding remains legal; multiple candidates retain only detectable codecs.

use crate::encoding::{Encoding, EncodingList, Substitute};
use crate::error::{MbError, MbResult};
use crate::state::State;

use super::{LiveFailure, LiveHost, convert_live};

/// Immutable policy captured before traversing any live PHP variable.
pub struct VariablePlan {
    to: Encoding,
    sources: Vec<Encoding>,
    strict: bool,
    order_significant: bool,
    substitute: Substitute,
    deprecation: Option<&'static str>,
}

impl VariablePlan {
    /// Validates the destination first, then resolves the caller's source list.
    pub fn prepare(
        state: &mut State,
        to: &[u8],
        from: EncodingList<'_>,
        order_significant: bool,
    ) -> MbResult<Self> {
        let resolved = state.resolve_encoding(
            Some(to), "mb_convert_variables", 1, "to_encoding",
        )?;
        let mut sources = state.parse_encodings(
            from, "mb_convert_variables", 2, "from_encoding",
        )?;
        if sources.len() > 1 {
            sources.retain(|encoding| encoding.supports_detection());
        }
        if sources.is_empty() {
            return Err(MbError::argument(
                "mb_convert_variables", 2, "from_encoding",
                "must specify at least one encoding",
            ));
        }
        Ok(Self {
            to: resolved.encoding,
            sources,
            strict: state.strict_detection(),
            order_significant,
            substitute: state.substitute(),
            deprecation: resolved.deprecation,
        })
    }

    /// Returns the destination's deprecation, if PHP emitted one during resolution.
    pub fn deprecation(&self) -> Option<&'static str> { self.deprecation }

    /// Converts roots without borrowing request state across callbacks.
    /// The caller records the returned illegal-character count after this call.
    pub fn convert<H: LiveHost>(
        &self,
        host: &mut H,
        roots: &[H::Handle],
    ) -> (Result<Encoding, LiveFailure<H::Error>>, u64) {
        convert_live(
            host, roots, self.to, &self.sources, self.strict,
            self.order_significant, self.substitute,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins destination-first errors and PHP's empty-source diagnostic.
    #[test]
    fn validates_destination_before_source_list() {
        let mut state = State::default();
        let invalid = EncodingList::Array(&[]);
        let error = VariablePlan::prepare(&mut state, b"bad-encoding", invalid, true)
            .err().expect("invalid destination");
        assert!(matches!(error, MbError::Value(message) if message.contains("Argument #1")));
        let error = VariablePlan::prepare(&mut state, b"UTF-8", invalid, true)
            .err().expect("empty source list");
        assert!(matches!(error, MbError::Value(message) if message.contains("Argument #2")));
    }
}
