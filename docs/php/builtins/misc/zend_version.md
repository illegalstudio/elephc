---
title: "zend_version()"
description: "Implemented by the compiler-injected version prelude."
sidebar:
  order: 628
---

## zend_version()

```php
function zend_version(): string
```

Implemented by the compiler-injected version prelude.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected version prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zend_version` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/zend_version.md).
