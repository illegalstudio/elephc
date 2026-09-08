<?php

$previous = error_reporting(E_ALL & ~E_DEPRECATED);
echo "Previous mask: ", $previous, "\n";
echo "Current mask: ", error_reporting(), "\n";
