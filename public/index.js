console.log('index.js loaded');
'use strict';

function getRelativeTime(timestamp) {
    const difference = Math.max(0, Math.floor(Date.now() / 1000) - timestamp);
    if (difference < 60) return 'Just now';
    const units = [[31536000, 'year'], [2592000, 'month'], [86400, 'day'], [3600, 'hour'], [60, 'minute']];
    for (const [seconds, unit] of units) {
        const count = Math.floor(difference / seconds);
        if (count > 0) return `${count} ${unit}${count === 1 ? '' : 's'} ago`;
    }
    return 'Just now';
}

document.querySelectorAll('.time-elapsed').forEach(element => {
    const timestamp = Number(element.dataset.timestamp);
    if (Number.isFinite(timestamp) && timestamp > 0) {
        element.textContent = getRelativeTime(timestamp);
    }
});

// --- Server-side search (debounced URL navigation) ---
let searchTimeout = null;

function handleSearch(value) {
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
        const params = new URLSearchParams(window.location.search);
        if (value.trim()) {
            params.set('search', value.trim());
        } else {
            params.delete('search');
        }
        params.set('page', '1');
        params.delete('v'); // exit player view on search
        window.location.href = '/?' + params.toString();
    }, 400);
}

// --- Server-side tag filter (URL navigation) ---
function filterByTag(tag) {
    const params = new URLSearchParams(window.location.search);
    if (tag === 'all' || !tag) {
        params.delete('tag');
    } else {
        params.set('tag', tag);
    }
    params.set('page', '1');
    params.delete('v'); // exit player view on tag filter
    window.location.href = '/?' + params.toString();
}

// --- Refresh index action ---
function refreshIndex(btn) {
    const icon = document.getElementById('refresh-icon');
    const text = document.getElementById('refresh-text');

    icon.classList.add('animate-spin');
    if (text) text.textContent = 'Syncing...';
    btn.disabled = true;

    fetch('/refresh', { method: 'POST' })
        .then(response => {
            if (response.ok) {
                setTimeout(() => {
                    window.location.reload();
                }, 800);
            } else {
                alert('Failed to refresh index.');
                icon.classList.remove('animate-spin');
                if (text) text.textContent = 'Refresh Index';
                btn.disabled = false;
            }
        })
        .catch(err => {
            console.error(err);
            alert('Network error while refreshing index.');
            icon.classList.remove('animate-spin');
            if (text) text.textContent = 'Refresh Index';
            btn.disabled = false;
        });
}