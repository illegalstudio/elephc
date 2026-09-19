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

// inet_pton()/inet_ntop() take IPv6 too, and the length is what says which family a packed
// address belongs to: four bytes for IPv4, sixteen for IPv6.
$v6 = inet_pton("2001:4860:4860::8888");
echo "v6 bytes:  " . strlen($v6) . "\n";
echo "v6 hex:    " . bin2hex($v6) . "\n";
echo "v6 back:   " . inet_ntop($v6) . "\n";

// Rendering picks PHP's canonical spelling: the longest run of zero groups becomes "::", and
// an IPv4-mapped address keeps its dotted-quad tail.
echo "expanded:  " . inet_ntop(inet_pton("2001:0db8:85a3:0000:0000:8a2e:0370:7334")) . "\n";
echo "mapped:    " . inet_ntop(inet_pton("::ffff:192.0.2.128")) . "\n";
echo "loopback6: " . inet_ntop(inet_pton("::1")) . "\n";

// Anything that is not an address of either family is false.
var_dump(inet_pton("1:2:3:4:5:6:7:8:9"));
var_dump(inet_ntop("xx"));
