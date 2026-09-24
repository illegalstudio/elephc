---
title: "var_export()"
description: "Renders a value as parsable PHP code, printed or returned."
sidebar:
  order: 971
---

## var_export()

```php
function var_export(mixed $value, bool $return = false): mixed
```

Renders a value as parsable PHP code, printed or returned.

**Parameters**:
- `$value` (`mixed`)
- `$return` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected var_export prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `var_export` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/var_export.md).
