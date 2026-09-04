---
title: "ini_get()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 847
---

## ini_get()

```php
function ini_get(string $option): mixed
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$option` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ini_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/ini_get.md).
