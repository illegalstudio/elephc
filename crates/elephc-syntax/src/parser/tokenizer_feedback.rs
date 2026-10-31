//! Purpose:
//! Records every name token that the PHP parser accepted in an identifier position.
//!
//! Called from:
//! - `crate::parser::parse_with_tokenizer_feedback` when TOKEN_PARSE asks for Zend's
//!   semi-reserved-word feedback.
//!
//! Key details:
//! - Collection is thread-local and enabled only around one validation parse.
//! - The caller verifies the recorded spelling at its mapped source offset before rewriting.
//! - Spans are mapped back to tokenizer byte offsets by the caller.

use std::cell::{Cell, RefCell};

use crate::lexer::TokenMetadata;
use crate::span::Span;

thread_local! {
    /// Whether the current parse should collect identifier feedback.
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    /// Name tokens accepted during the active parse, with their source spelling.
    static NAMES: RefCell<Vec<(Span, String)>> = const { RefCell::new(Vec::new()) };
}

/// Scoped feedback collection that restores the previous activation state on drop.
///
/// The guard makes a panicking parse leave no thread-local collection enabled, so a later
/// ordinary parse cannot record identifiers into a stale buffer.
pub struct FeedbackGuard {
    previous: bool,
}

impl FeedbackGuard {
    /// Enables collection for one parse, clearing names recorded by an earlier attempt.
    #[allow(dead_code)]
    pub fn start() -> Self {
        let previous = ACTIVE.with(|active| active.replace(true));
        NAMES.with(|names| names.borrow_mut().clear());
        Self { previous }
    }

    /// Returns the names accepted so far, in source order.
    #[allow(dead_code)]
    pub fn names(&self) -> Vec<(Span, String)> {
        NAMES.with(|names| names.borrow().clone())
    }
}

impl Drop for FeedbackGuard {
    /// Restores the activation state that was active before `start`.
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(self.previous));
    }
}

/// Records one token when it is a keyword the parser accepted as a name.
pub fn record(metadata: &TokenMetadata, spelling: &str) {
    ACTIVE.with(|active| {
        if active.get() {
            NAMES.with(|names| names.borrow_mut().push((metadata.span, spelling.to_string())));
        }
    });
}
