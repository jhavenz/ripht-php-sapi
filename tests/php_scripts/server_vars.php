<?php
/**
 * Server variables test script.
 * 
 * Tests $_SERVER superglobal population with request metadata.
 */

header('Content-Type: application/json');

$response = [
    'method' => $_SERVER['REQUEST_METHOD'] ?? null,
    'uri' => $_SERVER['REQUEST_URI'] ?? null,
    'script_name' => $_SERVER['SCRIPT_NAME'] ?? null,
    'script_filename' => $_SERVER['SCRIPT_FILENAME'] ?? null,
    'document_root' => $_SERVER['DOCUMENT_ROOT'] ?? null,
    'server_name' => $_SERVER['SERVER_NAME'] ?? null,
    'server_port' => $_SERVER['SERVER_PORT'] ?? null,
    'remote_addr' => $_SERVER['REMOTE_ADDR'] ?? null,
    'server_protocol' => $_SERVER['SERVER_PROTOCOL'] ?? null,
    'https' => isset($_SERVER['HTTPS']) ? $_SERVER['HTTPS'] : null,
];

echo json_encode($response, JSON_PRETTY_PRINT);
