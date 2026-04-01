<?php

function show_union_state() {
    ?int $count = null;
    echo $count ?? 41;
    echo ":";

    int|string $status = "ready";
    echo gettype($status);
    echo ":";
    echo $status;
}

show_union_state();
