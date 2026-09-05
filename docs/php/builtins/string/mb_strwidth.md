---
title: "mb_strwidth()"
description: "Returns the terminal display width of a string in the requested encoding."
sidebar:
  order: 469
---

## mb_strwidth()

```php
function mb_strwidth(string $string, string $encoding = null): int
```

Returns the terminal display width of a string in the requested encoding.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`string`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strwidth.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strwidth.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strwidth` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strwidth.md).
