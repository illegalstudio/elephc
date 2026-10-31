//! Purpose:
//! PHP 8.5 tokenizer constant contracts, including the double-colon alias and TOKEN_PARSE.
//!
//! Called from:
//! - The shared symbol registry and both tokenizer backends.
//!
//! Key details:
//! - Values match the vendored PHP 8.5.10 baseline, independently of the host PHP version.

use crate::{BuiltinId, ConstValue, ConstantContract, ConstantRoute, PhpModule};

/// Builds one predefined tokenizer integer constant.
const fn token(name: &'static str, value: i64) -> ConstantContract {
    ConstantContract {
        id: BuiltinId::from_canonical_name(name), name, module: PhpModule::Tokenizer,
        since: None, value: ConstValue::Int(value), route: ConstantRoute::Predefined,
        extension: false, internal: false,
    }
}

pub(crate) static CONSTANTS: &[ConstantContract] = &[
    token("TOKEN_PARSE", 1),
    token("T_ABSTRACT", 322),
    token("T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG", 409),
    token("T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG", 410),
    token("T_AND_EQUAL", 362),
    token("T_ARRAY", 344),
    token("T_ARRAY_CAST", 384),
    token("T_AS", 301),
    token("T_ATTRIBUTE", 355),
    token("T_BAD_CHARACTER", 411),
    token("T_BOOLEAN_AND", 369),
    token("T_BOOLEAN_OR", 368),
    token("T_BOOL_CAST", 386),
    token("T_BREAK", 307),
    token("T_CALLABLE", 345),
    token("T_CASE", 304),
    token("T_CATCH", 315),
    token("T_CLASS", 336),
    token("T_CLASS_C", 349),
    token("T_CLONE", 285),
    token("T_CLOSE_TAG", 396),
    token("T_COALESCE", 405),
    token("T_COALESCE_EQUAL", 367),
    token("T_COMMENT", 392),
    token("T_CONCAT_EQUAL", 360),
    token("T_CONST", 312),
    token("T_CONSTANT_ENCAPSED_STRING", 269),
    token("T_CONTINUE", 308),
    token("T_CURLY_OPEN", 401),
    token("T_DEC", 380),
    token("T_DECLARE", 299),
    token("T_DEFAULT", 305),
    token("T_DIR", 348),
    token("T_DIV_EQUAL", 359),
    token("T_DNUMBER", 261),
    token("T_DO", 292),
    token("T_DOC_COMMENT", 393),
    token("T_DOLLAR_OPEN_CURLY_BRACES", 400),
    token("T_DOUBLE_ARROW", 391),
    token("T_DOUBLE_CAST", 382),
    token("T_DOUBLE_COLON", 402),
    token("T_ECHO", 291),
    token("T_ELLIPSIS", 404),
    token("T_ELSE", 289),
    token("T_ELSEIF", 288),
    token("T_EMPTY", 334),
    token("T_ENCAPSED_AND_WHITESPACE", 268),
    token("T_ENDDECLARE", 300),
    token("T_ENDFOR", 296),
    token("T_ENDFOREACH", 298),
    token("T_ENDIF", 290),
    token("T_ENDSWITCH", 303),
    token("T_ENDWHILE", 294),
    token("T_END_HEREDOC", 399),
    token("T_ENUM", 339),
    token("T_EVAL", 274),
    token("T_EXIT", 286),
    token("T_EXTENDS", 340),
    token("T_FILE", 347),
    token("T_FINAL", 323),
    token("T_FINALLY", 316),
    token("T_FN", 311),
    token("T_FOR", 295),
    token("T_FOREACH", 297),
    token("T_FUNCTION", 310),
    token("T_FUNC_C", 352),
    token("T_GLOBAL", 320),
    token("T_GOTO", 309),
    token("T_HALT_COMPILER", 335),
    token("T_IF", 287),
    token("T_IMPLEMENTS", 341),
    token("T_INC", 379),
    token("T_INCLUDE", 272),
    token("T_INCLUDE_ONCE", 273),
    token("T_INLINE_HTML", 267),
    token("T_INSTANCEOF", 283),
    token("T_INSTEADOF", 319),
    token("T_INTERFACE", 338),
    token("T_INT_CAST", 381),
    token("T_ISSET", 333),
    token("T_IS_EQUAL", 370),
    token("T_IS_GREATER_OR_EQUAL", 375),
    token("T_IS_IDENTICAL", 372),
    token("T_IS_NOT_EQUAL", 371),
    token("T_IS_NOT_IDENTICAL", 373),
    token("T_IS_SMALLER_OR_EQUAL", 374),
    token("T_LINE", 346),
    token("T_LIST", 343),
    token("T_LNUMBER", 260),
    token("T_LOGICAL_AND", 279),
    token("T_LOGICAL_OR", 277),
    token("T_LOGICAL_XOR", 278),
    token("T_MATCH", 306),
    token("T_METHOD_C", 351),
    token("T_MINUS_EQUAL", 357),
    token("T_MOD_EQUAL", 361),
    token("T_MUL_EQUAL", 358),
    token("T_NAMESPACE", 342),
    token("T_NAME_FULLY_QUALIFIED", 263),
    token("T_NAME_QUALIFIED", 265),
    token("T_NAME_RELATIVE", 264),
    token("T_NEW", 284),
    token("T_NS_C", 354),
    token("T_NS_SEPARATOR", 403),
    token("T_NULLSAFE_OBJECT_OPERATOR", 390),
    token("T_NUM_STRING", 271),
    token("T_OBJECT_CAST", 385),
    token("T_OBJECT_OPERATOR", 389),
    token("T_OPEN_TAG", 394),
    token("T_OPEN_TAG_WITH_ECHO", 395),
    token("T_OR_EQUAL", 363),
    token("T_PAAMAYIM_NEKUDOTAYIM", 402),
    token("T_PIPE", 408),
    token("T_PLUS_EQUAL", 356),
    token("T_POW", 406),
    token("T_POW_EQUAL", 407),
    token("T_PRINT", 280),
    token("T_PRIVATE", 324),
    token("T_PRIVATE_SET", 327),
    token("T_PROPERTY_C", 353),
    token("T_PROTECTED", 325),
    token("T_PROTECTED_SET", 328),
    token("T_PUBLIC", 326),
    token("T_PUBLIC_SET", 329),
    token("T_READONLY", 330),
    token("T_REQUIRE", 275),
    token("T_REQUIRE_ONCE", 276),
    token("T_RETURN", 313),
    token("T_SL", 377),
    token("T_SL_EQUAL", 365),
    token("T_SPACESHIP", 376),
    token("T_SR", 378),
    token("T_SR_EQUAL", 366),
    token("T_START_HEREDOC", 398),
    token("T_STATIC", 321),
    token("T_STRING", 262),
    token("T_STRING_CAST", 383),
    token("T_STRING_VARNAME", 270),
    token("T_SWITCH", 302),
    token("T_THROW", 317),
    token("T_TRAIT", 337),
    token("T_TRAIT_C", 350),
    token("T_TRY", 314),
    token("T_UNSET", 332),
    token("T_UNSET_CAST", 387),
    token("T_USE", 318),
    token("T_VAR", 331),
    token("T_VARIABLE", 266),
    token("T_VOID_CAST", 388),
    token("T_WHILE", 293),
    token("T_WHITESPACE", 397),
    token("T_XOR_EQUAL", 364),
    token("T_YIELD", 281),
    token("T_YIELD_FROM", 282),
];

/// Returns the PHP 8.5 tokenizer identifier for `name`, or `-1` when the catalog has no such token.
///
/// The lookup runs at compile time when the argument is a literal, letting the parser
/// validation view compare integer token IDs without hashing the shared catalog per token.
pub const fn token_const_id(name: &str) -> i64 {
    let bytes = name.as_bytes();
    let mut index = 0;
    while index < CONSTANTS.len() {
        let constant = &CONSTANTS[index];
        if constant.name.len() == bytes.len() {
            let candidate = constant.name.as_bytes();
            let mut offset = 0;
            let mut same = true;
            while offset < bytes.len() {
                if candidate[offset] != bytes[offset] {
                    same = false;
                    break;
                }
                offset += 1;
            }
            if same {
                if let ConstValue::Int(value) = constant.value {
                    return value;
                }
            }
        }
        index += 1;
    }
    -1
}
