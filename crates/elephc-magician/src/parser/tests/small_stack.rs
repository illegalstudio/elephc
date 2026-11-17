//! Purpose:
//! Verifies cold eval parsing has enough stack when called from a small native activation.
//!
//! Called from:
//! - Magician parser regressions in CI.
//!
//! Key details:
//! - Both byte-fragment and included-token entry points run on an explicitly small caller stack.
//! - The branching constructor/callback shape previously exhausted a 256 KiB native Fiber stack.

/// Byte and token parsing use their own stack before returning the owned program to the caller.
#[test]
fn cold_fragment_and_token_parsing_fit_small_caller_stacks() {
    std::thread::Builder::new().stack_size(192 * 1024).spawn(|| {
        let source = br#"
            echo 'eval|';
            if ($mode === 0) { $consumer = new NativePausedConsumer('callback', true); }
            elseif ($mode === 1) {
                $consumer = new NativePausedConsumer('callback', false);
                $consumer->apply('callback');
            } else { NativePausedConsumer::applyStatic('callback'); }
        "#;
        let fragment = crate::parser::parse_fragment(source).expect("cold fragment must parse");
        assert_eq!(fragment.statements().len(), 2);
        let tokens = crate::lexer::tokenize(std::str::from_utf8(source).unwrap()).unwrap();
        let included = crate::parser::parse_tokens(tokens, source.len()).expect("included tokens must parse");
        assert_eq!(fragment, included);
    }).expect("small parser caller must start").join().expect("parser caller must finish");
}
