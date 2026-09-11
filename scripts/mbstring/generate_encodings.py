#!/usr/bin/env python3
"""Generate the shared encoding catalog from the captured PHP baseline."""

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "crates/elephc-mbstring/src/encoding"


def generate():
    """Write canonical names, aliases, MIME names, and available codec bindings."""
    surface = json.loads((ROOT / "scripts/mbstring/php_surface.json").read_text())
    single = json.loads((OUTPUT / "data/singlebyte.json").read_text())
    double = json.loads((OUTPUT / "data/doublebyte.json").read_text())
    mobile = json.loads((OUTPUT / "data/mobile_utf8.json").read_text())
    gb18030 = json.loads((OUTPUT / "data/gb18030.json").read_text())
    euctw = json.loads((OUTPUT / "data/euctw.json").read_text())
    jis = json.loads((OUTPUT / "data/jis.json").read_text())
    boundaries = json.loads((OUTPUT / "data/boundaries.json").read_text())
    if surface["php_version"] != single["php_version"] or surface["php_version"] != double["php_version"]:
        raise ValueError("Encoding tables and surface must use the same PHP baseline")
    core = {
        "ASCII": "Ascii", "7bit": "Ascii", "8bit": "EightBit", "UTF-8": "Utf8",
        "UTF-16": "Utf16", "UTF-16BE": "Utf16Be", "UTF-16LE": "Utf16Le",
        "UTF-32": "Utf32", "UTF-32BE": "Utf32Be", "UTF-32LE": "Utf32Le",
        "UCS-2": "Ucs2", "UCS-2BE": "Ucs2Be", "UCS-2LE": "Ucs2Le",
        "UCS-4": "Ucs4", "UCS-4BE": "Ucs4Be", "UCS-4LE": "Ucs4Le",
    }
    lines = [
        "//! Purpose:",
        "//! Provides the generated PHP mbstring encoding catalog and codec bindings.",
        "//!",
        "//! Called from:",
        "//! - `super::catalog` for encoding lookup and metadata.",
        "//!",
        "//! Key details:",
        "//! - Regenerate with `python3 scripts/mbstring/generate_encodings.py`.",
        "//! - Canonical spelling, order, and aliases come from PHP's reflection fixture.",
        "",
        "use super::{catalog::{Codec, EncodingInfo, Slicing}, doublebyte::DoubleByte, euctw::EucTw, gb18030::Gb18030, hz::Hz, iso2022kr::Iso2022Kr, jis::{Jis, Variant}, jis2004::Jis2004, mobile_utf8::MobileUtf8, mobile_sjis::MobileSjis, singlebyte::SingleByte, transfer::Transfer, utf7::Utf7, UnicodeEncoding};",
        "",
        "/// Complete encoding list in PHP's public enumeration order.",
        "pub(super) const ENCODINGS: &[EncodingInfo] = &[",
    ]
    for name, info in surface["encodings"].items():
        aliases = ", ".join(json.dumps(alias) for alias in info["aliases"])
        mime = f"Some({json.dumps(info['mime'])})" if info["mime"] else "None"
        if name in core:
            codec = f"Codec::Unicode(UnicodeEncoding::{core[name]})"
        elif name in ("BASE64", "Quoted-Printable", "UUENCODE", "HTML-ENTITIES"):
            variant = {"BASE64": "Base64", "Quoted-Printable": "QuotedPrintable", "UUENCODE": "Uuencode", "HTML-ENTITIES": "Html"}[name]
            codec = f"Codec::Transfer(Transfer::{variant})"
        elif name in ("UTF-7", "UTF7-IMAP"):
            codec = f"Codec::Utf7(Utf7({'true' if name == 'UTF7-IMAP' else 'false'}))"
        elif name == "ISO-2022-JP-2004":
            planes = []
            for plane, source in [("classic", "EUC-JP"), ("extended", "EUC-JP-2004")]:
                fields = ", ".join(f'{key}: include_bytes!("data/{value["file"]}")'
                                   for key, value in double["encodings"][source].items())
                planes.append(f"{plane}: DoubleByte {{ {fields} }}")
            codec = "Codec::Jis2004(Jis2004 { " + ", ".join(planes) + " })"
        elif name in jis["encodings"]:
            mapping = double["encodings"]["CP51932" if name == "ISO-2022-JP-MS" else "EUC-JP"]
            fields = ", ".join(f'{key}: include_bytes!("data/{value["file"]}")'
                               for key, value in mapping.items())
            encoder_name = "CP50221" if name == "CP50220" else name
            encode = jis["encodings"][encoder_name]["encode"]["file"]
            errors = jis["encodings"]["ISO-2022-JP" if name == "JIS" else encoder_name]["encode"]["file"]
            variant = {"JIS": "Jis", "ISO-2022-JP": "Iso2022", "ISO-2022-JP-MS": "Microsoft", "ISO-2022-JP-MOBILE#KDDI": "Kddi",
                       "CP50220": "Cp50220", "CP50221": "Cp50221", "CP50222": "Cp50222"}[name]
            supplementary = composites = "&[]"
            if name == "ISO-2022-JP-MOBILE#KDDI":
                supplementary = 'include_bytes!("data/jis-kddi-supplementary.bin")'
                composites = 'include_bytes!("data/jis-kddi-composites.bin")'
            codec = (f'Codec::Jis(Jis {{ base: DoubleByte {{ {fields} }}, '
                     f'encode: include_bytes!("data/{encode}"), error_encode: include_bytes!("data/{errors}"), '
                     f'supplementary: {supplementary}, composites: {composites}, '
                     f'variant: Variant::{variant} }})')
        elif name == "HZ":
            mapping = double["encodings"]["EUC-CN"]
            fields = ", ".join(f'{key}: include_bytes!("data/{value["file"]}")'
                               for key, value in mapping.items())
            codec = f"Codec::Hz(Hz(DoubleByte {{ {fields} }}))"
        elif name == "ISO-2022-KR":
            mapping = double["encodings"]["UHC"]
            fields = ", ".join(f'{key}: include_bytes!("data/{value["file"]}")'
                               for key, value in mapping.items())
            codec = f"Codec::Iso2022Kr(Iso2022Kr(DoubleByte {{ {fields} }}))"
        elif name in gb18030["encodings"]:
            mapping = gb18030["encodings"][name]
            fields = ", ".join(f'{key}: include_bytes!("data/{value["file"]}")'
                               for key, value in mapping.items())
            codec = f"Codec::Gb18030(Gb18030 {{ {fields} }})"
        elif name in euctw["encodings"]:
            mapping = euctw["encodings"][name]
            fields = ", ".join(f'{key}: include_bytes!("data/{value["file"]}")'
                               for key, value in mapping.items())
            codec = f"Codec::EucTw(EucTw {{ {fields} }})"
        elif name in single["encodings"]:
            mapping = single["encodings"][name]
            codec = ("Codec::SingleByte(SingleByte { "
                     f"decode: include_bytes!(\"data/{mapping['decode']['file']}\"), "
                     f"encode: include_bytes!(\"data/{mapping['encode']['file']}\") "
                     "})")
        elif name in double["encodings"]:
            mapping = double["encodings"][name]
            fields = ", ".join(f'{key}: include_bytes!("data/{value["file"]}")'
                               for key, value in mapping.items())
            if name.startswith("SJIS-Mobile#"):
                flags = "false" if name.endswith("DOCOMO") else "true"
                codec = f"Codec::MobileSjis(MobileSjis {{ base: DoubleByte {{ {fields} }}, flags: {flags} }})"
            else:
                codec = f"Codec::DoubleByte(DoubleByte {{ {fields} }})"
        elif name in mobile["encodings"]:
            mapping = mobile["encodings"][name]
            fields = ", ".join(f'{key}: include_bytes!("data/{value["file"]}")'
                               for key, value in mapping.items())
            codec = f"Codec::MobileUtf8(MobileUtf8 {{ {fields} }})"
        else:
            raise ValueError(f"Missing native codec for canonical encoding {name}")
        if name in ("ASCII", "7bit", "8bit", "UUENCODE") or name in single["encodings"]:
            slicing = "Slicing::Fixed(1)"
        elif name.startswith("UCS-2"):
            slicing = "Slicing::Fixed(2)"
        elif name.startswith(("UCS-4", "UTF-32")):
            slicing = "Slicing::Fixed(4)"
        elif name in boundaries["encodings"]:
            file = boundaries["encodings"][name]["file"]
            slicing = f'Slicing::LeadingByte(include_bytes!("data/{file}"))'
        else:
            slicing = "Slicing::Converted"
        lines.extend([
            "    EncodingInfo {",
            f"        name: {json.dumps(name)},",
            f"        aliases: &[{aliases}],",
            f"        mime: {mime},",
            f"        supports_ord_chr: {'true' if info['supports_ord_chr'] else 'false'},",
            f"        supports_detection: {'true' if info['supports_detection'] else 'false'},",
            f"        codec: {codec},",
            f"        slicing: {slicing},",
            "    },",
        ])
    lines.extend([
        "];",
        "",
    ])
    (OUTPUT / "catalog_data.rs").write_text("\n".join(lines))


if __name__ == "__main__":
    generate()
