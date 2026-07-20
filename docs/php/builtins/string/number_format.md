---
title: "number_format()"
description: "Formats a number with grouped thousands."
sidebar:
  order: 391
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

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/formatting/number_format.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/number_format.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `number_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/number_format.md).

