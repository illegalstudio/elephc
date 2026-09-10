//! Purpose:
//! Defines the versioned typed identity for builtin implementations callable
//! through the generated boxed-cell runtime ABI.
//!
//! Called from:
//! - Compiler runtime-wrapper emission.
//! - Magician registry assembly and runtime dispatch.
//!
//! Key details:
//! - Values are stable ABI numbers, distinct from hashed catalog `BuiltinId`s.
//! - Only builtins with an equivalent boxed-cell runtime contract are mapped.
//! - Backend support separately controls frontend availability when reference adapters are pending.

use crate::BuiltinId;

/// Stable operation identity for shared runtime engines and generated boxed-cell dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum RuntimeBuiltinId {
    /// PHP `boolval` with one argument.
    Boolval = 1,
    /// PHP `floatval` with one argument.
    Floatval = 2,
    /// PHP `intval` with the default base.
    Intval = 3,
    /// PHP `is_array`.
    IsArray = 4,
    /// PHP `is_null`.
    IsNull = 5,
    /// PHP `abs`.
    Abs = 6,
    /// PHP `ceil`.
    Ceil = 7,
    /// PHP `floor`.
    Floor = 8,
    /// PHP `sqrt`.
    Sqrt = 9,
    /// PHP `fdiv`.
    Fdiv = 10,
    /// PHP `fmod`.
    Fmod = 11,
    /// PHP `pow`.
    Pow = 12,
    /// PHP `round` with its supported optional precision.
    Round = 13,
    /// PHP byte-string `strrev`.
    Strrev = 14,
    /// PHP `array_key_exists` over boxed key and array cells.
    ArrayKeyExists = 15,
    /// PHP `ob_get_level`.
    ObGetLevel = 16,
    /// PHP `ob_get_length`.
    ObGetLength = 17,
    /// PHP `ob_clean`.
    ObClean = 18,
    /// PHP `ob_flush`.
    ObFlush = 19,
    /// PHP `ob_end_clean`.
    ObEndClean = 20,
    /// PHP `ob_end_flush`.
    ObEndFlush = 21,
    /// PHP `mb_strlen` through the shared mbstring bridge.
    MbStrlen = 22,
    /// PHP `mb_strwidth` through the shared mbstring bridge.
    MbStrwidth = 23,
    /// PHP `mb_strtoupper` through the shared mbstring bridge.
    MbStrtoupper = 24,
    /// PHP `mb_strtolower` through the shared mbstring bridge.
    MbStrtolower = 25,
    /// PHP `mb_convert_case` through the shared mbstring bridge.
    MbConvertCase = 26,
    /// PHP `mb_ucfirst` through the shared mbstring bridge.
    MbUcfirst = 27,
    /// PHP `mb_lcfirst` through the shared mbstring bridge.
    MbLcfirst = 28,
    /// PHP `mb_strimwidth` through the shared mbstring bridge.
    MbStrimwidth = 29,
    /// PHP `mb_substr` through the shared mbstring bridge.
    MbSubstr = 30,
    /// PHP `mb_strcut` through the shared mbstring bridge.
    MbStrcut = 31,
    /// PHP `mb_scrub` through the shared mbstring bridge.
    MbScrub = 32,
    /// PHP `mb_trim` through the shared mbstring bridge.
    MbTrim = 33,
    /// PHP `mb_ltrim` through the shared mbstring bridge.
    MbLtrim = 34,
    /// PHP `mb_rtrim` through the shared mbstring bridge.
    MbRtrim = 35,
    /// PHP `mb_str_pad` through the shared mbstring bridge.
    MbStrPad = 36,
    /// PHP `mb_convert_kana` through the shared mbstring bridge.
    MbConvertKana = 37,
    /// PHP `mb_substr_count` through the shared mbstring bridge.
    MbSubstrCount = 38,
    /// PHP `mb_ord` through the shared mbstring bridge.
    MbOrd = 39,
    /// PHP `mb_chr` through the shared mbstring bridge.
    MbChr = 40,
    /// PHP `mb_strpos` through the shared mbstring bridge.
    MbStrpos = 41,
    /// PHP `mb_stripos` through the shared mbstring bridge.
    MbStripos = 42,
    /// PHP `mb_strrpos` through the shared mbstring bridge.
    MbStrrpos = 43,
    /// PHP `mb_strripos` through the shared mbstring bridge.
    MbStrripos = 44,
    /// PHP `mb_strstr` through the shared mbstring bridge.
    MbStrstr = 45,
    /// PHP `mb_stristr` through the shared mbstring bridge.
    MbStristr = 46,
    /// PHP `mb_strrchr` through the shared mbstring bridge.
    MbStrrchr = 47,
    /// PHP `mb_strrichr` through the shared mbstring bridge.
    MbStrrichr = 48,
    /// PHP `mb_language` through the shared mbstring bridge.
    MbLanguage = 49,
    /// PHP `mb_internal_encoding` through the shared mbstring bridge.
    MbInternalEncoding = 50,
    /// PHP `mb_http_output` through the shared mbstring bridge.
    MbHttpOutput = 51,
    /// PHP `mb_encoding_aliases` through the shared mbstring bridge.
    MbEncodingAliases = 52,
    /// PHP `mb_str_split` through the shared mbstring bridge.
    MbStrSplit = 53,
    /// PHP `mb_preferred_mime_name` through the shared mbstring bridge.
    MbPreferredMimeName = 54,
    /// Validates strings, recursive arrays, or the request conversion-error count.
    MbCheckEncoding = 55,
    /// Reads or changes request-local character substitution.
    MbSubstituteCharacter = 56,
    /// Enumerates the shared canonical encoding catalog in PHP order.
    MbListEncodings = 57,
    /// Reads or updates the request detection list with ordered element coercion.
    MbDetectOrder = 58,
    /// Encodes numeric entities using an ordered integer conversion map.
    MbEncodeNumericentity = 59,
    /// Decodes numeric entities using an ordered integer conversion map.
    MbDecodeNumericentity = 60,
    /// Guesses a candidate encoding with request defaults and catalog-identity weighting.
    MbDetectEncoding = 61,
    /// Converts strings and recursive array keys/values through the shared codecs.
    MbConvertEncoding = 62,
    /// Decodes MIME header words into the current internal encoding.
    MbDecodeMimeheader = 63,
    /// Encodes MIME header words using request language and internal encoding.
    MbEncodeMimeheader = 64,
    /// Reads current mbstring request settings or one selected information value.
    MbGetInfo = 65,
    /// Reads recorded HTTP input identification and configured input encoding names.
    MbHttpInput = 66,
    /// Reads or changes the shared mbregex encoding independently of ordinary text encoding.
    MbRegexEncoding = 67,
    /// Reads or changes the shared mbregex defaults, returning the previous option string.
    MbRegexSetOptions = 68,
    /// Matches a raw multibyte pattern at the subject start through shared Oniguruma semantics.
    MbEregMatch = 69,
    /// PHP `mb_ereg_search_init` through the shared mbregex request engine.
    MbEregSearchInit = 70,
    /// PHP `mb_ereg_search` through the shared mbregex request engine.
    MbEregSearch = 71,
    /// PHP `mb_ereg_search_pos` through the shared mbregex request engine.
    MbEregSearchPos = 72,
    /// PHP `mb_ereg_search_regs` through the shared mbregex request engine.
    MbEregSearchRegs = 73,
    /// PHP `mb_ereg_search_getpos` through the shared mbregex request engine.
    MbEregSearchGetpos = 74,
    /// PHP `mb_ereg_search_getregs` through the shared mbregex request engine.
    MbEregSearchGetregs = 75,
    /// PHP `mb_ereg_search_setpos` through the shared mbregex request engine.
    MbEregSearchSetpos = 76,
    /// Splits multibyte strings through the shared mbregex engine.
    MbSplit = 77,
    /// Replaces multibyte regex matches through the shared request engine.
    MbEregReplace = 78,
    /// Replaces multibyte regex matches with forced case-insensitivity.
    MbEregiReplace = 79,
    /// Searches a multibyte pattern with optional capture-reference callbacks.
    MbEreg = 80,
    /// Searches without case sensitivity with optional capture-reference callbacks.
    MbEregi = 81,
    /// Parses URL-encoded input through shared detection and protected output-reference writes.
    MbParseStr = 82,
    /// Internal prelude access to shared Core and mbstring INI state.
    SharedIni = 83,
    /// Converts an output-buffer phase with protected response metadata and header publication.
    MbOutputHandler = 84,
    /// Replaces multibyte regex matches through protected PHP callbacks.
    MbEregReplaceCallback = 85,
}

