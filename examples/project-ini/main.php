<?php

// Prints the INI settings this binary was compiled with. A compiled program has no
// php.ini to read at startup, so the values are baked in at compile time, here from the
// [ini] table of the elephc.toml beside this file: elephc reads the nearest elephc.toml
// above the compiled PHP file, the same one native dependencies use.
//
//   elephc examples/project-ini/main.php
//
// An `--ini` on the command line wins over the file for the same directive:
//
//   elephc --ini mbstring.language=English examples/project-ini/main.php
//
// Reference PHP prints the same with the values passed as -d flags (or in its php.ini):
//
//   php -d mbstring.language=Japanese -d opcache.enable_cli=1 \
//       -d opcache.memory_consumption=64 examples/project-ini/main.php

/** Prints one setting, its name padded so the values line up. */
function show(string $name, string $value): void
{
    echo str_pad($name, 28), $value, "\n";
}

show('mbstring.language', (string) mb_language());
show('opcache.enable_cli', var_export(ini_get('opcache.enable_cli'), true));
show('opcache.memory_consumption', (string) ini_get('opcache.memory_consumption'));
