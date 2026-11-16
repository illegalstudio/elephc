---
title: "ptr_read_string()"
description: "Copies raw bytes from a pointer into a PHP string of the given length."
---

## ptr_read_string()

```php
function ptr_read_string(pointer $pointer, int $length): string
```

Copies raw bytes from a pointer into a PHP string of the given length.

**Parameters**:
- `$pointer` (`pointer`)
- `$length` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_read_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read_string.md).
