---
title: "mb_str_pad()"
description: "Pads a string to a character length using the requested side and encoding."
sidebar:
  order: 839
---

## mb_str_pad()

```php
function mb_str_pad(string $string, int $length, string $pad_string = ' ', int $pad_type = 1, ?string $encoding = null): string
```

Pads a string to a character length using the requested side and encoding.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`)
- `$pad_string` (`string`), default `' '`, optional
- `$pad_type` (`int`), default `1`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_str_pad.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_str_pad.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_str_pad` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_str_pad.md).
