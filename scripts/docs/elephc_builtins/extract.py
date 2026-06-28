"""Extract builtin metadata from the Elephc source tree.

We parse three layers:

1. ``src/types/checker/builtins/catalog.rs`` — the canonical list of
   PHP-visible builtins. This is our *set* of builtins.
2. ``src/types/signatures.rs`` — per-builtin canonical call signatures
   (param names, variadic, by-ref, first-class return type).
3. ``src/codegen_ir/lower_inst/builtins.rs`` and the per-area submodules
   — for each builtin we capture the lowering function name, the runtime
   helpers it calls, and the leading /// doc comment of that function.

The output is a list of :class:`registry.Builtin` written to a JSON file
in ``scripts/docs/builtin_registry.json``.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Optional

# Make ``registry`` importable when running this file directly.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from registry import (  # noqa: E402  (sys.path tweak above)
    AREA_BY_FILE,
    AREA_BY_LOWERING_FN,
    AREA_BY_MODULE,
    AREA_BY_NAME,
    AREAS,
    Builtin,
    BuiltinSig,
    DESCRIPTION_OVERRIDES,
    INTERNAL_NOTES,
    LoweringInfo,
    OPTIONAL_PARAM_OVERRIDES,
    PARAM_NAME_OVERRIDES,
    PARAM_TYPES,
    REF_PARAM_OVERRIDES,
    Parameter,
    RETURN_TYPE_OVERRIDES,
    VARIADIC_OVERRIDES,
    slug,
)


# ---------------------------------------------------------------------------
# catalog.rs
# ---------------------------------------------------------------------------

# We pull SUPPORTED_BUILTIN_FUNCTIONS and INTERNAL_BUILTIN_FUNCTIONS straight
# out of catalog.rs. The file is a simple list-of-string-literals, so a small
# state machine is enough — no need for a full Rust parser.

def parse_catalog(path: Path) -> tuple[list[str], list[str]]:
    """Return (supported_names, internal_names) from catalog.rs."""
    src = path.read_text(encoding="utf-8")
    return _extract_string_list(src, "SUPPORTED_BUILTIN_FUNCTIONS"), _extract_string_list(
        src, "INTERNAL_BUILTIN_FUNCTIONS"
    )


def _extract_string_list(src: str, const_name: str) -> list[str]:
    pattern = re.compile(
        r"const\s+" + re.escape(const_name) + r"\s*:\s*&?\[?&?str\]\s*=\s*&?\[(.*?)\];",
        re.DOTALL,
    )
    match = pattern.search(src)
    if not match:
        return []
    body = match.group(1)
    return re.findall(r'"([^"]+)"', body)


# ---------------------------------------------------------------------------
# signatures.rs
# ---------------------------------------------------------------------------

# Each arm of `builtin_call_sig` looks like:
#     "name" | "alias" | ... => Some(fixed(&["a", "b"])),
# or
#     "name" => Some(optional(&["a", "b"], 2, vec![int_lit(0)])),
# or
#     "name" => Some(variadic(&["a"], "rest")),
# or
#     "name" => {
#         let mut sig = first_param_ref(fixed(&["a", "b"]));
#         sig.ref_params[2] = true;
#         Some(sig)
#     }
#
# And `first_class_callable_builtin_sig` / `general_first_class_callable_builtin_sig`
# carry the canonical return type:
#     "name" => Some(typed_first_class_builtin_sig(name, &[PhpType::Str], PhpType::Str))
# or
#     "name" => Some(FunctionSig { return_type: PhpType::Int, ... })

_FN_CALL_SIG_RE = re.compile(
    r'"([^"]+)"\s*(?:\|\s*"[^"]+"\s*)*=>\s*Some\(([a-z_]+)\(([^)]*)\)\)',
    re.DOTALL,
)


def _split_name_list(arm: str) -> list[str]:
    return re.findall(r'"([^"]+)"', arm)


def _split_args(args: str) -> list[str]:
    # split top-level commas only — args of variadic(&["a"], "b") must stay grouped
    depth = 0
    parts: list[str] = []
    buf: list[str] = []
    for ch in args:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(buf).strip())
            buf = []
        else:
            buf.append(ch)
    if buf:
        parts.append("".join(buf).strip())
    return parts


def _parse_param_list(s: str) -> list[str]:
    """`&["a", "b"]` → ['a', 'b']"""
    return re.findall(r'"([^"]+)"', s)


def _parse_optional_defaults(s: str) -> dict[int, str]:
    """`vec![int_lit(0), string_lit(" ")]` → {0: '0', 1: "' '"}"""
    out: dict[int, str] = {}
    for idx, expr in enumerate(_split_args(s)):
        m = re.match(r"(int|bool|string|null)_lit\((.*)\)$", expr.strip())
        if m:
            kind, raw = m.group(1), m.group(2).strip()
            if kind == "int":
                out[idx] = raw
            elif kind == "bool":
                out[idx] = raw
            elif kind == "string":
                out[idx] = repr(raw)  # render as PHP string
            elif kind == "null":
                out[idx] = "null"
    return out


def _default_expr_renderer(expr_kind: str, raw: str) -> str:
    """Render a few special default expressions from signatures.rs."""
    expr_kind = expr_kind.strip()
    raw = raw.strip()
    if expr_kind == "FloatLiteral":
        # default for log() base is e — render as M_E
        return "M_E"
    if expr_kind == "ArrayLiteral":
        return "[]"
    if expr_kind in ("int", "Int"):
        return raw
    if expr_kind in ("bool", "Bool"):
        return raw
    if expr_kind in ("string", "Str"):
        return repr(raw)
    if expr_kind in ("null", "Null"):
        return "null"
    return raw


def _extract_function_body(src: str, fn_name: str) -> str:
    """Return the body of `pub(crate) fn <fn_name>(...)` (between matching braces), or ''."""
    for prefix in ("pub(crate) ", "pub(super) ", "pub ", ""):
        marker = f"{prefix}fn {fn_name}("
        start = src.find(marker)
        if start >= 0:
            break
    else:
        return ""
    brace = src.find("{", start)
    if brace < 0:
        return ""
    depth = 0
    for i in range(brace, len(src)):
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
            if depth == 0:
                return src[brace : i + 1]
    return ""


def _split_match_arms(body: str) -> list[str]:
    """Split a `match` body into individual arms.

    Each arm ends either at a top-level `,` (direct-expression arm) or at the
    closing `}` of an arm-block. The final closing `}` of the match itself
    (which has no leading `,`) terminates the walk.
    """
    arms: list[str] = []
    i = 0
    while i < len(body):
        m = re.search(r'"([^"]+)"(?:\s*\|\s*"([^"]+)")*\s*=>\s*', body[i:])
        if not m:
            break
        arm_start = i + m.start()
        rhs_start = i + m.end()
        # If the arm body starts with `{` (after optional whitespace/newlines),
        # the matching `}` ends the arm.
        scan = rhs_start
        while scan < len(body) and body[scan] in " \t\n\r":
            scan += 1
        is_block = scan < len(body) and body[scan] == "{"
        depth = 0
        j = rhs_start
        in_str = False
        ended_on = None  # "," or "}" once we terminate
        while j < len(body):
            ch = body[j]
            if ch == "\\" and j + 1 < len(body):
                j += 2
                continue
            if ch == '"':
                in_str = not in_str
            elif not in_str:
                if ch in "([{":
                    depth += 1
                elif ch in ")]}":
                    if depth == 0:
                        # Closing `}` of the whole match (not inside any arm).
                        ended_on = "}"
                        break
                    depth -= 1
                    # After decrement, depth may have hit 0: a block arm's
                    # closing `}` ends the arm here.
                    if is_block and depth == 0 and ch == "}":
                        ended_on = "}"
                        break
                elif ch == "," and depth == 0:
                    ended_on = ","
                    j += 1
                    break
            j += 1
        # Include the closing `}` for block arms so `_parse_rhs` sees a
        # balanced `{ ... }` (it relies on rhs.endswith("}")).
        end = j + 1 if (is_block and ended_on == "}") else j
        arm_text = body[arm_start:end].rstrip().rstrip(",").rstrip()
        arms.append(arm_text)
        if ended_on == "}" and not is_block:
            # closing of the whole match
            break
        i = j
    return arms


def parse_builtin_call_sigs(path: Path) -> dict[str, dict]:
    """Return {name: {params, variadic, ref_params, required}} from signatures.rs::builtin_call_sig."""
    src = path.read_text(encoding="utf-8")
    body = _extract_function_body(src, "builtin_call_sig")
    if not body:
        return {}
    out: dict[str, dict] = {}
    for arm in _split_match_arms(body):
        # names: everything before " =>"
        head, _, rhs = arm.partition("=>")
        names = _split_name_list(head)
        info = _parse_rhs(rhs)
        for n in names:
            out[n] = info
    return out


def _parse_rhs(rhs: str) -> dict:
    """Parse a single arm RHS — either a direct `Some(builder(...))` or a block with `let mut sig = ...`."""
    rhs = rhs.strip().rstrip(",")
    # Strip leading/trailing braces if it's a block.
    block: Optional[str] = None
    if rhs.startswith("{") and rhs.endswith("}"):
        block = rhs[1:-1]
        # Case A: `let mut sig = ...` (custom builder with overrides).
        m_let = re.search(r"let\s+mut\s+sig\s*=\s*([^;]+);", block)
        if m_let:
            builder_expr = m_let.group(1).strip()
            ref_overrides = {
                int(m.group(1)): m.group(2).strip() == "true"
                for m in re.finditer(r"sig\.ref_params\[(\d+)\]\s*=\s*(true|false)", block)
            }
        else:
            # Case B: block with a single `Some(builder(...))` expression.
            m_some = re.search(r"Some\s*\(\s*(.+?)\s*\)\s*(?:;|$)", block, re.DOTALL)
            if not m_some:
                return {}
            builder_expr = m_some.group(1).strip()
            ref_overrides = {
                int(m.group(1)): m.group(2).strip() == "true"
                for m in re.finditer(r"sig\.ref_params\[(\d+)\]\s*=\s*(true|false)", block)
            }
    else:
        # form: Some(builder(args))
        m = re.match(r"Some\s*\(\s*(.+?)\s*\)\s*$", rhs, re.DOTALL)
        if not m:
            return {}
        builder_expr = m.group(1).strip()
        ref_overrides = {}

    return _parse_builder(builder_expr, ref_overrides)


def _parse_builder(expr: str, ref_overrides: dict[int, bool]) -> dict:
    """Parse `fixed(&["a", "b"])` / `optional(...)` / `variadic(...)` / `first_param_ref(...)`.

    All regexes use `re.DOTALL` and tolerate whitespace between tokens, because
    the Rust source often spans multiple lines and inserts newlines + indentation.
    """
    # unwrap first_param_ref(...) if present
    m = re.match(r"first_param_ref\s*\(\s*(.+?)\s*\)\s*$", expr, re.DOTALL)
    if m:
        inner = m.group(1).strip()
        ref_overrides = {0: True, **ref_overrides}
        return _parse_builder(inner, ref_overrides)

    m = re.match(r"fixed\s*\(\s*&\[(.*?)\]\s*\)", expr, re.DOTALL)
    if m:
        params = _parse_param_list(m.group(1))
        return {
            "params": [{"name": p, "by_ref": ref_overrides.get(i, False), "default": None, "optional": False} for i, p in enumerate(params)],
            "variadic": None,
            "required": len(params),
        }
    m = re.match(r"optional\s*\(\s*&\[(.*?)\]\s*,\s*(\d+)\s*,\s*(.+?)\s*\)\s*$", expr, re.DOTALL)
    if m:
        params = _parse_param_list(m.group(1))
        required = int(m.group(2))
        defaults = _parse_optional_defaults(m.group(3))
        result = []
        for i, p in enumerate(params):
            opt = i >= required
            default = defaults.get(i - required)
            result.append({"name": p, "by_ref": ref_overrides.get(i, False), "default": default, "optional": opt})
        return {"params": result, "variadic": None, "required": required}
    m = re.match(r"variadic\s*\(\s*&\[(.*?)\]\s*,\s*\"([^\"]+)\"\s*\)\s*$", expr, re.DOTALL)
    if m:
        params = _parse_param_list(m.group(1))
        variadic = m.group(2)
        result = [{"name": p, "by_ref": ref_overrides.get(i, False), "default": None, "optional": False} for i, p in enumerate(params)]
        return {"params": result, "variadic": variadic, "required": len(params)}
    return {"params": [], "variadic": None, "required": 0}


def parse_first_class_return_types(path: Path) -> dict[str, str]:
    """Return {name: return_type_string} from signatures.rs::general_first_class_callable_builtin_sig."""
    src = path.read_text(encoding="utf-8")
    out: dict[str, str] = {}

    # explicit FunctionSig blocks (strlen, count, buffer_len, ...)
    for m in re.finditer(
        r'"([^"]+)"\s*=>\s*Some\(FunctionSig\s*\{[^}]*?return_type:\s*(PhpType::[A-Za-z0-9_]+(?:::[A-Za-z0-9_]+)?(?:\([^)]*\))?)[^}]*?\}',
        src,
        re.DOTALL,
    ):
        out[m.group(1)] = _render_phptype(m.group(2))

    # typed_first_class_builtin_sig(name, &[PhpType::X, ...], PhpType::Y)
    for m in re.finditer(
        r'typed_first_class_builtin_sig\(\s*name\s*,\s*&\[([^\]]*)\]\s*,\s*(PhpType::[A-Za-z0-9_]+(?:::[A-Za-z0-9_]+)?(?:\([^)]*\))?)\s*\)',
        src,
    ):
        # we don't know the names here without re-parsing the match arm; resolve by walking backward to find the most recent arm header
        # simpler: just use the previous match in src order
        pass

    # General "name | "name" => Some(typed_first_class_builtin_sig(...))" pattern
    arm_re = re.compile(
        r'"([^"]+)"(?:\s*\|\s*"[^"]+")*\s*=>\s*Some\(typed_first_class_builtin_sig\(\s*name\s*,\s*&\[([^\]]*)\]\s*,\s*(PhpType::[A-Za-z0-9_]+(?:::[A-Za-z0-9_]+)?(?:\([^)]*\))?)\)\)',
    )
    for m in arm_re.finditer(src):
        for name in _split_name_list(m.group(0).split("=>", 1)[0]):
            out[name] = _render_phptype(m.group(3))

    # return_typed_first_class_builtin_sig(name, PhpType::X)
    arm_re2 = re.compile(
        r'"([^"]+)"(?:\s*\|\s*"[^"]+")*\s*=>\s*return_typed_first_class_builtin_sig\(\s*name\s*,\s*(PhpType::[A-Za-z0-9_]+(?:::[A-Za-z0-9_]+)?(?:\([^)]*\))?)\s*\)',
    )
    for m in arm_re2.finditer(src):
        for name in _split_name_list(m.group(0).split("=>", 1)[0]):
            out[name] = _render_phptype(m.group(2))

    return out


def _render_phptype(ty: str) -> str:
    """Render a PhpType expression as a short user-facing string."""
    ty = ty.strip()
    mapping = {
        "PhpType::Int": "int",
        "PhpType::Float": "float",
        "PhpType::Bool": "bool",
        "PhpType::Str": "string",
        "PhpType::Void": "void",
        "PhpType::Null": "null",
        "PhpType::Mixed": "mixed",
        "PhpType::Never": "never",
    }
    if ty in mapping:
        return mapping[ty]
    if ty.startswith("PhpType::Array"):
        return "array"
    if ty.startswith("PhpType::Buffer"):
        return "buffer"
    if ty.startswith("PhpType::Union"):
        return "mixed"
    if ty.startswith("PhpType::AssocArray"):
        return "array"
    return "mixed"


# ---------------------------------------------------------------------------
# lowering: builtins.rs and per-area submodules
# ---------------------------------------------------------------------------

# Each dispatch arm in `lower_builtin_call` matches either a single name
# or a `|`-separated list. e.g. `"strlen" => lower_strlen(ctx, inst),`

DISPATCH_ARM_RE = re.compile(
    r'"([^"]+)"(?:\s*\|\s*"[^"]+")*\s*=>\s*([A-Za-z_][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*|lower_[A-Za-z0-9_]+|lower_unary_libm)\(',
)


def parse_lowering_dispatch(path: Path) -> dict[str, tuple[str, str, int]]:
    """Return {name: (module, codegen_function, line)} for arms in builtins.rs
    that dispatch a *single* builtin name to a dedicated lowering function.

    Multi-name arms (e.g. catch-all dispatchers that handle a whole family) are
    skipped, so the per-name entry wins when both exist.
    """
    src_lines = path.read_text(encoding="utf-8").splitlines()
    out: dict[str, tuple[str, str, int]] = {}
    for lineno, line in enumerate(src_lines, start=1):
        m = DISPATCH_ARM_RE.search(line)
        if not m:
            continue
        names_in_arm = re.findall(r'"([^"]+)"', line.split("=>", 1)[0])
        if len(names_in_arm) != 1:
            continue
        n = names_in_arm[0]
        out[n] = (m.group(2), m.group(3), lineno)
    return out


DOC_COMMENT_RE = re.compile(r"^///\s?(.*)$")


def _leading_doc_comment(src: str, line: int) -> str:
    """Return the /// doc comment block immediately above a function definition at `line`."""
    lines = src.splitlines()
    i = line - 2  # 1-based
    out: list[str] = []
    while i >= 0 and lines[i].lstrip().startswith("///"):
        m = DOC_COMMENT_RE.match(lines[i])
        if m:
            out.append(m.group(1).strip())
        i -= 1
    out.reverse()
    return "\n".join(out)


