---
title: "imagechar()"
description: "Draws one character horizontally with a built-in font."
---

## imagechar()

```php
function imagechar(mixed $image, int $font, int $x, int $y, string $char, int $color): bool
```

Draws one character horizontally with a built-in font.

**Parameters**:
- `$image` (`mixed`)
- `$font` (`int`)
- `$x` (`int`)
- `$y` (`int`)
- `$char` (`string`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagechar` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagechar.md).
