# Real PHP classes and constants

Compile a class into an extension and instantiate it directly from normal PHP.
Use the PHP 8.5 SDK matching the interpreter that loads the module:

```sh
elephc --emit phpext --heap-size=1048576 \
  --export-constant='PippoDemo\VERSION' \
  --export-constant='PippoDemo\DEFAULT_OPTIONS' main.php
php -n -d extension=./main.so driver.php
```

The native run calls `new PippoDemo\Pippo()` without including its PHP source or
loading a wrapper. The same driver can run the ordinary PHP source for comparison:

```sh
php -n driver.php
```

Expected output:

```text
Ciao, PHP!
Salve, elephc!
int(2)
bool(true)
int(42)
string(5) "mondo"
string(3) "1.0"
```

`#[Export]` selects the final class, its public constructor and methods, typed
properties and public class constants. Global constants use repeatable
`--export-constant` options. The class preserves object identity across compiled
method calls; strings and arrays are copied at the runtime boundary.
