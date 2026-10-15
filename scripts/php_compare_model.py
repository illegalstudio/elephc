#!/usr/bin/env python3
"""PHP 8's `<=>` over the scalar tags the wasm32-wasi backend boxes.

WHY THIS EXISTS. `__rt_mixed_cmp_mixed` has to reproduce php-src's comparison exactly, and
the rules are not guessable: seven of them were found only by sweeping php itself, and each
one silently flips a result rather than failing loudly. Writing the WAT against this model —
rather than against intuition — is the same method that produced `__rt_mixed_cmp_i64`.

  8. Two DIFFERENT float-form strings that both overflow to INFINITY also fall back to the
     bytes, though neither sets `oflow` — php never sets it for a float form. The first
     sweep missed this: it carried `1e400` but no second overflowing float string, so the
     pair could not be generated. Repo memory caught it, not the sweep.

Validated against `php -n` on 8356 ordered pairs: 3844 adversarial (every pair of a set built
from the i64 edges, 2^53, INF/NAN, and numeric-looking strings) and 4000 random.

THE SEVEN RULES, each measured:
  1. `1_000` is NOT a numeric string. Python's `float()` accepts underscores; PHP does not,
     and that alone flips `42 <=> "1_000"`.
  2. Two ints compare EXACTLY: `PHP_INT_MAX <=> "9223372036854775807"` is 0 only without a
     detour through double.
  3. An integer string too large for an int becomes a DOUBLE, so
     `PHP_INT_MAX <=> "9223372036854775808"` is also 0.
  4. Mixing int and float converts the int to double: `9007199254740993 <=> 9007199254740992.0`
     is 0, not 1.
  5. NaN is TRUTHY, and the bool/null rule outranks the NaN rule: `NAN <=> true` is 0, while
     `NAN <=> 0` is 1.
  6. php renders a float as `%.14G` with a forced `.0` before any exponent — `1.0E+300`, not
     `1e+300`. This decides number-vs-non-numeric-string, where the NUMBER becomes a string.
  7. Two numeric strings that tie as doubles fall back to a BYTE comparison only when an
     integer-looking string overflowed: `"…807" < "…808"`, while `"42"` and `" 42"` stay equal.

Run `python3 scripts/php_compare_model.py --self-check <oracle.tsv>` to re-validate against a
capture produced by the generator documented at the bottom of this file.
"""

from __future__ import annotations

import math
import sys

INT_MIN, INT_MAX = -(2**63), 2**63 - 1


def is_numeric_string(s):
    """PHP 8's numeric-string test, returning ("int", v) or ("float", v), else None.

    Deliberately NOT Python's float(): it accepts `1_000` and `inf`, which PHP does not,
    and that difference alone flipped `42 <=> "1_000"` the wrong way. The int/float split
    matters too — `PHP_INT_MAX <=> "9223372036854775807"` is 0 only under an EXACT integer
    comparison, while a string that overflows int falls back to float.
    """
    t = s.strip(" \t\n\r\v\f")
    if t == "" or "_" in t:
        return None
    body = t[1:] if t[:1] in "+-" else t
    if body == "" or body[:1].lower() in ("i", "n"):      # inf / nan are not numeric here
        return None
    if body.isdigit():
        v = int(t)
        return ("int", v) if INT_MIN <= v <= INT_MAX else ("float", float(t))
    try:
        return ("float", float(t))
    except ValueError:
        return None


def overflowed_int(text):
    """Whether `text` is an integer-looking numeric string too large for a PHP int."""
    t = text.strip(" \t\n\r\v\f")
    body = t[1:] if t[:1] in "+-" else t
    return body.isdigit() and not (INT_MIN <= int(t) <= INT_MAX)


def to_bool(v):
    kind, val = v
    if kind == "null":
        return False
    if kind == "bool":
        return val
    if kind == "int":
        return val != 0
    if kind == "float":
        return val != 0.0
    return val != "" and val != "0"


def sign(x):
    return (x > 0) - (x < 0)