/// Status returned by `__elephc_runtime_builtin_call_v1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum RuntimeBuiltinStatus {
    /// `result_out` owns one fresh boxed result cell.
    Success = 0,
    /// Arity, type, or runtime helper failure with no result ownership transfer.
    RuntimeFatal = 1,
    /// A throwable is pending in generated runtime state.
    PendingThrowable = 2,
    /// The ID or requested arity is not implemented by this ABI version.
    Unsupported = 3,
}

impl RuntimeBuiltinStatus {
    /// Decodes a raw C-ABI status, failing closed for unknown values.
    pub const fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Success),
            1 => Some(Self::RuntimeFatal),
            2 => Some(Self::PendingThrowable),
            3 => Some(Self::Unsupported),
            _ => None,
        }
    }
}

impl RuntimeBuiltinId {
    /// Every version-one runtime builtin in stable ABI order.
    pub const ALL: [Self; 85] = [
        Self::Boolval,
        Self::Floatval,
        Self::Intval,
        Self::IsArray,
        Self::IsNull,
        Self::Abs,
        Self::Ceil,
        Self::Floor,
        Self::Sqrt,
        Self::Fdiv,
        Self::Fmod,
        Self::Pow,
        Self::Round,
        Self::Strrev,
        Self::ArrayKeyExists,
        Self::ObGetLevel,
        Self::ObGetLength,
        Self::ObClean,
        Self::ObFlush,
        Self::ObEndClean,
        Self::ObEndFlush,
        Self::MbStrlen,
        Self::MbStrwidth,
        Self::MbStrtoupper,
        Self::MbStrtolower,
        Self::MbConvertCase,
        Self::MbUcfirst,
        Self::MbLcfirst,
        Self::MbStrimwidth,
        Self::MbSubstr,
        Self::MbStrcut,
        Self::MbScrub,
        Self::MbTrim,
        Self::MbLtrim,
        Self::MbRtrim,
        Self::MbStrPad,
        Self::MbConvertKana,
        Self::MbSubstrCount,
        Self::MbOrd,
        Self::MbChr,
        Self::MbStrpos,
        Self::MbStripos,
        Self::MbStrrpos,
        Self::MbStrripos,
        Self::MbStrstr,
        Self::MbStristr,
        Self::MbStrrchr,
        Self::MbStrrichr,
        Self::MbLanguage,
        Self::MbInternalEncoding,
        Self::MbHttpOutput,
        Self::MbEncodingAliases,
        Self::MbStrSplit,
        Self::MbPreferredMimeName,
        Self::MbCheckEncoding,
        Self::MbSubstituteCharacter,
        Self::MbListEncodings,
        Self::MbDetectOrder,
        Self::MbDetectEncoding,
        Self::MbConvertEncoding,
        Self::MbEncodeNumericentity,
        Self::MbDecodeNumericentity,
        Self::MbDecodeMimeheader,
        Self::MbEncodeMimeheader,
        Self::MbGetInfo,
        Self::MbHttpInput,
        Self::MbRegexEncoding,
        Self::MbRegexSetOptions,
        Self::MbEregMatch,
        Self::MbEregSearchInit,
        Self::MbEregSearch,
        Self::MbEregSearchPos,
        Self::MbEregSearchRegs,
        Self::MbEregSearchGetpos,
        Self::MbEregSearchGetregs,
        Self::MbEregSearchSetpos,
        Self::MbSplit,
        Self::MbEregReplace,
        Self::MbEregiReplace,
        Self::MbEreg,
        Self::MbEregi,
        Self::MbParseStr,
        Self::SharedIni,
        Self::MbOutputHandler,
        Self::MbEregReplaceCallback,
    ];

