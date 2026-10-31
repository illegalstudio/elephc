# Real PHP classes and constants

Compile a class into an extension and instantiate it directly from normal PHP.
Use the PHP 8.5 SDK matching the interpreter that loads the module:

```sh
elephc --emit phpext --heap-size=1048576 \
  --export-constant='PippoDemo\VERSION' \
  --export-constant='PippoDemo\DEFAULT_OPTIONS' main.php
php -n -d extension=./main.so driver.php
```

The native run starts with `Mode: native extension (main.so)` and calls
`new PippoDemo\Pippo()` without including its PHP source or loading a wrapper.
Use `-d extension=./main.so`; `-d main.so` does not load the extension.
The same driver can run the ordinary PHP source for comparison:

```sh
php -n driver.php
```

Without the extension, the first line is `Mode: PHP source (fallback)`.
Both modes then print the same class demonstration output:

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

## Compare execution time

The driver also calls `Pippo::countPrimes()` with a default limit of 2,000,000.
It uses the same trial-division algorithm in both modes and prints the prime count
plus elapsed seconds measured with `hrtime()`. The native timing includes one
complete method call through the extension adapter. PHP startup and the earlier
class demonstration are outside that timer.

```sh
# PHP source, no extension
php -n driver.php

# Compiled method in the extension
php -n -d extension=./main.so driver.php
```

Both runs must report `Primes: 148933`. Their mode headers and timings differ.
The input comes from the PHP host, so the compiler cannot fold the result at
compile time. Increase or decrease the limit with the first argument:

```sh
php -n driver.php 5000000
php -n -d extension=./main.so driver.php 5000000
```

Repeat each command a few times and compare medians. `-n` uses the same default
PHP configuration in both modes. The comparison does not enable CLI OPcache/JIT.
This CPU benchmark shows the cost of compiled integer loops; workloads dominated
by I/O, arrays or many tiny extension calls can behave differently. Shell `time` can also measure
the complete process, including PHP startup and module initialization.
