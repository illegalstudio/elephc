//! Purpose:
//! Defines PHP 8.5's Windows charset-name to code-page mapping for the
//! `sapi_windows_cp_conv()` AOT and eval implementations.
//!
//! Called from:
//! - `src/codegen/lower_inst/builtins/system.rs` for literal AOT selectors.
//! - `elephc-magician` for runtime eval selectors.
//!
//! Key details:
//! - Entries and aliases are pinned to php-src 8.5.6 `win32/cp_enc_map.c`.
//! - Lookup is ASCII-case-insensitive, like `php_win32_cp_get_by_enc()`.
//! - Numeric PHP arguments are validated separately as code-page identifiers;
//!   a numeric-looking string is accepted only when php-src lists it as an alias.

/// One Windows code page and every charset spelling accepted by php-src.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsCodepage {
    pub id: u32,
    pub names: &'static [&'static str],
}

/// PHP 8.5.6's generated Windows code-page catalog, in lookup order.
pub static WINDOWS_CODEPAGES: &[WindowsCodepage] = &[
    WindowsCodepage { id: 37, names: &["IBM037"] },
    WindowsCodepage { id: 437, names: &["IBM437"] },
    WindowsCodepage { id: 500, names: &["IBM500"] },
    WindowsCodepage { id: 708, names: &["ASMO-708"] },
    WindowsCodepage { id: 720, names: &["DOS-720"] },
    WindowsCodepage { id: 737, names: &["ibm737"] },
    WindowsCodepage { id: 775, names: &["ibm775"] },
    WindowsCodepage { id: 850, names: &["ibm850", "850", "CP850", "CSPC850MULTILINGUAL"] },
    WindowsCodepage { id: 852, names: &["ibm852"] },
    WindowsCodepage { id: 855, names: &["IBM855"] },
    WindowsCodepage { id: 857, names: &["ibm857"] },
    WindowsCodepage { id: 858, names: &["IBM00858"] },
    WindowsCodepage { id: 860, names: &["IBM860"] },
    WindowsCodepage { id: 861, names: &["ibm861"] },
    WindowsCodepage { id: 862, names: &["DOS-862", "862", "CP862", "IBM862", "CSPC862LATINHEBREW"] },
    WindowsCodepage { id: 863, names: &["IBM863"] },
    WindowsCodepage { id: 864, names: &["IBM864"] },
    WindowsCodepage { id: 865, names: &["IBM865"] },
    WindowsCodepage { id: 866, names: &["cp866", "866", "IBM866", "CSIBM866"] },
    WindowsCodepage { id: 869, names: &["ibm869"] },
    WindowsCodepage { id: 870, names: &["IBM870"] },
    WindowsCodepage { id: 874, names: &["windows-874", "CP874"] },
    WindowsCodepage { id: 875, names: &["cp875"] },
    WindowsCodepage { id: 932, names: &["shift_jis", "CP932", "MS_KANJI", "CSSHIFTJIS"] },
    WindowsCodepage { id: 936, names: &["gb2312", "GBK", "CP936", "MS936", "WINDOWS-936"] },
    WindowsCodepage { id: 949, names: &["ks_c_5601-1987", "CP949", "UHC"] },
    WindowsCodepage { id: 950, names: &["big5", "CP950", "BIG-5"] },
    WindowsCodepage { id: 1026, names: &["IBM1026"] },
    WindowsCodepage { id: 1047, names: &["IBM01047"] },
    WindowsCodepage { id: 1140, names: &["IBM01140"] },
    WindowsCodepage { id: 1141, names: &["IBM01141"] },
    WindowsCodepage { id: 1142, names: &["IBM01142"] },
    WindowsCodepage { id: 1143, names: &["IBM01143"] },
    WindowsCodepage { id: 1144, names: &["IBM01144"] },
    WindowsCodepage { id: 1145, names: &["IBM01145"] },
    WindowsCodepage { id: 1146, names: &["IBM01146"] },
    WindowsCodepage { id: 1148, names: &["IBM01148"] },
    WindowsCodepage { id: 1149, names: &["IBM01149"] },
    WindowsCodepage { id: 1250, names: &["windows-1250", "CP1250", "MS-EE"] },
    WindowsCodepage { id: 1251, names: &["windows-1251", "CP1251", "MS-CYRL"] },
    WindowsCodepage { id: 1252, names: &["windows-1252", "CP1252", "MS-ANSI"] },
    WindowsCodepage { id: 1253, names: &["windows-1253", "CP1253", "MS-GREEK"] },
    WindowsCodepage { id: 1254, names: &["windows-1254", "CP1254", "MS-TURK"] },
    WindowsCodepage { id: 1255, names: &["windows-1255", "CP1255", "MS-HEBR"] },
    WindowsCodepage { id: 1256, names: &["windows-1256", "CP1256", "MS-ARAB"] },
    WindowsCodepage { id: 1257, names: &["windows-1257", "CP1257", "WINBALTRIM"] },
    WindowsCodepage { id: 1258, names: &["windows-1258", "CP1258"] },
    WindowsCodepage { id: 1361, names: &["Johab", "CP1361"] },
    WindowsCodepage { id: 10000, names: &["macintosh", "MAC", "MACROMAN", "CSMACINTOSH"] },
    WindowsCodepage { id: 10001, names: &["x-mac-japanese"] },
    WindowsCodepage { id: 10002, names: &["x-mac-chinesetrad"] },
    WindowsCodepage { id: 10003, names: &["x-mac-korean"] },
    WindowsCodepage { id: 10004, names: &["x-mac-arabic", "MACARABIC"] },
    WindowsCodepage { id: 10005, names: &["x-mac-hebrew", "MACHEBREW"] },
    WindowsCodepage { id: 10006, names: &["x-mac-greek", "MACGREEK"] },
    WindowsCodepage { id: 10007, names: &["x-mac-cyrillic", "MACCYRILLIC"] },
    WindowsCodepage { id: 10008, names: &["x-mac-chinesesimp"] },
    WindowsCodepage { id: 10010, names: &["x-mac-romanian", "MACROMANIA"] },
    WindowsCodepage { id: 10017, names: &["x-mac-ukrainian", "MACUKRAINE"] },
    WindowsCodepage { id: 10021, names: &["x-mac-thai", "MACTHAI"] },
    WindowsCodepage { id: 10029, names: &["x-mac-ce", "MACCENTRALEUROPE"] },
    WindowsCodepage { id: 10079, names: &["x-mac-icelandic", "MACICELAND"] },
    WindowsCodepage { id: 10081, names: &["x-mac-turkish", "MACTURKISH"] },
    WindowsCodepage { id: 10082, names: &["x-mac-croatian", "MACCROATIAN"] },
    WindowsCodepage { id: 20000, names: &["x-Chinese_CNS"] },
    WindowsCodepage { id: 20001, names: &["x-cp20001"] },
    WindowsCodepage { id: 20002, names: &["x_Chinese-Eten"] },
    WindowsCodepage { id: 20003, names: &["x-cp20003"] },
    WindowsCodepage { id: 20004, names: &["x-cp20004"] },
    WindowsCodepage { id: 20005, names: &["x-cp20005"] },
    WindowsCodepage { id: 20105, names: &["x-IA5"] },
    WindowsCodepage { id: 20106, names: &["x-IA5-German"] },
    WindowsCodepage { id: 20107, names: &["x-IA5-Swedish"] },
    WindowsCodepage { id: 20108, names: &["x-IA5-Norwegian"] },
    WindowsCodepage { id: 20127, names: &["us-ascii"] },
    WindowsCodepage { id: 20261, names: &["x-cp20261"] },
    WindowsCodepage { id: 20269, names: &["x-cp20269"] },
    WindowsCodepage { id: 20273, names: &["IBM273"] },
    WindowsCodepage { id: 20277, names: &["IBM277"] },
    WindowsCodepage { id: 20278, names: &["IBM278"] },
    WindowsCodepage { id: 20280, names: &["IBM280"] },
    WindowsCodepage { id: 20284, names: &["IBM284"] },
    WindowsCodepage { id: 20285, names: &["IBM285"] },
    WindowsCodepage { id: 20290, names: &["IBM290"] },
    WindowsCodepage { id: 20297, names: &["IBM297"] },
    WindowsCodepage { id: 20420, names: &["IBM420"] },
    WindowsCodepage { id: 20423, names: &["IBM423"] },
    WindowsCodepage { id: 20424, names: &["IBM424"] },
    WindowsCodepage { id: 20833, names: &["x-EBCDIC-KoreanExtended"] },
    WindowsCodepage { id: 20838, names: &["IBM-Thai"] },
    WindowsCodepage { id: 20866, names: &["koi8-r", "CSKOI8R"] },
    WindowsCodepage { id: 20871, names: &["IBM871"] },
    WindowsCodepage { id: 20880, names: &["IBM880"] },
    WindowsCodepage { id: 20905, names: &["IBM905"] },
    WindowsCodepage { id: 20924, names: &["IBM00924"] },
    WindowsCodepage { id: 20932, names: &["EUC-JP", "EUCJP", "EXTENDED_UNIX_CODE_PACKED_FORMAT_FOR_JAPANESE", "CSEUCPKDFMTJAPANESE"] },
    WindowsCodepage { id: 20936, names: &["x-cp20936"] },
    WindowsCodepage { id: 21025, names: &["cp1025"] },
    WindowsCodepage { id: 21866, names: &["koi8-u"] },
    WindowsCodepage { id: 28591, names: &["iso-8859-1", "CP819", "IBM819", "ISO-IR-100", "ISO8859-1", "ISO_8859-1", "ISO_8859-1:1987", "L1", "LATIN1", "CSISOLATIN1"] },
    WindowsCodepage { id: 28592, names: &["iso-8859-2", "ISO-IR-101", "ISO8859-2", "ISO_8859-2", "ISO_8859-2:1987", "L2", "LATIN2", "CSISOLATIN2"] },
    WindowsCodepage { id: 28593, names: &["iso-8859-3", "ISO-IR-109", "ISO8859-3", "ISO_8859-3", "ISO_8859-3:1988", "L3", "LATIN3", "CSISOLATIN3"] },
    WindowsCodepage { id: 28594, names: &["iso-8859-4", "ISO-IR-110", "ISO8859-4", "ISO_8859-4", "ISO_8859-4:1988", "L4", "LATIN4", "CSISOLATIN4"] },
    WindowsCodepage { id: 28595, names: &["iso-8859-5", "CYRILLIC", "ISO-IR-144", "ISO8859-5", "ISO_8859-5", "ISO_8859-5:1988", "CSISOLATINCYRILLIC"] },
    WindowsCodepage { id: 28596, names: &["iso-8859-6", "ARABIC", "ASMO-708", "ECMA-114", "ISO-IR-127", "ISO8859-6", "ISO_8859-6", "ISO_8859-6:1987", "CSISOLATINARABIC"] },
    WindowsCodepage { id: 28597, names: &["iso-8859-7", "ECMA-118", "ELOT_928", "GREEK", "GREEK8", "ISO-IR-126", "ISO8859-7", "ISO_8859-7", "ISO_8859-7:1987", "ISO_8859-7:2003", "CSISOLATINGREEK"] },
    WindowsCodepage { id: 28598, names: &["iso-8859-8", "HEBREW", "ISO-IR-138", "ISO8859-8", "ISO_8859-8", "ISO_8859-8:1988", "CSISOLATINHEBREW"] },
    WindowsCodepage { id: 28599, names: &["iso-8859-9", "ISO-IR-148", "ISO8859-9", "ISO_8859-9", "ISO_8859-9:1989", "L5", "LATIN5", "CSISOLATIN5"] },
    WindowsCodepage { id: 28603, names: &["iso-8859-13", "ISO-IR-179", "ISO8859-13", "ISO_8859-13", "L7", "LATIN7"] },
    WindowsCodepage { id: 28605, names: &["iso-8859-15", "ISO-IR-203", "ISO8859-15", "ISO_8859-15", "ISO_8859-15:1998", "LATIN-9"] },
    WindowsCodepage { id: 38598, names: &["iso-8859-8-i"] },
    WindowsCodepage { id: 50220, names: &["iso-2022-jp", "CP50220"] },
    WindowsCodepage { id: 50221, names: &["csISO2022JP", "CP50221"] },
    WindowsCodepage { id: 50222, names: &["iso-2022-jp", "CP50222"] },
    WindowsCodepage { id: 50225, names: &["iso-2022-kr", "CSISO2022KR"] },
    WindowsCodepage { id: 50227, names: &["x-cp50227"] },
    WindowsCodepage { id: 50229, names: &["x-cp50229"] },
    WindowsCodepage { id: 51949, names: &["euc-kr", "EUCKR", "CSEUCKR"] },
    WindowsCodepage { id: 52936, names: &["hz-gb-2312", "HZ"] },
    WindowsCodepage { id: 54936, names: &["GB18030", "CSGB18030"] },
    WindowsCodepage { id: 57002, names: &["x-iscii-de"] },
    WindowsCodepage { id: 57003, names: &["x-iscii-be"] },
    WindowsCodepage { id: 57004, names: &["x-iscii-ta"] },
    WindowsCodepage { id: 57005, names: &["x-iscii-te"] },
    WindowsCodepage { id: 57006, names: &["x-iscii-as"] },
    WindowsCodepage { id: 57007, names: &["x-iscii-or"] },
    WindowsCodepage { id: 57008, names: &["x-iscii-ka"] },
    WindowsCodepage { id: 57009, names: &["x-iscii-ma"] },
    WindowsCodepage { id: 57010, names: &["x-iscii-gu"] },
    WindowsCodepage { id: 57011, names: &["x-iscii-pa"] },
    WindowsCodepage { id: 65000, names: &["utf-7"] },
    WindowsCodepage { id: 65001, names: &["utf-8"] },
];

