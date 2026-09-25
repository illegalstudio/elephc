---
title: "ini_set()"
description: "Overrides a configuration directive for the rest of the request."
sidebar:
  order: 976
---

## ini_set()

```php
function ini_set(string $option, string|int|float|bool|null $value): string|false
```

Overrides a configuration directive for the rest of the request.

**Parameters**:
- `$option` (`string`)
- `$value` (`string|int|float|bool|null`)

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ini_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/ini_set.md).
