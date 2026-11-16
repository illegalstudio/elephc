<?php
// The minimal handler from the FrankenPHP-vs-PHP-FPM benchmark article:
// one echo, one superglobal read, no framework, no I/O.
echo "Hello from " . ($_SERVER['SERVER_SOFTWARE'] ?? 'unknown server');
