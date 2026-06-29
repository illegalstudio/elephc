---
title: "str_pad()"
description: "Lowers `str_pad(string, length, pad_string?, pad_type?)` through the shared runtime helper."
sidebar:
  order: 365
---

## str_pad()

```php
function str_pad(string $string, int $length, string $pad_string, int $pad_type): string
```

Lowers `str_pad(string, length, pad_string?, pad_type?)` through the shared runtime helper.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`)
- `$pad_string` (`string`), optional
- `$pad_type` (`int`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `str_pad` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_pad.md).

