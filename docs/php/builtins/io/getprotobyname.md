---
title: "getprotobyname()"
description: "Lowers `getprotobyname(protocol)` and boxes a missing entry as PHP `false`."
sidebar:
  order: 163
---

## getprotobyname()

```php
function getprotobyname(string $protocol): mixed
```

Lowers `getprotobyname(protocol)` and boxes a missing entry as PHP `false`.

**Parameters**:
- `$protocol` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getprotobyname` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getprotobyname.md).

