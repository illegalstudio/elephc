---
title: "sodium_unpad()"
description: "Removes padding from a block-padded string."
---

## sodium_unpad()

```php
function sodium_unpad(string $string, int $block_size): string
```

Removes padding from a block-padded string.

**Parameters**:
- `$string` (`string`)
- `$block_size` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sodium_unpad` is implemented in the compiler, see [the internals page](../../../internals/builtins/sodium/sodium_unpad.md).
