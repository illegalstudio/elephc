---
title: "preg_match_all()"
description: "Performs a global regular expression match, including terminal empty matches, optionally filling `$matches`, and returns the number of matches. AOT capture output supports local variables and local references; unsupported non-local destinations are diagnosed. Unsupported flag bits or conflicting order modes return 0 and clear a supplied capture destination on both backends."
sidebar:
  order: 767
---

## preg_match_all()

```php
function preg_match_all(string $pattern, string $subject, mixed $matches = [], int $flags = 0): int
```

Performs a global regular expression match, including terminal empty matches, optionally filling `$matches`, and returns the number of matches. AOT capture output supports local variables and local references; unsupported non-local destinations are diagnosed. Unsupported flag bits or conflicting order modes return 0 and clear a supplied capture destination on both backends.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$matches` (`mixed`), passed by reference, default `[]`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_match_all.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_match_all.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_match_all` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_match_all.md).
