//! Purpose:
//! PHP-compatible XML attribute-name validation and DOM exceptions.
//!
//! Called from:
//! - The DOM HTML prelude before either stored or native attribute mutation.
//!
//! Key details:
//! - XML 1.0 Name rules are checked without a regex or native-library dependency.
//! - UTF-8 decoding rejects overlong, surrogate and out-of-range sequences.

/// Shared validation for HTML-only and XML-backed legacy DOM attribute writes.
pub(super) const SRC: &str = r###"
class DOMException extends Exception {}

function __elephc_dom_escape_namespace(string $value): string {
    $value = str_replace('&', '&amp;', $value);
    $value = str_replace('<', '&lt;', $value);
    $value = str_replace('>', '&gt;', $value);
    $value = str_replace('"', '&quot;', $value);
    $value = str_replace("\r", '&#13;', $value);
    $value = str_replace("\n", '&#10;', $value);
    return str_replace("\t", '&#9;', $value);
}

function __elephc_dom_valid_attribute_name(string $name): bool {
    $length = strlen($name);
    if ($length === 0) { return false; }
    for ($i = 0; $i < $length;) {
        $lead = ord(substr($name, $i, 1));
        $width = 1;
        $code = $lead;
        if ($lead >= 128) {
            if ($lead >= 194 && $lead <= 223) { $width = 2; $code = $lead & 31; }
            elseif ($lead >= 224 && $lead <= 239) { $width = 3; $code = $lead & 15; }
            elseif ($lead >= 240 && $lead <= 244) { $width = 4; $code = $lead & 7; }
            else { return false; }
            if ($i + $width > $length) { return false; }
            for ($j = 1; $j < $width; $j++) {
                $next = ord(substr($name, $i + $j, 1));
                if ($next < 128 || $next > 191) { return false; }
                $code = ($code << 6) | ($next & 63);
            }
            if (($width === 2 && $code < 128) || ($width === 3 && $code < 2048)
                || ($width === 4 && $code < 65536) || ($code >= 55296 && $code <= 57343)
                || $code > 1114111) { return false; }
        }
        $start = $code === 58 || $code === 95
            || ($code >= 65 && $code <= 90) || ($code >= 97 && $code <= 122)
            || ($code >= 192 && $code <= 214) || ($code >= 216 && $code <= 246)
            || ($code >= 248 && $code <= 767) || ($code >= 880 && $code <= 893)
            || ($code >= 895 && $code <= 8191) || ($code >= 8204 && $code <= 8205)
            || ($code >= 8304 && $code <= 8591) || ($code >= 11264 && $code <= 12271)
            || ($code >= 12289 && $code <= 55295) || ($code >= 63744 && $code <= 64975)
            || ($code >= 65008 && $code <= 65533) || ($code >= 65536 && $code <= 983039);
        if (!$start && ($i === 0 || !($code === 45 || $code === 46 || $code === 183
            || ($code >= 48 && $code <= 57) || ($code >= 768 && $code <= 879)
            || ($code >= 8255 && $code <= 8256)))) { return false; }
        $i = $i + $width;
    }
    return true;
}
"###;
