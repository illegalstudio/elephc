<?php
// FFI callbacks — C calling a PHP closure.
//
// `qsort()` invokes the comparator from C. Assigning the closure to a C function-pointer
// global builds a libffi closure, so the callback runs natively without a PHP VM in the
// C library's call path.

$libc = FFI::cdef("
    typedef int (*compar_fn)(const void *, const void *);
    void qsort(void *base, size_t nmemb, size_t size, compar_fn compar);
", null);

$values = FFI::new("int[6]");
$unsorted = [5, 2, 9, 1, 7, 3];
for ($index = 0; $index < 6; $index++) {
    $values[$index] = $unsorted[$index];
}

// Comparing through `int*` views of the two `const void *` arguments.
$comparator = $libc->new("compar_fn");
$comparator->cdata = function (\FFI\CData $left, \FFI\CData $right): int {
    $a = FFI::cast("int*", $left)[0];
    $b = FFI::cast("int*", $right)[0];
    return (int) ($a <=> $b);
};

$libc->qsort($values, 6, 4, $comparator);

for ($index = 0; $index < 6; $index++) {
    echo $values[$index] . ($index === 5 ? "\n" : " ");
}