def find_lowering_function_def(
    src: str, fn_name: str
) -> Optional[tuple[str, int]]:
    """Find the (path, line) of `fn <fn_name>(` in `src`."""
    lines = src.splitlines()
    for i, line in enumerate(lines, start=1):
        if re.match(rf"\s*(pub(?:\([^)]*\))?\s+)?fn\s+{re.escape(fn_name)}\s*\(", line):
            return (line, i)
    return None


def collect_runtime_helpers(notes: str, body: str) -> list[str]:
    """Find `__rt_*` symbols in the doc comment and the lowering body."""
    found = set(re.findall(r"\b__rt_[A-Za-z0-9_]+", notes)) | set(
        re.findall(r"\b__rt_[A-Za-z0-9_]+", body)
    )
    return sorted(found)


def parse_area_for_file(rel_path: str) -> tuple[Optional[str], str]:
    """Look up the (area, sub_area) for a given relative file path.

    Returns (None, "") as a sentinel when the file is the root dispatcher
    (builtins/builtins.rs) and the area should be inferred from the
    dispatch module/function instead.
    """
    key = rel_path.replace("builtins/", "").replace("builtins\\", "")
    if key in AREA_BY_FILE:
        val = AREA_BY_FILE[key]
        if val is None:
            return (None, "")  # sentinel
        return val
    # try the basename if no submodule match
    base = Path(key).name
    if base in AREA_BY_FILE:
        val = AREA_BY_FILE[base]
        if val is None:
            return (None, "")
        return val
    return ("Misc", "Misc")


