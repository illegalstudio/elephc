//! Purpose:
//! Differential and regression tests for the TOKEN_PARSE validation views.
//!
//! Called from:
//! - `cargo test` through `crate::syntax::tokenizer_validation::tests`.
//!
//! Key details:
//! - Fixtures compare the token view with the text parser view and PHP's diagnostics.
//! - Tests live beside the validation module so they can use its private view builders.

    use super::*;

    /// Scans one source into the reduced token view used by validation.
    fn scan_tokens(source: &[u8]) -> Vec<TokenView> {
        crate::scanner::scan(source)
            .expect("scan")
            .tokens
            .into_iter()
            .map(|token| TokenView {
                id: token.id,
                pos: token.pos,
                len: token.len,
                line: token.line,
            })
            .collect()
    }

    /// Verifies a goto statement is projected directly instead of forcing the text fallback.
    #[test]
    fn token_view_supports_goto_statements() {
        let source = b"<?php goto done;";
        let tokens = scan_tokens(source);
        let view = token_view::build(source, &tokens, &OnceCell::new()).expect("goto is projected");
        assert!(view
            .tokens
            .iter()
            .any(|(token, _)| *token == crate::syntax::lexer::Token::Goto));
        let old = crate::syntax::parser::with_tokenizer_validation(|| {
            validate_in_scope_with(source, &tokens, false)
        });
        let new = crate::syntax::parser::with_tokenizer_validation(|| {
            validate_in_scope_with(source, &tokens, true)
        });
        assert_eq!(old, new);
    }

    /// Verifies both validation paths produce the same diagnostics and identifier feedback.
    #[test]
    fn token_view_feedback_matches_text_view() {
        let sources: &[&[u8]] = &[
            b"<?php $a = 1; $b = $a + 2;",
            b"<?php class C { public int $x = 0 { get => $this->x; set => $value; } }",
            b"<?php $a = \"hello $name\";",
            b"<?php $a = &\\Foo\\Bar[0];",
            b"<?php echo ( int )$x;",
            b"<?php echo \"${a[0}\"; (integer)$x;",
            b"<?php function f() { (integer)$x;",
            b"<?php $a = &$b + 1;",
        ];
        for source in sources {
            let tokens = scan_tokens(source);
            let old = crate::syntax::parser::with_tokenizer_validation(|| {
                validate_in_scope_with(source, &tokens, false)
            });
            let new = crate::syntax::parser::with_tokenizer_validation(|| {
                validate_in_scope_with(source, &tokens, true)
            });
            assert_eq!(
                old,
                new,
                "feedback differs for {}",
                String::from_utf8_lossy(source)
            );
        }
    }

    /// Verifies the source-spanned token view matches the text parser view for supported code.
    #[test]
    fn token_view_matches_text_view_on_code_sources() {
        let sources: &[&[u8]] = &[
            b"<?php $a = 1; $b = $a + 2;",
            b"<?php class C { public int $x = 0; }",
            b"<?php #[Attr] class C { public int $x = 0 { get => $this->x; set => $value; } }",
            b"<?php echo 1; ?>html",
            b"<?= 1 ?>",
            b"<?php // comment\n$a = 1;",
            b"<?php /** doc */ $a = 1;",
            b"<?php\n$a = 1;\n",
            b"<?php new \\Foo\\Bar();",
            b"<?php Foo\\Bar::baz();",
            b"<?php namespace\\Foo;",
            b"<?php $a = 'hello';",
            b"<?php $a = \"hello\";",
            b"<?php $a = \"hello $name\";",
            b"<?php $a = <<<TXT\nhello\nTXT;",
            b"<?php $a = `echo hi`;",
            b"<?php $a = 'it\\'s';",
            b"<?php $a = true; $b = false; $c = null;",
            b"<?php $a = 1.5; $b = 1e3;",
            b"<?php $a = \"h\xc3\xa9llo\";",
            b"<?php $a = (int) $b; $c = (string) $d; $e = (array) $f;",
            b"<?php $a = (integer) $b; $c = (boolean) $d; $e = (double) $f;",
            b"<?php $a = (binary) $b; $c = (unset) $d;",
            b"<?php (void) $a;",
            b"<?php echo ( int )$x;",
            b"<?php $a = ( integer )$b; $c = ( string )$d;",
            b"<?php ( void )$a;",
            b"<?php echo C::{foo()}::BAR;",
            b"<?php $a::{ $b + 1 };",
            b"<?php function f(&$x) {}",
            b"<?php function &f() {}",
            b"<?php foreach ($a as &$v) {}",
            b"<?php $a = &$b;",
            b"<?php $a = [&$b];",
            b"<?php $a = array(&$b);",
            b"<?php $f = function () use (&$x) {};",
            b"<?php $f = fn&($x) => $x;",
            b"<?php foreach ($a as $k => &$v) {}",
            b"<?php $a = &$b[0];",
            b"<?php $a = &foo();",
            b"<?php $a = &$o->p;",
            b"<?php $a = &C::$x;",
            b"<?php $a = &static::$x;",
            b"<?php $a = &self::$x;",
            b"<?php $a = &foo()[0];",
            b"<?php $a = [0 => &$x[1]]; array(\"k\" => &$y[0]);",
            b"<?php [$p, &$q] = $arr; foreach ([1] as [$p, &$q]) {}",
            b"<?php foreach ($xs as &$a[0]) {}",
            b"<?php list(\"k\" => &$v) = $arr;",
            b"<?php $a = &$b + 1;",
            b"<?php $a = &$b[0] + 1;",
            b"<?php $a = &$b ? 1 : 2;",
            b"<?php $a = &$b && $c;",
            b"<?php $a = [&C::match()]; $b = [&$o->list];",
            b"<?php function f(...$xs) {}",
            b"<?php foo(...$xs);",
            b"<?php $a = [...$xs];",
        ];
        for source in [
            b"<?php done: echo 1;".as_slice(),
            b"<?php if (1) done: echo 1;",
            b"<?php\r\n$a = 1;\r\n",
            b"<?php $a = (void) $b;",
            b"<?php $a = &1;",
            b"<?php $a = [&$b + 1];",
            b"<?php foo(&$x);",
            b"<?php function f($a = &$b) {}",
            b"<?php $a = &$b = 1;",
            b"<?php $a = &$b->c = 1;",
            b"<?php $a = &foo() = 1;",
            b"<?php $a = &($b + 1);",
            b"<?php $a = &readonly();",
            b"<?php $a[&$b];",
            b"<?php $f = fn() => &$x;",
            b"<?php match(1) { 1 => &$x }",
            b"<?php strlen(...);",
            b"<?php C::class(...);",
            b"<?php C::$method(...);",
            b"<?php $c::class(...);",
            b"<?php $object->method(...);",
        ] {
            let tokens = scan_tokens(source);
            assert!(
                token_view::build(source, &tokens, &OnceCell::new()).is_none(),
                "label source must fall back: {}",
                String::from_utf8_lossy(source)
            );
        }
        let mut late_fallback = String::from("<?php ");
        for index in 0..40 {
            late_fallback.push_str(&format!("$x{index} = 1; "));
        }
        late_fallback.push_str("__halt_compiler();");
        let late_tokens = scan_tokens(late_fallback.as_bytes());
        assert!(
            token_view::build(late_fallback.as_bytes(), &late_tokens, &OnceCell::new()).is_none(),
            "unsupported token beyond the old 32-token sample must fall back"
        );
        for source in sources {
            let tokens = scan_tokens(source);
            let (view, map, _) =
                parser_view(source, &tokens, source.len(), &OnceCell::new());
            let old = crate::syntax::lexer::tokenize(&view).expect("old view");
            let old_lines = line_starts(&view);
            let positions: Vec<(u32, u32)> = old
                .iter()
                .map(|(_, metadata)| (metadata.span.line, metadata.span.col))
                .collect();
            let offsets = view_offsets(&view, &old_lines, &positions);
            let old_offsets: Vec<usize> = offsets
                .iter()
                .map(|position| {
                    map.get(*position)
                        .copied()
                        .unwrap_or(source.len() as u32) as usize
                })
                .collect();
            let new = token_view::build(source, &tokens, &OnceCell::new())
                .unwrap_or_else(|| panic!("code source supported: {}", String::from_utf8_lossy(source)));
            assert_eq!(
                old.len(),
                new.tokens.len(),
                "token count for {}",
                String::from_utf8_lossy(source)
            );
            let source_text = std::str::from_utf8(source).expect("valid fixture");
            let new_lines = line_starts(source_text);
            for (index, (pairs, old_offset)) in old
                .iter()
                .zip(new.tokens.iter())
                .zip(old_offsets.iter())
                .enumerate()
            {
                let ((old_token, _), (new_token, new_metadata)) = pairs;
                let same_kind = match (old_token, new_token) {
                    (crate::syntax::lexer::Token::Identifier(_), crate::syntax::lexer::Token::Identifier(_)) => true,
                    _ => old_token == new_token,
                };
                assert!(
                    same_kind,
                    "kind at {index} for {}",
                    String::from_utf8_lossy(source)
                );
                let new_offset = view_offset(
                    source_text,
                    &new_lines,
                    new_metadata.span.line,
                    new_metadata.span.col,
                );
                // The text view maps every byte of a multi-byte native token to its start;
                // the token view keeps the exact segment offset. Accept either the old
                // start mapping or an offset inside the native token that starts there.
                let same_native_token = tokens.iter().any(|native| {
                    native.pos() == *old_offset
                        && new_offset >= native.pos()
                        && new_offset < native.pos() + native.len()
                });
                assert!(
                    *old_offset == new_offset || same_native_token,
                    "offset at {index} for {}: old={} new={}",
                    String::from_utf8_lossy(source),
                    old_offset,
                    new_offset
                );
            }
        }
    }
