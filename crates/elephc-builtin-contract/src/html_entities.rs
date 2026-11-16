//! Purpose:
//! The named character references `htmlspecialchars(..., double_encode: false)` keeps, and the
//! rule deciding whether one `&...;` is kept, shared by the compiler prelude and the interpreter.
//!
//! Called from:
//! - `elephc::htmlspecialchars_prelude`, which builds the AOT helper from [`HTML401_ENTITY_NAMES`].
//! - `elephc_magician`'s `htmlspecialchars` hook, which calls [`html_entity_is_kept`].
//!
//! Key details:
//! - The table is php's own HTML 4.01 table (`get_html_translation_table(HTML_ENTITIES,
//!   ENT_QUOTES | ENT_HTML401)` on php 8.5.10, minus the numeric `&#039;`): 252 names, compared
//!   case-sensitively, so `&AMP;` is not kept and `&apos;` is not an HTML 4.01 entity.
//! - The doctype comes from the flags: `ENT_XML1` keeps only `amp lt gt quot apos`, `ENT_XHTML`
//!   adds `apos` to the HTML 4.01 table, and `ENT_HTML5` is approximated by the XHTML set (php's
//!   HTML 5 table has over two thousand names).

/// `ENT_XML1`, `ENT_XHTML` and `ENT_HTML5` share these two doctype bits.
const DOCTYPE_MASK: i64 = 48;
/// The `ENT_XML1` doctype value.
const DOCTYPE_XML1: i64 = 16;
/// The `ENT_XHTML` doctype value.
const DOCTYPE_XHTML: i64 = 32;
/// The `ENT_DISALLOWED` flag: a reference to a code point the doctype disallows is encoded.
const ENT_DISALLOWED: i64 = 128;
/// The `ENT_HTML401` doctype value (no doctype bits set).
const DOCTYPE_HTML401: i64 = 0;

/// The named references the HTML 4.01 doctype defines, sorted.
pub const HTML401_ENTITY_NAMES: [&str; 252] = [
    "AElig", "Aacute", "Acirc", "Agrave", "Alpha", "Aring", "Atilde", "Auml", "Beta", "Ccedil",
    "Chi", "Dagger", "Delta", "ETH", "Eacute", "Ecirc", "Egrave", "Epsilon", "Eta", "Euml",
    "Gamma", "Iacute", "Icirc", "Igrave", "Iota", "Iuml", "Kappa", "Lambda", "Mu", "Ntilde", "Nu",
    "OElig", "Oacute", "Ocirc", "Ograve", "Omega", "Omicron", "Oslash", "Otilde", "Ouml", "Phi",
    "Pi", "Prime", "Psi", "Rho", "Scaron", "Sigma", "THORN", "Tau", "Theta", "Uacute", "Ucirc",
    "Ugrave", "Upsilon", "Uuml", "Xi", "Yacute", "Yuml", "Zeta", "aacute", "acirc", "acute",
    "aelig", "agrave", "alefsym", "alpha", "amp", "and", "ang", "aring", "asymp", "atilde", "auml",
    "bdquo", "beta", "brvbar", "bull", "cap", "ccedil", "cedil", "cent", "chi", "circ", "clubs",
    "cong", "copy", "crarr", "cup", "curren", "dArr", "dagger", "darr", "deg", "delta", "diams",
    "divide", "eacute", "ecirc", "egrave", "empty", "emsp", "ensp", "epsilon", "equiv", "eta",
    "eth", "euml", "euro", "exist", "fnof", "forall", "frac12", "frac14", "frac34", "frasl",
    "gamma", "ge", "gt", "hArr", "harr", "hearts", "hellip", "iacute", "icirc", "iexcl", "igrave",
    "image", "infin", "int", "iota", "iquest", "isin", "iuml", "kappa", "lArr", "lambda", "lang",
    "laquo", "larr", "lceil", "ldquo", "le", "lfloor", "lowast", "loz", "lrm", "lsaquo", "lsquo",
    "lt", "macr", "mdash", "micro", "middot", "minus", "mu", "nabla", "nbsp", "ndash", "ne", "ni",
    "not", "notin", "nsub", "ntilde", "nu", "oacute", "ocirc", "oelig", "ograve", "oline", "omega",
    "omicron", "oplus", "or", "ordf", "ordm", "oslash", "otilde", "otimes", "ouml", "para", "part",
    "permil", "perp", "phi", "pi", "piv", "plusmn", "pound", "prime", "prod", "prop", "psi",
    "quot", "rArr", "radic", "rang", "raquo", "rarr", "rceil", "rdquo", "real", "reg", "rfloor",
    "rho", "rlm", "rsaquo", "rsquo", "sbquo", "scaron", "sdot", "sect", "shy", "sigma", "sigmaf",
    "sim", "spades", "sub", "sube", "sum", "sup", "sup1", "sup2", "sup3", "supe", "szlig", "tau",
    "there4", "theta", "thetasym", "thinsp", "thorn", "tilde", "times", "trade", "uArr", "uacute",
    "uarr", "ucirc", "ugrave", "uml", "upsih", "upsilon", "uuml", "weierp", "xi", "yacute", "yen",
    "yuml", "zeta", "zwj", "zwnj",
];

/// The only named references the XML 1.0 doctype defines.
pub const XML1_ENTITY_NAMES: [&str; 5] = ["amp", "lt", "gt", "quot", "apos"];

