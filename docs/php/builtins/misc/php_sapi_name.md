---
title: "php_sapi_name()"
description: "Implemented by the compiler-injected version prelude."
sidebar:
  order: 620
---

## php_sapi_name()

```php
function php_sapi_name(): string
```

Implemented by the compiler-injected version prelude.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected version prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `php_sapi_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/php_sapi_name.md).
