---
title: "printf()"
description: "Lowers `printf(format, values...)` as `sprintf()` followed by stdout emission."
sidebar:
  order: 355
---

## printf()

```php
function printf(string $format, ...$values): int
```

Lowers `printf(format, values...)` as `sprintf()` followed by stdout emission.

**Parameters**:
- `$format` (`string`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `printf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/printf.md).