def parse_check_builtin_returns(path: Path) -> dict[str, str]:
    """Return {name: return_type} extracted from check_builtin() in a checker file.

    We scan the body line-by-line, tracking:
    - the set of names that make up the current arm (multi-name and multi-line
      headers are handled by accumulating names until we see `=>` or a `}`),
    - and the return type as the *last* `Ok(Some(PhpType::<X>))` we observe
      before the arm closes.

    Conditional arms like `min`/`max` keep the *last* such return, which is the
    more specific (Float) branch.
    """
    text = path.read_text(encoding="utf-8")
    out: dict[str, str] = {}
    re_phptype = re.compile(
        r"Ok\s*\(\s*Some\s*\(\s*(PhpType::[A-Za-z0-9_]+(?:::[A-Za-z0-9_]+)?(?:\([^)]*\))?)\s*\)\s*\)"
    )
    re_name = re.compile(r'"([^"]+)"')
    in_match = False
    current_names: list[str] = []
    pending_names: list[str] = []  # names found on a line that didn't have `=>` yet
    arm_indent: int | None = None
    for line in text.splitlines():
        # Skip Rust string literals when scanning for tokens.
        # We do this by re-tokenising the line on quotes.
        if not in_match:
            if "match name" in line or re.search(r"match\s+\w+\s*\{", line):
                in_match = True
            continue
        # Detect a "names" line: a line with one or more `"name"` segments.
        names = re_name.findall(line)
        if "=>" in line:
            # commit pending names + this line's names
            current_names = list(dict.fromkeys(current_names + pending_names + names))
            pending_names = []
            arm_indent = len(line) - len(line.lstrip())
            continue
        if names:
            pending_names.extend(names)
        # Look for the return type on any line in the arm
        if current_names:
            m = re_phptype.search(line)
            if m:
                rt = _render_phptype(m.group(1))
                for n in current_names:
                    out[n] = rt
                current_names = []
        # arm ends at a `}` at the same indent as the header
        if arm_indent is not None and line.strip() == "}":
            cur_indent = len(line) - len(line.lstrip())
            if cur_indent == arm_indent:
                current_names = []
                pending_names = []
                arm_indent = None
    return out


