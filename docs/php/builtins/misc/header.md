---
title: "header()"
description: "Sends a raw HTTP header."
---

## header()

```php
function header(string $header, bool $replace = true, int $response_code = 0): void
```

Sends a raw HTTP header.

**Parameters**:
- `$header` (`string`)
- `$replace` (`bool`), default `true`, optional
- `$response_code` (`int`), default `0`, optional

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `header` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/header.md).
