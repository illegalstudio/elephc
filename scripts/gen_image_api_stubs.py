#!/usr/bin/env python3
"""Generate throwing-stub declarations + a coverage test for the PHP image OOP
API surface (Imagick / Gmagick families).

Reads the cached php.net class-synopsis HTML under `scripts/image_synopsis/`,
extracts the implemented-method set per class from `src/image_prelude.rs`, and
emits one stub block per class (to splice into the prelude) plus a coverage test
that calls every stub with type-default args and asserts each throws its
`*Exception("... not supported in elephc")`.

Signature transcription rules (verified against the elephc type checker):

* `mixed` / `resource` / `object` / `callable` / `iterable` param -> untyped.
* `= ?` -> type-appropriate empty default (`int`->`0`, `float`->`0.0`,
  `string`->`""`, `bool`->`false`, `array`->`[]`, class/other -> `null`).
* `= Imagick::CHANNEL_DEFAULT` / any `Class::CONST` default -> `0` (int param).
* explicit `= null` default on `string`/`array` -> `""` / `[]` (elephc rejects
  `string $x = null` and `array $x = null`).
* by-ref `&$x` params lose their default (become required); `array &$x = null`
  is rejected by elephc, and a throwing stub never honors the default anyway.
* return `static`/`self` -> the enclosing class name; `null` return -> `void`;
  union containing `false` (`T|false`) -> strip the `false` member (elephc does
  not parse `false` as a type expression); other unions (`array|bool`) kept.
* Capitalized param names (`$Imagick`, `$COLORSPACE`) -> lowercased.
"""

import html as ihtml
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
PRELUDE = os.path.join(ROOT, "src", "image_prelude.rs")
SYN_DIR = os.path.join(HERE, "image_synopsis")

# Each class: (class_name, synopsis html file, exception class, instance-ctor
# php expr used by the coverage test).
IMAGICK_FAMILY = [
    ("Imagick", "imagick.html", "ImagickException", "new Imagick()"),
    ("ImagickDraw", "imagickdraw.html", "ImagickDrawException", "new ImagickDraw()"),
    ("ImagickPixel", "imagickpixel.html", "ImagickPixelException", 'new ImagickPixel()'),
    ("ImagickPixelIterator", "imagickpixeliterator.html", "ImagickPixelIteratorException", "new ImagickPixelIterator(new Imagick())"),
    ("ImagickKernel", "imagickkernel.html", "ImagickKernelException", "new ImagickKernel()"),
]
GMAGICK_FAMILY = [
    ("Gmagick", "gmagick.html", "GmagickException", "new Gmagick()"),
    ("GmagickDraw", "gmagickdraw.html", "GmagickDrawException", "new GmagickDraw()"),
    ("GmagickPixel", "gmagickpixel.html", "GmagickPixelException", 'new GmagickPixel()'),
]

# A family bundles its classes with the coverage-test file/test-name it emits.
FAMILIES = [
    {"classes": IMAGICK_FAMILY, "test_file": "imagick_api_surface.rs",
     "test_name": "test_imagick_api_surface_all_stubs_throw",
     "stub_file": "imagick_family_stubs.php"},
    {"classes": GMAGICK_FAMILY, "test_file": "gmagick_api_surface.rs",
     "test_name": "test_gmagick_api_surface_all_stubs_throw",
     "stub_file": "gmagick_family_stubs.php"},
]

# All classes across families (for the prelude splice pass).
CLASSES = [c for fam in FAMILIES for c in fam["classes"]]

# Magic methods that must NOT be stubbed (they intercept undefined-method calls
# and would change call resolution). __construct/__destruct are implemented.
SKIP_MAGIC = {"__call", "__callStatic"}

# Marker comments bracketing each auto-generated stub block so re-runs are
# idempotent (the splicer replaces the bracketed region instead of appending).
MARK_BEGIN = "// --- begin auto-generated API-surface throwing stubs (do not edit; regen via scripts/gen_image_api_stubs.py) ---"
MARK_END = "// --- end auto-generated API-surface stubs ---"