def parse_check_builtin_param_types(path: Path) -> dict[str, list[tuple[int, str]]]:
    """Return {name: [(arg_index, type_str), ...]} extracted from check_builtin().

    We scan the arm body and watch for the pattern:
        let <var> = checker.infer_type(&args[N], env)?;
        if !matches!(<var>, PhpType::X | PhpType::Y) { ... }   # X|Y
        if <var> != PhpType::X { ... }                          # X
        match <var> { PhpType::X => ..., _ => Err }             # X

    For each such pair we record (N, type). For the common "must be array" /
    "must be string" / "must be int" patterns this gives us precise param
    types. The renderer picks the *first* declared type per arg index.
    """
    text = path.read_text(encoding="utf-8")
    out: dict[str, list[tuple[int, str]]] = {}
    in_match = False
    current_names: list[str] = []
    pending_names: list[str] = []
    arm_indent: int | None = None
    # Map from inferred-var name -> (arg_index, type_str) — collected while
    # scanning the current arm.
    inferred: dict[str, tuple[int, str]] = {}
    for line in text.splitlines():
        if not in_match:
            if "match name" in line or re.search(r"match\s+\w+\s*\{", line):
                in_match = True
            continue
        names = re.findall(r'"([^"]+)"', line)
        if "=>" in line:
            current_names = list(dict.fromkeys(current_names + pending_names + names))
            pending_names = []
            arm_indent = len(line) - len(line.lstrip())
            inferred = {}  # reset for new arm
            continue
        if names:
            pending_names.extend(names)
        # Pattern 1: `let <var> = checker.infer_type(&args[N], env)?;`
        m_infer = re.search(
            r"let\s+(\w+)\s*=\s*checker\.infer_type\(&args\[(\d+)\]\s*,\s*env\)?\s*;?",
            line,
        )
        if m_infer:
            var_name = m_infer.group(1)
            arg_idx = int(m_infer.group(2))
            # If we already saw a constraint on this var, use it now.
            if var_name in inferred:
                idx, ty = inferred[var_name]
                inferred[var_name] = (idx, ty)
            else:
                inferred[var_name] = (arg_idx, "mixed")
            continue
        # Pattern 2: `if !matches!(<var>, PhpType::X | PhpType::Y) {`
        m_match = re.search(
            r"if\s+!matches!\(\s*(\w+)\s*,\s*(PhpType::[A-Za-z0-9_]+(?:\([^)]*\))?(?:\s*\|\s*PhpType::[A-Za-z0-9_]+(?:\([^)]*\))?)*)\s*\)",
            line,
        )
        if m_match:
            var_name = m_match.group(1)
            type_pattern = m_match.group(2)
            # Take the FIRST PhpType from the alternation.
            first = re.match(r"(PhpType::[A-Za-z0-9_]+)", type_pattern)
            if first and var_name in inferred:
                idx, _ = inferred[var_name]
                inferred[var_name] = (idx, _render_phptype(first.group(1)))
            continue
        # Pattern 3: `if <var> != PhpType::X {`
        m_neq = re.search(
            r"if\s+(\w+)\s*!=\s*(PhpType::[A-Za-z0-9_]+(?:\([^)]*\))?)\s*\{",
            line,
        )
        if m_neq:
            var_name = m_neq.group(1)
            type_str = _render_phptype(m_neq.group(2))
            if var_name in inferred:
                idx, _ = inferred[var_name]
                inferred[var_name] = (idx, type_str)
            continue
        # Pattern 4: `match <var> { PhpType::X => ..., _ => Err }` — the first
        # match arm header gives the type. We look for lines that start with
        # `PhpType::X =>` and pair them with the var that was just matched.
        m_match_arm = re.match(
            r"^\s*(PhpType::[A-Za-z0-9_]+(?:\([^)]*\))?)\s*=>",
            line,
        )
        if m_match_arm and inferred:
            # We can't easily tell which var this `match` belongs to without
            # a brace counter. Cheap heuristic: take the most recently assigned
            # inferred var. The patterns above usually appear right after
            # `let <var> = ...`.
            type_str = _render_phptype(m_match_arm.group(1))
            for var, (idx, _) in list(inferred.items()):
                if inferred[var][1] == "mixed":
                    inferred[var] = (idx, type_str)
                    break
        # When the arm closes, commit the inferred types to the current names.
        if arm_indent is not None and line.strip() == "}":
            cur_indent = len(line) - len(line.lstrip())
            if cur_indent == arm_indent and current_names:
                # Group by arg index, keep the first non-mixed type per index.
                by_idx: dict[int, str] = {}
                for _, (idx, ty) in inferred.items():
                    if idx not in by_idx or ty != "mixed":
                        by_idx[idx] = ty
                for n in current_names:
                    out.setdefault(n, []).extend([(i, t) for i, t in sorted(by_idx.items())])
                current_names = []
                pending_names = []
                inferred = {}
                arm_indent = None
    return out


