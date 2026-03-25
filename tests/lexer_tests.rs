use elephc::lexer::{tokenize, Token};

/// Helper: extract just the tokens (discard spans) for easy comparison.
fn tokens(source: &str) -> Vec<Token> {
    tokenize(source)
        .unwrap()
        .into_iter()
        .map(|(t, _)| t)
        .collect()
}

// --- Ellipsis / spread ---

#[test]
fn test_ellipsis_token() {
    let t = tokens("<?php ...");
    assert_eq!(t, vec![Token::OpenTag, Token::Ellipsis, Token::Eof]);
}

#[test]
fn test_ellipsis_in_function_params() {
    let t = tokens("<?php function foo(...$args) {}");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Function,
            Token::Identifier("foo".into()),
            Token::LParen,
            Token::Ellipsis,
            Token::Variable("args".into()),
            Token::RParen,
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

#[test]
fn test_ellipsis_in_function_call() {
    let t = tokens("<?php foo(...$arr);");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Identifier("foo".into()),
            Token::LParen,
            Token::Ellipsis,
            Token::Variable("arr".into()),
            Token::RParen,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_dot_vs_ellipsis() {
    // Single dot is concat, three dots is ellipsis
    let t = tokens("<?php $a . $b ... $c");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Variable("a".into()),
            Token::Dot,
            Token::Variable("b".into()),
            Token::Ellipsis,
            Token::Variable("c".into()),
            Token::Eof,
        ]
    );
}

// --- Basic tokens ---

