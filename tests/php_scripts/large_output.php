<?php
/**
 * Large output test script.
 *
 * Generates a configurable response body for stress testing output handling.
 *
 * Query parameters:
 *   size - Output size in bytes (default: 1MB, max: 10MB)
 */

header('Content-Type: text/plain');

$size = isset($_GET['size']) ? (int)$_GET['size'] : (1024 * 1024);
$size = max(1, min($size, 10 * 1024 * 1024));

$chunk_size = 1024;
$chunks = (int)($size / $chunk_size);
$remainder = $size % $chunk_size;

$chunk = str_repeat('x', $chunk_size);

for ($i = 0; $i < $chunks; $i++) {
    echo $chunk;
}

if ($remainder > 0) {
    echo str_repeat('x', $remainder);
}
