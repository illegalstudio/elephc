---
title: "random_bytes()"
description: "Lowers `random_bytes()` into an owned CSPRNG binary string of the given length."
sidebar:
  order: 262
---

## random_bytes()

```php
function random_bytes(int $length): string
```

Lowers `random_bytes()` into an owned CSPRNG binary string of the given length.

**Parameters**:
- `$length` (`int`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `random_bytes` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/random_bytes.md).

