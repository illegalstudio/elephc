---
title: "gethostbyname()"
description: "Lowers `gethostbyname(hostname)` through the shared runtime resolver."
sidebar:
  order: 161
---

## gethostbyname()

```php
function gethostbyname(string $hostname): string
```

Lowers `gethostbyname(hostname)` through the shared runtime resolver.

**Parameters**:
- `$hostname` (`string`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gethostbyname` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gethostbyname.md).

