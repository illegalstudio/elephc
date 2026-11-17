---
title: "base64_decode()"
description: "Decodes a Base64-encoded string back into its original data."
---

## base64_decode()

```php
function base64_decode(string $string, bool $strict = false): mixed
```

Decodes a Base64-encoded string back into its original data.

**Parameters**:
- `$string` (`string`)
- `$strict` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `base64_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/base64_decode.md).
