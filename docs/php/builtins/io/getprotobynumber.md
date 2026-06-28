---
title: "getprotobynumber()"
description: "Lowers `getprotobynumber(number)` and boxes a missing entry as PHP `false`."
sidebar:
  order: 164
---

## getprotobynumber()

```php
function getprotobynumber(int $protocol): mixed
```

Lowers `getprotobynumber(number)` and boxes a missing entry as PHP `false`.

**Parameters**:
- `$protocol` (`int`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getprotobynumber` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/getprotobynumber.md).

