<?php
function countdown($from) {
    $i = $from;
    while ($i > 0) {
        echo $i . "... ";
        $i--;
    }
    echo "Go!\n";
}

countdown(10);
countdown(3);
