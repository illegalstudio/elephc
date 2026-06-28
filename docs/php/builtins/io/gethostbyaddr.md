---
title: "gethostbyaddr()"
description: "Lowers `gethostbyaddr(address)` and boxes malformed addresses as PHP `false`."
sidebar:
  order: 160
---

## gethostbyaddr()

```php
function gethostbyaddr(string $ip): mixed
```

Lowers `gethostbyaddr(address)` and boxes malformed addresses as PHP `false`.

**Parameters**:
- `$ip` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gethostbyaddr` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gethostbyaddr.md).

