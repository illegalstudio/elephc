---
title: "gethostbyaddr()"
description: "Gets the Internet host name corresponding to a given IP address."
---

## gethostbyaddr()

```php
function gethostbyaddr(string $ip): mixed
```

Gets the Internet host name corresponding to a given IP address.

**Parameters**:
- `$ip` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gethostbyaddr` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gethostbyaddr.md).
