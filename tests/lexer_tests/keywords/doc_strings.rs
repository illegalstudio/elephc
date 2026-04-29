use super::*;

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
