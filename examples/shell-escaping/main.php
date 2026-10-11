<?php

$argument = "O'Reilly; echo unsafe";
$command = "printf '%s\\n' hello && echo unsafe";

echo escapeshellarg($argument), "\n";
echo escapeshellcmd($command), "\n";
