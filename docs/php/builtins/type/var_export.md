---
title: "var_export()"
description: "Implemented by the compiler-injected var_export prelude."
sidebar:
  order: 845
---

## var_export()

```php
function var_export(mixed $value, bool $return = false): mixed
```

Implemented by the compiler-injected var_export prelude.

**Parameters**:
- `$value` (`mixed`)
- `$return` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected var_export prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `var_export` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/var_export.md).
