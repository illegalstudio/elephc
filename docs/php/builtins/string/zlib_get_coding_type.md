---
title: "zlib_get_coding_type()"
description: "Returns the compression the output layer applied, or false when none did."
sidebar:
  order: 554
---

## zlib_get_coding_type()

```php
function zlib_get_coding_type(): mixed
```

Returns the compression the output layer applied, or false when none did.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zlib_get_coding_type` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/zlib_get_coding_type.md).
