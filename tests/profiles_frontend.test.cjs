'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../public/index.js'), 'utf8');

function deferred() {
    let resolve;
    let reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    return { promise, resolve, reject };
}
function json(value, { status = 200, profile = null, contentType = 'application/json' } = {}) {
    return {
        ok: status >= 200 && status < 300, status,
        headers: { get(name) { if (name.toLowerCase() === 'content-type') return contentType; if (name.toLowerCase() === 'x-droptube-profile') return profile; return null; } },
        async json() { return value; },
    };
}
class Element {
    constructor(id = '') {
        this.id = id; this.dataset = {}; this.handlers = {}; this.children = []; this.parent = null;
        this.disabled = false; this.hidden = false; this.value = ''; this.textContent = '';
        this.attributes = {}; this.controls = [];
    }
    addEventListener(name, handler) {
        if (!this.handlers[name]) this.handlers[name] = [];
        this.handlers[name].push(handler);
    }
    async dispatch(name, target = this, properties = {}) {
        for (const handler of this.handlers[name] || []) await handler({ target, preventDefault() {}, ...properties });
    }
    setAttribute(name, value) { this.attributes[name] = value; if (name === 'aria-busy' && value === 'false' && this.onIdle) this.onIdle(); }
    removeAttribute(name) { delete this.attributes[name]; }
    append(...elements) { this.children.push(...elements); elements.forEach(element => { element.parent = this; }); }
    replaceChildren(...elements) { this.children = []; this.append(...elements); }
    querySelectorAll(selector) {
        if (selector === '[data-rating-value]') return this.children.filter(child => Object.hasOwn(child.dataset, 'ratingValue'));
        if (selector === 'input, button' || selector === 'input, select, button') {
            const removals = [];
            function visit(node) { if (Object.hasOwn(node.dataset, 'removeTag')) removals.push(node); node.children.forEach(visit); }
            this.children.forEach(visit); return [...this.controls, ...removals];
        }
        return [];
    }
    querySelector() { return null; }
    closest(selector) {
        if (selector === '[data-rating-value]' && Object.hasOwn(this.dataset, 'ratingValue')) return this;
        if (selector === '[data-remove-tag]' && Object.hasOwn(this.dataset, 'removeTag')) return this;
        return this.parent === null ? null : this.parent.closest(selector);
    }
    contains(element) { return element === this || this.children.some(child => child.contains(element)); }
    set innerHTML(_) { throw new Error('Unsafe HTML insertion'); }
}
function makeDocument(profile, { watch = true, metadataAvailable = true, sharedTags = ['<script>text</script>', 'favorites', 'sci-fi'] } = {}) {
    const elements = new Map();
    function element(id) { const element = new Element(id); elements.set(id, element); return element; }
    const menu = element('profile-menu'); const context = element('profile-context');
    context.dataset = { profileSelected: String(profile !== null), profile: profile === null ? '' : profile };
    element('profile-label').textContent = profile === null ? 'Choose profile' : profile;
    element('profile-action-label').textContent = profile === null ? 'Choose a profile' : 'Switch profile';
    const form = element('profile-form'); const input = element('profile-name'); input.value = profile === null ? 'guest' : profile;
    const select = element('profile-submit'); const retry = element('profile-retry');
    form.controls = [input, select, retry]; element('profile-status');
    const page = element('profile-page'); page.dataset.metadataAvailable = String(metadataAvailable);
    for (const id of ['profile-filters', 'profile-search-filters', 'profile-metadata-warning', 'library-results', 'library-pagination']) element(id);
    const cardAggregate = element('card-aggregate'); cardAggregate.dataset.videoId = 'film.mp4';
    if (watch) {
        const editor = element('watch-metadata'); editor.dataset.videoId = 'film.mp4';
        const shared = element('shared-tags');
        for (const label of sharedTags) { const span = new Element(); span.textContent = label; shared.append(span); }
        const rating = element('personal-rating'); const tags = element('personal-tags');
        const stars = [];
        for (let value = 0.5; value <= 5; value += 0.5) {
            const star = element('rating-star-' + value); star.dataset.ratingValue = String(value);
            star.setAttribute('aria-label', 'Rate ' + value + ' out of 5 stars'); stars.push(star);
        }
        rating.append(...stars); element('personal-rating-value');
        const newTag = element('new-tag'); const tagButton = element('tag-submit');
        editor.controls = [...stars, newTag, tagButton];
        editor.append(shared, rating, tags); page.append(editor);
        element('tag-form'); element('personal-status'); element('tag-suggestions');
        const aggregate = element('watch-aggregate'); aggregate.dataset.videoId = 'film.mp4';
    }
    const video = element('video-player'); video.currentTime = 37; video.src = '/video/film.mp4'; video.tracks = [{ src: 'blob:subtitle-existing' }];
    function register(node) {
        if (node.id) elements.set(node.id, node);
        node.replaceWith = next => {
            if (node.parent !== null) {
                const position = node.parent.children.indexOf(node);
                node.parent.children[position] = next; next.parent = node.parent;
            }
            register(next);
            // A parsed server region has its own child control map, imported as DOM children.
            if (next.regionControls) for (const control of next.regionControls) register(control);
        };
        node.children.forEach(register);
    }
    const editor = elements.get('watch-metadata');
    if (editor) editor.regionControls = ['shared-tags', 'personal-rating', 'personal-rating-value', 'personal-tags', 'new-tag', 'tag-submit', 'personal-status', 'tag-form', 'tag-suggestions', 'watch-aggregate'].map(id => elements.get(id));
    for (const node of elements.values()) register(node);
    return {
        elements, getElementById(id) { return elements.get(id) || null; },
        querySelector() { return null; },
        querySelectorAll(selector) {
            if (selector === '.community-rating') return ['card-aggregate', 'watch-aggregate'].map(id => elements.get(id)).filter(Boolean);
            return [];
        },
        createElement() { return new Element(); },
        importNode(node) { return node; },
    };
}
function browser({ profile = 'alice', watch = true, overrides = null, missing = null } = {}) {
    const document = makeDocument(profile, { watch }); const calls = []; const diagnostics = [];
    if (missing) document.elements.delete(missing);
    let selected = profile;
    const data = new Map([
        ['alice', { ratings: { 'film.mp4': 4 }, tags: { 'film.mp4': ['favorites', '<script>text</script>'] } }],
        ['bob', { ratings: { 'film.mp4': 5 }, tags: { 'film.mp4': ['sci-fi'] } }],
        ['guest', { ratings: {}, tags: {} }],
    ]);
    let aggregates = { ratings: { 'film.mp4': { average: 4.5, count: 2 } } };
    const firstIdle = deferred();
    const contextElement = document.getElementById('profile-context');
    if (contextElement) contextElement.onIdle = () => firstIdle.resolve();
    const focus = new Element();
    const window = {
        location: { pathname: '/', search: '?v=film.mp4', reload() { throw new Error('Playback was reloaded'); } },
        addEventListener(name, handler) { focus.addEventListener(name, handler); },
        DOMParser: class { parseFromString(text) { const snapshot = JSON.parse(text); return makeDocument(snapshot.profile, { watch, metadataAvailable: snapshot.available, sharedTags: snapshot.sharedTags }); } },
    };
    async function standard(url, options) {
        if (url.startsWith('/?')) {
            const sharedTags = [...new Set([...data.values()].flatMap(metadata => metadata.tags['film.mp4'] || []))].sort();
            return { ok: true, headers: { get() { return 'text/html'; } }, async text() { return JSON.stringify({ profile: selected, available: true, sharedTags }); } };
        }
        if (url === '/api/user/profile') {
            if (options.method === 'POST') selected = JSON.parse(options.body).profile;
            return json({ profile: selected });
        }
        if (url === '/api/user/metadata') return json(data.get(selected) || { ratings: {}, tags: {} }, { profile: selected });
        if (url === '/api/videos/ratings') return json(aggregates);
        const body = JSON.parse(options.body); const entry = data.get(selected);
        if (body.expected_profile !== selected) return json({ error: 'Active profile changed.' }, { status: 409 });
        if (url === '/api/user/rating') {
            entry.ratings[body.video_id] = body.rating;
            const contributions = [...data.values()].map(metadata => metadata.ratings['film.mp4']).filter(value => typeof value === "number");
            aggregates = { ratings: { 'film.mp4': { average: contributions.reduce((sum, rating) => sum + rating, 0) / contributions.length, count: contributions.length } } };
        } else {
            const tags = new Set(entry.tags[body.video_id] || []);
            if (options.method === 'DELETE') tags.delete(body.tag); else tags.add(body.tag);
            if (tags.size === 0) delete entry.tags[body.video_id]; else entry.tags[body.video_id] = [...tags];
        }
        return json(entry, { profile: selected });
    }
    const fetch = async (url, options = {}) => {
        const call = { url, options }; calls.push(call);
        if (overrides) {
            const overridden = await overrides(call, { standard, setProfile(value) { selected = value; }, data });
            if (overridden !== undefined) return overridden;
        }
        return standard(url, options);
    };
    const context = vm.createContext({ document, window, fetch, AbortController, console: { error(...args) { diagnostics.push(args); } } });
    vm.runInContext(source, context);
    return { document, context, calls, diagnostics, data, focus, window, ready: firstIdle.promise,
        setProfile(value) { selected = value; }, selected() { return selected; },
        get(id) { return document.getElementById(id); },
        helper(name, ...args) { return context[name](...args); } };
}
function helpers() {
    const document = { getElementById() { return null; }, querySelector() { return null; }, querySelectorAll() { return []; } };
    const context = vm.createContext({ document, window: {}, console: { error() {} } });
    vm.runInContext(source, context); return context;
}
test('profile validation rejects whitespace, traversal, non-ASCII, punctuation and Windows device names', () => {
    const context = helpers();
    assert.equal(context.normalizeProfile('Alice2'), 'alice2');
    for (const value of ['', ' alice', 'alice ', 'alice\n', 'alice\t', 'a_b', 'a-b', '../alice', '..\\alice', '%2e%2e', 'é', 'a'.repeat(33), 'CON', 'com9', 'LpT1', null]) {
        assert.throws(() => context.normalizeProfile(value), undefined, 'reject ' + String(value));
    }
    for (const value of [{}, { profile: '' }, { profile: 'Alice' }, { profile: 1 }]) assert.throws(() => context.validateProfileResponse(value));
    assert.equal(context.validateProfileResponse({ profile: null }), null);
});
test('metadata validators reject wrong types, invalid ratings and noncanonical tags; inherited keys are ignored', () => {
    const context = helpers();
    for (const value of [null, [], {}, { ratings: [], tags: {} }, { ratings: {}, tags: null },
        { ratings: { movie: 0 }, tags: {} }, { ratings: { movie: 4.25 }, tags: {} }, { ratings: { movie: '4' }, tags: {} },
        { ratings: {}, tags: { movie: 'tag' } }, { ratings: {}, tags: { movie: ['Tag'] } }, { ratings: {}, tags: { movie: ['a', 'a'] } }]) {
        assert.throws(() => context.validateMetadata(value));
    }
    const ratings = Object.create({ inherited: 5 }); ratings.movie = 4;
    const validated = context.validateMetadata({ ratings, tags: { movie: ['<script>text</script>'] } });
    assert.equal(validated.ratings.has('inherited'), false); assert.equal(validated.ratings.get('movie'), 4);
});
test('tags follow Unicode scalar limits and Rust whitespace rather than UTF16 or JavaScript trim rules', () => {
    const context = helpers();
    assert.equal(context.normalizeTag('  FAVORITES  '), 'favorites');
    assert.throws(() => context.normalizeTag('\u0085tag'));
    assert.throws(() => context.normalizeTag('\ttag'));
    assert.throws(() => context.normalizeTag('tag\n'));
    assert.equal(context.normalizeTag('\u2003TAG\u2003'), 'tag');
    assert.equal(context.normalizeTag('\uFEFFtag\uFEFF'), '\uFEFFtag\uFEFF');
    assert.equal(context.normalizeTag('😀'.repeat(64)), '😀'.repeat(64));
    assert.throws(() => context.normalizeTag('😀'.repeat(65)));
    assert.throws(() => context.normalizeTag('\uD800'));
    assert.throws(() => context.normalizeTag('\uDC00'));
});
test('aggregate validation keeps null/count invariant and rejects unsafe numbers', () => {
    const context = helpers();
    for (const rating of [{ average: 4.5, count: 0 }, { average: null, count: 2 }, { average: NaN, count: 1 }, { average: 6, count: 1 }, { average: 4, count: -1 }, { average: 4, count: 1.5 }, { average: 4, count: Number.MAX_SAFE_INTEGER + 1 }, { count: 1 }]) {
        assert.throws(() => context.validateAggregates({ ratings: { movie: rating } }));
    }
    assert.equal(context.validateAggregates({ ratings: { movie: { average: null, count: 0 } } }).get('movie').average, null);
});

