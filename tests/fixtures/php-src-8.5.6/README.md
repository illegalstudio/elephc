# php-src GMP test corpus

These files are copied verbatim from php-src so the GMP implementation is judged
against the same fixtures as PHP itself.

- Source: `php-src`, branch `PHP-8.5`, commit
  `5d3b85c1ee475c978236120007935daa63f9668a` (`PHP_VERSION` 8.5.6).
- Path: `ext/gmp/tests/*.phpt` (99 files).
- Licence: PHP License 3.01, reproduced in `../LICENSE`.

The corpus is read by `tests/codegen/gmp_phpt.rs`. Do not edit the `.phpt`
files: a divergence belongs in the runner's explicit exclusion table or in the
compiler, never in the fixture. Refresh procedure:

```bash
cp /path/to/php-src/ext/gmp/tests/*.phpt tests/fixtures/php-src-8.5.6/ext/gmp/tests/
```

and update the commit above.
