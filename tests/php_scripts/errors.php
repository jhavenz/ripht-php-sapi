<?php
/**
 * Tests PHP logging and error generation.
 *
 * Note: error_log() is confusingly named - it logs TO the error log,
 * but at LOG_NOTICE level (5), not LOG_ERR. This is just how PHP works.
 *
 * trigger_error() with display_errors=On goes straight to output (HTML),
 * not through log_message callback. You'll see these in the response body.
 */

header('Content-Type: application/json');

// goes through SAPI log_message callback at LOG_NOTICE level (yes, really)
error_log('Sending an error log...');

// these go to display output when display_errors is on
trigger_error('Sending a warning...', E_USER_WARNING);
trigger_error('Sending a notice...', E_USER_NOTICE);

// Fyi, this is how an error would be triggered, halting execution of this script.
// Uncommenting it will cause the script to exit right here, so you'll never see the JSON output below.
// trigger_error('Kabooom!!', E_USER_ERROR);

echo json_encode([
    'status' => 'ok',
    'note' => 'check the output above for trigger_error messages',
], JSON_PRETTY_PRINT);

