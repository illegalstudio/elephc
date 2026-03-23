<?php
// Multi-dimensional arrays (nested arrays)

// Create a 2D matrix
$matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];

// Access individual elements
echo "Element [0][1] = " . $matrix[0][1] . "\n";
echo "Element [2][2] = " . $matrix[2][2] . "\n";

// Nested foreach to print the matrix
echo "\nMatrix:\n";
foreach ($matrix as $row) {
    foreach ($row as $val) {
        echo $val . " ";
    }
    echo "\n";
}

// Push a new row to the matrix
$matrix[] = [10, 11, 12];
echo "\nAfter adding a row:\n";
echo "Rows: " . count($matrix) . "\n";
echo "New row: " . $matrix[3][0] . " " . $matrix[3][1] . " " . $matrix[3][2] . "\n";
