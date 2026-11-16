<?php

// GMP: arbitrary-precision integers through PHP's ext/gmp surface.

$factorial = gmp_fact(30);
echo "30! = ", gmp_strval($factorial), "\n";

$base = gmp_init('123456789012345678901234567890');
$sum = $base + 1;
echo "base + 1 = ", gmp_strval($sum), "\n";

// Modular exponentiation stays exact: no float rounding anywhere.
$modulus = gmp_init('fffffffffffffffffffffffffffffffeffffffffffffffff', 16);
$secret = gmp_init('deadbeefcafebabe', 16);
$power = gmp_powm($secret, gmp_init(65537), $modulus);
echo "powm = ", gmp_strval($power, 16), "\n";

// The extended GCD exposes the Bézout coefficients as named GMP values.
$extended = gmp_gcdext(gmp_init(240), gmp_init(46));
echo "gcd = ", gmp_strval($extended['g']),
    " s = ", gmp_strval($extended['s']),
    " t = ", gmp_strval($extended['t']), "\n";

// Comparison, bit tests and object casts share PHP's own operator rules.
var_dump($power > $secret);
var_dump(gmp_testbit($factorial, 0));
var_dump((int) gmp_init('42'));
