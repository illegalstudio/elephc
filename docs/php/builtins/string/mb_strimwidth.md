---
title: "mb_strimwidth()"
description: "Trims a string to a display width and appends a marker when needed."
sidebar:
  order: 842
---

## mb_strimwidth()

```php
function mb_strimwidth(string $string, int $start, int $width, string $trim_marker = '', ?string $encoding = null): string
```

Trims a string to a display width and appends a marker when needed.

**Parameters**:
- `$string` (`string`)
- `$start` (`int`)
- `$width` (`int`)
- `$trim_marker` (`string`), default `''`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strimwidth.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strimwidth.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strimwidth` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strimwidth.md).
