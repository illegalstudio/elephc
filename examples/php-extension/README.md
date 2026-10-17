# PHP extension example

The four exports demonstrate integers, binary strings, heterogeneous nested
arrays and a PHP exception. Build with a matching PHP 8.5 development SDK:

```sh
elephc --emit phpext main.php
php -n -d extension=./main.so driver.php
```

Run `php -n driver.php` for the identical pure PHP implementation. Both commands
must produce the same bytes. The array argument remains unchanged in the caller.

Set `ELEPHC_PHPEXT_PHP_CONFIG` when selecting a specific SDK. If that SDK reports
`NONE` for its interpreter, also set `ELEPHC_PHPEXT_PHP_BINARY` to the matching PHP
executable. The compiler validates the pair before linking.