    /// Every operation currently implemented by the optional mbstring engine.
    pub const MBSTRING: [Self; 64] = [
        Self::MbStrlen,
        Self::MbStrwidth,
        Self::MbStrtoupper,
        Self::MbStrtolower,
        Self::MbConvertCase,
        Self::MbUcfirst,
        Self::MbLcfirst,
        Self::MbStrimwidth,
        Self::MbSubstr,
        Self::MbStrcut,
        Self::MbScrub,
        Self::MbTrim,
        Self::MbLtrim,
        Self::MbRtrim,
        Self::MbStrPad,
        Self::MbConvertKana,
        Self::MbSubstrCount,
        Self::MbOrd,
        Self::MbChr,
        Self::MbStrpos,
        Self::MbStripos,
        Self::MbStrrpos,
        Self::MbStrripos,
        Self::MbStrstr,
        Self::MbStristr,
        Self::MbStrrchr,
        Self::MbStrrichr,
        Self::MbLanguage,
        Self::MbInternalEncoding,
        Self::MbHttpOutput,
        Self::MbEncodingAliases,
        Self::MbStrSplit,
        Self::MbPreferredMimeName,
        Self::MbCheckEncoding,
        Self::MbSubstituteCharacter,
        Self::MbListEncodings,
        Self::MbDetectOrder,
        Self::MbDetectEncoding,
        Self::MbConvertEncoding,
        Self::MbEncodeNumericentity,
        Self::MbDecodeNumericentity,
        Self::MbDecodeMimeheader,
        Self::MbEncodeMimeheader,
        Self::MbGetInfo,
        Self::MbHttpInput,
        Self::MbRegexEncoding,
        Self::MbRegexSetOptions,
        Self::MbEregMatch,
        Self::MbEregSearchInit,
        Self::MbEregSearch,
        Self::MbEregSearchPos,
        Self::MbEregSearchRegs,
        Self::MbEregSearchGetpos,
        Self::MbEregSearchGetregs,
        Self::MbEregSearchSetpos,
        Self::MbSplit,
        Self::MbEregReplace,
        Self::MbEregiReplace,
        Self::MbEreg,
        Self::MbEregi,
        Self::MbParseStr,
        Self::SharedIni,
        Self::MbOutputHandler,
        Self::MbEregReplaceCallback,
    ];

