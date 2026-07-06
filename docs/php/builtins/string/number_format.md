---
title: "number_format()"
description: "Formats a number with grouped thousands."
sidebar:
  order: 368
---

## number_format()

```php
function number_format(float $num, int $decimals = 0, string $decimal_separator = '.', string $thousands_separator = ','): string
```

Formats a number with grouped thousands.

**Parameters**:
- `$num` (`float`)
- `$decimals` (`int`), default `0`, optional
- `$decimal_separator` (`string`), default `'.'`, optional
- `$thousands_separator` (`string`), default `','`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `number_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/number_format.md).