# Canonical casing for image OOP class names; php.net synopses sometimes
# lowercase them (e.g. `gmagick $x`), so type annotations are normalized.
CANON_CLASS = {
    "imagick": "Imagick", "imagickdraw": "ImagickDraw", "imagickpixel": "ImagickPixel",
    "imagickpixeliterator": "ImagickPixelIterator", "imagickkernel": "ImagickKernel",
    "gmagick": "Gmagick", "gmagickdraw": "GmagickDraw", "gmagickpixel": "GmagickPixel",
}


def clean_block(b: str) -> str:
    b = b.replace("<br>", " ")
    b = re.sub(r"<[^>]+>", "", b)
    b = ihtml.unescape(b)
    b = re.sub(r"\s+", " ", b).strip()
    return b


def parse_methods(html_path: str):
    raw = open(html_path, encoding="utf-8").read()
    blocks = re.findall(
        r'<div class="(?:classsynopsisinfo|methodsynopsis dc-description)">(.*?)</div>',
        raw,
        re.S,
    )
    out = []
    for b in blocks:
        line = clean_block(b)
        if not line.startswith("public"):
            continue  # class header line
        m = re.match(r"^public (static )?function (\w+)\((.*)\)(?:: (.*))?$", line)
        if not m:
            continue
        is_static = bool(m.group(1))
        name = m.group(2)
        params_str = m.group(3).strip()
        ret = (m.group(4) or "").strip()
        params = parse_params(params_str)
        out.append({"name": name, "static": is_static, "params": params, "ret": ret})
    return out


def parse_params(s: str):
    if not s:
        return []
    # split on top-level commas (defaults here never contain commas)
    parts = [p.strip() for p in s.split(",")]
    params = []
    for p in parts:
        if not p:
            continue
        default = None
        if "=" in p:
            p, default = p.split("=", 1)
            p = p.strip()
            default = default.strip()
        # p is now "[TYPE] [&] $name" (type may precede the &, e.g. `array &$x`)
        m = re.match(r"^(?:(.*?)\s+)?(&\s*)?\$(\w+)$", p)
        if not m:
            continue
        ptype = (m.group(1) or "").strip() or None
        byref = bool(m.group(2))
        pname = m.group(3)
        params.append({"type": ptype, "name": pname, "byref": byref, "default": default})
    return params


def map_param_type(t, cls):
    if t is None:
        return None
    t = t.strip()
    low = t.lower()
    if low in ("mixed", "resource", "object", "callable", "iterable", "null"):
        return None
    # php.net synopsis artifacts: the C handle types map to the PHP class.
    if t == "MagickWand":
        return "Imagick"
    if t == "GmagickWand" or t == "GraphicsMagickWand":
        return "Gmagick"
    if t == "static" or t == "self":
        return cls
    # php.net sometimes lowercases class type names (e.g. `gmagick $x`); PHP
    # class names are case-insensitive, but normalize to canonical casing so
    # the stub type annotation resolves unambiguously.
    canonical = CANON_CLASS.get(low)
    if canonical is not None:
        return canonical
    if t.startswith("?"):
        inner = "?" + map_param_type(t[1:], cls)
        return inner  # nullable, preserved
    if "|" in t:
        members = [m.strip() for m in t.split("|")]
        # drop `false` members (elephc can't parse false as a type)
        members = [m for m in members if m.lower() != "false"]
        if not members:
            return None
        if len(members) == 1:
            inner = map_param_type(members[0], cls)
            return inner
        return "|".join(members)
    return t