    /// Reports whether this identity belongs to the shared mbstring bridge.
    pub const fn is_mbstring(self) -> bool {
        if matches!(self, Self::SharedIni | Self::MbOutputHandler) { return true; }
        if self.is_mbregex() { return true; }
        matches!(self, Self::MbRegexEncoding | Self::MbRegexSetOptions | Self::MbStrlen | Self::MbStrwidth | Self::MbStrtoupper | Self::MbStrtolower | Self::MbConvertCase | Self::MbUcfirst | Self::MbLcfirst | Self::MbStrimwidth
            | Self::MbSubstr | Self::MbStrcut | Self::MbScrub | Self::MbTrim | Self::MbLtrim | Self::MbRtrim | Self::MbStrPad | Self::MbConvertKana | Self::MbSubstrCount | Self::MbOrd | Self::MbChr | Self::MbStrpos | Self::MbStripos | Self::MbStrrpos | Self::MbStrripos | Self::MbStrstr | Self::MbStristr | Self::MbStrrchr | Self::MbStrrichr | Self::MbLanguage | Self::MbInternalEncoding | Self::MbHttpOutput | Self::MbEncodingAliases | Self::MbStrSplit | Self::MbPreferredMimeName | Self::MbCheckEncoding | Self::MbSubstituteCharacter | Self::MbListEncodings | Self::MbDetectOrder | Self::MbDetectEncoding | Self::MbConvertEncoding | Self::MbEncodeNumericentity | Self::MbDecodeNumericentity | Self::MbDecodeMimeheader | Self::MbEncodeMimeheader | Self::MbGetInfo | Self::MbHttpInput | Self::MbParseStr)
    }

    /// Identifies shared operations that require the managed Oniguruma provider.
    pub const fn is_mbregex(self) -> bool {
        matches!(self, Self::MbEregMatch | Self::MbEreg | Self::MbEregi | Self::MbSplit
            | Self::MbEregReplace | Self::MbEregiReplace | Self::MbEregReplaceCallback
            | Self::MbEregSearchInit | Self::MbEregSearch | Self::MbEregSearchPos | Self::MbEregSearchRegs | Self::MbEregSearchGetpos | Self::MbEregSearchGetregs | Self::MbEregSearchSetpos)
    }

