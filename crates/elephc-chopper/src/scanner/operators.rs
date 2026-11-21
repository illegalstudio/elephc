//! Purpose:
//! Recognizes multi-byte operators and supplies their Zend token IDs.
//!
//! Called from:
//! - The PHP tokenizer and the compiler lexer.
//!
//! Key details:
//! - Longest spellings precede their shorter prefixes.
//! - The two spellings of loose inequality have the same Zend ID.

macro_rules! operator_prefix {
    ($first:literal, $second:literal, $third:literal) => { ($first, $second, $third) };
    ($first:literal, $second:literal) => { ($first, $second, _) };
}

macro_rules! operators {
    ($($name:ident = [$($byte:literal),+] => $php_name:literal,)*) => {
        /// A multi-byte operator shared by the compiler and PHP tokenizer.
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub enum Operator {
            $($name,)*
        }

        /// Returns an operator at the start of source and its byte length.
        #[inline]
        pub fn operator(source: &[u8]) -> Option<(Operator, usize)> {
            let [first, second, rest @ ..] = source else {
                return None;
            };
            let third = rest.first().copied().unwrap_or_default();
            match (*first, *second, third) {
                $(operator_prefix!($($byte),+) => Some((Operator::$name, [$($byte),+].len())),)*
                _ => None,
            }
        }

        impl Operator {
            /// Returns the corresponding ID from the PHP token catalog.
            pub(crate) fn php_id(self) -> i64 {
                use elephc_builtin_contract::token_const_id;
                match self {
                    $(Self::$name => const { token_const_id($php_name) },)*
                }
            }
        }
    };
}

operators! {
    StarStarAssign = [b'*', b'*', b'='] => "T_POW_EQUAL",
    Ellipsis = [b'.', b'.', b'.'] => "T_ELLIPSIS",
    Spaceship = [b'<', b'=', b'>'] => "T_SPACESHIP",
    EqualEqualEqual = [b'=', b'=', b'='] => "T_IS_IDENTICAL",
    NotEqualEqual = [b'!', b'=', b'='] => "T_IS_NOT_IDENTICAL",
    LessLessAssign = [b'<', b'<', b'='] => "T_SL_EQUAL",
    GreaterGreaterAssign = [b'>', b'>', b'='] => "T_SR_EQUAL",
    QuestionQuestionAssign = [b'?', b'?', b'='] => "T_COALESCE_EQUAL",
    QuestionArrow = [b'?', b'-', b'>'] => "T_NULLSAFE_OBJECT_OPERATOR",
    StarStar = [b'*', b'*'] => "T_POW",
    PlusPlus = [b'+', b'+'] => "T_INC",
    MinusMinus = [b'-', b'-'] => "T_DEC",
    DoubleArrow = [b'=', b'>'] => "T_DOUBLE_ARROW",
    LessEqual = [b'<', b'='] => "T_IS_SMALLER_OR_EQUAL",
    GreaterEqual = [b'>', b'='] => "T_IS_GREATER_OR_EQUAL",
    EqualEqual = [b'=', b'='] => "T_IS_EQUAL",
    NotEqual = [b'!', b'='] => "T_IS_NOT_EQUAL",
    LessGreater = [b'<', b'>'] => "T_IS_NOT_EQUAL",
    PlusAssign = [b'+', b'='] => "T_PLUS_EQUAL",
    MinusAssign = [b'-', b'='] => "T_MINUS_EQUAL",
    StarAssign = [b'*', b'='] => "T_MUL_EQUAL",
    SlashAssign = [b'/', b'='] => "T_DIV_EQUAL",
    DotAssign = [b'.', b'='] => "T_CONCAT_EQUAL",
    PercentAssign = [b'%', b'='] => "T_MOD_EQUAL",
    AmpAssign = [b'&', b'='] => "T_AND_EQUAL",
    PipeAssign = [b'|', b'='] => "T_OR_EQUAL",
    CaretAssign = [b'^', b'='] => "T_XOR_EQUAL",
    AndAnd = [b'&', b'&'] => "T_BOOLEAN_AND",
    OrOr = [b'|', b'|'] => "T_BOOLEAN_OR",
    QuestionQuestion = [b'?', b'?'] => "T_COALESCE",
    PipeArrow = [b'|', b'>'] => "T_PIPE",
    DoubleColon = [b':', b':'] => "T_DOUBLE_COLON",
    Arrow = [b'-', b'>'] => "T_OBJECT_OPERATOR",
    LessLess = [b'<', b'<'] => "T_SL",
    GreaterGreater = [b'>', b'>'] => "T_SR",
}