def map_default(default, ptype, byref):
    """Return the elephc-compatible default literal, or None to drop the default."""
    if default is None:
        return None
    if byref:
        return None  # by-ref params become required
    d = default.strip()
    low = d.lower()
    # synopsis shorthand for "optional, default unspecified"
    if d == "?":
        return _empty_default(ptype)
    # class-constant defaults (Imagick::CHANNEL_DEFAULT, Gmagick::COLOR_BLACK, ...)
    if "::" in d or re.match(r"^[A-Z_]\w*$", d):
        # these are integer channel/color constants in the synopsis
        return "0"
    # explicit null: only valid for int/bool/float/?T/untyped
    if low == "null":
        return _empty_default(ptype)
    if low == "true":
        return "true"
    if low == "false":
        return "false"
    # numeric / string-literal / [] defaults are kept verbatim
    return d


def _empty_default(ptype):
    if ptype is None:
        return "null"
    low = ptype.lower()
    if low == "int":
        return "0"
    if low == "float":
        return "0.0"
    if low == "string":
        return '""'
    if low == "bool":
        return "false"
    if low == "array":
        return "[]"
    if ptype.startswith("?"):
        return "null"
    # class-typed or union -> null is the safe "absent" default
    return "null"


def map_return(ret, cls):
    if not ret:
        return None
    ret = ret.strip()
    low = ret.lower()
    if low == "null":
        return "void"
    if low in ("void", "mixed"):
        return ret
    if ret in ("static", "self"):
        return cls
    if ret.startswith("?"):
        return ret
    if "|" in ret:
        members = [m.strip() for m in ret.split("|")]
        members = [m for m in members if m.lower() != "false"]
        if not members:
            return None
        if len(members) == 1:
            return map_return(members[0], cls)
        return "|".join(members)
    if low in ("iterable",):
        return "array"
    if low in ("object", "callable", "resource"):
        return None
    return CANON_CLASS.get(low, ret)


def implemented_methods_per_class(prelude_src):
    """Return {class_name: set(lowercased method names)} declared in the prelude."""
    classes = {}
    # match `class ClassName ... {` blocks at column 0
    for m in re.finditer(r"^class (\w+)[^\n]*\{", prelude_src, re.M):
        cls = m.group(1)
        start = m.end()
        # find the matching closing brace at column 0
        end = prelude_src.find("\n}\n", start)
        if end == -1:
            end = len(prelude_src)
        body = prelude_src[start:end]
        # exclude any previously-spliced auto-generated stub region so the
        # implemented set reflects only hand-written methods, not generated stubs
        # (otherwise re-runs would treat stubs as implemented and skip them).
        body = re.sub(
            re.escape(MARK_BEGIN) + r".*?" + re.escape(MARK_END),
            "",
            body,
            flags=re.S,
        )
        names = set()
        for fm in re.finditer(r"public\s+static\s+function\s+(\w+)|public\s+function\s+(\w+)", body):
            names.add((fm.group(1) or fm.group(2)).lower())
        classes[cls] = names
    return classes


def emit_stub(method, cls, exc):
    name = method["name"]
    sig_parts = []
    for p in method["params"]:
        ptype = map_param_type(p["type"], cls)
        # lowercase only a leading uppercase char ($Imagick -> $imagick); leave
        # the rest of the name intact so camelCase names stay PHP-idiomatic.
        raw = p["name"]
        pname = raw[0].lower() + raw[1:] if raw and raw[0].isupper() else raw
        tok = ""
        if ptype:
            tok += ptype + " "
        if p["byref"]:
            tok += "&"
        tok += "$" + pname
        d = map_default(p["default"], ptype, p["byref"])
        if d is not None:
            tok += " = " + d
        sig_parts.append(tok)
    ret = map_return(method["ret"], cls)
    ret_str = ": " + ret if ret else ""
    static = "static " if method["static"] else ""
    lines = []
    lines.append("    public " + static + "function " + name + "(" + ", ".join(sig_parts) + ")" + ret_str + " {")
    for p in method["params"]:
        raw = p["name"]
        pname = raw[0].lower() + raw[1:] if raw and raw[0].isupper() else raw
        lines.append('        $_u_' + pname + " = $" + pname + ";")
    lines.append('        throw new ' + exc + '("' + cls + "::" + name + '() is not supported in elephc");')
    lines.append("    }")
    return "\n".join(lines)