    /// Returns the canonical shared-contract identity implemented by this ABI ID.
    pub const fn builtin_id(self) -> BuiltinId {
        let name = match self {
            Self::Boolval => "boolval",
            Self::Floatval => "floatval",
            Self::Intval => "intval",
            Self::IsArray => "is_array",
            Self::IsNull => "is_null",
            Self::Abs => "abs",
            Self::Ceil => "ceil",
            Self::Floor => "floor",
            Self::Sqrt => "sqrt",
            Self::Fdiv => "fdiv",
            Self::Fmod => "fmod",
            Self::Pow => "pow",
            Self::Round => "round",
            Self::Strrev => "strrev",
            Self::ArrayKeyExists => "array_key_exists",
            Self::ObGetLevel => "ob_get_level",
            Self::ObGetLength => "ob_get_length",
            Self::ObClean => "ob_clean",
            Self::ObFlush => "ob_flush",
            Self::ObEndClean => "ob_end_clean",
            Self::ObEndFlush => "ob_end_flush",
            Self::MbStrlen => "mb_strlen",
            Self::MbStrwidth => "mb_strwidth",
            Self::MbStrtoupper => "mb_strtoupper",
            Self::MbStrtolower => "mb_strtolower",
            Self::MbConvertCase => "mb_convert_case",
            Self::MbUcfirst => "mb_ucfirst",
            Self::MbLcfirst => "mb_lcfirst",
            Self::MbStrimwidth => "mb_strimwidth",
            Self::MbSubstr => "mb_substr",
            Self::MbStrcut => "mb_strcut",
            Self::MbScrub => "mb_scrub",
            Self::MbTrim => "mb_trim",
            Self::MbLtrim => "mb_ltrim",
            Self::MbRtrim => "mb_rtrim",
            Self::MbStrPad => "mb_str_pad",
            Self::MbConvertKana => "mb_convert_kana",
            Self::MbSubstrCount => "mb_substr_count",
            Self::MbOrd => "mb_ord",
            Self::MbChr => "mb_chr",
            Self::MbStrpos => "mb_strpos",
            Self::MbStripos => "mb_stripos",
            Self::MbStrrpos => "mb_strrpos",
            Self::MbStrripos => "mb_strripos",
            Self::MbStrstr => "mb_strstr",
            Self::MbStristr => "mb_stristr",
            Self::MbStrrchr => "mb_strrchr",
            Self::MbStrrichr => "mb_strrichr",
            Self::MbLanguage => "mb_language",
            Self::MbInternalEncoding => "mb_internal_encoding",
            Self::MbHttpOutput => "mb_http_output",
            Self::MbEncodingAliases => "mb_encoding_aliases",
            Self::MbStrSplit => "mb_str_split",
            Self::MbPreferredMimeName => "mb_preferred_mime_name",
            Self::MbCheckEncoding => "mb_check_encoding",
            Self::MbSubstituteCharacter => "mb_substitute_character",
            Self::MbListEncodings => "mb_list_encodings",
            Self::MbDetectOrder => "mb_detect_order",
            Self::MbDetectEncoding => "mb_detect_encoding",
            Self::MbConvertEncoding => "mb_convert_encoding",
            Self::MbDecodeMimeheader => "mb_decode_mimeheader",
            Self::MbEncodeMimeheader => "mb_encode_mimeheader",
            Self::MbGetInfo => "mb_get_info",
            Self::MbHttpInput => "mb_http_input",
            Self::MbRegexEncoding => "mb_regex_encoding",
            Self::MbRegexSetOptions => "mb_regex_set_options",
            Self::MbEregMatch => "mb_ereg_match",
            Self::MbEregSearchInit => "mb_ereg_search_init",
            Self::MbEregSearch => "mb_ereg_search",
            Self::MbEregSearchPos => "mb_ereg_search_pos",
            Self::MbEregSearchRegs => "mb_ereg_search_regs",
            Self::MbEregSearchGetpos => "mb_ereg_search_getpos",
            Self::MbEregSearchGetregs => "mb_ereg_search_getregs",
            Self::MbEregSearchSetpos => "mb_ereg_search_setpos",
            Self::MbSplit => "mb_split",
            Self::MbEregReplace => "mb_ereg_replace",
            Self::MbEregiReplace => "mb_eregi_replace",
            Self::MbEreg => "mb_ereg",
            Self::MbEregi => "mb_eregi",
            Self::MbParseStr => "mb_parse_str",
            Self::SharedIni => "__elephc_shared_ini",
            Self::MbOutputHandler => "mb_output_handler",
            Self::MbEregReplaceCallback => "mb_ereg_replace_callback",
            Self::MbEncodeNumericentity => "mb_encode_numericentity",
            Self::MbDecodeNumericentity => "mb_decode_numericentity",
        };
        BuiltinId::from_canonical_name(name)
    }

