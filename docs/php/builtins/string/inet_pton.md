---
title: "inet_pton()"
description: "Lowers `inet_ntop()` and `inet_pton()` and boxes invalid-address results as PHP false."
sidebar:
  order: 344
---

## inet_pton()

```php
function inet_pton(string $ip): mixed
```

Lowers `inet_ntop()` and `inet_pton()` and boxes invalid-address results as PHP false.

**Parameters**:
- `$ip` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `inet_pton` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/inet_pton.md).

