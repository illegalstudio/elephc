//! Purpose:
//! Owns the PHP language profile runtime eval reports, so that a binary compiled
//! `--php-version 8.2` reports `8.2.0` from inside `eval()` exactly as it does
//! natively. Without this, the version surface forks at the eval boundary.
//!
//! Called from:
//! - `crate::ffi::context::__elephc_eval_set_php_version_id` (generated code sets
//!   the profile before every runtime eval dispatch).
//! - `crate::interpreter::constant_eval` (`PHP_VERSION`, `PHP_VERSION_ID`,
//!   `PHP_MINOR_VERSION`).
//! - `crate::interpreter::builtins::network_env` (`phpversion()` and the four
//!   opcache builtins whose directive set is profile-gated).
//!
//! Key details:
//! - Thread-local, mirroring `crate::strict_php_mode`: the profile is a property
//!   of the whole compiled binary, and elephc programs execute the setter and
//!   every eval fragment on one thread (fibers switch stacks, not OS threads),
//!   while parallel `cargo test` threads stay isolated.
//! - The default is the NEWEST profile, not the oldest. Anything linking this
//!   archive without elephc's codegen — every test harness in this crate
//!   included — observes the pre-existing behaviour unchanged.
//! - An id outside the supported set is IGNORED rather than stored. The version
//!   string is a lookup, not a computation, so an unknown id has no spelling;
//!   keeping the default is the only answer that stays a real PHP version.

use std::cell::Cell;

/// The profile eval reports when generated code never sets one.
///
/// KEEP IN SYNC with the default of `--php-version` in the compiler
/// (`crate::web_prelude::PhpVersion::default()`).
const DEFAULT_EVAL_PHP_VERSION_ID: u32 = 80500;

/// The supported profiles, paired with the `PHP_VERSION` string each one reports.
///
/// KEEP IN SYNC with `crate::php_version::PhpVersion::ALL` and `version_string()`
/// in the compiler. The patch component is `0` for every entry by the same rule
/// the compiler applies — elephc targets a language profile, not an upstream
/// patch release.
const EVAL_PHP_PROFILES: &[(u32, &str)] = &[
    (80000, "8.0.0"),
    (80100, "8.1.0"),
    (80200, "8.2.0"),
    (80300, "8.3.0"),
    (80400, "8.4.0"),
    (80500, "8.5.0"),
    (80600, "8.6.0"),
];

