---
title: "ptr_read_string()"
description: "Lowers `ptr_read_string(pointer, length)` by copying raw bytes into an owned PHP string."
sidebar:
  order: 293
---

## ptr_read_string()

```php
function ptr_read_string(pointer $pointer, int $length): string
```

Lowers `ptr_read_string(pointer, length)` by copying raw bytes into an owned PHP string.

**Parameters**:
- `$pointer` (`pointer`)
- `$length` (`int`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_read_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_read_string.md).