test('personal validation accepts exactly ten half-star values and keeps arbitrary community fractions', () => {
    const context = helpers();
    for (let value = 0.5; value <= 5; value += 0.5) {
        assert.equal(context.validateMetadata({ ratings: { movie: value }, tags: {} }).ratings.get('movie'), value);
    }
    for (const value of [0, -1, 0.25, 4.2, 4.25, 5.5, NaN, Infinity, -Infinity, null, true, '4.5']) {
        assert.throws(() => context.validateMetadata({ ratings: { movie: value }, tags: {} }));
    }
    for (const value of [0.5, 4.25, 3.8333333333333335]) {
        assert.equal(context.validateAggregates({ ratings: { movie: { average: value, count: 3 } } }).get('movie').average, value);
    }
    assert.throws(() => context.validateAggregates({ ratings: { movie: { average: 0.25, count: 1 } } }));
});

test('each half-star control saves once and preserves exact personal and community values', async () => {
    const fixture = browser(); await fixture.ready;
    for (let value = 0.5; value <= 5; value += 0.5) {
        const before = fixture.calls.filter(call => call.url === '/api/user/rating').length;
        await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-' + value));
        assert.equal(fixture.calls.filter(call => call.url === '/api/user/rating').length, before + 1);
        assert.equal(fixture.data.get('alice').ratings['film.mp4'], value);
        assert.equal(fixture.get('personal-rating-value').textContent, value + '/5');
        assert.equal(fixture.get('rating-star-' + value).attributes['aria-pressed'], 'true');
        assert.equal(fixture.get('watch-aggregate').textContent, (Math.round((value + 5) * 5) / 10).toFixed(1) + '/5 · 2 ratings');
    }
});

