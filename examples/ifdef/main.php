<?php

// Build default: cargo run -- examples/ifdef/main.php
// Build debug branch: cargo run -- --define DEBUG examples/ifdef/main.php

ifdef DEBUG {
    echo "mode=debug\n";
    echo "extra checks enabled\n";
} else {
    echo "mode=release\n";
}

echo "always-on logic\n";