/// Version-sensitive tokenizer constants exposed inside runtime eval fragments.
///
/// KEEP IN SYNC with `crate::types::token_constants::TOKEN_INT_CONSTANTS` in the compiler.
const EVAL_TOKEN_IDS: &[(&str, [i64; 7])] = &[
    ("T_LNUMBER", [256, 255, 255, 260, 259, 260, 266]),
    ("T_DNUMBER", [257, 256, 256, 261, 260, 261, 267]),
    ("T_STRING", [258, 257, 257, 262, 261, 262, 268]),
    ("T_NAME_FULLY_QUALIFIED", [259, 258, 258, 263, 262, 263, 269]),
    ("T_NAME_RELATIVE", [260, 259, 259, 264, 263, 264, 270]),
    ("T_NAME_QUALIFIED", [261, 260, 260, 265, 264, 265, 271]),
    ("T_VARIABLE", [262, 261, 261, 266, 265, 266, 272]),
    ("T_INLINE_HTML", [263, 262, 262, 267, 266, 267, 273]),
    ("T_ENCAPSED_AND_WHITESPACE", [264, 263, 263, 268, 267, 268, 274]),
    ("T_CONSTANT_ENCAPSED_STRING", [265, 264, 264, 269, 268, 269, 275]),
    ("T_STRING_VARNAME", [266, 265, 265, 270, 269, 270, 276]),
    ("T_NUM_STRING", [267, 266, 266, 271, 270, 271, 277]),
    ("T_INCLUDE", [268, 267, 267, 272, 271, 272, 278]),
    ("T_INCLUDE_ONCE", [269, 268, 268, 273, 272, 273, 279]),
    ("T_EVAL", [270, 269, 269, 274, 273, 274, 280]),
    ("T_REQUIRE", [271, 270, 270, 275, 274, 275, 281]),
    ("T_REQUIRE_ONCE", [272, 271, 271, 276, 275, 276, 282]),
    ("T_LOGICAL_OR", [273, 272, 272, 277, 276, 277, 283]),
    ("T_LOGICAL_XOR", [274, 273, 273, 278, 277, 278, 284]),
    ("T_LOGICAL_AND", [275, 274, 274, 279, 278, 279, 285]),
    ("T_PRINT", [276, 275, 275, 280, 279, 280, 286]),
    ("T_YIELD", [277, 276, 276, 281, 280, 281, 287]),
    ("T_YIELD_FROM", [278, 277, 277, 282, 281, 282, 288]),
    ("T_INSTANCEOF", [279, 278, 278, 283, 282, 283, 289]),
    ("T_NEW", [280, 279, 279, 284, 283, 284, 290]),
    ("T_CLONE", [281, 280, 280, 285, 284, 285, 291]),
    ("T_EXIT", [282, 281, 281, 286, 285, 286, 292]),
    ("T_IF", [283, 282, 282, 287, 286, 287, 293]),
    ("T_ELSEIF", [284, 283, 283, 288, 287, 288, 294]),
    ("T_ELSE", [285, 284, 284, 289, 288, 289, 295]),
    ("T_ENDIF", [286, 285, 285, 290, 289, 290, 296]),
    ("T_ECHO", [287, 286, 286, 291, 290, 291, 297]),
    ("T_DO", [288, 287, 287, 292, 291, 292, 298]),
    ("T_WHILE", [289, 288, 288, 293, 292, 293, 299]),
    ("T_ENDWHILE", [290, 289, 289, 294, 293, 294, 300]),
    ("T_FOR", [291, 290, 290, 295, 294, 295, 301]),
    ("T_ENDFOR", [292, 291, 291, 296, 295, 296, 302]),
    ("T_FOREACH", [293, 292, 292, 297, 296, 297, 303]),
    ("T_ENDFOREACH", [294, 293, 293, 298, 297, 298, 304]),
    ("T_DECLARE", [295, 294, 294, 299, 298, 299, 305]),
    ("T_ENDDECLARE", [296, 295, 295, 300, 299, 300, 306]),
    ("T_AS", [297, 296, 296, 301, 300, 301, 307]),
    ("T_SWITCH", [298, 297, 297, 302, 301, 302, 308]),
    ("T_ENDSWITCH", [299, 298, 298, 303, 302, 303, 309]),
    ("T_CASE", [300, 299, 299, 304, 303, 304, 310]),
    ("T_DEFAULT", [301, 300, 300, 305, 304, 305, 311]),
    ("T_MATCH", [302, 301, 301, 306, 305, 306, 312]),
    ("T_BREAK", [303, 302, 302, 307, 306, 307, 313]),
    ("T_CONTINUE", [304, 303, 303, 308, 307, 308, 314]),
    ("T_GOTO", [305, 304, 304, 309, 308, 309, 315]),
    ("T_FUNCTION", [306, 305, 305, 310, 309, 310, 316]),
    ("T_FN", [307, 306, 306, 311, 310, 311, 317]),
    ("T_CONST", [308, 307, 307, 312, 311, 312, 318]),
    ("T_RETURN", [309, 308, 308, 313, 312, 313, 319]),
    ("T_TRY", [310, 309, 309, 314, 313, 314, 320]),
    ("T_CATCH", [311, 310, 310, 315, 314, 315, 321]),
    ("T_FINALLY", [312, 311, 311, 316, 315, 316, 322]),
    ("T_THROW", [313, 312, 312, 317, 316, 317, 323]),
    ("T_USE", [314, 313, 313, 318, 317, 318, 324]),
    ("T_INSTEADOF", [315, 314, 314, 319, 318, 319, 325]),
    ("T_GLOBAL", [316, 315, 315, 320, 319, 320, 326]),
    ("T_STATIC", [317, 316, 316, 321, 320, 321, 327]),
    ("T_ABSTRACT", [318, 317, 317, 322, 321, 322, 328]),
    ("T_FINAL", [319, 318, 318, 323, 322, 323, 329]),
    ("T_PRIVATE", [320, 319, 319, 324, 323, 324, 330]),
    ("T_PROTECTED", [321, 320, 320, 325, 324, 325, 331]),
    ("T_PUBLIC", [322, 321, 321, 326, 325, 326, 332]),
    ("T_PRIVATE_SET", [323, 322, 322, 327, 326, 327, 333]),
    ("T_PROTECTED_SET", [324, 323, 323, 328, 327, 328, 334]),
    ("T_PUBLIC_SET", [325, 324, 324, 329, 328, 329, 335]),
    ("T_READONLY", [326, 325, 325, 330, 329, 330, 336]),
    ("T_VAR", [327, 326, 326, 331, 330, 331, 337]),
    ("T_UNSET", [328, 327, 327, 332, 331, 332, 338]),
    ("T_ISSET", [329, 328, 328, 333, 332, 333, 339]),
    ("T_EMPTY", [330, 329, 329, 334, 333, 334, 340]),
    ("T_HALT_COMPILER", [331, 330, 330, 335, 334, 335, 341]),
    ("T_CLASS", [332, 331, 331, 336, 335, 336, 342]),
    ("T_TRAIT", [333, 332, 332, 337, 336, 337, 343]),
    ("T_INTERFACE", [334, 333, 333, 338, 337, 338, 344]),
    ("T_ENUM", [335, 334, 334, 339, 338, 339, 345]),
    ("T_EXTENDS", [336, 335, 335, 340, 339, 340, 346]),
    ("T_IMPLEMENTS", [337, 336, 336, 341, 340, 341, 347]),
    ("T_NAMESPACE", [338, 337, 337, 342, 341, 342, 348]),
    ("T_LIST", [339, 338, 338, 343, 342, 343, 349]),
    ("T_ARRAY", [340, 339, 339, 344, 343, 344, 350]),
    ("T_CALLABLE", [341, 340, 340, 345, 344, 345, 351]),
    ("T_LINE", [342, 341, 341, 346, 345, 346, 352]),
    ("T_FILE", [343, 342, 342, 347, 346, 347, 353]),
    ("T_DIR", [344, 343, 343, 348, 347, 348, 354]),
    ("T_CLASS_C", [345, 344, 344, 349, 348, 349, 355]),
    ("T_TRAIT_C", [346, 345, 345, 350, 349, 350, 356]),
    ("T_METHOD_C", [347, 346, 346, 351, 350, 351, 357]),
    ("T_FUNC_C", [348, 347, 347, 352, 351, 352, 358]),
    ("T_PROPERTY_C", [349, 348, 348, 353, 352, 353, 359]),
    ("T_NS_C", [350, 349, 349, 354, 353, 354, 360]),
    ("T_ATTRIBUTE", [351, 350, 350, 355, 354, 355, 361]),
    ("T_PLUS_EQUAL", [352, 351, 351, 356, 355, 356, 362]),
    ("T_MINUS_EQUAL", [353, 352, 352, 357, 356, 357, 363]),
    ("T_MUL_EQUAL", [354, 353, 353, 358, 357, 358, 364]),
    ("T_DIV_EQUAL", [355, 354, 354, 359, 358, 359, 365]),
    ("T_CONCAT_EQUAL", [356, 355, 355, 360, 359, 360, 366]),
    ("T_MOD_EQUAL", [357, 356, 356, 361, 360, 361, 367]),
    ("T_AND_EQUAL", [358, 357, 357, 362, 361, 362, 368]),
    ("T_OR_EQUAL", [359, 358, 358, 363, 362, 363, 369]),
    ("T_XOR_EQUAL", [360, 359, 359, 364, 363, 364, 370]),
    ("T_SL_EQUAL", [361, 360, 360, 365, 364, 365, 371]),
    ("T_SR_EQUAL", [362, 361, 361, 366, 365, 366, 372]),
    ("T_COALESCE_EQUAL", [363, 362, 362, 367, 366, 367, 373]),
    ("T_BOOLEAN_OR", [364, 363, 363, 368, 367, 368, 374]),
    ("T_BOOLEAN_AND", [365, 364, 364, 369, 368, 369, 375]),
    ("T_IS_EQUAL", [366, 365, 365, 370, 369, 370, 376]),
    ("T_IS_NOT_EQUAL", [367, 366, 366, 371, 370, 371, 377]),
    ("T_IS_IDENTICAL", [368, 367, 367, 372, 371, 372, 378]),
    ("T_IS_NOT_IDENTICAL", [369, 368, 368, 373, 372, 373, 379]),
    ("T_IS_SMALLER_OR_EQUAL", [370, 369, 369, 374, 373, 374, 380]),
    ("T_IS_GREATER_OR_EQUAL", [371, 370, 370, 375, 374, 375, 381]),
    ("T_SPACESHIP", [372, 371, 371, 376, 375, 376, 382]),
    ("T_SL", [373, 372, 372, 377, 376, 377, 383]),
    ("T_SR", [374, 373, 373, 378, 377, 378, 384]),
    ("T_INC", [375, 374, 374, 379, 378, 379, 385]),
    ("T_DEC", [376, 375, 375, 380, 379, 380, 386]),
    ("T_INT_CAST", [377, 376, 376, 381, 380, 381, 387]),
    ("T_DOUBLE_CAST", [378, 377, 377, 382, 381, 382, 388]),
    ("T_STRING_CAST", [379, 378, 378, 383, 382, 383, 389]),
    ("T_ARRAY_CAST", [380, 379, 379, 384, 383, 384, 390]),
    ("T_OBJECT_CAST", [381, 380, 380, 385, 384, 385, 391]),
    ("T_BOOL_CAST", [382, 381, 381, 386, 385, 386, 392]),
    ("T_UNSET_CAST", [383, 382, 382, 387, 386, 387, 393]),
    ("T_VOID_CAST", [384, 383, 383, 388, 387, 388, 394]),
    ("T_OBJECT_OPERATOR", [385, 384, 384, 389, 388, 389, 395]),
    ("T_NULLSAFE_OBJECT_OPERATOR", [386, 385, 385, 390, 389, 390, 396]),
    ("T_DOUBLE_ARROW", [387, 386, 386, 391, 390, 391, 397]),
    ("T_COMMENT", [388, 387, 387, 392, 391, 392, 398]),
    ("T_DOC_COMMENT", [389, 388, 388, 393, 392, 393, 399]),
    ("T_OPEN_TAG", [390, 389, 389, 394, 393, 394, 400]),
    ("T_OPEN_TAG_WITH_ECHO", [391, 390, 390, 395, 394, 395, 401]),
    ("T_CLOSE_TAG", [392, 391, 391, 396, 395, 396, 402]),
    ("T_WHITESPACE", [393, 392, 392, 397, 396, 397, 403]),
    ("T_START_HEREDOC", [394, 393, 393, 398, 397, 398, 404]),
    ("T_END_HEREDOC", [395, 394, 394, 399, 398, 399, 405]),
    ("T_DOLLAR_OPEN_CURLY_BRACES", [396, 395, 395, 400, 399, 400, 406]),
    ("T_CURLY_OPEN", [397, 396, 396, 401, 400, 401, 407]),
    ("T_PAAMAYIM_NEKUDOTAYIM", [398, 397, 397, 402, 401, 402, 408]),
    ("T_DOUBLE_COLON", [398, 397, 397, 402, 401, 402, 408]),
    ("T_NS_SEPARATOR", [399, 398, 398, 403, 402, 403, 409]),
    ("T_ELLIPSIS", [400, 399, 399, 404, 403, 404, 410]),
    ("T_COALESCE", [401, 400, 400, 405, 404, 405, 411]),
    ("T_POW", [402, 401, 401, 406, 405, 406, 412]),
    ("T_POW_EQUAL", [403, 402, 402, 407, 406, 407, 413]),
    ("T_PIPE", [404, 403, 403, 408, 407, 408, 414]),
    ("T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG", [405, 404, 404, 409, 408, 409, 415]),
    ("T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG", [406, 405, 405, 410, 409, 410, 416]),
    ("T_BAD_CHARACTER", [407, 406, 406, 411, 410, 411, 417]),
];