    /// Returns the stable integer passed through the versioned C ABI.
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Decodes one raw version-one ABI value, failing closed for unknown IDs.
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Boolval),
            2 => Some(Self::Floatval),
            3 => Some(Self::Intval),
            4 => Some(Self::IsArray),
            5 => Some(Self::IsNull),
            6 => Some(Self::Abs),
            7 => Some(Self::Ceil),
            8 => Some(Self::Floor),
            9 => Some(Self::Sqrt),
            10 => Some(Self::Fdiv),
            11 => Some(Self::Fmod),
            12 => Some(Self::Pow),
            13 => Some(Self::Round),
            14 => Some(Self::Strrev),
            15 => Some(Self::ArrayKeyExists),
            16 => Some(Self::ObGetLevel),
            17 => Some(Self::ObGetLength),
            18 => Some(Self::ObClean),
            19 => Some(Self::ObFlush),
            20 => Some(Self::ObEndClean),
            21 => Some(Self::ObEndFlush),
            22 => Some(Self::MbStrlen),
            23 => Some(Self::MbStrwidth),
            24 => Some(Self::MbStrtoupper),
            25 => Some(Self::MbStrtolower),
            26 => Some(Self::MbConvertCase),
            27 => Some(Self::MbUcfirst),
            28 => Some(Self::MbLcfirst),
            29 => Some(Self::MbStrimwidth),
            30 => Some(Self::MbSubstr),
            31 => Some(Self::MbStrcut),
            32 => Some(Self::MbScrub),
            33 => Some(Self::MbTrim),
            34 => Some(Self::MbLtrim),
            35 => Some(Self::MbRtrim),
            36 => Some(Self::MbStrPad),
            37 => Some(Self::MbConvertKana),
            38 => Some(Self::MbSubstrCount),
            39 => Some(Self::MbOrd),
            40 => Some(Self::MbChr),
            41 => Some(Self::MbStrpos),
            42 => Some(Self::MbStripos),
            43 => Some(Self::MbStrrpos),
            44 => Some(Self::MbStrripos),
            45 => Some(Self::MbStrstr),
            46 => Some(Self::MbStristr),
            47 => Some(Self::MbStrrchr),
            48 => Some(Self::MbStrrichr),
            49 => Some(Self::MbLanguage),
            50 => Some(Self::MbInternalEncoding),
            51 => Some(Self::MbHttpOutput),
            52 => Some(Self::MbEncodingAliases),
            53 => Some(Self::MbStrSplit),
            54 => Some(Self::MbPreferredMimeName),
            55 => Some(Self::MbCheckEncoding),
            56 => Some(Self::MbSubstituteCharacter),
            57 => Some(Self::MbListEncodings),
            58 => Some(Self::MbDetectOrder),
            59 => Some(Self::MbEncodeNumericentity),
            60 => Some(Self::MbDecodeNumericentity),
            61 => Some(Self::MbDetectEncoding),
            62 => Some(Self::MbConvertEncoding),
            63 => Some(Self::MbDecodeMimeheader),
            64 => Some(Self::MbEncodeMimeheader),
            65 => Some(Self::MbGetInfo),
            66 => Some(Self::MbHttpInput),
            67 => Some(Self::MbRegexEncoding),
            68 => Some(Self::MbRegexSetOptions),
            69 => Some(Self::MbEregMatch),
            70 => Some(Self::MbEregSearchInit),
            71 => Some(Self::MbEregSearch),
            72 => Some(Self::MbEregSearchPos),
            73 => Some(Self::MbEregSearchRegs),
            74 => Some(Self::MbEregSearchGetpos),
            75 => Some(Self::MbEregSearchGetregs),
            76 => Some(Self::MbEregSearchSetpos),
            77 => Some(Self::MbSplit),
            78 => Some(Self::MbEregReplace),
            79 => Some(Self::MbEregiReplace),
            80 => Some(Self::MbEreg),
            81 => Some(Self::MbEregi),
            82 => Some(Self::MbParseStr),
            83 => Some(Self::SharedIni),
            84 => Some(Self::MbOutputHandler),
            85 => Some(Self::MbEregReplaceCallback),
            _ => None,
        }
    }

    /// Returns whether version one implements this PHP argument count.
    pub const fn supports_arity(self, arg_count: usize) -> bool {
        match self {
            Self::Boolval
            | Self::Floatval
            | Self::Intval
            | Self::IsArray
            | Self::IsNull
            | Self::Abs
            | Self::Ceil
            | Self::Floor
            | Self::Sqrt
            | Self::Strrev => arg_count == 1,
            Self::Fdiv | Self::Fmod | Self::Pow | Self::ArrayKeyExists => arg_count == 2,
            Self::Round | Self::MbStrlen | Self::MbStrwidth | Self::MbStrtoupper
            | Self::MbStrtolower | Self::MbUcfirst | Self::MbLcfirst => arg_count == 1 || arg_count == 2,
            Self::MbConvertCase => arg_count == 2 || arg_count == 3,
            Self::MbEncodeMimeheader => arg_count >= 1 && arg_count <= 5,
            Self::MbStrimwidth => arg_count >= 3 && arg_count <= 5,
            Self::MbSubstr => arg_count >= 2 && arg_count <= 4,
            Self::MbStrcut => arg_count >= 2 && arg_count <= 4,
            Self::MbScrub => arg_count >= 1 && arg_count <= 2,
            Self::MbTrim => arg_count >= 1 && arg_count <= 3,
            Self::MbLtrim => arg_count >= 1 && arg_count <= 3,
            Self::MbRtrim => arg_count >= 1 && arg_count <= 3,
            Self::MbStrPad => arg_count >= 2 && arg_count <= 5,
            Self::MbConvertKana => arg_count >= 1 && arg_count <= 3,
            Self::MbSubstrCount => arg_count >= 2 && arg_count <= 3,
            Self::MbOrd => arg_count >= 1 && arg_count <= 2,
            Self::MbChr => arg_count >= 1 && arg_count <= 2,
            Self::MbStrpos => arg_count >= 2 && arg_count <= 4,
            Self::MbStripos => arg_count >= 2 && arg_count <= 4,
            Self::MbStrrpos => arg_count >= 2 && arg_count <= 4,
            Self::MbStrripos => arg_count >= 2 && arg_count <= 4,
            Self::MbStrstr => arg_count >= 2 && arg_count <= 4,
            Self::MbStristr => arg_count >= 2 && arg_count <= 4,
            Self::MbStrrchr => arg_count >= 2 && arg_count <= 4,
            Self::MbStrrichr => arg_count >= 2 && arg_count <= 4,
            Self::MbCheckEncoding => arg_count <= 2,
            Self::MbSubstituteCharacter => arg_count <= 1,
            Self::MbLanguage => arg_count <= 1,
            Self::MbInternalEncoding => arg_count <= 1,
            Self::MbHttpOutput | Self::MbGetInfo | Self::MbHttpInput => arg_count <= 1,
            Self::MbRegexEncoding | Self::MbRegexSetOptions => arg_count <= 1,
            Self::MbParseStr => arg_count == 2,
            Self::SharedIni => arg_count == 4,
            Self::MbOutputHandler => arg_count == 2,
            Self::MbEregMatch | Self::MbEreg | Self::MbEregi | Self::MbSplit => arg_count == 2 || arg_count == 3,
            Self::MbEregReplace | Self::MbEregiReplace | Self::MbEregReplaceCallback => arg_count == 3 || arg_count == 4,
            Self::MbEregSearchInit => arg_count >= 1 && arg_count <= 3,
            Self::MbEregSearch | Self::MbEregSearchPos | Self::MbEregSearchRegs => arg_count <= 2,
            Self::MbEregSearchGetpos | Self::MbEregSearchGetregs => arg_count == 0,
            Self::MbEregSearchSetpos => arg_count == 1,
            Self::MbStrSplit => arg_count >= 1 && arg_count <= 3,
            Self::MbEncodingAliases | Self::MbPreferredMimeName | Self::MbDecodeMimeheader => arg_count == 1,
            Self::MbListEncodings => arg_count == 0,
            Self::MbDetectOrder => arg_count <= 1,
            Self::MbDetectEncoding => arg_count >= 1 && arg_count <= 3,
            Self::MbConvertEncoding => arg_count >= 2 && arg_count <= 3,
            Self::MbEncodeNumericentity => arg_count >= 2 && arg_count <= 4,
            Self::MbDecodeNumericentity => arg_count >= 2 && arg_count <= 3,
            Self::ObGetLevel
            | Self::ObGetLength
            | Self::ObClean
            | Self::ObFlush
            | Self::ObEndClean
            | Self::ObEndFlush => arg_count == 0,
        }
    }
}

