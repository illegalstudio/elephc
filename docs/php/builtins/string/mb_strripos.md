---
title: "mb_strripos()"
description: "Finds the last character position using simple case folding, or returns false."
sidebar:
  order: 849
---

## mb_strripos()

```php
function mb_strripos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false
```

Finds the last character position using simple case folding, or returns false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strripos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strripos.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strripos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strripos.md).
