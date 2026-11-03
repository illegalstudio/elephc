---
title: "preg_grep()"
description: "Returns entries whose values match a regular expression while preserving keys."
sidebar:
  order: 777
---

## preg_grep()

```php
function preg_grep(string $pattern, mixed $array, int $flags = 0): mixed
```

Returns entries whose values match a regular expression while preserving keys.

**Parameters**:
- `$pattern` (`string`)
- `$array` (`mixed`)
- `$flags` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_grep.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_grep.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_grep` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/preg_grep.md).