/// Maps a shared contract identity onto the boxed runtime ABI when supported.
pub fn runtime_builtin_id(id: BuiltinId) -> Option<RuntimeBuiltinId> {
    RuntimeBuiltinId::ALL
        .into_iter()
        .find(|runtime_id| id == runtime_id.builtin_id())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every published runtime ID round-trips through its raw ABI value.
    #[test]
    fn runtime_builtin_ids_round_trip() {
        for expected in RuntimeBuiltinId::ALL {
            let raw = expected.as_u32();
            let id = RuntimeBuiltinId::from_u32(raw).expect("published runtime ID must decode");
            assert_eq!(id, expected);
            assert_eq!(id.as_u32(), raw);
            assert!(id.supports_arity(match id {
                RuntimeBuiltinId::MbConvertCase
                | RuntimeBuiltinId::Fdiv
                | RuntimeBuiltinId::Fmod
                | RuntimeBuiltinId::Pow
                | RuntimeBuiltinId::ArrayKeyExists => 2,
                RuntimeBuiltinId::ObGetLevel
                | RuntimeBuiltinId::ObGetLength
                | RuntimeBuiltinId::ObClean
                | RuntimeBuiltinId::ObFlush
                | RuntimeBuiltinId::ObEndClean
                | RuntimeBuiltinId::ObEndFlush => 0,
                RuntimeBuiltinId::MbEncodeNumericentity | RuntimeBuiltinId::MbDecodeNumericentity | RuntimeBuiltinId::MbConvertEncoding => 2,
                RuntimeBuiltinId::MbStrimwidth => 3,
                RuntimeBuiltinId::MbSubstr => 2,
                RuntimeBuiltinId::MbStrcut => 2,
                RuntimeBuiltinId::MbScrub => 1,
                RuntimeBuiltinId::MbTrim => 1,
                RuntimeBuiltinId::MbLtrim => 1,
                RuntimeBuiltinId::MbRtrim => 1,
                RuntimeBuiltinId::MbStrPad => 2,
                RuntimeBuiltinId::MbConvertKana => 1,
                RuntimeBuiltinId::MbSubstrCount => 2,
                RuntimeBuiltinId::MbOrd => 1,
                RuntimeBuiltinId::MbChr => 1,
                RuntimeBuiltinId::MbStrpos => 2,
                RuntimeBuiltinId::MbStripos => 2,
                RuntimeBuiltinId::MbStrrpos => 2,
                RuntimeBuiltinId::MbStrripos => 2,
                RuntimeBuiltinId::MbStrstr => 2,
                RuntimeBuiltinId::MbStristr => 2,
                RuntimeBuiltinId::MbStrrchr => 2,
                RuntimeBuiltinId::MbStrrichr => 2,
                RuntimeBuiltinId::MbLanguage => 0,
                RuntimeBuiltinId::MbInternalEncoding => 0,
                RuntimeBuiltinId::MbRegexEncoding | RuntimeBuiltinId::MbRegexSetOptions => 0,
                RuntimeBuiltinId::MbParseStr => 2,
                RuntimeBuiltinId::SharedIni => 4,
                RuntimeBuiltinId::MbOutputHandler => 2,
                RuntimeBuiltinId::MbEregMatch | RuntimeBuiltinId::MbEreg | RuntimeBuiltinId::MbEregi | RuntimeBuiltinId::MbSplit => 2,
                RuntimeBuiltinId::MbEregReplace | RuntimeBuiltinId::MbEregiReplace | RuntimeBuiltinId::MbEregReplaceCallback => 3,
                RuntimeBuiltinId::MbEregSearch | RuntimeBuiltinId::MbEregSearchPos | RuntimeBuiltinId::MbEregSearchRegs
                | RuntimeBuiltinId::MbEregSearchGetpos | RuntimeBuiltinId::MbEregSearchGetregs => 0,
                RuntimeBuiltinId::MbHttpOutput => 0,
                RuntimeBuiltinId::MbListEncodings | RuntimeBuiltinId::MbDetectOrder => 0,
                _ => 1,
            }));
        }
        assert_eq!(RuntimeBuiltinId::from_u32(0), None);
        let next = RuntimeBuiltinId::ALL.iter().map(|id| id.as_u32()).max().unwrap() + 1;
        assert_eq!(RuntimeBuiltinId::from_u32(next), None);
        assert_eq!(
            RuntimeBuiltinStatus::from_i32(0),
            Some(RuntimeBuiltinStatus::Success)
        );
        assert_eq!(RuntimeBuiltinStatus::from_i32(4), None);
    }

    /// Verifies only explicitly compatible catalog identities enter runtime dispatch.
    #[test]
    fn catalog_mapping_is_explicit_and_fail_closed() {
        assert_eq!(
            runtime_builtin_id(BuiltinId::from_canonical_name("abs")),
            Some(RuntimeBuiltinId::Abs)
        );
        assert_eq!(
            runtime_builtin_id(BuiltinId::from_canonical_name("settype")),
            None
        );
    }
}
