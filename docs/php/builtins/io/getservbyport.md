---
title: "getservbyport()"
description: "Gets the Internet service that corresponds to a port and protocol."
---

## getservbyport()

```php
function getservbyport(int $port, string $protocol): mixed
```

Gets the Internet service that corresponds to a port and protocol.

**Parameters**:
- `$port` (`int`)
- `$protocol` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getservbyport` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getservbyport.md).
