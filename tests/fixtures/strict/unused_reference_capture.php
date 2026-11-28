<?php
(function() use (&$missing) { echo 'deferred'; });
echo array_key_exists('missing', get_defined_vars()) ? 'yes|' : 'no|';
echo gettype($missing), '|';
$missing = 'after';
echo $missing, '|';
