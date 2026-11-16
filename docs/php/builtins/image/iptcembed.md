---
title: "iptcembed()"
description: "Embeds an IPTC block into a JPEG file."
---

## iptcembed()

```php
function iptcembed(string $iptcdata, string $jpeg_file_name, int $spool = 0): mixed
```

Embeds an IPTC block into a JPEG file.

**Parameters**:
- `$iptcdata` (`string`)
- `$jpeg_file_name` (`string`)
- `$spool` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iptcembed` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/iptcembed.md).
