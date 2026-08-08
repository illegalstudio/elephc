#!/usr/bin/env python3
"""Generate throwing-stub declarations + a coverage test for the PHP image OOP
API surface (Imagick / Gmagick families).

The prelude these stubs go into is `synthetic_class` builder calls, so that is what is emitted:
`.method(method("x").param(...).body(vec![s_throw(...)]))`, not PHP text. The transcription rules
below are unchanged — they decide the SIGNATURE, and are independent of how it is rendered.

Reads the compact method spec `crates/elephc-image/tools/api_spec.json` (one
entry per class: name / static / params / return, extracted from the php.net
manual — see `crates/elephc-image/tools/README.md` for provenance and
licensing), extracts
the implemented-method set per class from `src/image_prelude.rs`, and emits one
stub block per class (spliced into the prelude) plus a coverage test that calls
every stub with type-default args and asserts each throws its
`*Exception("... not supported in elephc")`.

To refresh the spec when php.net adds methods, update `api_spec.json` directly
(it is the pinned, human-reviewable input); the rendered HTML pages are no
longer vendored.

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

import json
import os
import re
import sys

def _find_repo_root(start):
    """Walk up from `start` to the workspace root (the dir whose `Cargo.toml`
    declares `[workspace]`). The generator lives inside the elephc-image crate
    but edits files in the top-level compiler crate (`src/image_prelude.rs`,
    `tests/codegen/image/`), so it must locate the repo root regardless of how
    deeply the script is nested."""
    d = start
    while True:
        cargo = os.path.join(d, "Cargo.toml")
        if os.path.isfile(cargo):
            try:
                if "[workspace]" in open(cargo, encoding="utf-8").read():
                    return d
            except OSError:
                pass
        parent = os.path.dirname(d)
        if parent == d:
            raise SystemExit("could not locate workspace root (Cargo.toml with [workspace])")
        d = parent


HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = _find_repo_root(HERE)
PRELUDE = os.path.join(ROOT, "src", "image_prelude.rs")
SPEC_FILE = os.path.join(HERE, "api_spec.json")

# Each class: (class_name, exception class, instance-ctor php expr used by the
# coverage test). The method list comes from `api_spec.json`, keyed by name.
IMAGICK_FAMILY = [
    ("Imagick", "ImagickException", "new Imagick()"),
    ("ImagickDraw", "ImagickDrawException", "new ImagickDraw()"),
    ("ImagickPixel", "ImagickPixelException", 'new ImagickPixel()'),
    ("ImagickPixelIterator", "ImagickPixelIteratorException", "new ImagickPixelIterator(new Imagick())"),
    ("ImagickKernel", "ImagickKernelException", "new ImagickKernel()"),
]
GMAGICK_FAMILY = [
    ("Gmagick", "GmagickException", "new Gmagick()"),
    ("GmagickDraw", "GmagickDrawException", "new GmagickDraw()"),
    ("GmagickPixel", "GmagickPixelException", 'new GmagickPixel()'),
]

# A family bundles its classes with the coverage-test file/test-name it emits.
FAMILIES = [
    {"classes": IMAGICK_FAMILY, "test_file": "imagick_api_surface.rs",
     "test_name": "test_imagick_api_surface_all_stubs_throw"},
    {"classes": GMAGICK_FAMILY, "test_file": "gmagick_api_surface.rs",
     "test_name": "test_gmagick_api_surface_all_stubs_throw"},
]

# All classes across families (for the prelude splice pass).
CLASSES = [c for fam in FAMILIES for c in fam["classes"]]

# Magic methods that must NOT be stubbed (they intercept undefined-method calls
# and would change call resolution). __construct/__destruct are implemented.
SKIP_MAGIC = {"__call", "__callStatic"}

# Marker comments bracketing each auto-generated stub block so re-runs are
# idempotent (the splicer replaces the bracketed region instead of appending).
MARK_BEGIN = "// --- begin auto-generated API-surface throwing stubs (do not edit; regen via crates/elephc-image/tools/gen_image_api_stubs.py) ---"
MARK_END = "// --- end auto-generated API-surface stubs ---"

# Canonical casing for image OOP class names; php.net synopses sometimes
# lowercase them (e.g. `gmagick $x`), so type annotations are normalized.
CANON_CLASS = {
    "imagick": "Imagick", "imagickdraw": "ImagickDraw", "imagickpixel": "ImagickPixel",
    "imagickpixeliterator": "ImagickPixelIterator", "imagickkernel": "ImagickKernel",
    "gmagick": "Gmagick", "gmagickdraw": "GmagickDraw", "gmagickpixel": "GmagickPixel",
}


def load_spec():
    """Load the compact per-class method spec from `api_spec.json`.

    Returns `{class_name: [method, ...]}` where each method is a dict with
    `name` / `static` / `params` (type/name/byref/default) / `ret`, matching the
    shape the prelude splicer and coverage emitter consume.
    """
    with open(SPEC_FILE, encoding="utf-8") as f:
        return json.load(f)


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
    """Return {class_name: set(lowercased method names)} declared in the prelude.

    The prelude is Rust builder calls, not PHP: a class is `class("Imagick")` inside a
    `fn decl_class_imagick()`, and its methods are `method("name")`. The auto-generated region is
    excluded via the marker comments so a re-run does not mistake its own stubs for
    implementations and skip re-emitting them.
    """
    classes = {}
    for m in re.finditer(r'^fn (decl_class_\w+)\(\) -> Stmt \{', prelude_src, re.M):
        start = m.end()
        end = prelude_src.find("\nfn ", start)
        if end == -1:
            end = len(prelude_src)
        body = prelude_src[start:end]
        name = re.search(r'class\("(\w+)"\)', body)
        if not name:
            continue
        body = re.sub(re.escape(MARK_BEGIN) + r".*?" + re.escape(MARK_END), "", body, flags=re.S)
        classes[name.group(1)] = {
            mm.group(1).lower() for mm in re.finditer(r'method\("([^"]+)"\)', body)
        }
    return classes


def rust_type(php_type):
    """One PHP type as the `synthetic_class` expression that builds it."""
    if not php_type:
        return None
    if php_type.startswith("?"):
        return "t_nullable(%s)" % rust_type(php_type[1:])
    if "|" in php_type:
        members = ", ".join(rust_type(part) for part in php_type.split("|"))
        return "t_union(vec![%s])" % members
    simple = {
        "int": "TypeExpr::Int",
        "float": "TypeExpr::Float",
        "string": "TypeExpr::Str",
        "bool": "TypeExpr::Bool",
        "void": "TypeExpr::Void",
        "mixed": "t_mixed()",
        "array": "t_array()",
    }
    return simple.get(php_type, 't_class("%s")' % php_type)


def rust_default(php_default):
    """One PHP default literal as the expression that builds it.

    Recognised by SHAPE rather than from a fixed table: the spec carries whatever php.net
    documents, so a table is a list of the defaults that happened to exist when it was written.
    Anything genuinely unrecognised still stops the generator instead of being guessed at.
    """
    fixed = {
        "false": "e_bool(false)",
        "true": "e_bool(true)",
        "[]": "e_array(vec![])",
        "null": "e_null()",
    }
    if php_default in fixed:
        return fixed[php_default]
    if re.fullmatch(r'"[^"]*"', php_default):
        return "e_str(%s)" % php_default
    if re.fullmatch(r"-?\d+", php_default):
        value = int(php_default)
        # A negative literal is `Negate(IntLiteral)` in the AST, not a negative IntLiteral.
        return "e_neg(e_int(%d))" % -value if value < 0 else "e_int(%d)" % value
    if re.fullmatch(r"-?\d+\.\d+", php_default):
        value = float(php_default)
        return "e_neg(e_float(%r))" % -value if value < 0 else "e_float(%r)" % value
    raise SystemExit("unmodelled stub default: %r" % php_default)


def emit_stub(method, cls, exc):
    """One throwing stub, as the builder calls that construct it.

    The signature comes from the same `map_param_type` / `map_default` / `map_return` rules the
    PHP emitter used; only the rendering differs. The unused-parameter assignments are kept
    because the checker warns on an unread parameter and a stub reads none of them.

    The indentation reproduces the hand-written methods around it — the block is spliced into a
    live builder chain, so anything else would make the generated region visibly foreign.
    """
    name = method["name"]
    out = ['            method("%s")' % name]
    if method["static"]:
        out.append("                .static_()")

    body = []
    for p in method["params"]:
        ptype = map_param_type(p["type"], cls)
        raw = p["name"]
        pname = raw[0].lower() + raw[1:] if raw and raw[0].isupper() else raw
        d = map_default(p["default"], ptype, p["byref"])
        rt = rust_type(ptype)

        if p["byref"]:
            hint = "Some(%s)" % rt if rt else "None"
            if d is None:
                out.append('                .param_by_ref("%s", %s)' % (pname, hint))
            else:
                out.append('                .param_by_ref_default("%s", %s, %s)'
                           % (pname, hint, rust_default(d)))
        elif rt is None:
            if d is None:
                out.append('                .param_untyped("%s")' % pname)
            else:
                out.append('                .param_untyped_default("%s", %s)' % (pname, rust_default(d)))
        elif d is None:
            out.append('                .param("%s", %s)' % (pname, rt))
        else:
            out.append('                .param_default("%s", %s, %s)' % (pname, rt, rust_default(d)))
        body.append('                    s_assign("_u_%s", e_var("%s")),' % (pname, pname))

    ret = rust_type(map_return(method["ret"], cls))
    if ret:
        out.append("                .returns(%s)" % ret)

    body.append('                    s_throw(e_new("%s", vec![e_str("%s::%s() is not supported in elephc")])),'
                % (exc, cls, name))
    out.append("                .body(vec![")
    out.extend(body)
    out.append("                ]),")
    return "        .method(\n" + "\n".join(out) + "\n        )"


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
    spec = load_spec()
    prelude_src = open(PRELUDE, encoding="utf-8").read()
    implemented = implemented_methods_per_class(prelude_src)

    stub_blocks = {}      # cls -> list of stub strings
    coverage = {}         # cls -> list of (name, static, call, helpers)
    stats = {}

    for cls, exc, _ctor in CLASSES:
        methods = spec.get(cls)
        if methods is None:
            print("MISSING spec entry:", cls, file=sys.stderr)
            continue
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

    # per family: write a coverage test file
    for fam in FAMILIES:
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
    for cls, _, _ in CLASSES:
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
    for cls, _, _ in CLASSES:
        stubs = stub_blocks.get(cls, [])
        if not stubs:
            continue
        # Canonical block content, no leading/trailing newline: it sits among the chain's
        # other `.method(...)` calls, between the last hand-written one and `.build()`.
        block = "        " + MARK_BEGIN + "\n" + "\n".join(stubs) + "\n        " + MARK_END
        # locate the class builder
        m = re.search(r'\bclass\("' + cls + r'"\)', src)
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
            # already spliced: swap the marker-bounded region in place. The pattern
            # matches the markers at the chain's own indentation and leaves the surrounding
            # newlines untouched, so re-running is a no-op on whitespace (idempotent).
            pat = re.compile(re.escape("        " + MARK_BEGIN) + r".*?" + re.escape("        " + MARK_END), re.S)
            new_body = pat.sub(block, body, count=1)
        else:
            # first splice: the chain's terminating `.build()` closes the class, so the
            # block goes just above it — after `body` would be past the expression entirely.
            anchor = body.rfind("\n        .build()")
            if anchor == -1:
                print("  WARN: no .build() terminator for %s; skipping" % cls, file=sys.stderr)
                continue
            new_body = body[:anchor] + "\n" + block + body[anchor:]
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
    for cls, _, _ in classes:
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