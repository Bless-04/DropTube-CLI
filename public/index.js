console.log('index.js loaded');

// Dynamic relative time calculator
function getRelativeTime(timestamp) {
    const now = Math.floor(Date.now() / 1000);
    const diff = now - timestamp;
    if (diff < 0) return 'Just now';
    if (diff < 60) return 'Just now';
    const mins = Math.floor(diff / 60);
    if (mins < 60) return mins + 'm ago';
    const hours = Math.floor(mins / 60);
    if (hours < 24) return hours + 'h ago';
    const days = Math.floor(hours / 24);
    if (days < 30) return days + 'd ago';
    const months = Math.floor(days / 30);
    return months + 'mo ago';
}

// Update all relative time elements on page load
document.querySelectorAll('.time-elapsed').forEach(el => {
    const timestamp = parseInt(el.getAttribute('data-timestamp'));
    if (!isNaN(timestamp)) {
        el.textContent = getRelativeTime(timestamp);
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