# ---------------------------------------------------------------------------
# Orchestration
# ---------------------------------------------------------------------------

def build_registry(repo: Path) -> list[Builtin]:
    """Build the full list of builtins from the given repo root."""
    src = repo / "src"

    catalog = src / "types" / "checker" / "builtins" / "catalog.rs"
    sigs = src / "types" / "signatures.rs"
    dispatch = src / "codegen_ir" / "lower_inst" / "builtins.rs"
    lowering_dir = src / "codegen_ir" / "lower_inst" / "builtins"

    if not catalog.exists():
        sys.exit(f"catalog.rs not found: {catalog}")
    if not sigs.exists():
        sys.exit(f"signatures.rs not found: {sigs}")
    if not dispatch.exists():
        sys.exit(f"builtins.rs not found: {dispatch}")

    supported, internal = parse_catalog(catalog)
    internal_names = {name.lower() for name in internal}
    catalog_names = list(dict.fromkeys([*supported, *internal]))
    call_sigs = parse_builtin_call_sigs(sigs)
    first_class_returns = parse_first_class_return_types(sigs)
    dispatch_map = parse_lowering_dispatch(dispatch)

    # Cache file contents to avoid re-reading.
    file_cache: dict[Path, str] = {}

    def read(p: Path) -> str:
        if p not in file_cache:
            file_cache[p] = p.read_text(encoding="utf-8")
        return file_cache[p]

    builtins: list[Builtin] = []

    # Pre-parse checker return/param types once. Re-parsing every checker file
    # inside the per-builtin loop is O(N*M); doing it once keeps extraction fast.
    checker_dir = src / "types" / "checker" / "builtins"
    check_builtin_returns: dict[str, str] = {}
    check_builtin_params: dict[str, list[tuple[int, str]]] = {}
    for cb_path in checker_dir.rglob("*.rs"):
        for n, rt in parse_check_builtin_returns(cb_path).items():
            check_builtin_returns[n] = rt
        for n, inferred_params in parse_check_builtin_param_types(cb_path).items():
            check_builtin_params.setdefault(n, []).extend(inferred_params)

    for name in catalog_names:
        canonical = name.lower()
        in_catalog = canonical not in internal_names
        # Any function whose canonical name starts with __elephc_ is a compiler
        # internal helper; it gets an internals page but no user-facing page.
        is_internal = canonical.startswith("__elephc_") or canonical in internal_names

        # Fallback: if signatures.rs didn't yield a result, use the call_sigs
        # entry directly (may be empty) so the builtin still renders.
        sig_info = call_sigs.get(canonical)
        if sig_info is None:
            # No match at all in signatures.rs — use an empty stub.
            sig_info = {"params": [], "variadic": None, "required": 0}
        params = [
            Parameter(
                name=p["name"],
                php_type="mixed",
                by_ref=p["by_ref"],
                default=p["default"],
                optional=p["optional"],
            )
            for p in sig_info.get("params", [])
        ]
        return_type = first_class_returns.get(canonical, "mixed")
        # Second-pass return type from check_builtin() in src/types/checker/builtins/*.rs.
        # This gives precise types for things like `floor -> float`, `is_array -> bool`, etc.
        # It overrides the first_class_returns value when more specific.
        if canonical in check_builtin_returns and check_builtin_returns[canonical] != "mixed":
            return_type = check_builtin_returns[canonical]
        # Apply hand-curated return-type and variadic overrides. These win
        # over anything parsed from check_builtin() / signatures.rs.
        if canonical in RETURN_TYPE_OVERRIDES:
            return_type = RETURN_TYPE_OVERRIDES[canonical]
        if canonical in VARIADIC_OVERRIDES:
            # Force variadic even when signatures.rs says fixed.
            # Re-shape params: drop the fixed stub, switch to variadic.
            vname = VARIADIC_OVERRIDES[canonical]
            params = []
            sig_info["variadic"] = vname
            sig_info.pop("__stub", None)
        # Apply PARAM_TYPES to refine parameter names/types. For each known
        # (arg_index, type) pair, override the parameter's `php_type` when it
        # is currently `mixed` and the inferred type is more specific.
        if canonical in PARAM_TYPES:
            table = PARAM_TYPES[canonical]
            # Normalize entries: accept either `str` or `(type, name)` tuple.
            norm: list[tuple[str, str]] = []
            for entry in table:
                if entry is None:
                    continue
                if isinstance(entry, str):
                    norm.append((entry or "mixed", "value"))
                else:
                    # tuple (type, name)
                    ty, entry_name = entry
                    norm.append((ty or "mixed", entry_name or "value"))

            # Apply name overrides on top of PARAM_TYPES.
            name_overrides = PARAM_NAME_OVERRIDES.get(canonical, [])
            for idx, override_name in enumerate(name_overrides):
                if override_name is not None and idx < len(norm):
                    norm[idx] = (norm[idx][0], override_name)

            # Case A: signatures.rs gave us nothing — materialize all params
            # from PARAM_TYPES (skip when signatures.rs says variadic, the
            # variadic info is the authoritative shape).
            if not params and not sig_info.get("variadic"):
                for ty, entry_name in norm:
                    params.append(Parameter(name=entry_name, php_type=ty))
            else:
                is_single_stub = (
                    len(params) == 1 and params[0].name == "…"
                )
                is_named = all(p.name not in (None, "…") for p in params)
                if is_single_stub or is_named:
                    # Refine existing params in place; PARAM_TYPES can
                    # override the name when it's a tuple.
                    target_count = len(params) if is_named else len(norm)
                    for idx, (ty, entry_name) in enumerate(norm[:target_count]):
                        if 0 <= idx < len(params) and ty and params[idx].php_type == "mixed":
                            new_name = params[idx].name if is_named else entry_name
                            if is_named and entry_name != "value":
                                new_name = entry_name  # explicit override
                            params[idx] = Parameter(
                                name=new_name,
                                php_type=ty,
                                by_ref=params[idx].by_ref,
                                default=params[idx].default,
                                optional=params[idx].optional,
                            )
                # Append any extra params that signatures.rs missed (e.g.
                # `preg_match_all` declares 2 params in signatures.rs but
                # PHP takes a 3rd `&$matches` array). Skip when variadic.
                if not sig_info.get("variadic") and len(norm) > len(params):
                    for ty, entry_name in norm[len(params):]:
                        params.append(Parameter(name=entry_name, php_type=ty))
        # Apply by-reference overrides after PARAM_TYPES/checker refinement.
        if canonical in REF_PARAM_OVERRIDES:
            for idx, by_ref in enumerate(REF_PARAM_OVERRIDES[canonical]):
                if 0 <= idx < len(params):
                    params[idx] = Parameter(
                        name=params[idx].name,
                        php_type=params[idx].php_type,
                        by_ref=by_ref,
                        default=params[idx].default,
                        optional=params[idx].optional,
                    )
        # Apply optional/default overrides.
        if canonical in OPTIONAL_PARAM_OVERRIDES:
            for idx, default in enumerate(OPTIONAL_PARAM_OVERRIDES[canonical]):
                if default is not None and 0 <= idx < len(params):
                    params[idx] = Parameter(
                        name=params[idx].name,
                        php_type=params[idx].php_type,
                        by_ref=params[idx].by_ref,
                        default=default,
                        optional=True,
                    )
        # Then refine further with what the check_builtin() arms tell us.
        # We only apply this when the entry is unambiguous (one type per
        # arg_index, and the inferred type is not `mixed`).
        if canonical in check_builtin_params and any(
            p.name not in (None, "…") for p in params
        ):
            # Group by arg index, keep the first non-mixed type per index.
            by_idx: dict[int, str] = {}
            for idx, ty in check_builtin_params[canonical]:
                if ty == "mixed":
                    continue
                if idx not in by_idx:
                    by_idx[idx] = ty
            for idx, ty in by_idx.items():
                if 0 <= idx < len(params) and params[idx].php_type == "mixed":
                    params[idx] = Parameter(
                        name=params[idx].name,
                        php_type=ty,
                        by_ref=params[idx].by_ref,
                        default=params[idx].default,
                        optional=params[idx].optional,
                    )
        # If still no signature at all (signatures.rs had no entry AND
        # check_builtin() didn't yield anything useful), mark the builtin
        # with a TODO stub so the renderer can show a placeholder.
        # Skip when signatures.rs already gave us info (even an empty list
        # means "the builtin takes no arguments", which is a real signature).
        if (
            not params
            and not sig_info.get("variadic")
            and canonical not in call_sigs
        ):
            params = [Parameter(name="…", php_type="mixed", optional=False)]
            sig_info["__stub"] = True

        lowering = LoweringInfo(
            sig_file=str(sigs.relative_to(repo)),
            sig_line=None,
            sig_arm=None,
        )

        fn_name = ""
        if canonical in dispatch_map:
            module, fn_name, line = dispatch_map[canonical]
        else:
            # No dedicated dispatch arm. Try a heuristic: if the builtin name
            # matches a `lower_<name>` function defined in builtins.rs root, use it.
            guessed_fn = f"lower_{canonical}"
            guessed = find_lowering_function_def(read(dispatch), guessed_fn)
            if guessed is not None:
                module, fn_name, line = "", guessed_fn, guessed[1]
            else:
                module, fn_name, line = "", "", 0
        if fn_name:
            # search in builtins.rs first, then submodules
            for candidate in [dispatch, *sorted(lowering_dir.rglob("*.rs"))]:
                src_text = read(candidate)
                defn = find_lowering_function_def(src_text, fn_name)
                if defn is None:
                    continue
                _, def_line = defn
                doc = _leading_doc_comment(src_text, def_line)
                # pull the next 30 lines to look for runtime helpers
                body = "\n".join(src_text.splitlines()[def_line - 1 : def_line + 30])
                helpers = collect_runtime_helpers(doc, body)
                notes = [l for l in doc.splitlines() if l.strip()]
                lowering = LoweringInfo(
                    codegen_file=str(candidate.relative_to(repo)),
                    codegen_line=def_line,
                    codegen_function=fn_name,
                    runtime_helpers=helpers,
                    notes=notes,
                )
                break

        # Attach hand-curated notes for compiler-internal helpers.
        if is_internal and canonical in INTERNAL_NOTES:
            lowering.notes = INTERNAL_NOTES[canonical]

        # Area resolution priority (most specific first):
        #   1. AREA_BY_NAME — hand-curated per-name overrides (e.g. sin→Math).
        #   2. Lowering-fn location (the file the lowering lives in).
        #   3. AREA_BY_LOWERING_FN — generic libm dispatcher mapping.
        #   4. AREA_BY_MODULE — based on the dispatch arm's module prefix.
        #   5. Misc (last resort).
        area = AREA_BY_NAME.get(canonical, ("Misc", "Misc"))
        if area == ("Misc", "Misc") and lowering.codegen_file:
            cf = lowering.codegen_file
            prefix = "src/codegen_ir/lower_inst/builtins"
            if cf.startswith(prefix + "/"):
                rel_under = cf[len(prefix) + 1:]
            else:
                rel_under = cf  # legacy safety
            file_area = parse_area_for_file(rel_under)
            if file_area[0] is not None and (file_area[0] != "Misc" or file_area[1] != "Misc"):
                area = file_area
        if area == ("Misc", "Misc"):
            fn_area = AREA_BY_LOWERING_FN.get(fn_name) if fn_name else None
            if fn_area is not None:
                area = fn_area
            elif module:
                mod_area = AREA_BY_MODULE.get(module)
                if mod_area is not None:
                    area = mod_area

        # Derive a one-line description from hand-curated overrides or from
        # the first line of the lowering function's doc comment.
        description = DESCRIPTION_OVERRIDES.get(canonical, "")
        if not description and lowering.notes:
            description = lowering.notes[0]

        b = Builtin(
            name=name,
            canonical_name=canonical,
            in_catalog=in_catalog,
            is_internal=is_internal,
            area=area[0],
            sub_area=area[1],
            sig=BuiltinSig(
                params=params,
                variadic=sig_info.get("variadic"),
                return_type=return_type,
            ),
            lowering=lowering,
            description=description,
        )
        builtins.append(b)

    return builtins


