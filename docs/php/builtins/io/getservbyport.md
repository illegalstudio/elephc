---
title: "getservbyport()"
description: "Lowers `getservbyport(port, protocol)` and boxes a missing entry as PHP `false`."
sidebar:
  order: 166
---

## getservbyport()

```php
function getservbyport(int $port, string $protocol): mixed
```

Lowers `getservbyport(port, protocol)` and boxes a missing entry as PHP `false`.

**Parameters**:
- `$port` (`int`)
- `$protocol` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getservbyport` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getservbyport.md).