# coverage-test arg for a required (no-default) param of a given mapped type
def cov_arg(ptype, cls, helpers, byref=False):
    if byref:
        # by-ref params need a variable, not a literal; reuse shared ref slots
        low = ptype.lower() if ptype else ""
        if low == "array":
            helpers.add("refa")
            return "$refa"
        helpers.add("refn")
        return "$refn"
    if ptype is None:
        return "null"
    low = ptype.lower()
    if low == "int":
        return "1"
    if low == "float":
        return "1.0"
    if low == "string":
        return '"x"'
    if low == "bool":
        return "false"
    if low == "array":
        return "[]"
    if ptype.startswith("?"):
        return "null"
    # union (A|B|...): pick the first member that yields a concrete, non-null
    # coverage value. `null` would not satisfy a non-nullable union, so a
    # member like `ImagickPixel|string` resolves to an ImagickPixel (`$px`).
    if "|" in ptype:
        for m in ptype.split("|"):
            m = m.strip()
            if not m or m.lower() in ("false", "null"):
                continue
            a = cov_arg(m, cls, helpers, byref=False)
            if a != "null":
                return a
        return "null"
    # class-typed
    if ptype in ("Imagick", "ImagickDraw", "ImagickPixel", "ImagickPixelIterator", "ImagickKernel",
                 "Gmagick", "GmagickDraw", "GmagickPixel"):
        var = {"Imagick": "im", "ImagickDraw": "draw", "ImagickPixel": "px",
               "ImagickPixelIterator": "pi", "ImagickKernel": "kern",
               "Gmagick": "gm", "GmagickDraw": "gmdraw", "GmagickPixel": "gmpx"}[ptype]
        helpers.add(var)
        return "$" + var
    return "null"


def main():
    prelude_src = open(PRELUDE, encoding="utf-8").read()
    implemented = implemented_methods_per_class(prelude_src)

    stub_blocks = {}      # cls -> list of stub strings
    coverage = {}         # cls -> list of (name, static, call, helpers)
    stats = {}

    for cls, htmlf, exc, _ctor in CLASSES:
        html_path = os.path.join(SYN_DIR, htmlf)
        if not os.path.exists(html_path):
            print("MISSING synopsis:", html_path, file=sys.stderr)
            continue
        methods = parse_methods(html_path)
        impl = implemented.get(cls, set())
        stubs = []
        cov = []
        seen = set()
        for m in methods:
            name = m["name"]
            if name in SKIP_MAGIC:
                continue
            lname = name.lower()
            if lname in impl or lname in seen:
                continue
            seen.add(lname)
            stubs.append(emit_stub(m, cls, exc))
            # coverage call: only required params (no default after transcription)
            helpers = set()
            args = []
            for p in m["params"]:
                ptype = map_param_type(p["type"], cls)
                d = map_default(p["default"], ptype, p["byref"])
                if d is None:
                    args.append(cov_arg(ptype, cls, helpers, p["byref"]))
            call = ("Cls::" if m["static"] else "$obj->") + name + "(" + ", ".join(args) + ")"
            cov.append((name, m["static"], call, helpers))
        stub_blocks[cls] = stubs
        coverage[cls] = cov
        stats[cls] = (len(stubs), len(methods), len(impl))

    # per family: write the standalone stub block + a coverage test file
    for fam in FAMILIES:
        stub_path = os.path.join(SYN_DIR, fam["stub_file"])
        chunks = []
        for cls, _, _, _ in fam["classes"]:
            stubs = stub_blocks.get(cls, [])
            if not stubs:
                continue
            chunks.append("// ===== %s throwing stubs (%d) =====\n%s" % (cls, len(stubs), "\n".join(stubs)))
        with open(stub_path, "w", encoding="utf-8") as f:
            f.write("\n\n".join(chunks) + "\n")
        print("wrote stub block:", stub_path)
        write_coverage_test(fam, coverage)
        print("wrote coverage test:", os.path.join(ROOT, "tests", "codegen", "image", fam["test_file"]))

    # splice the stubs into the prelude (idempotent via marker comments)
    spliced = splice_into_prelude(prelude_src, stub_blocks)
    if spliced is not None:
        open(PRELUDE, "w", encoding="utf-8").write(spliced)
        print("spliced stubs into:", PRELUDE)
    else:
        print("prelude already spliced (markers present); skipping")

    print("\nstats (stubs / synopsis / implemented):")
    total = 0
    for cls, _, _, _ in CLASSES:
        s, ms, im = stats.get(cls, (0, 0, 0))
        total += s
        print("  %-22s %4d / %4d / %4d" % (cls, s, ms, im))
    print("  TOTAL stubs:", total)


