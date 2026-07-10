---
title: "Regex builtins"
description: "Builtins in the Regex category."
sidebar:
  order: 106
---

## Regex builtins

| Function | Signature | Returns |
|---|---|---|
| [`mb_ereg_match()`](./regex/mb_ereg_match.md) | `(string $pattern, string $subject, string $options = null): bool` | `bool` |
| [`preg_match()`](./regex/preg_match.md) | `(string $pattern, string $subject, array $matches = []): int` | `int` |
| [`preg_match_all()`](./regex/preg_match_all.md) | `(string $pattern, string $subject): int` | `int` |
| [`preg_replace()`](./regex/preg_replace.md) | `(string $pattern, string $replacement, string $subject): string` | `string` |
| [`preg_replace_callback()`](./regex/preg_replace_callback.md) | `(string $pattern, callable $callback, string $subject): string` | `string` |
| [`preg_split()`](./regex/preg_split.md) | `(string $pattern, string $subject, int $limit = -1, int $flags = 0): array` | `array` |
