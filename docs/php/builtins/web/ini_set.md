---
title: "ini_set()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 945
---

## ini_set()

```php
function ini_set(string $option, string|int|float|bool|null $value): string|false
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$option` (`string`)
- `$value` (`string|int|float|bool|null`)

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ini_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/ini_set.md).
