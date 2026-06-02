<?php
// Converting between dotted-quad addresses, integers, and packed bytes.

// long2ip(): integer -> dotted-quad string.
echo "loopback:  " . long2ip(2130706433) . "\n";
echo "private:   " . long2ip(3232235777) . "\n";
echo "broadcast: " . long2ip(4294967295) . "\n";

// ip2long(): dotted-quad string -> integer.
var_dump(ip2long("192.168.1.1"));
var_dump(ip2long("not an address"));

// inet_ntop(): 4-byte binary string -> dotted-quad string.
$packed = chr(10) . chr(0) . chr(0) . chr(1);
echo "packed -> " . inet_ntop($packed) . "\n";

// inet_pton(): dotted-quad string -> 4-byte binary string.
var_dump(inet_pton("8.8.8.8"));
