<?php

// Build default: cargo run -- build examples/ifdef/main.php
// Build debug branch: cargo run -- build --define DEBUG examples/ifdef/main.php

ifdef DEBUG {
    echo "mode=debug\n";
    echo "extra checks enabled\n";
} else {
    echo "mode=release\n";
}

echo "always-on logic\n";
