<?php
// array_push() appends at the next integer key and keeps existing string keys.
$scores = ["Ada" => 98, "Grace" => 95];
$count = array_push($scores, 91, 89);

echo "count=$count\n";
foreach ($scores as $key => $score) {
    echo "$key=$score\n";
}
