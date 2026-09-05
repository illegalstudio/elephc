---
title: "strip_tags()"
description: "Strips HTML and PHP tags from a string. Optional allowed_tags is a string like \"<p><a>\" or an array of tag names."
sidebar:
  order: 498
---

## strip_tags()

```php
function strip_tags(string $string, mixed $allowed_tags = null): string
```

Strips HTML and PHP tags from a string. Optional allowed_tags is a string like "<p><a>" or an array of tag names.

**Parameters**:
- `$string` (`string`)
- `$allowed_tags` (`mixed`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strip_tags.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strip_tags.rs)).

**Examples**:

echo strip_tags("<p>Hello <b>World</b></p>");

echo strip_tags("<p>Hello <b>World</b></p>", "<p>");

echo strip_tags("<p>Hello <b>World</b></p>", ["p"]);

## Internals

For how `strip_tags` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strip_tags.md).
