---
title: "imagegammacorrect()"
description: "Applies a gamma correction from one gamma value to another."
---

## imagegammacorrect()

```php
function imagegammacorrect(mixed $image, float $input_gamma, float $output_gamma): bool
```

Applies a gamma correction from one gamma value to another.

**Parameters**:
- `$image` (`mixed`)
- `$input_gamma` (`float`)
- `$output_gamma` (`float`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagegammacorrect` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagegammacorrect.md).
