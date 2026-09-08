'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const script = fs.readFileSync(path.join(__dirname, '../public/index.js'), 'utf8');
function setup({ observerAvailable = true, refresh = null } = {}) {
}

test('offscreen thumbnails stay unloaded until they approach the viewport', () => {
});
test('browsers without an observer receive native-lazy image sources', () => {
});
test('broken thumbnails fall back to the placeholder', () => {
});
test('refresh reloads only after the server confirms completion', async () => {
});
test('refresh failures stay on the current page and allow retry', async () => {
});
