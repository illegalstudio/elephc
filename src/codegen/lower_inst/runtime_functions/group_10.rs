//! Purpose:
//! Dispatches one bounded group of typed builtin runtime targets.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - Extracted bodies remain thin calls into target-aware backend emitters.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{RuntimeFnId, Instruction};

/// Lowers a target owned by bounded dispatch group 10, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::MbStrposInt
        | RuntimeFnId::MbStriposInt
        | RuntimeFnId::MbStrrposInt
        | RuntimeFnId::MbStrriposInt => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_position_int(ctx, inst, target)
        }),
        RuntimeFnId::MbStrstrLen
        | RuntimeFnId::MbStristrLen
        | RuntimeFnId::MbStrrchrLen
        | RuntimeFnId::MbStrrichrLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_strstr_len(ctx, inst, target)
        }),
        RuntimeFnId::MbEncodingAliasesCount => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_encoding_aliases_count(ctx, inst, target)
        }),
        RuntimeFnId::MbInternalEncodingInt
        | RuntimeFnId::MbLanguageInt
        | RuntimeFnId::MbHttpOutputInt
        | RuntimeFnId::MbRegexEncodingInt
        | RuntimeFnId::MbSubstituteCharacterInt => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_setting_int(ctx, inst, target)
        }),
        RuntimeFnId::MbHttpInputLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_http_input_len(ctx, inst, target)
        }),
        RuntimeFnId::MbRegexSetOptionsLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_regex_set_options_len(ctx, inst, target)
        }),
        RuntimeFnId::MbGetInfoLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_get_info_len(ctx, inst, target)
        }),
        RuntimeFnId::MbListEncodingsCount => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_list_encodings_count(ctx, inst, target)
        }),
        RuntimeFnId::MbPreferredMimeNameLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_preferred_mime_name_len(ctx, inst, target)
        }),
        RuntimeFnId::MbSplitCount => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_split_count(ctx, inst, target)
        }),
        RuntimeFnId::MbEregReplaceLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_ereg_replace_len(ctx, inst, target)
        }),
        RuntimeFnId::MbEncodeNumericentityLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_encode_numericentity_len(ctx, inst, target)
        }),
        RuntimeFnId::MbDecodeNumericentityLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_decode_numericentity_len(ctx, inst, target)
        }),
        RuntimeFnId::MbConvertEncodingLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_convert_encoding_len(ctx, inst, target)
        }),
        RuntimeFnId::MbStrSplitCount => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_str_split_count(ctx, inst, target)
        }),
        RuntimeFnId::MbOrdInt => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_ord_int(ctx, inst, target)
        }),
        RuntimeFnId::MbDetectOrderSingle => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_detect_order_single(ctx, inst, target)
        }),
        RuntimeFnId::MbDetectEncodingLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_detect_encoding_len(ctx, inst, target)
        }),
        RuntimeFnId::MbChrLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_chr_len(ctx, inst, target)
        }),
        RuntimeFnId::MbStrimwidthLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_strimwidth_len(ctx, inst, target)
        }),
        RuntimeFnId::MbStrlen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mb_strlen(
                ctx, inst, target.mbstring_operation().expect("mbstring runtime operation"),
            )
        }),
        RuntimeFnId::MbStrwidth
        | RuntimeFnId::MbStrtoupper
        | RuntimeFnId::MbStrtolower
        | RuntimeFnId::MbConvertCase
        | RuntimeFnId::MbUcfirst
        | RuntimeFnId::MbLcfirst
        | RuntimeFnId::MbStrimwidth
        | RuntimeFnId::MbSubstr
        | RuntimeFnId::MbStrcut
        | RuntimeFnId::MbScrub
        | RuntimeFnId::MbTrim
        | RuntimeFnId::MbLtrim
        | RuntimeFnId::MbRtrim
        | RuntimeFnId::MbStrPad
        | RuntimeFnId::MbConvertKana
        | RuntimeFnId::MbSubstrCount
        | RuntimeFnId::MbOrd
        | RuntimeFnId::MbChr
        | RuntimeFnId::MbStrpos
        | RuntimeFnId::MbStripos
        | RuntimeFnId::MbStrrpos
        | RuntimeFnId::MbStrripos
        | RuntimeFnId::MbStrstr
        | RuntimeFnId::MbStristr
        | RuntimeFnId::MbStrrchr
        | RuntimeFnId::MbStrrichr
        | RuntimeFnId::MbLanguage
        | RuntimeFnId::MbInternalEncoding
        | RuntimeFnId::MbHttpOutput
        | RuntimeFnId::MbEncodingAliases
        | RuntimeFnId::MbStrSplit
        | RuntimeFnId::MbSubstituteCharacter
        | RuntimeFnId::MbListEncodings
        | RuntimeFnId::MbDetectOrder
        | RuntimeFnId::MbDetectEncoding
        | RuntimeFnId::MbConvertEncoding
        | RuntimeFnId::MbConvertVariables
        | RuntimeFnId::MbEncodeNumericentity
        | RuntimeFnId::MbDecodeMimeheader
        | RuntimeFnId::MbEncodeMimeheader
        | RuntimeFnId::MbOutputHandler
        | RuntimeFnId::MbGetInfo
        | RuntimeFnId::SharedIni
        | RuntimeFnId::MbParseStr
        | RuntimeFnId::MbSendMail
        | RuntimeFnId::MbHttpInput
        | RuntimeFnId::MbRegexEncoding
        | RuntimeFnId::MbRegexSetOptions
        | RuntimeFnId::MbEregMatch
        | RuntimeFnId::MbEreg
        | RuntimeFnId::MbEregi
        | RuntimeFnId::MbEregSearchInit
        | RuntimeFnId::MbEregSearch
        | RuntimeFnId::MbEregSearchPos
        | RuntimeFnId::MbEregSearchRegs
        | RuntimeFnId::MbEregSearchGetpos
        | RuntimeFnId::MbEregSearchGetregs
        | RuntimeFnId::MbEregSearchSetpos
        | RuntimeFnId::MbSplit
        | RuntimeFnId::MbEregReplace
        | RuntimeFnId::MbEregReplaceCallback
        | RuntimeFnId::MbEregiReplace
        | RuntimeFnId::MbDecodeNumericentity
        | RuntimeFnId::MbCheckEncoding
        | RuntimeFnId::MbPreferredMimeName => Some({
            crate::codegen::lower_inst::builtins::strings::lower_mbstring(
                ctx, inst, target.mbstring_operation().expect("mbstring runtime operation"),
            )
        }),
        RuntimeFnId::Md5 => Some({
            crate::codegen::lower_inst::builtins::strings::lower_md5(ctx, inst)
        }),
        RuntimeFnId::NumberFormat => Some({
            crate::codegen::lower_inst::builtins::strings::lower_number_format(ctx, inst)
        }),
        RuntimeFnId::Ord => Some({
            crate::codegen::lower_inst::builtins::strings::lower_ord(ctx, inst)
        }),
        RuntimeFnId::Printf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_printf(ctx, inst)
        }),
        RuntimeFnId::Rtrim => Some({
            crate::codegen::lower_inst::builtins::strings::lower_trim_like(
                    ctx,
                    inst,
                    "rtrim",
                    "__rt_rtrim",
                    "__rt_rtrim_mask",
                )
        }),
        RuntimeFnId::Sha1 => Some({
            crate::codegen::lower_inst::builtins::strings::lower_sha1(ctx, inst)
        }),
        RuntimeFnId::Sprintf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_sprintf(ctx, inst)
        }),
        RuntimeFnId::Sscanf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_sscanf(ctx, inst)
        }),
        RuntimeFnId::StrContains => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_contains(ctx, inst)
        }),
        RuntimeFnId::StrEndsWith => Some({
            crate::codegen::lower_inst::builtins::strings::lower_binary_string_runtime(
                    ctx,
                    inst,
                    "str_ends_with",
                    "__rt_str_ends_with",
                )
        }),
        RuntimeFnId::StrIreplace => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_replace(
                    ctx,
                    inst,
                    "str_ireplace",
                    "__rt_str_ireplace",
                )
        }),
        RuntimeFnId::StrPad => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_pad(ctx, inst)
        }),
        RuntimeFnId::StrRepeat => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_repeat(ctx, inst)
        }),
        RuntimeFnId::StrReplace => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_replace(
                    ctx,
                    inst,
                    "str_replace",
                    "__rt_str_replace",
                )
        }),
        RuntimeFnId::StrSplit => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_split(ctx, inst)
        }),
        RuntimeFnId::StrStartsWith => Some({
            crate::codegen::lower_inst::builtins::strings::lower_binary_string_runtime(
                    ctx,
                    inst,
                    "str_starts_with",
                    "__rt_str_starts_with",
                )
        }),
        RuntimeFnId::Strcasecmp => Some({
            crate::codegen::lower_inst::builtins::strings::lower_binary_string_runtime(
                    ctx,
                    inst,
                    "strcasecmp",
                    "__rt_strcasecmp",
                )
        }),
        RuntimeFnId::Strcmp => Some({
            crate::codegen::lower_inst::builtins::strings::lower_binary_string_runtime(
                    ctx,
                    inst,
                    "strcmp",
                    "__rt_strcmp",
                )
        }),
        RuntimeFnId::Strncasecmp => Some({
            crate::codegen::lower_inst::builtins::strings::lower_length_limited_compare(
                    ctx,
                    inst,
                    "strncasecmp",
                    "__rt_strncasecmp",
                )
        }),
        RuntimeFnId::Strncmp => Some({
            crate::codegen::lower_inst::builtins::strings::lower_length_limited_compare(
                    ctx,
                    inst,
                    "strncmp",
                    "__rt_strncmp",
                )
        }),
        RuntimeFnId::StripTags => Some({
            crate::codegen::lower_inst::builtins::strings::lower_strip_tags(ctx, inst)
        }),
        RuntimeFnId::StripTagsLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_strip_tags_len(ctx, inst)
        }),
        RuntimeFnId::Stripos => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position(
                    ctx,
                    inst,
                    "stripos",
                    "__rt_stripos",
                    crate::codegen::lower_inst::builtins::strings::StringPositionDirection::Forward,
                )
        }),
        RuntimeFnId::StrposInt => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position_int(
                ctx,
                inst,
                "strpos",
                "__rt_strpos",
                crate::codegen::lower_inst::builtins::strings::StringPositionDirection::Forward,
            )
        }),
        RuntimeFnId::StriposInt => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position_int(
                ctx,
                inst,
                "stripos",
                "__rt_stripos",
                crate::codegen::lower_inst::builtins::strings::StringPositionDirection::Forward,
            )
        }),
        RuntimeFnId::StrrposInt => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position_int(
                ctx,
                inst,
                "strrpos",
                "__rt_strrpos",
                crate::codegen::lower_inst::builtins::strings::StringPositionDirection::Reverse,
            )
        }),
        RuntimeFnId::StrriposInt => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position_int(
                ctx,
                inst,
                "strripos",
                "__rt_strripos",
                crate::codegen::lower_inst::builtins::strings::StringPositionDirection::Reverse,
            )
        }),
        RuntimeFnId::Strripos => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position(
                    ctx,
                    inst,
                    "strripos",
                    "__rt_strripos",
                    crate::codegen::lower_inst::builtins::strings::StringPositionDirection::Reverse,
                )
        }),
        RuntimeFnId::Strpos => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position(
                    ctx,
                    inst,
                    "strpos",
                    "__rt_strpos",
                    crate::codegen::lower_inst::builtins::strings::StringPositionDirection::Forward,
                )
        }),
        RuntimeFnId::Strrpos => Some({
            crate::codegen::lower_inst::builtins::strings::lower_string_position(
                    ctx,
                    inst,
                    "strrpos",
                    "__rt_strrpos",
                    crate::codegen::lower_inst::builtins::strings::StringPositionDirection::Reverse,
                )
        }),
        RuntimeFnId::StrSplitCount => Some({
            crate::codegen::lower_inst::builtins::strings::lower_str_split_count(ctx, inst)
        }),
        RuntimeFnId::Bin2HexLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_bin2hex_len(ctx, inst)
        }),
        RuntimeFnId::StrstrLen => Some({
            crate::codegen::lower_inst::builtins::strings::lower_strstr_len(ctx, inst)
        }),
        RuntimeFnId::Strstr => Some({
            crate::codegen::lower_inst::builtins::strings::lower_strstr(ctx, inst)
        }),
        RuntimeFnId::ParseUrl => Some({
            crate::codegen::lower_inst::builtins::strings::lower_parse_url(ctx, inst)
        }),
        RuntimeFnId::ParseUrlCount => Some({
            crate::codegen::lower_inst::builtins::strings::lower_parse_url_count(ctx, inst)
        }),
        RuntimeFnId::Substr => Some({
            crate::codegen::lower_inst::builtins::strings::lower_substr(ctx, inst)
        }),
        RuntimeFnId::SubstrCount => Some({
            crate::codegen::lower_inst::builtins::strings::lower_substr_count(ctx, inst)
        }),
        RuntimeFnId::SubstrReplace => Some({
            crate::codegen::lower_inst::builtins::strings::lower_substr_replace(ctx, inst)
        }),
        RuntimeFnId::Trim => Some({
            crate::codegen::lower_inst::builtins::strings::lower_trim_like(
                    ctx,
                    inst,
                    "trim",
                    "__rt_trim",
                    "__rt_trim_mask",
                )
        }),
        RuntimeFnId::Ucfirst => Some({
            crate::codegen::lower_inst::builtins::strings::lower_ucfirst(ctx, inst)
        }),
        RuntimeFnId::Ucwords => Some({
            crate::codegen::lower_inst::builtins::strings::lower_ucwords(ctx, inst)
        }),
        RuntimeFnId::Vprintf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_vprintf(ctx, inst)
        }),
        RuntimeFnId::Vsprintf => Some({
            crate::codegen::lower_inst::builtins::strings::lower_vsprintf(ctx, inst)
        }),
        RuntimeFnId::Wordwrap => Some({
            crate::codegen::lower_inst::builtins::strings::lower_wordwrap(ctx, inst)
        }),
        RuntimeFnId::ElephcGmmktimeRaw => Some({
            crate::codegen::lower_inst::builtins::system::lower_gmmktime(ctx, inst)
        }),
        RuntimeFnId::ElephcMktimeRaw => Some({
            crate::codegen::lower_inst::builtins::system::lower_mktime(ctx, inst)
        }),
        RuntimeFnId::ElephcStrtotimeRaw => Some({
            crate::codegen::lower_inst::builtins::system::lower_elephc_strtotime_raw(ctx, inst)
        }),
        RuntimeFnId::Checkdate => Some({
            crate::codegen::lower_inst::builtins::system::lower_checkdate(ctx, inst)
        }),
        RuntimeFnId::ClassAttributeArgs => Some({
            crate::codegen::lower_inst::builtins::attributes::lower_class_attribute_args(ctx, inst)
        }),
        _ => None,
    }
}
