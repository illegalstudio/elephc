---
title: "random_bytes()"
description: "Generates cryptographically secure pseudo-random bytes."
sidebar:
  order: 607
---

## random_bytes()

```php
function random_bytes(int $length): string
```

Generates cryptographically secure pseudo-random bytes.

**Parameters**:
- `$length` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected random prelude.
- **AOT signature compatibility**: `runtime-arity-validation` - compiled code accepts any argument count and raises PHP's `ArgumentCountError` at run time, so the signature shown is the PHP-visible one.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `random_bytes` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/random_bytes.md).