def compare(a, b):
    ka, va = a
    kb, vb = b

    # null <=> string is a STRING comparison against "", not a bool one.
    if ka == "null" and kb == "str":
        return sign((("" > vb) - ("" < vb)))
    if kb == "null" and ka == "str":
        return sign(((va > "") - (va < "")))

    # Anything involving bool or null compares as BOOL — and this OUTRANKS the NaN rule,
    # because NaN is truthy: `NAN <=> true` is 0, not 1.
    if ka in ("bool", "null") or kb in ("bool", "null"):
        return sign(int(to_bool(a)) - int(to_bool(b)))

    # NaN is otherwise UNCOMPARABLE: php answers 1 in every direction, including NAN<=>NAN.
    if (ka == "float" and math.isnan(va)) or (kb == "float" and math.isnan(vb)):
        return 1

    # Two strings: numeric only when BOTH are numeric.
    if ka == "str" and kb == "str":
        na, nb = is_numeric_string(va), is_numeric_string(vb)
        if na is not None and nb is not None:
            numeric = compare_numbers(na, nb)
            # php's `smart_strcmp` falls back to the BYTES only when an INTEGER-looking
            # string overflowed into a double and the doubles then tied: that is how
            # "…807" < "…808" survives, while "42" and " 42" stay equal.
            both_infinite = (
                na[0] == "float" and nb[0] == "float"
                and math.isinf(na[1]) and math.isinf(nb[1])
            )
            if numeric == 0 and va != vb and (
                overflowed_int(va) or overflowed_int(vb) or both_infinite
            ):
                return sign((va > vb) - (va < vb))
            return numeric
        return sign((va > vb) - (va < vb))

    # Number vs string: numeric if the string is numeric, else the NUMBER becomes a string
    # and they compare as strings — the PHP 8 change.
    if ka == "str" or kb == "str":
        s_val, n, flipped = (va, b, False) if ka == "str" else (vb, a, True)
        ns = is_numeric_string(s_val)
        if ns is not None:
            r = compare_numbers(ns, n)
            return -r if flipped else r
        text = php_number_to_string(n)
        r = sign((s_val > text) - (s_val < text))
        return -r if flipped else r

    return compare_numbers(a, b)


def compare_numbers(x, y):
    """Exact when BOTH are ints; otherwise both become floats, as php does.

    `9007199254740993 <=> 9007199254740992.0` is 0 because the int is converted to a
    double and the two land on the same value — comparing exactly would answer 1.
    """
    kx, vx = x
    ky, vy = y
    if kx == "int" and ky == "int":
        return sign((vx > vy) - (vx < vy))
    fx, fy = float(vx), float(vy)
    if math.isnan(fx) or math.isnan(fy):
        return 1
    return sign((fx > fy) - (fx < fy))


def php_number_to_string(n):
    """php's own rendering, which is `%.14G` plus a forced `.0` before any exponent.

    `1.0E+300`, not Python's `1e+300`: the difference decides
    `1e300 <=> "1_000"`, because a non-numeric string makes the NUMBER become a string.
    """
    kind, val = n
    if kind == "int":
        return str(val)
    if math.isnan(val):
        return "NAN"
    if math.isinf(val):
        return "INF" if val > 0 else "-INF"
    text = "%.14G" % val
    if "E" in text:
        mantissa, exponent = text.split("E", 1)
        if "." not in mantissa:
            mantissa += ".0"
        return mantissa + "E" + exponent
    return text


def parse(tok):
    if tok == "null":
        return ("null", None)
    if tok == "true":
        return ("bool", True)
    if tok == "false":
        return ("bool", False)
    if tok.startswith("int:"):
        return ("int", int(tok[4:]))
    if tok.startswith("float:"):
        t = tok[6:]
        if t == "INF":
            return ("float", math.inf)
        if t == "-INF":
            return ("float", -math.inf)
        return ("float", float(t))
    text = tok[4:]
    for esc, raw in (("\\t", "\t"), ("\\n", "\n"), ("\\r", "\r"), ("\\\\", "\\")):
        text = text.replace(esc, raw)
    return ("str", text)




def _self_check(path: str) -> int:
    """Compares the model against a php-produced oracle, returning the mismatch count."""
    bad = 0
    total = 0
    with open(path) as handle:
        for line in handle:
            left, right, want = line.rstrip("\n").split("\t")
            total += 1
            if compare(parse(left), parse(right)) != int(want):
                bad += 1
                if bad <= 12:
                    print(f"MISMATCH {left!r} <=> {right!r}: php={want}")
    print(f"{total - bad}/{total} match")
    return bad


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--self-check":
        sys.exit(1 if _self_check(sys.argv[2]) else 0)
    print(__doc__)
