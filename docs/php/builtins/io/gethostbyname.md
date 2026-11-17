---
title: "gethostbyname()"
description: "Gets the IPv4 address corresponding to the given Internet host name."
---

## gethostbyname()

```php
function gethostbyname(string $hostname): string
```

Gets the IPv4 address corresponding to the given Internet host name.

**Parameters**:
- `$hostname` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gethostbyname` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gethostbyname.md).
