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

/// Parses short and deeply nested type syntax from a small caller stack.
#[test]
fn standalone_type_parsing_fits_small_caller_stacks() {
    use elephc_generics::Type;

    std::thread::Builder::new().stack_size(192 * 1024).spawn(|| {
        for depth in [24, 96] {
            let source = format!("{}int{}", "Box<".repeat(depth), ">".repeat(depth));
            let mut ty = crate::parser::generics::parse_type_string(&source).expect("nested generic type");
            for _ in 0..depth {
                let Type::Apply(name, mut args) = ty else { panic!("expected a nested application") };
                assert_eq!(name, "Box");
                assert_eq!(args.len(), 1);
                ty = args.pop().expect("one generic argument");
            }
            assert_eq!(ty, Type::atom("int"));

            let declaration = format!("<T: {source}>");
            let tokens = crate::lexer::tokenize(&declaration).expect("generic parameter tokens");
            let names = crate::lower::type_parameter_names(tokens).expect("nested parameter bound");
            assert_eq!(names, ["T"]);
        }
    }).expect("small parser caller must start").join().expect("parser caller must finish");
}
