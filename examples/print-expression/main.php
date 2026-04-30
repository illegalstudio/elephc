<?php

$status = print "ready\n";
echo "status=";
echo $status;
echo "\n";

echo print "nested\n";
echo "\n";

$fallback = print false ?: "fallback\n";
echo "fallback status=";
echo $fallback;
echo "\n";