test('half stars stay selected independently across profile switching and rounded community displays', async () => {
    const fixture = browser(); await fixture.ready;
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-3.5'));
    assert.equal(fixture.get('rating-star-3.5').dataset.filled, 'true');
    assert.equal(fixture.get('rating-star-4').dataset.filled, 'false');
    const player = fixture.get('video-player');
    fixture.get('profile-name').value = 'bob'; await fixture.get('profile-form').dispatch('submit');
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-4.5'));
    assert.equal(fixture.get('watch-aggregate').textContent, '4.0/5 · 2 ratings');
    fixture.get('profile-name').value = 'alice'; await fixture.get('profile-form').dispatch('submit');
    assert.equal(fixture.get('personal-rating-value').textContent, '3.5/5');
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-4'));
    assert.equal(fixture.get('watch-aggregate').textContent, '4.3/5 · 2 ratings');
    assert.equal(fixture.get('video-player'), player);
    assert.equal(player.currentTime, 37);
});

test('stale half-star proposals are discarded and uncertain half-star commits reconcile without replay', async () => {
    const stale = browser(); await stale.ready; stale.setProfile('bob');
    await stale.get('profile-page').dispatch('click', stale.get('rating-star-0.5'));
    assert.equal(stale.calls.some(call => call.url === '/api/user/rating'), false);
    assert.equal(stale.get('personal-rating-value').textContent, '5/5');
    const uncertain = browser({ overrides: async (call, { standard }) => {
        if (call.url === '/api/user/rating') { await standard(call.url, call.options); throw new Error('Lost half-star response after commit'); }
    } });
    await uncertain.ready;
    await uncertain.get('profile-page').dispatch('click', uncertain.get('rating-star-0.5'));
    assert.equal(uncertain.calls.filter(call => call.url === '/api/user/rating').length, 1);
    assert.equal(uncertain.get('personal-rating-value').textContent, '0.5/5');
    assert.equal(uncertain.get('rating-star-0.5').disabled, false);
    assert.equal(uncertain.get('watch-aggregate').textContent, '2.8/5 · 2 ratings');
});
test('JSON boundaries validate error contracts, content types, malformed body and profile headers', async () => {
    const fixture = browser(); await fixture.ready;
    fixture.context.fetch = async () => json({}, { contentType: 'text/html' });
    await assert.rejects(fixture.helper('requestJson', '/api/user/profile'), /response type/);
    fixture.context.fetch = async () => json({ error: 4 }, { status: 400 });
    await assert.rejects(fixture.helper('requestJson', '/api/user/profile'), /invalid error/);
    fixture.context.fetch = async () => ({ ...json({}), async json() { throw new Error('parse failed'); } });
    await assert.rejects(fixture.helper('requestJson', '/api/user/profile'), /malformed JSON/);
    assert.throws(() => fixture.helper('metadataProfile', json({}, { profile: 'Alice' })), /normalized/);
    assert.throws(() => fixture.helper('metadataProfile', json({})));
});
test('personal and community ratings stay distinct and tags render exclusively as text', async () => {
    const fixture = browser(); await fixture.ready;
    assert.equal(fixture.get('personal-rating').dataset.rating, '4');
    assert.equal(fixture.get('personal-rating-value').textContent, '4/5');
    assert.equal(fixture.get('rating-star-4').attributes['aria-pressed'], 'true');
    assert.equal(fixture.get('rating-star-3').dataset.filled, 'true');
    assert.equal(fixture.get('rating-star-5').dataset.filled, 'false');
    assert.equal(fixture.get('watch-aggregate').textContent, '4.5/5 · 2 ratings');
    assert.equal(fixture.get('personal-tags').children[0].children[0].textContent, '<script>text</script>');
    assert.equal(fixture.get('rating-star-4').disabled, false);
});
test('community display rounds exact ties upward while retaining the unrounded aggregate', async () => {
    const aggregate = { ratings: { 'film.mp4': { average: 4.25, count: 4 } } };
    const fixture = browser({ overrides(call) { if (call.url === '/api/videos/ratings') return json(aggregate); } });
    await fixture.ready;
    assert.equal(fixture.get('watch-aggregate').textContent, '4.3/5 · 4 ratings');
    assert.equal(fixture.get('card-aggregate').textContent, '4.3/5 · 4 ratings');
    assert.equal(aggregate.ratings['film.mp4'].average, 4.25);
});
test('switching profiles refreshes regions while preserving the player and subtitle resources', async () => {
    const fixture = browser(); await fixture.ready;
    const video = fixture.get('video-player'); const tracks = video.tracks;
    fixture.get('profile-name').value = 'Bob';
    await fixture.get('profile-form').dispatch('submit');
    assert.equal(fixture.selected(), 'bob'); assert.equal(fixture.get('personal-rating').dataset.rating, '5');
    assert.equal(fixture.get('watch-aggregate').textContent, '4.5/5 · 2 ratings');
    assert.equal(fixture.get('video-player'), video); assert.equal(video.currentTime, 37); assert.equal(video.tracks, tracks);
    assert.equal(fixture.get('personal-tags').children[0].children[0].textContent, 'sci-fi');
    assert.deepEqual(fixture.get('shared-tags').children.map(span => span.textContent), ['<script>text</script>', 'favorites', 'sci-fi']);
    assert.ok(fixture.get('shared-tags').children.every(span => span.children.length === 0));
    assert.equal(fixture.get('profile-name').disabled, false);
});
test('a first visit loads public ratings while personal controls wait for selection', async () => {
    const fixture = browser({ profile: null }); await fixture.ready;
    assert.equal(fixture.get('rating-star-1').disabled, true); assert.equal(fixture.get('profile-menu').open, true);
    assert.equal(fixture.get('watch-aggregate').textContent, '4.5/5 · 2 ratings');
    assert.equal(fixture.calls.some(call => call.url === '/api/user/metadata'), false);
    assert.deepEqual(fixture.get('shared-tags').children.map(span => span.textContent), ['<script>text</script>', 'favorites', 'sci-fi']);
});
test('missing menu/editor controls disable only the profile feature and report contextual errors', async () => {
    const missing = browser({ missing: 'profile-label' });
    assert.equal(missing.get('profile-name').disabled, true);
    assert.equal(missing.get('rating-star-1').disabled, true);
    assert.equal(missing.get('new-tag').disabled, true);
    assert.equal(missing.get('rating-star-5').disabled, true);
    assert.equal(missing.get('tag-submit').disabled, true);
    assert.match(missing.get('profile-status').textContent, /required menu element is missing/);
    assert.match(missing.get('personal-status').textContent, /Reload this page to retry/);
    assert.equal(missing.get('video-player').src, '/video/film.mp4');
    assert.ok(missing.diagnostics.some(args => String(args[0]).includes('required menu')));
    const editor = browser({ missing: 'personal-tags' }); await editor.ready;
    assert.equal(editor.get('rating-star-1').disabled, true);
    assert.match(editor.get('profile-status').textContent, /missing required video controls/);
});
test('malformed metadata and HTTP failures recover selection controls without pretending to be empty success', async () => {
    const fixture = browser({ overrides(call) { if (call.url === '/api/user/metadata') return json({ ratings: null, tags: {} }, { profile: 'alice' }); } });
    await fixture.ready; assert.equal(fixture.get('rating-star-1').disabled, true);
    assert.equal(fixture.get('profile-name').disabled, false); assert.equal(fixture.get('profile-retry').hidden, false);
    assert.match(fixture.get('profile-status').textContent, /Could not load/); assert.ok(fixture.diagnostics.length > 0);
});
test('pre-edit profile refresh discards stale proposals rather than submitting to the newly selected profile', async () => {
    const fixture = browser(); await fixture.ready;
    fixture.setProfile('bob');
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-2'));
    assert.equal(fixture.calls.some(call => call.url === '/api/user/rating'), false);
    assert.equal(fixture.data.get('bob').ratings['film.mp4'], 5);
    assert.equal(fixture.get('personal-rating').dataset.rating, '5');
    assert.match(fixture.get('profile-status').textContent, /profile changed/);
});
test('focus preserves the saved stars and unsaved tag draft when the active profile is unchanged', async () => {
    const fixture = browser(); await fixture.ready;
    fixture.get('new-tag').value = 'unsaved tag';
    const editor = fixture.get('watch-metadata');
    const initialCalls = fixture.calls.length;
    await fixture.focus.dispatch('focus');
    assert.equal(fixture.get('personal-rating').dataset.rating, '4');
    assert.equal(fixture.get('new-tag').value, 'unsaved tag');
    assert.equal(fixture.get('watch-metadata'), editor);
    assert.equal(fixture.get('rating-star-4').disabled, false);
    assert.equal(fixture.calls.length, initialCalls + 1);
    assert.equal(fixture.calls[initialCalls].url, '/api/user/profile');
});
test('focus reloads a changed profile and asks the user to review and retry discarded drafts', async () => {
    const fixture = browser(); await fixture.ready;
    fixture.get('new-tag').value = 'unsaved tag';
    const editor = fixture.get('watch-metadata');
    fixture.setProfile('bob');
    await fixture.focus.dispatch('focus');
    assert.equal(fixture.get('profile-label').textContent, 'bob');
    assert.equal(fixture.get('personal-rating').dataset.rating, '5');
    assert.equal(fixture.get('new-tag').value, '');
    assert.notEqual(fixture.get('watch-metadata'), editor);
    assert.match(fixture.get('profile-status').textContent, /profile changed.*retry your edit/);
    assert.equal(fixture.calls.some(call => call.url === '/api/user/rating' || call.url === '/api/user/tag'), false);
});
test('selecting a star automatically saves with the displayed profile precondition and updates both values', async () => {
    const fixture = browser(); await fixture.ready;
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-2'));
    const write = fixture.calls.find(call => call.url === '/api/user/rating');
    assert.deepEqual(JSON.parse(write.options.body), { video_id: 'film.mp4', expected_profile: 'alice', rating: 2 });
    assert.equal(Object.hasOwn(write.options.headers, 'X-Droptube-Profile'), false);
    assert.equal(fixture.get('personal-rating').dataset.rating, '2'); assert.equal(fixture.get('watch-aggregate').textContent, '3.5/5 · 2 ratings');
    assert.equal(fixture.get('personal-rating-value').textContent, '2/5');
    assert.equal(fixture.get('rating-star-2').attributes['aria-pressed'], 'true');
    assert.equal(fixture.get('rating-star-4').attributes['aria-pressed'], 'false');
    assert.equal(fixture.get('rating-star-3').dataset.filled, 'false');
    assert.equal(fixture.calls.filter(call => call.url === '/api/user/rating').length, 1);
});
test('clicking the selected star is unchanged; selecting another star image immediately rerates', async () => {
    const fixture = browser(); await fixture.ready;
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-4'));
    assert.equal(fixture.calls.some(call => call.url === '/api/user/rating'), false);
    const image = new Element(); fixture.get('rating-star-1').append(image);
    await fixture.get('profile-page').dispatch('click', image);
    assert.equal(fixture.data.get('alice').ratings['film.mp4'], 1);
    assert.equal(fixture.get('rating-star-1').attributes['aria-pressed'], 'true');
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-5'));
    assert.equal(fixture.data.get('alice').ratings['film.mp4'], 5);
    assert.equal(fixture.calls.filter(call => call.url === '/api/user/rating').length, 2);
    assert.equal(fixture.get('rating-star-5').attributes['aria-pressed'], 'true');
    assert.equal(fixture.get('rating-star-1').attributes['aria-pressed'], 'false');
    assert.equal(fixture.get('watch-aggregate').textContent, '5.0/5 · 2 ratings');
});
test('an unrated selected profile has five empty stars until a selection is saved', async () => {
    const fixture = browser({ profile: 'guest' }); await fixture.ready;
    assert.equal(fixture.get('personal-rating').dataset.rating, '');
    assert.equal(fixture.get('personal-rating-value').textContent, 'Not rated');
    for (let value = 0.5; value <= 5; value += 0.5) {
        const star = fixture.get('rating-star-' + value);
        assert.equal(star.dataset.filled, 'false');
        assert.equal(star.attributes['aria-pressed'], 'false');
        assert.equal(star.disabled, false);
    }
    assert.equal(fixture.calls.some(call => call.url === '/api/user/rating'), false);
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-5'));
    assert.equal(fixture.data.get('guest').ratings['film.mp4'], 5);
    assert.equal(fixture.get('personal-rating-value').textContent, '5/5');
    assert.equal(fixture.get('watch-aggregate').textContent, '4.7/5 · 3 ratings');
});
test('invalid star values cannot start a write and missing star controls disable the editor', async () => {
    const fixture = browser(); await fixture.ready;
    fixture.get('rating-star-2').dataset.ratingValue = '2.25';
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-2'));
    assert.equal(fixture.calls.some(call => call.url === '/api/user/rating'), false);
    assert.match(fixture.get('profile-status').textContent, /from 0.5 to 5/);
    const missing = browser(); missing.get('personal-rating').children.pop();
    await missing.ready;
    assert.equal(missing.get('rating-star-1').disabled, true);
    assert.match(missing.get('profile-status').textContent, /ten half-star controls/);
});
test('rejected automatic rating saves retain the committed stars and recover all star controls', async () => {
    const fixture = browser({ overrides(call) {
        if (call.url === '/api/user/rating') return json({ error: 'Rating could not be accepted.' }, { status: 400 });
    } });
    await fixture.ready;
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-2'));
    assert.equal(fixture.data.get('alice').ratings['film.mp4'], 4);
    assert.equal(fixture.get('personal-rating').dataset.rating, '4');
    assert.equal(fixture.get('rating-star-4').attributes['aria-pressed'], 'true');
    assert.equal(fixture.get('rating-star-2').attributes['aria-pressed'], 'false');
    assert.equal(fixture.get('rating-star-1').disabled, false);
    assert.equal(fixture.get('rating-star-5').disabled, false);
    assert.match(fixture.get('profile-status').textContent, /Rating could not be accepted.*try again/);
});
test('server-rendered rating images use native labelled buttons without a confirmation form', () => {
    const template = fs.readFileSync(path.join(__dirname, '../templates/home.html'), 'utf8');
    assert.match(template, /id="personal-rating"[^>]*role="group"[^>]*aria-labelledby="personal-rating-label"/);
    assert.match(template, /<button type="button" class="rating-star rating-half"[^>]*data-rating-value=/);
    assert.match(template, /aria-label="Rate \{\{ full as f64 - 0.5 \}\} out of 5 stars" aria-pressed=/);
    assert.match(template, /<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path/);
    assert.equal(template.includes('id="rating-form"'), false);
    assert.equal(template.includes('<select id="personal-rating"'), false);
    assert.equal(template.includes('Save rating'), false);
});
test('tag additions use canonical values and deletion removes only the selected label', async () => {
    const fixture = browser(); await fixture.ready; fixture.get('new-tag').value = '  NEW TAG  ';
    await fixture.get('profile-page').dispatch('submit', fixture.get('tag-form'));
    assert.ok(fixture.data.get('alice').tags['film.mp4'].includes('new tag'));
    const span = fixture.get('personal-tags').children.find(span => span.children[0].textContent === 'favorites');
    await fixture.get('profile-page').dispatch('click', span.children[1]);
    assert.ok(!fixture.data.get('alice').tags['film.mp4'].includes('favorites'));
    assert.ok(fixture.data.get('alice').tags['film.mp4'].includes('new tag'));
    assert.ok(fixture.get('shared-tags').children.some(span => span.textContent === 'new tag'));
    assert.ok(fixture.get('shared-tags').children.some(span => span.textContent === 'sci-fi'));
});

