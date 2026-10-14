'use strict';

/** @typedef {{ratings: Map<string, number>, tags: Map<string, string[]>}} PersonalMetadata */
/** @typedef {{average: number|null, count: number}} CommunityRating */
/** @typedef {'no-profile'|'loading'|'ready'|'saving'|'error'} ProfileState */

/** Validate profile identifiers without changing invalid input. */
function normalizeProfile(value) {
    if (typeof value !== 'string' || value.length < 1 || value.length > 32 || /[^A-Za-z0-9]/.test(value)) throw new Error('Use 1–32 letters and digits for the profile name.');
    const profile = value.toLowerCase();
    if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/.test(profile)) throw new Error('That profile name is reserved by Windows.');
    return profile;
}
/** Match Rust Unicode whitespace trimming; control characters are rejected before trimming. */
function normalizeTag(value) {
    if (typeof value !== 'string' || /[\u0000-\u001F\u007F-\u009F\uD800-\uDFFF]/u.test(value)) throw new Error('Tags cannot contain control characters or invalid Unicode.');
    const whitespace = '[\\u0009-\\u000D \\u0085\\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000]';
    const tag = value.replace(new RegExp('^' + whitespace + '+|' + whitespace + '+$', 'gu'), '').toLowerCase();
    if (tag === '' || [...tag].length > 64) throw new Error('Use a tag containing 1–64 characters.');
    return tag;
}
function requireObject(value, context) {
    if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error(context + ' must be an object.');
    return value;
}
function hasOwn(object, key) { return Object.prototype.hasOwnProperty.call(object, key); }
function validateProfileResponse(value) {
    const object = requireObject(value, 'Profile response');
    if (!hasOwn(object, 'profile')) throw new Error('Profile response is missing its selection.');
    if (object.profile === null) return null;
    const profile = normalizeProfile(object.profile);
    if (object.profile !== profile) throw new Error('Profile response is not normalized.');
    return profile;
}
/** Return validated maps; inherited properties are never treated as metadata. */
function validateMetadata(value) {
    const object = requireObject(value, 'Personal metadata');
    if (!hasOwn(object, 'ratings') || !hasOwn(object, 'tags')) throw new Error('Personal metadata is missing required maps.');
    const ratings = requireObject(object.ratings, 'Personal ratings');
    const tags = requireObject(object.tags, 'Personal tags');
    const result = { ratings: new Map(), tags: new Map() };
    for (const id of Object.keys(ratings)) {
        const rating = ratings[id];
        if (id === '' || !Number.isInteger(rating) || rating < 1 || rating > 5) throw new Error('Personal rating is invalid.');
        result.ratings.set(id, rating);
    }
    for (const id of Object.keys(tags)) {
        if (id === '' || !Array.isArray(tags[id])) throw new Error('Personal tag list is invalid.');
        const labels = new Set();
        for (const tag of tags[id]) {
            if (normalizeTag(tag) !== tag || labels.has(tag)) throw new Error('Personal tag is invalid or duplicated.');
            labels.add(tag);
        }
        result.tags.set(id, [...labels].sort());
    }
    return result;
}
function validateAggregates(value) {
    const object = requireObject(value, 'Community ratings');
    if (!hasOwn(object, 'ratings')) throw new Error('Community ratings are missing their map.');
    const ratings = requireObject(object.ratings, 'Community rating map');
    const result = new Map();
    for (const id of Object.keys(ratings)) {
        const rating = requireObject(ratings[id], 'Community rating');
        if (id === '' || !hasOwn(rating, 'average') || !hasOwn(rating, 'count') || !Number.isSafeInteger(rating.count) || rating.count < 0 ||
            (rating.count === 0 ? rating.average !== null : typeof rating.average !== 'number' || !Number.isFinite(rating.average) || rating.average < 1 || rating.average > 5)) {
            throw new Error('Community rating average/count is invalid.');
        }
        result.set(id, { average: rating.average, count: rating.count });
    }
    return result;
}
class ApiFailure extends Error {
    constructor(message, status) { super(message); this.name = 'ApiFailure'; this.status = status; }
}
/** JSON status, content type, and body are checked before a feature validator is applied. */
async function requestJson(url, options = {}) {
    const response = await fetch(url, { credentials: 'same-origin', cache: 'no-store', ...options });
    const contentType = response.headers.get('content-type');
    if (typeof contentType !== 'string' || !/^application\/json(?:\s*;|$)/i.test(contentType)) throw new ApiFailure('The server returned an unexpected response type.', response.status);
    let value;
    try { value = await response.json(); } catch (error) { throw new ApiFailure('The server returned malformed JSON.', response.status); }
    if (!response.ok) {
        const body = requireObject(value, 'Error response');
        if (!hasOwn(body, 'error') || typeof body.error !== 'string' || body.error === '') throw new ApiFailure('The server returned an invalid error response.', response.status);
        throw new ApiFailure(body.error, response.status);
    }
    return { value, response };
}
function metadataProfile(response) {
    const value = response.headers.get('x-droptube-profile');
    const profile = normalizeProfile(value);
    if (profile !== value) throw new Error('Metadata response profile is not normalized.');
    return profile;
}
function readRenderedProfile(element) {
    if (element === null) throw new Error('The rendered profile context is missing.');
    if (element.dataset.profileSelected === 'false' && element.dataset.profile === '') return null;
    if (element.dataset.profileSelected !== 'true') throw new Error('The rendered profile context is invalid.');
    const profile = normalizeProfile(element.dataset.profile);
    if (profile !== element.dataset.profile) throw new Error('The rendered profile is not normalized.');
    return profile;
}
function getRelativeTime(timestamp) {
    const difference = Math.max(0, Math.floor(Date.now() / 1000) - timestamp);
    if (difference < 60) return 'Just now';
    const units = [[31536000, 'year'], [2592000, 'month'], [86400, 'day'], [3600, 'hour'], [60, 'minute']];
    for (const [seconds, unit] of units) { const count = Math.floor(difference / seconds); if (count > 0) return count + ' ' + unit + (count === 1 ? '' : 's') + ' ago'; }
    return 'Just now';
}

