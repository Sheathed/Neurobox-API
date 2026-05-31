// ── Pixel star particles ────────────────────────────────
(function () {
    const container = document.getElementById('stars');
    if (!container) return;

    const COUNT = 40;
    const PIXEL = 4;

    for (let i = 0; i < COUNT; i++) {
        const el = document.createElement('div');
        el.classList.add('star');

        // Random size (1–3 pixels)
        const size = (1 + Math.floor(Math.random() * 3)) * PIXEL;
        const left = Math.random() * 100;
        const top  = Math.random() * 100;
        const dur  = 1.5 + Math.random() * 3;
        const delay = Math.random() * dur;

        // Purple–pink hue range
        const hue = 270 + Math.floor(Math.random() * 60);

        el.style.left   = left + '%';
        el.style.top    = top + '%';
        el.style.width  = size + 'px';
        el.style.height = size + 'px';
        el.style.background = `hsl(${hue}, 80%, 65%)`;
        el.style.animationDuration = dur + 's';
        el.style.animationDelay = delay + 's';

        container.appendChild(el);
    }
})();
