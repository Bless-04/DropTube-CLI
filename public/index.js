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

// Only request images close to the viewport. Feed cards never request video bytes.
const thumbnails = document.querySelectorAll('img[data-src]');
function loadThumbnail(image) {
    image.addEventListener('error', () => {
        image.removeAttribute('src');
        image.hidden = true;
    }, { once: true });
    image.src = image.dataset.src;
    delete image.dataset.src;
}
if ('IntersectionObserver' in window) {
    const observer = new IntersectionObserver(entries => {
        for (const entry of entries) {
            if (entry.isIntersecting) {
                loadThumbnail(entry.target);
                observer.unobserve(entry.target);
            }
        }
    }, { rootMargin: '200px 0px' });
    thumbnails.forEach(image => observer.observe(image));
} else {
    // Native lazy loading is also present for older browsers and the no-JS fallback.
    thumbnails.forEach(loadThumbnail);
}

// Search is a regular GET form: Enter, the search button, and no-JS browsing all work.
const searchForm = document.querySelector('.search-form');
searchForm?.addEventListener('submit', () => {
    const search = searchForm.querySelector('[name="search"]');
    search.value = search.value.trim();
});

const menuToggle = document.getElementById('menu-toggle');
menuToggle?.addEventListener('click', () => {
    const collapsed = document.body.classList.toggle('sidebar-collapsed');
    menuToggle.setAttribute('aria-expanded', String(!collapsed));
});

const refreshButton = document.getElementById('refresh-button');
refreshButton?.addEventListener('click', async () => {
    const icon = document.getElementById('refresh-icon');
    const status = document.getElementById('status-message');
    refreshButton.disabled = true;
    refreshButton.setAttribute('aria-busy', 'true');
    icon.classList.add('animate-spin');
    status.textContent = 'Refreshing your library…';
    status.hidden = false;
    try {
        const response = await fetch('/refresh', { method: 'POST' });
        if (!response.ok) throw new Error('Refresh failed');
        // The endpoint responds only after the new index is ready.
        window.location.reload();
    } catch {
        status.textContent = 'Could not refresh your library. Please try again.';
        refreshButton.disabled = false;
        refreshButton.removeAttribute('aria-busy');
        icon.classList.remove('animate-spin');
    }
});
