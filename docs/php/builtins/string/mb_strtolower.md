---
title: "mb_strtolower()"
description: "Returns the Unicode lowercase form of a string using PHP 8.5 UTF-8 semantics."
sidebar:
  order: 469
---

## mb_strtolower()

```php
function mb_strtolower(string $string, string $encoding = null): string
```

Returns the Unicode lowercase form of a string using PHP 8.5 UTF-8 semantics.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strtolower.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strtolower.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strtolower` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strtolower.md).
