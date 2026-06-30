---
title: "ptr_write_string()"
description: "Lowers `ptr_write_string(pointer, string)` by copying PHP string bytes into raw memory."
sidebar:
  order: 299
---

## ptr_write_string()

```php
function ptr_write_string(pointer $pointer, string $string): int
```

Lowers `ptr_write_string(pointer, string)` by copying PHP string bytes into raw memory.

**Parameters**:
- `$pointer` (`pointer`)
- `$string` (`string`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_write_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write_string.md).

