---
title: "mb_strpos()"
description: "Finds the first character position of an encoded substring, or returns false."
sidebar:
  order: 846
---

## mb_strpos()

```php
function mb_strpos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false
```

Finds the first character position of an encoded substring, or returns false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strpos.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strpos.md).
