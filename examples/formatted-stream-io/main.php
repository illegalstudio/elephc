<?php
// fprintf() formats its arguments like sprintf() and writes the result to a
// stream, returning the number of bytes written. fscanf() reads one line from
// a stream and parses it with the sscanf() engine, returning the matched
// fields as an array.

$f = fopen("php://temp", "r+");

// Write three rows of name / age / price. fprintf returns each write's byte
// count; %.2f formats the float price with two decimals.
$n  = fprintf($f, "%s %d %.2f\n", "alice", 30, 9.5);
$n += fprintf($f, "%s %d %.2f\n", "bob", 25, 12.0);
$n += fprintf($f, "%s %d %.2f\n", "carol", 41, 7.25);
echo "wrote $n bytes\n";

// Rewind and read the rows back one at a time with fscanf. %f parses the price
// field (returned as its captured substring, like %d/%s).
rewind($f);
while (($row = fscanf($f, "%s %d %f")) && count($row) === 3) {
    echo "  " . $row[0] . " is " . $row[1] . " (\$" . $row[2] . ")\n";
}

fclose($f);
