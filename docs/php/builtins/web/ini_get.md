---
title: "ini_get()"
description: "Returns the value of a configuration directive."
sidebar:
  order: 973
---

## ini_get()

```php
function ini_get(string $option): mixed
```

Returns the value of a configuration directive.

**Parameters**:
- `$option` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ini_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/ini_get.md).