def splice_into_prelude(src, stub_blocks):
    """Insert each class's stub block before its closing brace.

    Idempotent: a class whose body already contains MARK_BEGIN is left alone.
    Returns the modified source, or None if every class was already spliced.
    """
    changed = False
    for cls, _, _, _ in CLASSES:
        stubs = stub_blocks.get(cls, [])
        if not stubs:
            continue
        # Canonical block content, no leading/trailing newline: it sits between
        # the last hand-written method's `}\n` and the class's closing `\n}`.
        block = "    " + MARK_BEGIN + "\n" + "\n".join(stubs) + "\n    " + MARK_END
        # locate the class declaration line
        m = re.search(r"^class " + cls + r"\b[^\n]*\{\n", src, re.M)
        if not m:
            print("  WARN: class %s not found in prelude; skipping" % cls, file=sys.stderr)
            continue
        body_start = m.end()
        # find the matching top-level closing brace (a `}` at column 0)
        close = re.search(r"\n\}\n", src[body_start:])
        if not close:
            print("  WARN: close brace for %s not found; skipping" % cls, file=sys.stderr)
            continue
        close_abs = body_start + close.start()
        body = src[body_start:close_abs]
        if MARK_BEGIN in body:
            # already spliced: swap the marker-bounded region in place. The
            # pattern matches only `    MARK_BEGIN ...    MARK_END` and leaves
            # the surrounding newlines untouched, so re-running is a no-op on
            # whitespace (idempotent).
            pat = re.compile(re.escape("    " + MARK_BEGIN) + r".*?" + re.escape("    " + MARK_END), re.S)
            new_body = pat.sub(block, body, count=1)
        else:
            # first splice: append on its own line after the last method.
            new_body = body + "\n" + block
        src = src[:body_start] + new_body + src[close_abs:]
        changed = True
    return src if changed else None