/// Returns whether `&body;` is left alone by `htmlspecialchars()` with `double_encode` false.
///
/// Transcribes php-src's `process_numeric_entity` / `process_named_entity_html`: a numeric
/// reference is `#` plus decimal digits, or `#x`/`#X` plus hex digits, naming a code point the
/// doctype allows; a named one is ASCII letters and digits found in the doctype's table.
pub fn html_entity_is_kept(body: &[u8], flags: i64) -> bool {
    let doctype = flags & DOCTYPE_MASK;
    if let Some(numeric) = body.strip_prefix(b"#") {
        let (digits, radix) = match numeric.first() {
            Some(b'x') | Some(b'X') => (&numeric[1..], 16),
            _ => (numeric, 10),
        };
        if digits.is_empty() || !digits.iter().all(|byte| (*byte as char).is_digit(radix)) {
            return false;
        }
        let significant: &[u8] = {
            let first = digits.iter().position(|byte| *byte != b'0').unwrap_or(digits.len());
            &digits[first..]
        };
        if significant.len() > 8 {
            return false;
        }
        let text = std::str::from_utf8(significant).unwrap_or("0");
        let code_point = if text.is_empty() {
            0
        } else {
            u32::from_str_radix(text, radix).unwrap_or(u32::MAX)
        };
        return numeric_code_point_is_allowed(code_point, flags);
    }
    if body.is_empty() || !body.iter().all(u8::is_ascii_alphanumeric) {
        return false;
    }
    let Ok(name) = std::str::from_utf8(body) else {
        return false;
    };
    if doctype == DOCTYPE_XML1 {
        return XML1_ENTITY_NAMES.contains(&name);
    }
    if name == "apos" {
        return doctype != DOCTYPE_HTML401;
    }
    HTML401_ENTITY_NAMES.binary_search(&name).is_ok()
}

/// php-src's `process_numeric_entity` bound plus `numeric_entity_is_allowed`: any code point up
/// to U+10FFFF is kept, and only under `ENT_DISALLOWED` is the doctype's own rule applied on top.
///
/// HTML 4.01 disallows nothing more; XHTML and XML 1.0 follow the XML 1.0 `Char` production;
/// HTML5 allows printable ASCII, tab, line feed, form feed and U+00A0 upward minus the
/// noncharacters (U+FDD0..U+FDEF and the last two code points of each plane).
fn numeric_code_point_is_allowed(code_point: u32, flags: i64) -> bool {
    if code_point > 0x10FFFF {
        return false;
    }
    if flags & ENT_DISALLOWED == 0 {
        return true;
    }
    match flags & DOCTYPE_MASK {
        DOCTYPE_XML1 | DOCTYPE_XHTML => {
            (0x20..=0xD7FF).contains(&code_point)
                || matches!(code_point, 0x09 | 0x0A | 0x0D)
                || ((0xE000..=0x10FFFF).contains(&code_point)
                    && code_point != 0xFFFE
                    && code_point != 0xFFFF)
        }
        DOCTYPE_HTML401 => true,
        _ => {
            (0x20..=0x7E).contains(&code_point)
                || matches!(code_point, 0x09 | 0x0A | 0x0C)
                || (code_point >= 0xA0
                    && (code_point & 0xFFFF) < 0xFFFE
                    && !(0xFDD0..=0xFDEF).contains(&code_point))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table must stay sorted: `html_entity_is_kept` binary-searches it.
    #[test]
    fn html401_table_is_sorted_for_binary_search() {
        assert!(HTML401_ENTITY_NAMES.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// Every case php 8.5.10 prints for `htmlspecialchars(…, ENT_QUOTES | ENT_SUBSTITUTE,
    /// 'UTF-8', false)`.
    #[test]
    fn keeps_exactly_what_php_keeps_under_html401() {
        let flags = 3 | 8;
        for kept in ["nbsp", "Eacute", "eacute", "euro", "#39", "#x27", "#1114111", "#0"] {
            assert!(html_entity_is_kept(kept.as_bytes(), flags), "{kept}");
        }
        for encoded in ["apos", "foo", "#xZZ", "#1114112", "", "#", "AMP", "#65 x", "#-1"] {
            assert!(!html_entity_is_kept(encoded.as_bytes(), flags), "{encoded}");
        }
    }

    /// The XML doctype keeps `apos` but not the HTML names.
    #[test]
    fn xml1_keeps_only_its_five_names() {
        assert!(html_entity_is_kept(b"apos", 16 | 3));
        assert!(!html_entity_is_kept(b"nbsp", 16 | 3));
    }

    /// A numeric reference is subject to the doctype's rule only under `ENT_DISALLOWED`, as in
    /// php 8.5.10 (`htmlspecialchars("&#1;", ENT_XML1, 'UTF-8', false)` keeps it).
    #[test]
    fn numeric_references_are_checked_only_under_ent_disallowed() {
        assert!(html_entity_is_kept(b"#1", 16));
        assert!(html_entity_is_kept(b"#xD800", 16));
        assert!(!html_entity_is_kept(b"#1", 16 | 128));
        assert!(!html_entity_is_kept(b"#xFFFE", 16 | 128));
        assert!(html_entity_is_kept(b"#x80", 16 | 128));
        assert!(!html_entity_is_kept(b"#1", 48 | 128));
        assert!(html_entity_is_kept(b"#xD800", 48 | 128));
        assert!(!html_entity_is_kept(b"#xD", 48 | 128));
        assert!(!html_entity_is_kept(b"#x9F", 48 | 128));
        assert!(!html_entity_is_kept(b"#xFDD0", 48 | 128));
        assert!(html_entity_is_kept(b"#x9", 48 | 128));
        assert!(!html_entity_is_kept(b"#1", 32 | 128));
        assert!(html_entity_is_kept(b"#1", 128));
        assert!(!html_entity_is_kept(b"#x110000", 128));
    }
}