thread_local! {
    /// The profile the binary embedding this bridge was compiled for.
    static EVAL_PHP_VERSION_ID: Cell<u32> = const { Cell::new(DEFAULT_EVAL_PHP_VERSION_ID) };

    /// Whether the embedding binary was compiled `--web`, which decides `PHP_SAPI`.
    static EVAL_WEB_SAPI: Cell<bool> = const { Cell::new(false) };
}

/// Selects the SAPI eval reports on the current thread, mirroring the compile mode.
///
/// Generated code forwards `--web` through this exactly as it forwards the version profile.
/// Without it a `--web` binary reported `cli-server` natively and `cli` from inside `eval()`,
/// and Symfony's DUMPED container -- which is interpreted -- decides web-versus-console mode
/// with `in_array(PHP_SAPI, ['cli', 'phpdbg', 'embed'], true)`. Reading `cli` there put a web
/// request on the console path: the container wired `CliErrorRenderer`, whose `render()` writes
/// through a stream it then failed to open, so every request after the first answered 500.
pub(crate) fn set_eval_web_sapi(web: bool) {
    EVAL_WEB_SAPI.with(|cell| cell.set(web));
}

/// Whether a leading `#!` line is removed from every file this binary parses, as PHP's CLI does.
///
/// KEEP IN SYNC with `elephc::sapi::skips_leading_shebang` in the compiler. The interpreter
/// re-parses files the compiler already saw -- Symfony Runtime re-requires the entry script
/// through `$_SERVER['SCRIPT_FILENAME']` -- so the two answering differently prints a console
/// entry point's shebang exactly once, which is how this was found.
pub(crate) fn eval_skips_leading_shebang() -> bool {
    !EVAL_WEB_SAPI.with(Cell::get)
}

