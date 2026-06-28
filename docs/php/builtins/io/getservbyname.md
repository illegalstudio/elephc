---
title: "getservbyname()"
description: "Lowers `getservbyname(service, protocol)` and boxes a missing entry as PHP `false`."
sidebar:
  order: 165
---

## getservbyname()

```php
function getservbyname(string $service, string $protocol): mixed
```

Lowers `getservbyname(service, protocol)` and boxes a missing entry as PHP `false`.

**Parameters**:
- `$service` (`string`)
- `$protocol` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getservbyname` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getservbyname.md).

