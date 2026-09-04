---
title: "iptcembed()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 537
---

## iptcembed()

```php
function iptcembed(string $iptcdata, string $jpeg_file_name, int $spool = 0): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$iptcdata` (`string`)
- `$jpeg_file_name` (`string`)
- `$spool` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iptcembed` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/iptcembed.md).