/// Returns `PHP_SAPI` for the compile mode active on the current thread.
///
/// KEEP IN SYNC with `crate::web_prelude::sapi_name()` in the compiler.
pub(crate) fn eval_php_sapi() -> &'static str {
    if EVAL_WEB_SAPI.with(Cell::get) {
        "cli-server"
    } else {
        "cli"
    }
}

/// Selects the PHP profile eval reports on the current thread.
///
/// Ids outside [`EVAL_PHP_PROFILES`] leave the current profile untouched.
pub(crate) fn set_eval_php_version_id(id: u32) {
    if EVAL_PHP_PROFILES.iter().any(|(known, _)| *known == id) {
        EVAL_PHP_VERSION_ID.with(|cell| cell.set(id));
    }
}

/// Returns `PHP_VERSION_ID` for the profile active on the current thread.
pub(crate) fn eval_php_version_id() -> u32 {
    EVAL_PHP_VERSION_ID.with(Cell::get)
}

/// Returns `PHP_VERSION` for the profile active on the current thread.
pub(crate) fn eval_php_version_string() -> &'static str {
    let id = eval_php_version_id();
    EVAL_PHP_PROFILES
        .iter()
        .find(|(known, _)| *known == id)
        .map_or("8.5.0", |(_, spelling)| *spelling)
}

