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
    const browser = setup();
    assert.ok(
        browser.images.every(image => image.src === undefined),
        'All images should initially have undefined src before intersecting viewport'
    );
    browser.intersect([{ target: browser.images[1], isIntersecting: false }]);
    assert.equal(
        browser.images[1].src,
        undefined,
        'Offscreen image should remain unloaded when not intersecting'
    );
    browser.intersect([{ target: browser.images[0], isIntersecting: true }]);
    assert.equal(
        browser.images[0].src,
        'first.jpg',
        'Visible image should load its source URL from data-src when intersecting'
    );
    assert.equal(
        browser.images[1].src,
        undefined,
        'Unintersected offscreen image should remain unloaded'
    );
    assert.equal(
        browser.watched.has(browser.images[0]),
        false,
        'Loaded image should be unobserved from intersection observer'
    );
});

test('browsers without an observer receive native-lazy image sources', () => {
    const browser = setup({ observerAvailable: false });
    assert.deepEqual(
        browser.images.map(image => image.src),
        ['first.jpg', 'offscreen.jpg'],
        'When IntersectionObserver is unavailable, fallback should immediately populate all image src attributes'
    );
});

test('broken thumbnails fall back to the placeholder', () => {
    const browser = setup({ observerAvailable: false });
    browser.images[0].handlers.error();
    assert.equal(
        browser.images[0].src,
        undefined,
        'Failed image element should have its broken src attribute removed'
    );
    assert.equal(
        browser.images[0].hidden,
        true,
        'Failed image element should be hidden to let placeholder show'
    );
});

test('refresh reloads only after the server confirms completion', async () => {
    let finish;
    const browser = setup({ refresh: () => new Promise(resolve => { finish = resolve; }) });
    const refreshing = browser.controls['refresh-button'].click();
    assert.equal(
        browser.controls['refresh-button'].disabled,
        true,
        'Refresh button should be disabled immediately while refresh request is in-flight'
    );
    assert.equal(
        browser.reloaded(),
        false,
        'Browser should not reload page while refresh request is still pending'
    );
    finish({ ok: true });
    await refreshing;
    assert.equal(
        browser.reloaded(),
        true,
        'Browser should reload page after refresh request successfully completes'
    );
});

test('refresh failures stay on the current page and allow retry', async () => {
    const browser = setup({ refresh: async () => ({ ok: false }) });
    await browser.controls['refresh-button'].click();
    assert.equal(
        browser.reloaded(),
        false,
        'Browser should not reload page when refresh request fails'
    );
    assert.equal(
        browser.controls['refresh-button'].disabled,
        false,
        'Refresh button should be re-enabled after failure so user can retry'
    );
    assert.match(
        browser.controls['status-message'].textContent,
        /Please try again/,
        'Status message should display user-friendly retry guidance on failure'
    );
});
