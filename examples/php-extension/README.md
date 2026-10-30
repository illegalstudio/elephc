# PHP extension example

The four exports demonstrate integers, binary strings, heterogeneous nested
arrays and a PHP exception. Build with a matching PHP 8.5 development SDK:

```sh
elephc --emit phpext --heap-size=1048576 main.php
php -n -d extension=./main.so driver.php
```

Run `php -n driver.php` for the identical pure PHP implementation. Both commands
must produce the same bytes. The array argument remains unchanged in the caller.
The explicit 1 MiB heap limits the workload's native memory budget. Size it for
the exported workload; reachable scratch and allocator bookkeeping also contribute
to snapshot size. Unreachable runtime helpers and their buffers are omitted before
the request image is laid out.

Set `ELEPHC_PHPEXT_PHP_CONFIG` when selecting a specific SDK. If that SDK reports
`NONE` for its interpreter, also set `ELEPHC_PHPEXT_PHP_BINARY` to the matching PHP
executable. The compiler validates the pair before linking.
