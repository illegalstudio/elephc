---
title: "ptr_write_string()"
description: "Copies PHP string bytes into raw memory at the given pointer."
sidebar:
  order: 299
---

## ptr_write_string()

```php
function ptr_write_string(pointer $pointer, string $string): int
```

Copies PHP string bytes into raw memory at the given pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$string` (`string`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_write_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write_string.md).

