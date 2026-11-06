//! Purpose:
//! HTML entity decoding for the legacy libxml-compatible DOM prelude.
//!
//! Called from:
//! - `crate::dom_html_prelude::parsed_prelude()` prepends these PHP helpers.
//!
//! Key details:
//! - The case-sensitive table contains HTML4 named entities plus XML apos.
//! - Numeric references use XML character ranges and bounded Unicode accumulation.
//! - Decoding is a single pass, so an escaped ampersand cannot decode twice.
//! - Byte probes return scalars to avoid retaining cloned string-return arguments.

/// PHP helpers for context-independent HTML entity decoding (no opening PHP tag).
pub(super) const SRC: &str = r###"
function __elephc_dom_named_entity_codepoint(string $name): int {
    static $_entities = [
        "AElig" => 198,
        "Aacute" => 193,
        "Acirc" => 194,
        "Agrave" => 192,
        "Alpha" => 913,
        "Aring" => 197,
        "Atilde" => 195,
        "Auml" => 196,
        "Beta" => 914,
        "Ccedil" => 199,
        "Chi" => 935,
        "Dagger" => 8225,
        "Delta" => 916,
        "ETH" => 208,
        "Eacute" => 201,
        "Ecirc" => 202,
        "Egrave" => 200,
        "Epsilon" => 917,
        "Eta" => 919,
        "Euml" => 203,
        "Gamma" => 915,
        "Iacute" => 205,
        "Icirc" => 206,
        "Igrave" => 204,
        "Iota" => 921,
        "Iuml" => 207,
        "Kappa" => 922,
        "Lambda" => 923,
        "Mu" => 924,
        "Ntilde" => 209,
        "Nu" => 925,
        "OElig" => 338,
        "Oacute" => 211,
        "Ocirc" => 212,
        "Ograve" => 210,
        "Omega" => 937,
        "Omicron" => 927,
        "Oslash" => 216,
        "Otilde" => 213,
        "Ouml" => 214,
        "Phi" => 934,
        "Pi" => 928,
        "Prime" => 8243,
        "Psi" => 936,
        "Rho" => 929,
        "Scaron" => 352,
        "Sigma" => 931,
        "THORN" => 222,
        "Tau" => 932,
        "Theta" => 920,
        "Uacute" => 218,
        "Ucirc" => 219,
        "Ugrave" => 217,
        "Upsilon" => 933,
        "Uuml" => 220,
        "Xi" => 926,
        "Yacute" => 221,
        "Yuml" => 376,
        "Zeta" => 918,
        "aacute" => 225,
        "acirc" => 226,
        "acute" => 180,
        "aelig" => 230,
        "agrave" => 224,
        "alefsym" => 8501,
        "alpha" => 945,
        "amp" => 38,
        "and" => 8743,
        "ang" => 8736,
        "apos" => 39,
        "aring" => 229,
        "asymp" => 8776,
        "atilde" => 227,
        "auml" => 228,
        "bdquo" => 8222,
        "beta" => 946,
        "brvbar" => 166,
        "bull" => 8226,
        "cap" => 8745,
        "ccedil" => 231,
        "cedil" => 184,
        "cent" => 162,
        "chi" => 967,
        "circ" => 710,
        "clubs" => 9827,
        "cong" => 8773,
        "copy" => 169,
        "crarr" => 8629,
        "cup" => 8746,
        "curren" => 164,
        "dArr" => 8659,
        "dagger" => 8224,
        "darr" => 8595,
        "deg" => 176,
        "delta" => 948,
        "diams" => 9830,
        "divide" => 247,
        "eacute" => 233,
        "ecirc" => 234,
        "egrave" => 232,
        "empty" => 8709,
        "emsp" => 8195,
        "ensp" => 8194,
        "epsilon" => 949,
        "equiv" => 8801,
        "eta" => 951,
        "eth" => 240,
        "euml" => 235,
        "euro" => 8364,
        "exist" => 8707,
        "fnof" => 402,
        "forall" => 8704,
        "frac12" => 189,
        "frac14" => 188,
        "frac34" => 190,
        "frasl" => 8260,
        "gamma" => 947,
        "ge" => 8805,
        "gt" => 62,
        "hArr" => 8660,
        "harr" => 8596,
        "hearts" => 9829,
        "hellip" => 8230,
        "iacute" => 237,
        "icirc" => 238,
        "iexcl" => 161,
        "igrave" => 236,
        "image" => 8465,
        "infin" => 8734,
        "int" => 8747,
        "iota" => 953,
        "iquest" => 191,
        "isin" => 8712,
        "iuml" => 239,
        "kappa" => 954,
        "lArr" => 8656,
        "lambda" => 955,
        "lang" => 9001,
        "laquo" => 171,
        "larr" => 8592,
        "lceil" => 8968,
        "ldquo" => 8220,
        "le" => 8804,
        "lfloor" => 8970,
        "lowast" => 8727,
        "loz" => 9674,
        "lrm" => 8206,
        "lsaquo" => 8249,
        "lsquo" => 8216,
        "lt" => 60,
        "macr" => 175,
        "mdash" => 8212,
        "micro" => 181,
        "middot" => 183,
        "minus" => 8722,
        "mu" => 956,
        "nabla" => 8711,
        "nbsp" => 160,
        "ndash" => 8211,
        "ne" => 8800,
        "ni" => 8715,
        "not" => 172,
        "notin" => 8713,
        "nsub" => 8836,
        "ntilde" => 241,
        "nu" => 957,
        "oacute" => 243,
        "ocirc" => 244,
        "oelig" => 339,
        "ograve" => 242,
        "oline" => 8254,
        "omega" => 969,
        "omicron" => 959,
        "oplus" => 8853,
        "or" => 8744,
        "ordf" => 170,
        "ordm" => 186,
        "oslash" => 248,
        "otilde" => 245,
        "otimes" => 8855,
        "ouml" => 246,
        "para" => 182,
        "part" => 8706,
        "permil" => 8240,
        "perp" => 8869,
        "phi" => 966,
        "pi" => 960,
        "piv" => 982,
        "plusmn" => 177,
        "pound" => 163,
        "prime" => 8242,
        "prod" => 8719,
        "prop" => 8733,
        "psi" => 968,
        "quot" => 34,
        "rArr" => 8658,
        "radic" => 8730,
        "rang" => 9002,
        "raquo" => 187,
        "rarr" => 8594,
        "rceil" => 8969,
        "rdquo" => 8221,
        "real" => 8476,
        "reg" => 174,
        "rfloor" => 8971,
        "rho" => 961,
        "rlm" => 8207,
        "rsaquo" => 8250,
        "rsquo" => 8217,
        "sbquo" => 8218,
        "scaron" => 353,
        "sdot" => 8901,
        "sect" => 167,
        "shy" => 173,
        "sigma" => 963,
        "sigmaf" => 962,
        "sim" => 8764,
        "spades" => 9824,
        "sub" => 8834,
        "sube" => 8838,
        "sum" => 8721,
        "sup" => 8835,
        "sup1" => 185,
        "sup2" => 178,
        "sup3" => 179,
        "supe" => 8839,
        "szlig" => 223,
        "tau" => 964,
        "there4" => 8756,
        "theta" => 952,
        "thetasym" => 977,
        "thinsp" => 8201,
        "thorn" => 254,
        "tilde" => 732,
        "times" => 215,
        "trade" => 8482,
        "uArr" => 8657,
        "uacute" => 250,
        "uarr" => 8593,
        "ucirc" => 251,
        "ugrave" => 249,
        "uml" => 168,
        "upsih" => 978,
        "upsilon" => 965,
        "uuml" => 252,
        "weierp" => 8472,
        "xi" => 958,
        "yacute" => 253,
        "yen" => 165,
        "yuml" => 255,
        "zeta" => 950,
        "zwj" => 8205,
        "zwnj" => 8204,
    ];
    if (isset($_entities[$name])) {
        return $_entities[$name];
    }
    return 0;
}

