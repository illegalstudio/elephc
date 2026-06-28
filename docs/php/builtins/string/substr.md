---
title: "substr()"
description: "Lowers `substr(string, offset, length?)` with target-local pointer arithmetic."
sidebar:
  order: 378
---

## substr()

```php
function substr(string $string, int $offset, int $length): string
```

Lowers `substr(string, offset, length?)` with target-local pointer arithmetic.

**Parameters**:
- `$string` (`string`)
- `$offset` (`int`)
- `$length` (`int`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `substr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/substr.md).

