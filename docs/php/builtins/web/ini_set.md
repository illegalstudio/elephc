---
title: "ini_set()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 849
---

## ini_set()

```php
function ini_set(string $option, mixed $value): mixed
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$option` (`string`)
- `$value` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ini_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/ini_set.md).
