use super::*;

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
