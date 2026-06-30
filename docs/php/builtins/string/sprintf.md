---
title: "sprintf()"
description: "Lowers `sprintf(format, values...)` by packing variadic records for `__rt_sprintf`."
sidebar:
  order: 375
---

## sprintf()

```php
function sprintf(string $format, ...$values): string
```

Lowers `sprintf(format, values...)` by packing variadic records for `__rt_sprintf`.

**Parameters**:
- `$format` (`string`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/sprintf.md).

