---
title: "preg_split()"
description: "Splits a string by a regular expression."
sidebar:
  order: 344
---

## preg_split()

```php
function preg_split(string $pattern, string $subject, int $limit = -1, int $flags = 0): array
```

Splits a string by a regular expression.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$limit` (`int`), default `-1`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_split.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_split.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `preg_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_split.md).