def write_coverage_test(fam, coverage):
    """Emit tests/codegen/image/<fam test_file> for one family of classes."""
    classes = fam["classes"]
    label = fam["test_file"].replace("_api_surface.rs", "")  # imagick | gmagick
    # ctors for the receiver/argument helper objects a stub call may reference.
    helper_ctor = {
        "im": "new Imagick()", "draw": "new ImagickDraw()", "px": "new ImagickPixel()",
        "pi": "new ImagickPixelIterator(new Imagick())", "kern": "new ImagickKernel()",
        "gm": "new Gmagick()", "gmdraw": "new GmagickDraw()", "gmpx": "new GmagickPixel()",
        "refa": "[]", "refn": "0.0",
    }
    # per-class instance variable used as the receiver for instance-method calls.
    instance = {
        "Imagick": "$im", "ImagickDraw": "$draw", "ImagickPixel": "$px",
        "ImagickPixelIterator": "$pi", "ImagickKernel": "$kern",
        "Gmagick": "$gm", "GmagickDraw": "$gmdraw", "GmagickPixel": "$gmpx",
    }
    helper_order = ("im", "draw", "px", "pi", "kern", "gm", "gmdraw", "gmpx", "refa", "refn")

    # Build the PHP body as one helper function per stub. Each helper holds a
    # SINGLE try/catch around the stub call and returns 1 on a match, 0
    # otherwise. main() sums the helpers. This is deliberate: the EIR backend's
    # exception-cleanup path for discarded refcounted return values (Gmagick's
    # fluent methods return the object) grows super-linearly with the number of
    # try/catches in ONE function, and at ~8 it overflows the conditional-branch
    # range ("fixup value out of range"). One try/catch per function keeps every
    # function in the linear regime, so a program with hundreds of stub checks
    # still assembles and runs.
    funcs = []
    calls = []
    expected = 0
    idx = 0
    for cls, _, _, _ in classes:
        cov = coverage.get(cls, [])
        if not cov:
            continue
        funcs.append("// --- %s ---" % cls)
        for name, is_static, call, hs in cov:
            # the receiver object counts as a needed helper for instance calls.
            needed = set(hs)
            if not is_static:
                needed.add(instance[cls].lstrip("$"))
            decls = ["    $%s = %s;" % (k, helper_ctor[k]) for k in helper_order if k in needed]
            if is_static:
                call_php = cls + "::" + call.split("::", 1)[1]
            else:
                call_php = instance[cls] + "->" + call.split("->", 1)[1]
            f = ["function _cov_%d() {" % idx]
            f.extend(decls)
            f.append("    try { " + call_php + "; } catch (\\Exception $e) {")
            f.append('        if (strpos($e->getMessage(), "not supported in elephc") !== false) { return 1; }')
            f.append("    }")
            f.append("    return 0;")
            f.append("}")
            funcs.append("\n".join(f))
            calls.append("$n += _cov_%d();" % idx)
            idx += 1
            expected += 1
    body = ["$n = 0;"]
    body.extend(calls)
    body.append('echo $n . "/" . ' + str(expected) + ";")
    php = "<?php\n" + "\n".join(funcs) + "\n" + "\n".join(body) + "\n"
    expected_str = str(expected)

    rs = []
    rs.append("//! Purpose:")
    rs.append("//! Coverage test for the %s-family API-surface throwing stubs." % label.capitalize())
    rs.append("//!")
    rs.append("//! Called from:")
    rs.append("//! - `cargo test` through Rust's test harness.")
    rs.append("//!")
    rs.append("//! Key details:")
    rs.append("//! - Every declared stub is called with type-default args (optional params")
    rs.append("//!   omitted) inside a try/catch; the test asserts each throws a")
    rs.append("//!   `*Exception(\"... not supported in elephc\")`, proving the signature")
    rs.append("//!   type-checks, is callable, and throws at runtime.")
    rs.append("")
    rs.append("use crate::support::*;")
    rs.append("")
    rs.append('/// Calls every %s-family throwing stub and asserts each throws its' % label.capitalize())
    rs.append('/// `*Exception("... not supported in elephc")`.')
    rs.append("#[test]")
    rs.append("fn %s() {" % fam["test_name"])
    rs.append("    let out = compile_and_run(")
    # raw string with `##` delimiters: the PHP contains `"` but never `"##`,
    # so a `r##"..."##` literal embeds it verbatim (real newlines, no escaping).
    rs.append("        r##\"" + php + "\"##,")
    rs.append("    );")
    rs.append('    assert_eq!(out, "%d/%d");' % (expected, expected))
    rs.append("}")
    rs.append("")
    out_path = os.path.join(ROOT, "tests", "codegen", "image", fam["test_file"])
    open(out_path, "w", encoding="utf-8").write("\n".join(rs))


if __name__ == "__main__":
    main()