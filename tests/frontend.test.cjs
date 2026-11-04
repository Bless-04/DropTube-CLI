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
        'refresh-button': { dataset: {}, addEventListener(_, handler) { this.click = handler; }, setAttribute() {}, removeAttribute() {} },
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


test('convertSrtToVtt transforms timestamps and prepends WEBVTT', () => {
    const sandbox = {};
    vm.runInNewContext(script, sandbox);
    const convert = sandbox.convertSrtToVtt;
    assert.equal(typeof convert, 'function');

    const srt = '1\r\n00:01:20,123 --> 00:01:25,456\r\nHello world!\r\n\r\n2\r\n0:02:00,000 --> 00:02:05,500\r\nSecond line.';
    const vtt = convert(srt);
    assert.ok(vtt.startsWith('WEBVTT\n\n'));
    assert.ok(vtt.includes('00:01:20.123 --> 00:01:25.456\nHello world!'));
    assert.ok(vtt.includes('00:02:00.000 --> 00:02:05.500\nSecond line.'));
    assert.ok(!vtt.includes('\r'));

    // Strip BOM
    const srtWithBom = '\uFEFF1\n00:00:01,000 --> 00:00:02,000\nHi';
    assert.ok(!convert(srtWithBom).includes('\uFEFF'));
});

test('convertSrtToVtt rejects non-string and malformed SRT cues', () => {
    const sandbox = {};
    vm.runInNewContext(script, sandbox);
    const convert = sandbox.convertSrtToVtt;

    assert.throws(() => convert(123), /must be a string/);
    assert.throws(() => convert(null), /must be a string/);
    assert.throws(() => convert('just some text with no timestamps'), /malformed/);
});

test('setupSubtitles converts SRT tracks into WebVTT Blob URLs and attaches error listener', async () => {
    const createdBlobs = [];
    const revokedBlobs = [];
    class MockBlob {
        constructor(parts, options) {
            this.parts = parts;
            this.options = options;
            createdBlobs.push(this);
        }
    }
    let blobCounter = 0;
    const windowMock = {
        Blob: MockBlob,
        URL: {
            createObjectURL() {
                return `blob:test-${++blobCounter}`;
            },
            revokeObjectURL(url) {
                revokedBlobs.push(url);
            },
        },
        pagehideHandlers: [],
        addEventListener(event, handler) {
            if (event === 'pagehide') this.pagehideHandlers.push(handler);
        },
    };

    const srtTrack = {
        src: '/video/sub.srt',
        handlers: {},
        getAttribute(name) { return name === 'src' ? this.src : null; },
        addEventListener(event, handler) { this.handlers[event] = handler; },
    };
    const vttTrack = {
        src: '/video/sub.vtt',
        handlers: {},
        getAttribute(name) { return name === 'src' ? this.src : null; },
        addEventListener(event, handler) { this.handlers[event] = handler; },
    };
    const videoMock = {
        querySelectorAll(selector) {
            if (selector === 'track') return [srtTrack, vttTrack];
            return [];
        },
    };

    let fetchCalled = false;
    const fetchMock = async (url) => {
        if (url === '/video/sub.srt') {
            fetchCalled = true;
            return {
                ok: true,
                status: 200,
                statusText: 'OK',
                text: async () => '1\n00:00:01,000 --> 00:00:02,000\nHello',
            };
        }
        return { ok: false, status: 404, statusText: 'Not Found' };
    };

    const sandbox = {
        fetch: fetchMock,
        console: { error() {} },
    };
    vm.runInNewContext(script, sandbox);
    sandbox.setupSubtitles(videoMock, windowMock);

    await new Promise(resolve => setImmediate(resolve));

    assert.ok(fetchCalled, 'SRT track should be fetched');
    assert.equal(srtTrack.src, 'blob:test-1', 'SRT track src should be updated with blob URL');
    assert.equal(vttTrack.src, '/video/sub.vtt', 'VTT track should remain untouched');
    assert.equal(createdBlobs[0].options.type, 'text/vtt');
    assert.ok(createdBlobs[0].parts[0].startsWith('WEBVTT\n\n'));

    // Test pagehide revokes Blob URLs
    assert.equal(windowMock.pagehideHandlers.length, 1);
    windowMock.pagehideHandlers[0]();
    assert.deepEqual(revokedBlobs, ['blob:test-1']);
});

test('setupSubtitles logs error on network failure, parse error, or track error event', async () => {
    const errorLogs = [];
    const windowMock = {
        Blob: class {},
        URL: { createObjectURL() {}, revokeObjectURL() {} },
        addEventListener() {},
    };
    const failingTrack = {
        src: '/video/missing.srt',
        handlers: {},
        getAttribute(name) { return name === 'src' ? this.src : null; },
        addEventListener(event, handler) { this.handlers[event] = handler; },
    };
    const videoMock = {
        querySelectorAll(selector) {
            if (selector === 'track') return [failingTrack];
            return [];
        },
    };

    const sandbox = {
        fetch: async () => ({ ok: false, status: 404, statusText: 'Not Found' }),
        console: {
            error(...args) { errorLogs.push(args.join(' ')); },
        },
    };
    vm.runInNewContext(script, sandbox);
    sandbox.setupSubtitles(videoMock, windowMock);

    await new Promise(resolve => setImmediate(resolve));

    assert.ok(errorLogs.some(log => log.includes('Failed to load or convert SRT subtitle from /video/missing.srt: Error: HTTP 404 Not Found')));

    failingTrack.handlers.error({ type: 'error' });
    assert.ok(errorLogs.some(log => log.includes('Subtitle track failed to load from /video/missing.srt:')));
});
