<?php

$selected = setlocale(LC_ALL, ["en_US.UTF-8", "C"]);

if ($selected === false) {
    echo "No requested locale is installed.\n";
} else {
    echo "Selected locale: ", $selected, "\n";
}
