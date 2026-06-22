---
title: "number_format()"
description: "Lowers `number_format()` by arranging its runtime helper arguments."
sidebar:
  order: 353
---

## number_format()

```php
function number_format(float $num, int $decimals, string $decimal_separator, string $thousands_separator): string
```

Lowers `number_format()` by arranging its runtime helper arguments.

**Parameters**:
- `$num` (`float`)
- `$decimals` (`int`), optional
- `$decimal_separator` (`string`), optional
- `$thousands_separator` (`string`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `number_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/number_format.md).

