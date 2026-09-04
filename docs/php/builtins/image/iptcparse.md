---
title: "iptcparse()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 538
---

## iptcparse()

```php
function iptcparse(string $iptcblock): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$iptcblock` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iptcparse` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/iptcparse.md).
