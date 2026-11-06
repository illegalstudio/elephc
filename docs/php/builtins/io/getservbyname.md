---
title: "getservbyname()"
description: "Gets port number associated with an Internet service and protocol."
---

## getservbyname()

```php
function getservbyname(string $service, string $protocol): mixed
```

Gets port number associated with an Internet service and protocol.

**Parameters**:
- `$service` (`string`)
- `$protocol` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getservbyname` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getservbyname.md).
