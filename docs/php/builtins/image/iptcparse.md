---
title: "iptcparse()"
description: "Parses an IPTC block into its tag arrays."
sidebar:
  order: 541
---

## iptcparse()

```php
function iptcparse(string $iptcblock): mixed
```

Parses an IPTC block into its tag arrays.

**Parameters**:
- `$iptcblock` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iptcparse` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/iptcparse.md).