#[test]
fn test_open_tag() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_echo_string() {
    let t = tokens("<?php echo \"hello\";");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Echo,
            Token::StringLiteral("hello".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_string_escape_sequences() {
    let t = tokens("<?php \"hello\\nworld\\t!\"");
    assert_eq!(t[1], Token::StringLiteral("hello\nworld\t!".into()));
}

#[test]
fn test_integer_literal() {
    let t = tokens("<?php 42");
    assert_eq!(t[1], Token::IntLiteral(42));
}

#[test]
fn test_variable() {
    let t = tokens("<?php $foo");
    assert_eq!(t[1], Token::Variable("foo".into()));
}

// --- Operators ---

#[test]
fn test_arithmetic_operators() {
    let t = tokens("<?php + - * / %");
    assert_eq!(
        t[1..6],
        [Token::Plus, Token::Minus, Token::Star, Token::Slash, Token::Percent]
    );
}

#[test]
fn test_assignment_and_dot() {
    let t = tokens("<?php . =");
    assert_eq!(t[1..3], [Token::Dot, Token::Assign]);
}

#[test]
fn test_comparison_operators() {
    let t = tokens("<?php == != < > <= >=");
    assert_eq!(
        t[1..7],
        [
            Token::EqualEqual,
            Token::NotEqual,
            Token::Less,
            Token::Greater,
            Token::LessEqual,
            Token::GreaterEqual,
        ]
    );
}

#[test]
fn test_logical_operators() {
    let t = tokens("<?php && ||");
    assert_eq!(t[1..3], [Token::AndAnd, Token::OrOr]);
}

#[test]
fn test_bang() {
    let t = tokens("<?php !");
    assert_eq!(t[1], Token::Bang);
}

#[test]
fn test_compound_assignment() {
    let t = tokens("<?php += -= *= /= .= %=");
    assert_eq!(
        t[1..7],
        [
            Token::PlusAssign,
            Token::MinusAssign,
            Token::StarAssign,
            Token::SlashAssign,
            Token::DotAssign,
            Token::PercentAssign,
        ]
    );
}

#[test]
fn test_boolean_keywords() {
    let t = tokens("<?php true false");
    assert_eq!(t[1..3], [Token::True, Token::False]);
}

#[test]
fn test_increment_decrement() {
    let t = tokens("<?php ++ --");
    assert_eq!(t[1..3], [Token::PlusPlus, Token::MinusMinus]);
}

#[test]
fn test_braces() {
    let t = tokens("<?php { }");
    assert_eq!(t[1..3], [Token::LBrace, Token::RBrace]);
}

#[test]
fn test_parens() {
    let t = tokens("<?php ( )");
    assert_eq!(t[1..3], [Token::LParen, Token::RParen]);
}

#[test]
fn test_comma() {
    let t = tokens("<?php ,");
    assert_eq!(t[1], Token::Comma);
}

// --- Keywords ---

#[test]
fn test_keyword_echo() {
    assert_eq!(tokens("<?php echo")[1], Token::Echo);
}

#[test]
fn test_keyword_if() {
    assert_eq!(tokens("<?php if")[1], Token::If);
}

#[test]
fn test_keyword_else() {
    assert_eq!(tokens("<?php else")[1], Token::Else);
}

#[test]
fn test_keyword_elseif() {
    assert_eq!(tokens("<?php elseif")[1], Token::ElseIf);
}

#[test]
fn test_keyword_while() {
    assert_eq!(tokens("<?php while")[1], Token::While);
}

#[test]
fn test_keyword_for() {
    assert_eq!(tokens("<?php for")[1], Token::For);
}

#[test]
fn test_keyword_break() {
    assert_eq!(tokens("<?php break")[1], Token::Break);
}

#[test]
fn test_keyword_continue() {
    assert_eq!(tokens("<?php continue")[1], Token::Continue);
}

#[test]
fn test_keyword_function() {
    assert_eq!(tokens("<?php function")[1], Token::Function);
}

#[test]
fn test_keyword_return() {
    assert_eq!(tokens("<?php return")[1], Token::Return);
}

#[test]
fn test_identifier() {
    assert_eq!(
        tokens("<?php foo")[1],
        Token::Identifier("foo".into())
    );
}

// --- Comments ---

#[test]
fn test_line_comment() {
    let t = tokens("<?php // this is a comment\necho \"hi\";");
    assert_eq!(t[1], Token::Echo);
}

#[test]
fn test_block_comment() {
    let t = tokens("<?php /* block */ echo \"hi\";");
    assert_eq!(t[1], Token::Echo);
}

#[test]
fn test_consecutive_comments() {
    let t = tokens("<?php /* a *//* b */// c\necho \"ok\";");
    assert_eq!(t[1], Token::Echo);
}

// --- Complex tokens ---

#[test]
fn test_assignment_statement() {
    let t = tokens("<?php $x = 42;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Variable("x".into()),
            Token::Assign,
            Token::IntLiteral(42),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_equals_vs_assign() {
    // = followed by = should be ==, not two Assigns
    let t = tokens("<?php == =");
    assert_eq!(t[1], Token::EqualEqual);
    assert_eq!(t[2], Token::Assign);
}

#[test]
fn test_function_declaration_tokens() {
    let t = tokens("<?php function add($a, $b) { return $a; }");
    assert_eq!(t[1], Token::Function);
    assert_eq!(t[2], Token::Identifier("add".into()));
    assert_eq!(t[3], Token::LParen);
    assert_eq!(t[4], Token::Variable("a".into()));
    assert_eq!(t[5], Token::Comma);
    assert_eq!(t[6], Token::Variable("b".into()));
    assert_eq!(t[7], Token::RParen);
    assert_eq!(t[8], Token::LBrace);
    assert_eq!(t[9], Token::Return);
}

// --- Error cases ---

#[test]
fn test_missing_open_tag() {
    assert!(tokenize("echo \"hi\";").is_err());
}

#[test]
fn test_unterminated_string() {
    assert!(tokenize("<?php \"no closing").is_err());
}

// --- Spans ---

#[test]
fn test_span_tracking() {
    let spanned = tokenize("<?php\necho \"hi\";").unwrap();
    let echo_span = spanned[1].1;
    assert_eq!(echo_span.line, 2);
    assert_eq!(echo_span.col, 1);
}

#[test]
fn test_span_multiline() {
    let spanned = tokenize("<?php\n\n\n$x").unwrap();
    let var_span = spanned[1].1;
    assert_eq!(var_span.line, 4);
}

// --- Strict comparison ---

#[test]
fn test_strict_equal() {
    let t = tokens("<?php ===");
    assert_eq!(t[1], Token::EqualEqualEqual);
}

#[test]
fn test_strict_not_equal() {
    let t = tokens("<?php !==");
    assert_eq!(t[1], Token::NotEqualEqual);
}

#[test]
fn test_strict_equal_vs_loose_equal() {
    let t = tokens("<?php === ==");
    assert_eq!(t[1], Token::EqualEqualEqual);
    assert_eq!(t[2], Token::EqualEqual);
}

#[test]
fn test_strict_not_equal_vs_loose_not_equal() {
    let t = tokens("<?php !== !=");
    assert_eq!(t[1], Token::NotEqualEqual);
    assert_eq!(t[2], Token::NotEqual);
}

// --- Include/Require ---

#[test]
fn test_include_keyword() {
    let t = tokens("<?php include");
    assert_eq!(t[1], Token::Include);
}

#[test]
fn test_include_once_keyword() {
    let t = tokens("<?php include_once");
    assert_eq!(t[1], Token::IncludeOnce);
}

#[test]
fn test_require_keyword() {
    let t = tokens("<?php require");
    assert_eq!(t[1], Token::Require);
}

#[test]
fn test_require_once_keyword() {
    let t = tokens("<?php require_once");
    assert_eq!(t[1], Token::RequireOnce);
}

// --- StarStar ---

#[test]
fn test_star_star() {
    let t = tokens("<?php **");
    assert_eq!(t[1], Token::StarStar);
}

#[test]
fn test_star_vs_star_star() {
    let t = tokens("<?php ** *");
    assert_eq!(t[1], Token::StarStar);
    assert_eq!(t[2], Token::Star);
}

// --- Constants ---

#[test]
fn test_php_int_max_token() {
    let t = tokens("<?php PHP_INT_MAX");
    assert_eq!(t[1], Token::PhpIntMax);
}

#[test]
fn test_m_pi_token() {
    let t = tokens("<?php M_PI");
    assert_eq!(t[1], Token::MPi);
}

// --- INF / NAN ---

#[test]
fn test_inf_keyword() {
    let t = tokens("<?php INF");
    assert_eq!(t[1], Token::Inf);
}

#[test]
fn test_nan_keyword() {
    let t = tokens("<?php NAN");
    assert_eq!(t[1], Token::Nan);
}

// --- Float literals ---

#[test]
fn test_float_literal() {
    let t = tokens("<?php 3.14");
    assert_eq!(t[1], Token::FloatLiteral(3.14));
}

#[test]
fn test_dot_prefix_float() {
    let t = tokens("<?php .5");
    assert_eq!(t[1], Token::FloatLiteral(0.5));
}

#[test]
fn test_scientific_notation() {
    let t = tokens("<?php 1.5e3");
    assert_eq!(t[1], Token::FloatLiteral(1500.0));
}

#[test]
fn test_scientific_notation_negative_exponent() {
    let t = tokens("<?php 1.0e-5");
    assert_eq!(t[1], Token::FloatLiteral(1.0e-5));
}

#[test]
fn test_integer_not_mistaken_for_float() {
    let t = tokens("<?php 42");
    assert_eq!(t[1], Token::IntLiteral(42));
}

#[test]
fn test_dot_operator_not_float() {
    let t = tokens("<?php \"a\" . \"b\"");
    assert_eq!(t[2], Token::Dot);
}

// --- Print keyword ---

#[test]
fn test_print_keyword() {
    let t = tokens("<?php print \"hello\";");
    assert_eq!(t[1], Token::Print);
}

// --- STDIN / STDOUT / STDERR ---

#[test]
fn test_stdin_token() {
    let t = tokens("<?php STDIN;");
    assert_eq!(t[1], Token::Stdin);
}

#[test]
fn test_stdout_token() {
    let t = tokens("<?php STDOUT;");
    assert_eq!(t[1], Token::Stdout);
}

#[test]
fn test_stderr_token() {
    let t = tokens("<?php STDERR;");
    assert_eq!(t[1], Token::Stderr);
}

// --- v0.6: Arrays, switch, match ---

#[test]
fn test_double_arrow_token() {
    let t = tokens("<?php [1 => 2];");
    assert!(t.contains(&Token::DoubleArrow));
}

#[test]
fn test_switch_token() {
    let t = tokens("<?php switch ($x) {}");
    assert_eq!(t[1], Token::Switch);
}

#[test]
fn test_case_token() {
    let t = tokens("<?php case 1:");
    assert_eq!(t[1], Token::Case);
}

#[test]
fn test_default_token() {
    let t = tokens("<?php default:");
    assert_eq!(t[1], Token::Default);
}

#[test]
fn test_match_token() {
    let t = tokens("<?php match($x) {}");
    assert_eq!(t[1], Token::Match);
}

#[test]
fn test_fn_token() {
    let t = tokens("<?php fn($x) => $x;");
    assert_eq!(t[1], Token::Fn);
}

#[test]
fn test_use_token() {
    let t = tokens("<?php use;");
    assert_eq!(t[1], Token::Use);
}

#[test]
fn test_function_token_anonymous() {
    let t = tokens("<?php function($x) {}");
    assert_eq!(t[1], Token::Function);
    assert_eq!(t[2], Token::LParen);
}

// --- Bitwise operator tokens ---

#[test]
fn test_ampersand_token() {
    let t = tokens("<?php $x & $y;");
    assert!(t.contains(&Token::Ampersand));
}

#[test]
fn test_pipe_token() {
    let t = tokens("<?php $x | $y;");
    assert!(t.contains(&Token::Pipe));
}

#[test]
fn test_caret_token() {
    let t = tokens("<?php $x ^ $y;");
    assert!(t.contains(&Token::Caret));
}

#[test]
fn test_tilde_token() {
    let t = tokens("<?php ~$x;");
    assert!(t.contains(&Token::Tilde));
}

#[test]
fn test_shift_left_token() {
    let t = tokens("<?php $x << $y;");
    assert!(t.contains(&Token::LessLess));
}

#[test]
fn test_shift_right_token() {
    let t = tokens("<?php $x >> $y;");
    assert!(t.contains(&Token::GreaterGreater));
}

#[test]
fn test_ampersand_vs_andand() {
    let t = tokens("<?php $x & $y && $z;");
    assert!(t.contains(&Token::Ampersand));
    assert!(t.contains(&Token::AndAnd));
}

#[test]
fn test_pipe_vs_oror() {
    let t = tokens("<?php $x | $y || $z;");
    assert!(t.contains(&Token::Pipe));
    assert!(t.contains(&Token::OrOr));
}

// --- Spaceship operator ---

#[test]
fn test_spaceship_token() {
    let t = tokens("<?php $x <=> $y;");
    assert!(t.contains(&Token::Spaceship));
}

// --- Null coalescing operator ---

#[test]
fn test_question_question_token() {
    let t = tokens("<?php $x ?? $y;");
    assert!(t.contains(&Token::QuestionQuestion));
}

#[test]
fn test_question_vs_question_question() {
    let t = tokens("<?php $x ? $y : $z ?? $w;");
    assert!(t.contains(&Token::Question));
    assert!(t.contains(&Token::QuestionQuestion));
}

// --- Heredoc / Nowdoc ---

#[test]
fn test_heredoc_token() {
    let t = tokens("<?php <<<EOT\nHello\nEOT;");
    assert!(t.contains(&Token::StringLiteral("Hello".into())));
}

#[test]
fn test_nowdoc_token() {
    let t = tokens("<?php <<<'EOT'\nHello\nEOT;");
    assert!(t.contains(&Token::StringLiteral("Hello".into())));
}

// --- Const keyword ---

#[test]
fn test_const_keyword() {
    let t = tokens("<?php const MAX = 100;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Const,
            Token::Identifier("MAX".into()),
            Token::Assign,
            Token::IntLiteral(100),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_const_string_value() {
    let t = tokens("<?php const NAME = \"test\";");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Const,
            Token::Identifier("NAME".into()),
            Token::Assign,
            Token::StringLiteral("test".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- Global keyword ---

#[test]
fn test_global_keyword() {
    let t = tokens("<?php global $x;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Global,
            Token::Variable("x".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_global_multiple() {
    let t = tokens("<?php global $a, $b;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Global,
            Token::Variable("a".into()),
            Token::Comma,
            Token::Variable("b".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- Static keyword ---

#[test]
fn test_static_keyword() {
    let t = tokens("<?php static $x = 0;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Static,
            Token::Variable("x".into()),
            Token::Assign,
            Token::IntLiteral(0),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- Reference parameter ---

#[test]
fn test_ref_param_in_function() {
    let t = tokens("<?php function foo(&$x) {}");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Function,
            Token::Identifier("foo".into()),
            Token::LParen,
            Token::Ampersand,
            Token::Variable("x".into()),
            Token::RParen,
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ]
    );
}