/** Initialize one document. State and resource ownership stay inside their feature closures. */
function initializePage(document, window) {
    if (document.documentElement !== undefined) {
        if (document.documentElement.dataset.droptubeInitialized === 'true') return;
        document.documentElement.dataset.droptubeInitialized = 'true';
    }
    let observer = null;
    const loadedImages = new WeakSet();
    function enhanceMedia(root) {
        root.querySelectorAll('.time-elapsed').forEach(element => {
            const timestamp = Number(element.dataset.timestamp);
            if (Number.isSafeInteger(timestamp) && timestamp > 0) element.textContent = getRelativeTime(timestamp);
        });
        root.querySelectorAll('img[data-src]').forEach(image => {
            if (image.dataset.thumbnailInitialized === 'true') return;
            image.dataset.thumbnailInitialized = 'true';
            if (observer !== null) observer.observe(image); else loadThumbnail(image);
        });
    }
    function loadThumbnail(image) {
        if (loadedImages.has(image)) return;
        loadedImages.add(image);
        image.addEventListener('error', () => { image.removeAttribute('src'); image.hidden = true; }, { once: true });
        image.src = image.dataset.src; delete image.dataset.src;
    }
    if ('IntersectionObserver' in window) {
        observer = new window.IntersectionObserver(entries => {
            for (const entry of entries) if (entry.isIntersecting) { loadThumbnail(entry.target); observer.unobserve(entry.target); }
        }, { rootMargin: '200px 0px' });
    }
    enhanceMedia(document);
    if (observer !== null && typeof window.addEventListener === 'function') window.addEventListener('pagehide', () => observer.disconnect());
    const searchForm = document.querySelector('.search-form');
    if (searchForm !== null && searchForm.dataset.initialized !== 'true') {
        const search = searchForm.querySelector('[name="search"]');
        if (search !== null) { searchForm.dataset.initialized = 'true'; searchForm.addEventListener('submit', () => { search.value = search.value.trim(); }); }
        else console.error('Search form is missing its input.');
    }
    const menuToggle = document.getElementById('menu-toggle');
    if (menuToggle !== null && menuToggle.dataset.initialized !== 'true') {
        menuToggle.dataset.initialized = 'true';
        menuToggle.addEventListener('click', () => { const collapsed = document.body.classList.toggle('sidebar-collapsed'); menuToggle.setAttribute('aria-expanded', String(!collapsed)); });
    }
    const refreshButton = document.getElementById('refresh-button');
    if (refreshButton !== null && refreshButton.dataset.initialized !== 'true') {
        const icon = document.getElementById('refresh-icon'); const status = document.getElementById('status-message');
        if (icon === null || status === null) { refreshButton.disabled = true; console.error('Library refresh is missing required controls.'); }
        else {
            refreshButton.dataset.initialized = 'true';
            refreshButton.addEventListener('click', async () => {
                if (refreshButton.disabled) return;
                refreshButton.disabled = true; refreshButton.setAttribute('aria-busy', 'true'); icon.classList.add('animate-spin');
                status.textContent = 'Refreshing your library…'; status.hidden = false;
                try { const response = await fetch('/refresh', { method: 'POST', credentials: 'same-origin' }); if (!response.ok) throw new Error('Refresh returned HTTP ' + response.status); window.location.reload(); }
                catch (error) { console.error('Library refresh failed:', error); status.textContent = 'Could not refresh your library. Please try again.'; }
                finally { refreshButton.disabled = false; refreshButton.removeAttribute('aria-busy'); icon.classList.remove('animate-spin'); }
            });
        }
    }
    initializeProfiles(document, window, { enhanceMedia, releaseRegion(root) { if (observer !== null) root.querySelectorAll('img').forEach(image => observer.unobserve(image)); } });
}

