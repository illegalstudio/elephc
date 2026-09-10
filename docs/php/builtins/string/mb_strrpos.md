---
title: "mb_strrpos()"
description: "Finds the last character position of an encoded substring, or returns false."
sidebar:
  order: 850
---

## mb_strrpos()

```php
function mb_strrpos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false
```

Finds the last character position of an encoded substring, or returns false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strrpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strrpos.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strrpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strrpos.md).