/// Returns `PHP_MINOR_VERSION` for the profile active on the current thread.
///
/// `PHP_MAJOR_VERSION`, `PHP_RELEASE_VERSION` and `PHP_EXTRA_VERSION` have no
/// equivalent here because they are invariant across every supported profile:
/// `8`, `0` and `""` from 8.2 through 8.5.
pub(crate) fn eval_php_minor_version() -> i64 {
    i64::from((eval_php_version_id() / 100) % 100)
}

/// Returns one tokenizer identifier for the active PHP profile.
pub(crate) fn eval_token_id(name: &str) -> Option<i64> {
    let profile_index = match eval_php_version_id() {
        80000 => 0,
        80100 => 1,
        80200 => 2,
        80300 => 3,
        80400 => 4,
        80500 => 5,
        80600 => 6,
        _ => return None,
    };
    EVAL_TOKEN_IDS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, values)| values[profile_index])
}

/// Returns the profile-specific PHP `E_ALL` and default error-reporting mask.
pub(crate) fn eval_all_error_mask() -> i64 {
    if eval_php_version_id() >= 80_500 {
        30_719
    } else {
        32_767
    }
}

/// Returns the Zend Engine profile corresponding to the active PHP minor version.
pub(crate) fn eval_zend_version_string() -> String {
    format!("4.{}.0", eval_php_minor_version())
}