function __elephc_dom_entity_utf8(int $code): string {
    if ($code < 0 || $code > 1114111
        || ($code < 32 && $code !== 9 && $code !== 10 && $code !== 13)
        || ($code >= 55296 && $code <= 57343) || $code === 65534 || $code === 65535) {
        return "";
    }
    if ($code < 128) {
        return chr($code);
    }
    if ($code < 2048) {
        return chr(192 | ($code >> 6)) . chr(128 | ($code & 63));
    }
    if ($code < 65536) {
        return chr(224 | ($code >> 12)) . chr(128 | (($code >> 6) & 63))
            . chr(128 | ($code & 63));
    }
    return chr(240 | ($code >> 18)) . chr(128 | (($code >> 12) & 63))
        . chr(128 | (($code >> 6) & 63)) . chr(128 | ($code & 63));
}

function __elephc_dom_entity_byte(string $text, int $index): int {
    if ($index < 0 || $index >= strlen($text)) {
        return -1;
    }
    $_char = $text[$index];
    return ord($_char);
}

function __elephc_dom_entity_digit(int $_ord, int $base): int {
    if ($_ord >= 48 && $_ord <= 57) {
        return $_ord - 48;
    }
    if ($base === 16 && $_ord >= 65 && $_ord <= 70) {
        return $_ord - 55;
    }
    if ($base === 16 && $_ord >= 97 && $_ord <= 102) {
        return $_ord - 87;
    }
    return -1;
}