/// Looks up one php-src charset spelling.
pub fn windows_codepage_by_name(name: &str) -> Option<WindowsCodepage> {
    WINDOWS_CODEPAGES
        .iter()
        .copied()
        .find(|entry| entry.names.iter().any(|candidate| candidate.eq_ignore_ascii_case(name)))
}

/// Reports whether one numeric identifier belongs to php-src's Windows table.
pub fn windows_codepage_by_id(id: u32) -> Option<WindowsCodepage> {
    WINDOWS_CODEPAGES.iter().copied().find(|entry| entry.id == id)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins representative canonical, alias, duplicate-name, and rejection cases.

    use super::*;

    #[test]
    fn php_85_windows_codepage_aliases_match_the_generated_table() {
        assert_eq!(windows_codepage_by_name("UTF-8").unwrap().id, 65_001);
        assert_eq!(windows_codepage_by_name("ms-ansi").unwrap().id, 1_252);
        assert_eq!(windows_codepage_by_name("latin1").unwrap().id, 28_591);
        assert_eq!(windows_codepage_by_name("iso-2022-jp").unwrap().id, 50_220);
        assert!(windows_codepage_by_name("ansi").is_none());
        assert!(windows_codepage_by_name("1252").is_none());
        assert_eq!(windows_codepage_by_id(54_936).unwrap().id, 54_936);
        assert!(windows_codepage_by_id(709).is_none());
    }
}
