---
title: "ptr_read_string()"
description: "Copies raw bytes from a pointer into a PHP string of the given length."
sidebar:
  order: 293
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

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_read_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read_string.md).