function __elephc_dom_decode_entities(string $text): string {
    if (strpos($text, "&") === false) {
        return $text;
    }
    $_out = "";
    $_i = 0;
    $_len = strlen($text);
    while ($_i < $_len) {
        $_current = __elephc_dom_entity_byte($text, $_i);
        if ($_current !== 38) {
            $_start = $_i;
            while ($_i < $_len) {
                $_current = __elephc_dom_entity_byte($text, $_i);
                if ($_current === 38) {
                    break;
                }
                $_i++;
            }
            $_chunk = substr($text, $_start, $_i - $_start);
            $_out = $_out . $_chunk;
            continue;
        }
        $_next = __elephc_dom_entity_byte($text, $_i + 1);
        if ($_next === 35) {
            $_i = $_i + 2;
            $_base = 10;
            $_char = __elephc_dom_entity_byte($text, $_i);
            if ($_char === 120 || $_char === 88) {
                $_base = 16;
                $_i++;
            }
            $_code = 0;
            while ($_i < $_len) {
                $_current = __elephc_dom_entity_byte($text, $_i);
                $_digit = __elephc_dom_entity_digit($_current, $_base);
                if ($_digit < 0 || $_digit >= $_base) {
                    break;
                }
                // Consume overflowing references without overflowing the integer accumulator.
                if ($_code <= 1114111) {
                    $_code = $_code * $_base + $_digit;
                }
                $_i++;
            }
            $_current = __elephc_dom_entity_byte($text, $_i);
            if ($_current === 59) {
                $_i++;
            }
            $_decoded = __elephc_dom_entity_utf8($_code);
            $_out = $_out . $_decoded;
            continue;
        }
        $_end = $_i + 1;
        while ($_end < $_len) {
            $_current = __elephc_dom_entity_byte($text, $_end);
            if (!(($_current >= 65 && $_current <= 90)
                || ($_current >= 97 && $_current <= 122)
                || ($_current >= 48 && $_current <= 57)
                || $_current === 45 || $_current === 95 || $_current === 58)) {
                break;
            }
            $_end++;
        }
        $_current = __elephc_dom_entity_byte($text, $_end);
        if ($_current === 59) {
            $_name = substr($text, $_i + 1, $_end - $_i - 1);
            $_code = __elephc_dom_named_entity_codepoint($_name);
            if ($_code !== 0) {
                $_decoded = __elephc_dom_entity_utf8($_code);
                $_out = $_out . $_decoded;
                $_i = $_end + 1;
                continue;
            }
        }
        $_out = $_out . "&";
        $_i++;
    }
    return $_out;
}
"###;
