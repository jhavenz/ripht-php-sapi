<?php
header('Content-Type: application/json');
echo json_encode([
    'message' => 'Hello from PHP!',
    'datetime' => (new DateTimeImmutable())->format('Y-m-d H:i:s')
], JSON_PRETTY_PRINT);