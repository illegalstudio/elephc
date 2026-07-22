---
title: "str_pad()"
description: "Pads a string to a certain length with another string."
sidebar:
  order: 405
---

## str_pad()

```php
function str_pad(string $string, int $length, string $pad_string = ' ', int $pad_type = 1): string
```

Pads a string to a certain length with another string.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`)
- `$pad_string` (`string`), default `' '`, optional
- `$pad_type` (`int`), default `1`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_pad.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_pad.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `str_pad` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_pad.md).
