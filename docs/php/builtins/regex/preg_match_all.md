---
title: "preg_match_all()"
description: "Performs a global regular expression match, optionally filling `$matches`, and returns the number of matches."
sidebar:
  order: 401
---

## preg_match_all()

```php
function preg_match_all(string $pattern, string $subject, mixed $matches = [], int $flags = 0): int
```

Performs a global regular expression match, optionally filling `$matches`, and returns the number of matches.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$matches` (`mixed`), passed by reference, default `[]`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_match_all.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_match_all.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_match_all` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_match_all.md).