/** Owns the transient cookie-bound context. Reads may be cancelled; mutations are never replayed. */
function initializeProfiles(document, window, media) {
    const form = document.getElementById('profile-form');
    if (form === null || form.dataset.initialized === 'true') return;
    const menu = document.getElementById('profile-menu');
    const context = document.getElementById('profile-context');
    const label = document.getElementById('profile-label');
    const actionLabel = document.getElementById('profile-action-label');
    const input = document.getElementById('profile-name');
    const status = document.getElementById('profile-status');
    const retry = document.getElementById('profile-retry');
    const page = document.getElementById('profile-page');
    const required = [menu, context, label, actionLabel, input, status, retry];
    if (required.some(element => element === null)) {
        const message = 'Profiles could not initialize: a required menu element is missing. Reload this page to retry.';
        console.error(message);
        form.querySelectorAll('input, button').forEach(element => { element.disabled = true; });
        const editor = document.getElementById('watch-metadata');
        if (editor !== null) editor.querySelectorAll('input, select, button').forEach(element => { element.disabled = true; });
        if (status !== null) status.textContent = message;
        const personal = document.getElementById('personal-status');
        if (personal !== null) personal.textContent = message;
        return;
    }
    form.dataset.initialized = 'true';
    let profile = null; let state = 'loading'; let generation = 0; let readController = null;
    let metadata = null; let mutationPending = false; let reconcileRequired = false;
    function show(message) { status.textContent = message; const personal = document.getElementById('personal-status'); if (personal !== null) personal.textContent = message; }
    function controls() {
        form.querySelectorAll('input, button').forEach(element => { element.disabled = mutationPending; });
        const editor = document.getElementById('watch-metadata');
        if (editor !== null) editor.querySelectorAll('input, select, button').forEach(element => { element.disabled = state !== 'ready' || mutationPending || reconcileRequired; });
        retry.hidden = state !== 'error'; context.setAttribute('aria-busy', String(state === 'loading' || state === 'saving'));
    }
    function setProfile(next) { profile = next; context.dataset.profileSelected = String(next !== null); context.dataset.profile = next === null ? '' : next; label.textContent = next === null ? 'Choose profile' : next; actionLabel.textContent = next === null ? 'Choose a profile' : 'Switch profile'; input.value = next === null ? 'guest' : next; }
    function current(token) { return token === generation && !readController.signal.aborted; }
    function aggregateUnavailable(error) {
        document.querySelectorAll('.community-rating').forEach(element => { element.textContent = 'Rating unavailable'; });
        console.error('Community ratings could not load:', error); show('Community ratings are unavailable. Your saved personal edits are retained.');
    }
    async function loadAggregates(signal) {
        const result = await requestJson('/api/videos/ratings', { signal });
        return validateAggregates(result.value);
    }
    function renderAggregates(ratings) {
        const elements = [...document.querySelectorAll('.community-rating')];
        elements.forEach(element => {
            const id = element.dataset.videoId; if (typeof id !== 'string' || !ratings.has(id)) throw new Error('Community response is missing an indexed video.');
        });
        elements.forEach(element => {
            const id = element.dataset.videoId;
            const rating = ratings.get(id); element.textContent = rating.count === 0 ? 'Unrated' : (Math.round(rating.average * 10) / 10).toFixed(1) + '/5 · ' + rating.count + (rating.count === 1 ? ' rating' : ' ratings');
        });
    }
    function renderPersonal() {
        const editor = document.getElementById('watch-metadata'); if (editor === null || metadata === null) return;
        const id = editor.dataset.videoId;
        const rating = document.getElementById('personal-rating'); const tags = document.getElementById('personal-tags');
        if (typeof id !== 'string' || id === '' || rating === null || tags === null) throw new Error('The personal editor is missing required video controls.');
        rating.value = metadata.ratings.has(id) ? String(metadata.ratings.get(id)) : '';
        const elements = [];
        if (metadata.tags.has(id)) for (const tag of metadata.tags.get(id)) {
            const span = document.createElement('span'); span.className = 'personal-tag';
            const text = document.createElement('span'); text.textContent = tag;
            const remove = document.createElement('button'); remove.type = 'button'; remove.dataset.removeTag = tag; remove.setAttribute('aria-label', 'Remove tag ' + tag); remove.textContent = '×';
            span.append(text, remove); elements.push(span);
        }
        tags.replaceChildren(...elements);
    }
    /** Parse trusted same-origin server templates; verify context/regions before importing escaped DOM. */
    async function refreshRegions(expected, signal) {
        if (page === null) return;
        const response = await fetch(window.location.pathname + window.location.search, { credentials: 'same-origin', cache: 'no-store', signal });
        if (!response.ok || !/^text\/html(?:\s*;|$)/i.test(response.headers.get('content-type') || '')) throw new Error('Could not reload profile-aware browsing.');
        const parsed = new window.DOMParser().parseFromString(await response.text(), 'text/html');
        if (readRenderedProfile(parsed.getElementById('profile-context')) !== expected) throw new Error('Profile changed while browsing data was loading.');
        const sourcePage = parsed.getElementById('profile-page');
        if (sourcePage === null || sourcePage.dataset.metadataAvailable !== 'true') throw new Error('Personal metadata is unavailable in the refreshed page.');
        const ids = ['profile-filters', 'profile-search-filters', 'profile-metadata-warning', 'library-results', 'library-pagination'];
        if (document.getElementById('watch-metadata') !== null) ids.push('watch-metadata');
        const replacements = ids.map(id => {
            const old = document.getElementById(id); const next = parsed.getElementById(id);
            if (old === null || next === null) throw new Error('Profile browsing region is missing: ' + id);
            if (next.querySelector('script, video, track') !== null) throw new Error('Profile region unexpectedly contains active media or scripts.');
            if (id === 'watch-metadata' && old.dataset.videoId !== next.dataset.videoId) throw new Error('The watched video changed while its metadata was loading.');
            return { old, next };
        });
        if (signal.aborted) return;
        for (const { old, next } of replacements) { media.releaseRegion(old); old.replaceWith(document.importNode(next, true)); }
        page.dataset.metadataAvailable = 'true'; media.enhanceMedia(document);
    }
    /** Reconcile context and metadata before edits. Each read generation owns its resulting DOM update. */
    async function reconcile(refreshPage, contextOnly = false) {
        const previousState = state;
        const previousProfile = profile;
        const token = ++generation; if (readController !== null) readController.abort(); readController = new AbortController();
        const signal = readController.signal; state = 'loading'; controls();
        try {
            const selected = validateProfileResponse((await requestJson('/api/user/profile', { signal })).value);
            if (!current(token)) return false;
            // Focus checks identity first so unchanged profiles retain unsaved form values.
            if (contextOnly && selected === previousProfile && ['ready', 'no-profile'].includes(previousState)) {
                state = previousState;
                return true;
            }
            let loaded = null;
            if (selected !== null) {
                const result = await requestJson('/api/user/metadata', { signal });
                if (metadataProfile(result.response) !== selected) throw new Error('Profile changed while personal metadata was loading. Retry loading.');
                loaded = validateMetadata(result.value);
            }
            if (!current(token)) return false;
            if (refreshPage || selected !== profile || (page !== null && page.dataset.metadataAvailable !== 'true')) await refreshRegions(selected, signal);
            if (!current(token)) return false;
            setProfile(selected); metadata = loaded; renderPersonal();
            try {
                const ratings = await loadAggregates(signal);
                if (!current(token)) return false;
                renderAggregates(ratings);
            }
            catch (error) {
                if (!current(token)) return false;
                aggregateUnavailable(error);
                if (reconcileRequired) throw new Error('The previous save must be verified. Retry loading before editing.');
            }
            if (!current(token)) return false;
            reconcileRequired = false; state = selected === null ? 'no-profile' : 'ready';
            if (selected === null) { menu.open = true; show('Choose a profile to save ratings and tags.'); }
            if (contextOnly && selected !== previousProfile) show('The active profile changed. Review its metadata and retry your edit.');
            return true;
        } catch (error) {
            if (!current(token)) return false;
            state = 'error'; menu.open = true; show('Could not load your profile. ' + error.message); console.error('Profile state could not load:', error); return false;
        } finally { if (token === generation) controls(); }
    }
    async function saveEdit(kind, value) {
        if (mutationPending || state !== 'ready' || reconcileRequired) return;
        const expected = profile; const editor = document.getElementById('watch-metadata');
        if (editor === null || typeof editor.dataset.videoId !== 'string' || editor.dataset.videoId === '') { state = 'error'; show('The personal editor is missing its video identifier.'); controls(); return; }
        const id = editor.dataset.videoId;
        mutationPending = true; controls();
        let sent = false;
        try {
            if (!await reconcile(false)) return;
            if (profile !== expected) { await reconcile(true); show('The active profile changed. Review its metadata and retry your edit.'); return; }
            const payload = { video_id: id, expected_profile: expected };
            if (kind === 'rating') payload.rating = value; else payload.tag = value;
            state = 'saving'; controls(); sent = true;
            const result = await requestJson(kind === 'rating' ? '/api/user/rating' : '/api/user/tag', { method: kind === 'remove' ? 'DELETE' : 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
            if (metadataProfile(result.response) !== expected) throw new Error('The save response belongs to another profile.');
            metadata = validateMetadata(result.value); renderPersonal();
            await reconcile(true);
            if (state === 'ready') show('Saved to ' + expected + '.');
        } catch (error) {
            console.error('Personal metadata edit failed:', error);
            if (sent && error instanceof ApiFailure && [400, 404, 409].includes(error.status)) {
                await reconcile(true);
                show(error.message + (error.status === 409 ? ' Review the active profile and retry your edit.' : ' Review your edit and try again.'));
            } else if (sent) {
                reconcileRequired = true;
                const recovered = await reconcile(true);
                show(recovered ? 'The save did not return a confirmed response. Reloaded saved data; review it before retrying.' : 'The save outcome is uncertain. Retry loading before editing.');
            } else { show(error.message); }
        } finally { mutationPending = false; controls(); }
    }
    form.addEventListener('submit', async event => {
        event.preventDefault(); if (mutationPending) return;
        let next;
        try { next = normalizeProfile(input.value); } catch (error) { show(error.message); return; }
        mutationPending = true; state = 'saving'; ++generation; if (readController !== null) readController.abort(); controls();
        try {
            const result = await requestJson('/api/user/profile', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ profile: next }) });
            if (validateProfileResponse(result.value) !== next) throw new Error('The server selected an unexpected profile.');
            if (await reconcile(true)) { menu.open = false; show('Using profile ' + profile + '.'); }
        } catch (error) {
            console.error('Profile selection failed:', error); reconcileRequired = true;
            await reconcile(true); show('Profile selection could not be confirmed. Reloaded the current selection; review it before continuing.');
        } finally { mutationPending = false; controls(); }
    });
    if (page !== null) page.addEventListener('submit', async event => {
        if (event.target.id !== 'rating-form' && event.target.id !== 'tag-form') return;
        event.preventDefault();
        try {
            if (event.target.id === 'rating-form') {
                const rating = document.getElementById('personal-rating');
                if (rating === null || rating.value.length !== 1 || !/^[1-5]$/.test(rating.value)) throw new Error('Choose a personal rating from 1 to 5.');
                await saveEdit('rating', Number(rating.value));
            } else { const tag = document.getElementById('new-tag'); if (tag === null) throw new Error('The tag input is missing.'); await saveEdit('tag', normalizeTag(tag.value)); }
        } catch (error) { show(error.message); console.error('Personal edit could not start:', error); }
    });
    if (page !== null) page.addEventListener('click', async event => {
        const button = event.target.closest('[data-remove-tag]'); if (button === null || !page.contains(button)) return;
        try { await saveEdit('remove', normalizeTag(button.dataset.removeTag)); } catch (error) { show(error.message); console.error('Tag removal could not start:', error); }
    });
    retry.addEventListener('click', async () => { if (!mutationPending) await reconcile(true); });
    window.addEventListener('focus', async () => { if (!mutationPending) await reconcile(true, true); });
    window.addEventListener('pagehide', () => { ++generation; if (readController !== null) readController.abort(); });
    window.addEventListener('pageshow', async event => { if (event.persisted && !mutationPending) await reconcile(true); });
    try { profile = readRenderedProfile(context); }
    catch (error) { state = 'error'; show(error.message); console.error('Rendered profile context is invalid:', error); controls(); return; }
    controls();
    // Explicit boundary: reconcile handles/logs every failure and intentional cancellation.
    void reconcile(false);
}

initializePage(document, window);