/// RAII guard restoring the previous profile on drop.
///
/// Test fixtures hold one of these instead of calling [`set_eval_php_version_id`]
/// in pairs, so a panicking assertion cannot leak a profile into later fixtures
/// on the same thread.
#[cfg(test)]
pub(crate) struct EvalPhpProfileGuard {
    previous: u32,
}

#[cfg(test)]
impl Drop for EvalPhpProfileGuard {
    /// Restores the profile captured when the guard was created.
    fn drop(&mut self) {
        set_eval_php_version_id(self.previous);
    }
}

/// Selects `id` as the eval profile and returns a guard restoring the previous one.
#[cfg(test)]
pub(crate) fn scoped_profile(id: u32) -> EvalPhpProfileGuard {
    let previous = eval_php_version_id();
    set_eval_php_version_id(id);
    EvalPhpProfileGuard { previous }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default profile is the newest one, so linking this archive without
    /// elephc's codegen observes the behaviour that shipped before the setter.
    #[test]
    fn default_profile_is_the_newest_one() {
        assert_eq!(eval_php_version_id(), 80500);
        assert_eq!(eval_php_version_string(), "8.5.0");
        assert_eq!(eval_php_minor_version(), 5);
    }

    /// Every supported profile round-trips through the setter.
    #[test]
    fn every_supported_profile_reports_its_own_spelling() {
        for (id, spelling) in EVAL_PHP_PROFILES {
            let _guard = scoped_profile(*id);
            assert_eq!(eval_php_version_id(), *id);
            assert_eq!(eval_php_version_string(), *spelling);
            assert_eq!(eval_php_minor_version(), i64::from((*id / 100) % 100));
        }
    }

    /// Every supported profile exposes the tokenizer identifier from its parser table.
    #[test]
    fn heredoc_token_identifier_follows_the_active_profile() {
        for ((id, _), expected) in EVAL_PHP_PROFILES
            .iter()
            .zip([394, 393, 393, 398, 397, 398, 404])
        {
            let _guard = scoped_profile(*id);
            assert_eq!(eval_token_id("T_START_HEREDOC"), Some(expected));
        }
    }

    /// Every supported profile exposes its whitespace-token parser identifier.
    #[test]
    fn whitespace_token_identifier_follows_the_active_profile() {
        for ((id, _), expected) in EVAL_PHP_PROFILES
            .iter()
            .zip([393, 392, 392, 397, 396, 397, 403])
        {
            let _guard = scoped_profile(*id);
            assert_eq!(eval_token_id("T_WHITESPACE"), Some(expected));
        }
    }

    /// The eval tokenizer registry contains no duplicate names and preserves aliases.
    #[test]
    fn tokenizer_registry_is_unique_and_preserves_aliases() {
        let mut names = EVAL_TOKEN_IDS.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
        assert_eq!(eval_token_id("T_DOUBLE_COLON"), eval_token_id("T_PAAMAYIM_NEKUDOTAYIM"));
    }

    /// Verifies every supported PHP profile maps to its Zend Engine 4 minor.
    #[test]
    fn zend_version_tracks_the_php_profile_minor() {
        for (id, _) in EVAL_PHP_PROFILES {
            let _guard = scoped_profile(*id);
            assert_eq!(
                eval_zend_version_string(),
                format!("4.{}.0", (id / 100) % 100)
            );
        }
    }

    /// An unsupported id leaves the active profile alone rather than inventing
    /// a version string for it.
    #[test]
    fn an_unsupported_id_is_ignored() {
        let _guard = scoped_profile(80200);
        set_eval_php_version_id(90000);
        assert_eq!(eval_php_version_id(), 80200);
        assert_eq!(eval_php_version_string(), "8.2.0");
    }

    /// The spelling table agrees with the id it is keyed by, so a typo in one
    /// column cannot silently ship.
    #[test]
    fn spellings_agree_with_their_ids() {
        for (id, spelling) in EVAL_PHP_PROFILES {
            let expected = format!("{}.{}.0", id / 10000, (id / 100) % 100);
            assert_eq!(*spelling, expected);
        }
    }
}
