console.log('index.js loaded');
alert("Js Loaded.");
// Active tag filter tracking
let activeTag = 'all';

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

document.querySelectorAll('.time-elapsed').forEach(el => {
    const timestamp = parseInt(el.getAttribute('data-timestamp'));
    if (!isNaN(timestamp)) {
        el.textContent = getRelativeTime(timestamp);
    }
});

// Tag filter action
function filterByTag(tag, element) {
    activeTag = tag;

    // Reset and update tag button states
    document.querySelectorAll('.tag-btn').forEach(btn => {
        btn.classList.remove('bg-red-600', 'text-white', 'font-semibold');
        btn.classList.add('bg-zinc-800', 'text-zinc-300', 'font-normal');
    });

    if (element) {
        element.classList.remove('bg-zinc-800', 'text-zinc-300', 'font-normal');
        element.classList.add('bg-red-600', 'text-white', 'font-semibold');
    }

    filterVideos();
}

// Search & Tag composite filter
function filterVideos() {
    const query = document.getElementById('search').value.toLowerCase().trim();
    const cards = document.querySelectorAll('.video-card');

    cards.forEach(card => {
        const title = card.getAttribute('data-title').toLowerCase();
        const tagsRaw = card.getAttribute('data-tags') || '';
        const tags = tagsRaw.toLowerCase().split(',').map(t => t.trim());

        const matchesQuery = title.includes(query);
        const matchesTag = activeTag === 'all' || tags.includes(activeTag.toLowerCase());

        if (matchesQuery && matchesTag) {
            card.style.display = '';
        } else {
            card.style.display = 'none';
        }
    });
}

// Trigger manual background index scan
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