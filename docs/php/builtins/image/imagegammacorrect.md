---
title: "imagegammacorrect()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 509
---

## imagegammacorrect()

```php
function imagegammacorrect(mixed $image, float $input_gamma, float $output_gamma): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$input_gamma` (`float`)
- `$output_gamma` (`float`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagegammacorrect` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagegammacorrect.md).
