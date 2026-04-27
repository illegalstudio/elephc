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
fn test_named_arguments_tokens() {
    let t = tokens("<?php foo(name: \"Alice\", age: 30);");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Identifier("foo".into()),
            Token::LParen,
            Token::Identifier("name".into()),
            Token::Colon,
            Token::StringLiteral("Alice".into()),
            Token::Comma,
            Token::Identifier("age".into()),
            Token::Colon,
            Token::IntLiteral(30),
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
fn test_exception_keywords_tokens() {
    let t = tokens("<?php try { throw $e; } catch (Exception $e) { } finally { }");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Try,
            Token::LBrace,
            Token::Throw,
            Token::Variable("e".into()),
            Token::Semicolon,
            Token::RBrace,
            Token::Catch,
            Token::LParen,
            Token::Identifier("Exception".into()),
            Token::Variable("e".into()),
            Token::RParen,
            Token::LBrace,
            Token::RBrace,
            Token::Finally,
            Token::LBrace,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

#[test]
fn test_ifdef_keyword_token() {
    let t = tokens("<?php ifdef DEBUG { echo 1; }");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::IfDef,
            Token::Identifier("DEBUG".into()),
            Token::LBrace,
            Token::Echo,
            Token::IntLiteral(1),
            Token::Semicolon,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

#[test]
fn test_namespace_and_backslash_tokens() {
    let t = tokens("<?php namespace Foo\\Bar; use Baz\\Qux;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Namespace,
            Token::Identifier("Foo".into()),
            Token::Backslash,
            Token::Identifier("Bar".into()),
            Token::Semicolon,
            Token::Use,
            Token::Identifier("Baz".into()),
            Token::Backslash,
            Token::Identifier("Qux".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_packed_class_and_buffer_type_tokens() {
    let t = tokens("<?php packed class Vec2 { public float $x; public float $y; } buffer<int> $points = buffer_new<int>(2);");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Packed,
            Token::Class,
            Token::Identifier("Vec2".into()),
            Token::LBrace,
            Token::Public,
            Token::Identifier("float".into()),
            Token::Variable("x".into()),
            Token::Semicolon,
            Token::Public,
            Token::Identifier("float".into()),
            Token::Variable("y".into()),
            Token::Semicolon,
            Token::RBrace,
            Token::Identifier("buffer".into()),
            Token::Less,
            Token::Identifier("int".into()),
            Token::Greater,
            Token::Variable("points".into()),
            Token::Assign,
            Token::Identifier("buffer_new".into()),
            Token::Less,
            Token::Identifier("int".into()),
            Token::Greater,
            Token::LParen,
            Token::IntLiteral(2),
            Token::RParen,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

#[test]
fn test_enum_tokens() {
    let t = tokens("<?php enum Color: int { case Red = 1; case Green = 2; }");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Enum,
            Token::Identifier("Color".into()),
            Token::Colon,
            Token::Identifier("int".into()),
            Token::LBrace,
            Token::Case,
            Token::Identifier("Red".into()),
            Token::Assign,
            Token::IntLiteral(1),
            Token::Semicolon,
            Token::Case,
            Token::Identifier("Green".into()),
            Token::Assign,
            Token::IntLiteral(2),
            Token::Semicolon,
            Token::RBrace,
            Token::Eof,
        ]
    );
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
    let t = tokens("<?php && || and or xor");
    assert_eq!(
        t[1..6],
        [Token::AndAnd, Token::OrOr, Token::And, Token::Or, Token::Xor]
    );
}

#[test]
fn test_word_logical_operators_are_case_insensitive() {
    let t = tokens("<?php AND Or xOr");
    assert_eq!(t[1..4], [Token::And, Token::Or, Token::Xor]);
}

#[test]
fn test_instanceof_keyword_is_case_insensitive() {
    let t = tokens("<?php instanceof INSTANCEOF InstanceOf");
    assert_eq!(t[1..4], [Token::InstanceOf, Token::InstanceOf, Token::InstanceOf]);
}

#[test]
fn test_interface_related_keywords() {
    let t = tokens("<?php interface implements abstract final");
    assert_eq!(t[1], Token::Interface);
    assert_eq!(t[2], Token::Implements);
    assert_eq!(t[3], Token::Abstract);
    assert_eq!(t[4], Token::Final);
}

#[test]
fn test_bang() {
    let t = tokens("<?php !");
    assert_eq!(t[1], Token::Bang);
}

#[test]
fn test_compound_assignment() {
    let t = tokens("<?php += -= *= **= /= .= %= &= |= ^= <<= >>=");
    assert_eq!(
        t[1..13],
        [
            Token::PlusAssign,
            Token::MinusAssign,
            Token::StarAssign,
            Token::StarStarAssign,
            Token::SlashAssign,
            Token::DotAssign,
            Token::PercentAssign,
            Token::AmpAssign,
            Token::PipeAssign,
            Token::CaretAssign,
            Token::LessLessAssign,
            Token::GreaterGreaterAssign,
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
fn test_keyword_trait() {
    assert_eq!(tokens("<?php trait")[1], Token::Trait);
}

#[test]
fn test_keyword_protected() {
    assert_eq!(tokens("<?php protected")[1], Token::Protected);
}

#[test]
fn test_keyword_self() {
    assert_eq!(tokens("<?php self")[1], Token::Self_);
}

#[test]
fn test_keyword_insteadof() {
    assert_eq!(tokens("<?php insteadof")[1], Token::InsteadOf);
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
fn test_keyword_do() {
    assert_eq!(tokens("<?php do")[1], Token::Do);
}

#[test]
fn test_keyword_foreach() {
    assert_eq!(tokens("<?php foreach")[1], Token::Foreach);
}

#[test]
fn test_keyword_as() {
    assert_eq!(tokens("<?php as")[1], Token::As);
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

#[test]
fn test_additional_numeric_constant_tokens() {
    let t = tokens(
        "<?php PHP_INT_MIN PHP_FLOAT_MAX PHP_FLOAT_MIN PHP_FLOAT_EPSILON M_E M_SQRT2 M_PI_2 M_PI_4 M_LOG2E M_LOG10E",
    );
    assert_eq!(
        t[1..11],
        [
            Token::PhpIntMin,
            Token::PhpFloatMax,
            Token::PhpFloatMin,
            Token::PhpFloatEpsilon,
            Token::ME,
            Token::MSqrt2,
            Token::MPi2,
            Token::MPi4,
            Token::MLog2e,
            Token::MLog10e,
        ]
    );
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

#[test]
fn test_runtime_constant_tokens() {
    let t = tokens("<?php PHP_EOL PHP_OS DIRECTORY_SEPARATOR");
    assert_eq!(
        t[1..4],
        [Token::PhpEol, Token::PhpOs, Token::DirectorySeparator]
    );
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

#[test]
fn test_lex_class_keyword() {
    let t = tokens("<?php class Point { public $x; private $y; public readonly $id; } $p = new Point();");
    assert!(t.contains(&Token::Class));
    assert!(t.contains(&Token::New));
    assert!(t.contains(&Token::Public));
    assert!(t.contains(&Token::Private));
    assert!(t.contains(&Token::ReadOnly));
}

#[test]
fn test_lex_typed_property_tokens() {
    let t = tokens("<?php class User { public ?string $email = null; final public int $id; }");
    assert!(t.contains(&Token::Class));
    assert!(t.contains(&Token::Public));
    assert!(t.contains(&Token::Question));
    assert!(t.contains(&Token::Identifier("string".into())));
    assert!(t.contains(&Token::Variable("email".into())));
    assert!(t.contains(&Token::Null));
    assert!(t.contains(&Token::Final));
    assert!(t.contains(&Token::Identifier("int".into())));
    assert!(t.contains(&Token::Variable("id".into())));
}

#[test]
fn test_lex_arrow_operator() {
    let t = tokens("<?php $obj->prop;");
    assert!(t.contains(&Token::Arrow));
}

#[test]
fn test_lex_double_colon() {
    let t = tokens("<?php Point::origin();");
    assert!(t.contains(&Token::DoubleColon));
}

#[test]
fn test_lex_this() {
    let t = tokens("<?php $this->value;");
    assert_eq!(t[1], Token::This);
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
fn test_question_question_assign_token() {
    let t = tokens("<?php $x ??= $y;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Variable("x".into()),
            Token::QuestionQuestionAssign,
            Token::Variable("y".into()),
            Token::Semicolon,
            Token::Eof,
        ]
    );
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

#[test]
fn test_heredoc_interpolation_token() {
    let t = tokens("<?php <<<EOT\nHello $name\nEOT;");
    assert!(t.contains(&Token::Variable("name".into())));
    assert!(t.contains(&Token::Dot));
    assert!(t.contains(&Token::StringLiteral("Hello ".into())));
}

#[test]
fn test_nowdoc_no_interpolation_token() {
    let t = tokens("<?php <<<'EOT'\nHello $name\nEOT;");
    // Nowdoc: $name stays as literal text, no Variable token
    assert!(t.contains(&Token::StringLiteral("Hello $name".into())));
    assert!(!t.contains(&Token::Variable("name".into())));
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

// --- Hex integer literals ---

#[test]
fn test_hex_literal_lowercase() {
    let t = tokens("<?php 0xff;");
    assert_eq!(t[1], Token::IntLiteral(255));
}

#[test]
fn test_hex_literal_uppercase_x() {
    let t = tokens("<?php 0XFF;");
    assert_eq!(t[1], Token::IntLiteral(255));
}

#[test]
fn test_hex_literal_mixed_case_digits() {
    let t = tokens("<?php 0x1aB;");
    assert_eq!(t[1], Token::IntLiteral(427));
}

#[test]
fn test_hex_literal_zero() {
    let t = tokens("<?php 0x0;");
    assert_eq!(t[1], Token::IntLiteral(0));
}

// --- EOF / empty input handling ---

#[test]
fn test_empty_after_open_tag() {
    let t = tokens("<?php ");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_open_tag_no_trailing_space() {
    let t = tokens("<?php");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_open_tag_newline_only() {
    let t = tokens("<?php\n");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_open_tag_with_comment_no_code() {
    let t = tokens("<?php // nothing here\n");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_open_tag_with_block_comment_no_code() {
    let t = tokens("<?php /* empty */");
    assert_eq!(t, vec![Token::OpenTag, Token::Eof]);
}

#[test]
fn test_lex_extern_keyword() {
    let t = tokens("<?php extern function abs(int $n): int;");
    assert!(t.contains(&Token::Extern));
    assert!(t.contains(&Token::Function));
}

#[test]
fn test_nullable_and_union_type_tokens() {
    let t = tokens("<?php ?int|string $value = null;");
    assert_eq!(
        t,
        vec![
            Token::OpenTag,
            Token::Question,
            Token::Identifier("int".into()),
            Token::Pipe,
            Token::Identifier("string".into()),
            Token::Variable("value".into()),
            Token::Assign,
            Token::Null,
            Token::Semicolon,
            Token::Eof,
        ]
    );
}

// --- Magic constants ---

#[test]
fn test_dunder_dir_token() {
    let t = tokens("<?php __DIR__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderDir, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_dunder_file_token() {
    let t = tokens("<?php __FILE__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderFile, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_dunder_line_token() {
    let t = tokens("<?php __LINE__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderLine, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_dunder_function_token() {
    let t = tokens("<?php __FUNCTION__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderFunction, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_dunder_class_token() {
    let t = tokens("<?php __CLASS__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderClass, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_dunder_method_token() {
    let t = tokens("<?php __METHOD__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderMethod, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_dunder_namespace_token() {
    let t = tokens("<?php __NAMESPACE__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderNamespace, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_dunder_trait_token() {
    let t = tokens("<?php __TRAIT__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderTrait, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_dunder_lowercase_is_magic_constant() {
    let t = tokens("<?php __dir__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderDir, Token::Semicolon, Token::Eof]
    );
}

#[test]
fn test_dunder_mixed_case_is_magic_constant() {
    let t = tokens("<?php __FuNcTiOn__;");
    assert_eq!(
        t,
        vec![Token::OpenTag, Token::DunderFunction, Token::Semicolon, Token::Eof]
    );
}
