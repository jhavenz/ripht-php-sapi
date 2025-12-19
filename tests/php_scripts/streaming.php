<?php
/**
 * Streaming output test script.
 * 
 * Tests incremental output generation for streaming functionality.
 */

header('Content-Type: text/plain');

for ($i = 1; $i <= 10; $i++) {
    echo "Chunk $i\n";
    flush();
    usleep(100000);
}

echo "[DONE]\n";
