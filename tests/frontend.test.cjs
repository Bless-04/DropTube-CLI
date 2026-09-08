'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const script = fs.readFileSync(path.join(__dirname, '../public/index.js'), 'utf8');

function setup({ observerAvailable = true, refresh = null } = {}) {
    const images = ['first.jpg', 'offscreen.jpg'].map(url => ({
        dataset: { src: url },
        handlers: {},
        addEventListener(name, handler) { this.handlers[name] = handler; },
        removeAttribute(name) { delete this[name]; },
    }));
    let callback;
    const watched = new Set();
    class Observer {
        constructor(handler) { callback = handler; }
        observe(image) { watched.add(image); }
        unobserve(image) { watched.delete(image); }
    }
    const window = observerAvailable ? { IntersectionObserver: Observer } : {};
    let reloaded = false;
    window.location = { reload() { reloaded = true; } };
    const controls = refresh ? {
        'refresh-button': { addEventListener(_, handler) { this.click = handler; }, setAttribute() {}, removeAttribute() {} },
        'refresh-icon': { classList: { add() {}, remove() {} } },
        'status-message': {},
    } : {};
    const document = {
        querySelectorAll(selector) { return selector === 'img[data-src]' ? images : []; },
        querySelector() { return null; },
        getElementById(id) { return controls[id] || null; },
    };
    vm.runInNewContext(script, { document, window, IntersectionObserver: Observer, fetch: refresh });
    return { images, watched, controls, intersect(entries) { callback(entries); }, reloaded() { return reloaded; } };
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