test('removing your tag retains another profile contribution in the shared region and mounted player', async () => {
    const fixture = browser(); await fixture.ready;
    fixture.data.get('bob').tags['film.mp4'].push('favorites');
    const player = fixture.get('video-player'); const tracks = player.tracks;
    const owned = fixture.get('personal-tags').children.find(span => span.children[0].textContent === 'favorites');
    assert.equal(owned.children[1].attributes['aria-label'], 'Remove your tag favorites');
    await fixture.get('profile-page').dispatch('click', owned.children[1]);
    assert.ok(!fixture.data.get('alice').tags['film.mp4'].includes('favorites'));
    assert.ok(fixture.data.get('bob').tags['film.mp4'].includes('favorites'));
    assert.ok(fixture.get('shared-tags').children.some(span => span.textContent === 'favorites'));
    assert.ok(fixture.get('personal-tags').children.every(span => span.children[0].textContent !== 'favorites'));
    assert.equal(fixture.get('video-player'), player); assert.equal(player.currentTime, 37); assert.equal(player.tracks, tracks);
});

test('watch template separates escaped shared labels from your removable contributions', () => {
    const template = fs.readFileSync(path.join(__dirname, '../templates/home.html'), 'utf8');
    assert.match(template, /id="shared-tags"[^>]*>[\s\S]*?for tag in video\.tags[\s\S]*?class="shared-tag">\{\{ tag \}\}<\/span>/);
    assert.match(template, /id="personal-tags"[^>]*>\{% for tag in video\.personal_tags %\}/);
    assert.match(template, /aria-label="Remove your tag \{\{ tag \}\}"/);
    assert.ok(template.includes('>All tags</h2>') && template.includes('>Your tags</h2>'));
});
test('duplicate star selections are ignored while a save is pending and controls recover', async () => {
    const started = deferred(); const finish = deferred();
    const fixture = browser({ overrides(call) { if (call.url === '/api/user/rating') { started.resolve(); return finish.promise; } } });
    await fixture.ready;
    const saving = fixture.get('profile-page').dispatch('click', fixture.get('rating-star-2')); await started.promise;
    assert.equal(fixture.get('rating-star-2').disabled, true);
    assert.equal(fixture.get('rating-star-5').disabled, true);
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-2'));
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-3'));
    assert.equal(fixture.calls.filter(call => call.url === '/api/user/rating').length, 1);
    finish.resolve(json({ ratings: { 'film.mp4': 2 }, tags: {} }, { profile: 'alice' }));
    await saving; assert.equal(fixture.get('rating-star-2').disabled, false);
});
test('uncertain committed writes reload authoritative state and are never replayed', async () => {
    const fixture = browser({ overrides: async (call, { standard }) => {
        if (call.url === '/api/user/rating') { await standard(call.url, call.options); throw new Error('Connection lost after commit'); }
    } });
    await fixture.ready;
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-2'));
    assert.equal(fixture.calls.filter(call => call.url === '/api/user/rating').length, 1);
    assert.equal(fixture.get('personal-rating').dataset.rating, '2'); assert.equal(fixture.get('watch-aggregate').textContent, '3.5/5 · 2 ratings');
    assert.match(fixture.get('profile-status').textContent, /review it before retrying/);
});
test('uncertain saves keep edits disabled until aggregate reconciliation succeeds', async () => {
    let saved = false;
    const fixture = browser({ overrides: async (call, { standard }) => {
        if (call.url === '/api/user/rating') { saved = true; await standard(call.url, call.options); throw new Error('lost response'); }
        if (saved && call.url === '/api/videos/ratings') throw new Error('aggregate offline');
    } });
    await fixture.ready;
    await fixture.get('profile-page').dispatch('click', fixture.get('rating-star-2'));
    assert.equal(fixture.get('rating-star-2').disabled, true); assert.equal(fixture.get('profile-retry').hidden, false);
    assert.match(fixture.get('profile-status').textContent, /uncertain/);
    saved = false; await fixture.get('profile-retry').dispatch('click');
    assert.equal(fixture.get('rating-star-2').disabled, false);
});
test('out-of-order cancelled reads cannot overwrite a later profile selection', async () => {
    let held = false; const started = deferred(); const finish = deferred();
    const fixture = browser({ overrides(call) {
        if (held && call.url === '/api/user/profile' && !call.options.method) { held = false; started.resolve(call); return finish.promise; }
    } });
    await fixture.ready; held = true;
    const focusing = fixture.focus.dispatch('focus'); const old = await started.promise;
    fixture.get('profile-name').value = 'bob'; await fixture.get('profile-form').dispatch('submit');
    finish.resolve(json({ profile: 'alice' })); await focusing;
    assert.equal(old.options.signal.aborted, true); assert.equal(fixture.get('profile-label').textContent, 'bob');
    assert.equal(fixture.get('personal-rating').dataset.rating, '5');
});
test('BFCache restoration reconciles a cancelled read and a changed cookie without duplicating initial reads', async () => {
    let held = false; const started = deferred(); const finish = deferred();
    const fixture = browser({ overrides(call) {
        if (held && call.url === '/api/user/profile' && !call.options.method) { held = false; started.resolve(call); return finish.promise; }
    } });
    await fixture.ready;
    const initialCalls = fixture.calls.length;
    await fixture.focus.dispatch('pageshow', fixture.focus, { persisted: false });
    assert.equal(fixture.calls.length, initialCalls);
    held = true;
    const focusing = fixture.focus.dispatch('focus'); const old = await started.promise;
    await fixture.focus.dispatch('pagehide');
    assert.equal(old.options.signal.aborted, true);
    assert.equal(fixture.get('rating-star-5').disabled, true);
    fixture.setProfile('bob');
    await fixture.focus.dispatch('pageshow', fixture.focus, { persisted: true });
    assert.equal(fixture.get('profile-label').textContent, 'bob');
    assert.equal(fixture.get('personal-rating').dataset.rating, '5');
    assert.equal(fixture.get('rating-star-5').disabled, false);
    finish.resolve(json({ profile: 'alice' })); await focusing;
    assert.equal(fixture.get('profile-label').textContent, 'bob');
});
test('metadata response profile mismatches are errors rather than silently retargeting the UI', async () => {
    const fixture = browser({ overrides(call) { if (call.url === '/api/user/metadata') return json({ ratings: {}, tags: {} }, { profile: 'bob' }); } });
    await fixture.ready; assert.equal(fixture.get('rating-star-1').disabled, true);
    assert.match(fixture.get('profile-status').textContent, /Profile changed/);
});
test('duplicate feature initialization adds neither listeners nor reads', async () => {
    const fixture = browser(); await fixture.ready;
    const count = fixture.calls.length; const handlers = fixture.get('profile-form').handlers.submit.length;
    fixture.helper('initializeProfiles', fixture.document, fixture.window, { enhanceMedia() {}, releaseRegion() {} });
    assert.equal(fixture.calls.length, count); assert.equal(fixture.get('profile-form').handlers.submit.length, handlers);
});