def main_with(repo_root: Path, out: Path) -> int:
    builtins = build_registry(repo_root)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(
        json.dumps([_builtin_to_dict(b) for b in builtins], indent=2, sort_keys=True),
        encoding="utf-8",
    )
    print(f"Wrote {len(builtins)} builtins to {out}", file=sys.stderr)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()
    repo = args.repo_root.resolve()
    out = (args.out or repo / "scripts" / "docs" / "builtin_registry.json").resolve()
    return main_with(repo, out)


def _builtin_to_dict(b: Builtin) -> dict:
    return {
        "name": b.name,
        "canonical_name": b.canonical_name,
        "slug": slug(b.name),
        "area": b.area,
        "sub_area": b.sub_area,
        "in_catalog": b.in_catalog,
        "is_internal": b.is_internal,
        "description": b.description,
        "sig": {
            "params": [
                {
                    "name": p.name,
                    "type": p.php_type,
                    "by_ref": p.by_ref,
                    "default": p.default,
                    "optional": p.optional,
                }
                for p in b.sig.params
            ],
            "variadic": b.sig.variadic,
            "return_type": b.sig.return_type,
        },
        "lowering": {
            "sig_file": b.lowering.sig_file,
            "sig_line": b.lowering.sig_line,
            "sig_arm": b.lowering.sig_arm,
            "checker_file": b.lowering.checker_file,
            "checker_line": b.lowering.checker_line,
            "codegen_file": b.lowering.codegen_file,
            "codegen_line": b.lowering.codegen_line,
            "codegen_function": b.lowering.codegen_function,
            "runtime_helpers": b.lowering.runtime_helpers,
            "notes": b.lowering.notes,
        },
    }


if __name__ == "__main__":
    sys.exit(main())


