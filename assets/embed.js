/* calrs embed runtime — pasted into a host site and exposes window.Calrs.
 *
 * API:
 *   Calrs.inline({ selector, link, config })
 *   Calrs.floatingButton({ link, buttonText, buttonPosition, buttonColor, textColor, showIcon, config })
 *   Calrs.elementClick()
 *
 * Where `link` is a full URL to a calrs booking page (e.g.
 * "https://cal.example.com/u/alice/intro") and `config` is an object with
 * optional `layout` ("month" | "week" | "column"), `theme` ("auto" | "light"
 * | "dark"), and `brand` (hex color, with or without leading "#"). These end
 * up as query parameters on the iframe src and feed into the embed-mode
 * rendering on the calrs side.
 */
(function () {
  if (window.Calrs) return;

  function buildSrc(link, config) {
    if (!link) return 'about:blank';
    var url = link + (link.indexOf('?') >= 0 ? '&' : '?') + 'embed=1';
    if (config) {
      if (config.layout) url += '&layout=' + encodeURIComponent(config.layout);
      if (config.theme) url += '&theme=' + encodeURIComponent(config.theme);
      if (config.brand) {
        var b = String(config.brand).replace(/^#/, '');
        url += '&brand=' + encodeURIComponent(b);
      }
    }
    return url;
  }

  function originOf(url) {
    try { return new URL(url, location.href).origin; } catch (e) { return null; }
  }

  // Auto-size: parent listens for {type:'calrs:resize', height} from the
  // booking page inside the iframe. The booking page emits this on load and
  // whenever its content height changes. We accept messages only from the
  // iframe's own origin and contentWindow to avoid trusting unrelated frames.
  function attachResize(iframe) {
    var iframeOrigin = originOf(iframe.src);
    window.addEventListener('message', function (ev) {
      if (!ev.data || ev.data.type !== 'calrs:resize') return;
      if (iframeOrigin && ev.origin !== iframeOrigin) return;
      if (ev.source !== iframe.contentWindow) return;
      var h = Math.max(60, Math.floor(ev.data.height || 0)) + 8;
      iframe.style.height = h + 'px';
    });
  }

  // Single shared modal — reused by floatingButton and elementClick.
  var modal = null;
  function ensureModal() {
    if (modal) return modal;
    var overlay = document.createElement('div');
    overlay.style.cssText =
      'position:fixed;inset:0;z-index:2147483647;background:rgba(0,0,0,0.55);' +
      'display:none;align-items:center;justify-content:center;padding:1rem;';
    var box = document.createElement('div');
    box.style.cssText =
      'background:#fff;border-radius:12px;width:min(960px,100%);height:min(90vh,720px);' +
      'overflow:hidden;position:relative;display:flex;flex-direction:column;' +
      'box-shadow:0 20px 60px rgba(0,0,0,0.4);';
    var close = document.createElement('button');
    close.type = 'button';
    close.setAttribute('aria-label', 'Close');
    close.innerHTML = '&times;';
    close.style.cssText =
      'position:absolute;top:0.5rem;right:0.5rem;width:34px;height:34px;border-radius:50%;' +
      'border:0;background:rgba(0,0,0,0.06);color:#000;font-size:22px;line-height:1;' +
      'cursor:pointer;z-index:2;';
    close.addEventListener('click', closeModal);
    var iframe = document.createElement('iframe');
    iframe.style.cssText = 'flex:1;width:100%;border:0;display:block;';
    iframe.setAttribute('allow', 'clipboard-write');
    iframe.setAttribute('title', 'Booking');
    box.appendChild(close);
    box.appendChild(iframe);
    overlay.appendChild(box);
    overlay.addEventListener('click', function (ev) {
      if (ev.target === overlay) closeModal();
    });
    document.addEventListener('keydown', function (ev) {
      if (ev.key === 'Escape' && modal && modal.overlay.style.display !== 'none') closeModal();
    });
    document.body.appendChild(overlay);
    modal = { overlay: overlay, iframe: iframe };
    return modal;
  }
  function openModal(link, config) {
    var m = ensureModal();
    m.iframe.src = buildSrc(link, config);
    m.overlay.style.display = 'flex';
    document.body.style.overflow = 'hidden';
  }
  function closeModal() {
    if (!modal) return;
    modal.overlay.style.display = 'none';
    modal.iframe.src = 'about:blank';
    document.body.style.overflow = '';
  }

  function calendarIcon() {
    var svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', '18');
    svg.setAttribute('height', '18');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '2');
    svg.setAttribute('stroke-linecap', 'round');
    svg.setAttribute('stroke-linejoin', 'round');
    svg.innerHTML =
      '<rect x="3" y="4" width="18" height="18" rx="2" ry="2"></rect>' +
      '<line x1="16" y1="2" x2="16" y2="6"></line>' +
      '<line x1="8" y1="2" x2="8" y2="6"></line>' +
      '<line x1="3" y1="10" x2="21" y2="10"></line>';
    return svg;
  }

  var Calrs = {
    inline: function (opts) {
      opts = opts || {};
      var target = typeof opts.selector === 'string' ? document.querySelector(opts.selector) : opts.selector;
      if (!target) return;
      var iframe = document.createElement('iframe');
      iframe.src = buildSrc(opts.link, opts.config);
      iframe.style.cssText = 'width:100%;min-height:560px;border:0;display:block;';
      iframe.setAttribute('loading', 'lazy');
      iframe.setAttribute('title', 'Booking');
      target.innerHTML = '';
      target.appendChild(iframe);
      attachResize(iframe);
    },

    floatingButton: function (opts) {
      opts = opts || {};
      var btn = document.createElement('button');
      btn.type = 'button';
      var pos = opts.buttonPosition || 'bottom-right';
      var bg = opts.buttonColor || '#2563eb';
      var fg = opts.textColor || '#ffffff';
      var positions = {
        'bottom-right': 'bottom:1rem;right:1rem;',
        'bottom-left':  'bottom:1rem;left:1rem;',
        'top-right':    'top:1rem;right:1rem;',
        'top-left':     'top:1rem;left:1rem;'
      };
      btn.style.cssText =
        'position:fixed;' + (positions[pos] || positions['bottom-right']) +
        'z-index:2147483646;padding:0.75rem 1.25rem;border:0;border-radius:999px;' +
        'font-family:inherit;font-size:0.95rem;font-weight:600;cursor:pointer;' +
        'display:inline-flex;align-items:center;gap:0.5rem;' +
        'box-shadow:0 4px 14px rgba(0,0,0,0.15);' +
        'background:' + bg + ';color:' + fg + ';';
      if (opts.showIcon !== false) btn.appendChild(calendarIcon());
      var label = document.createElement('span');
      label.textContent = opts.buttonText || 'Book a meeting';
      btn.appendChild(label);
      btn.addEventListener('click', function () { openModal(opts.link, opts.config); });
      document.body.appendChild(btn);
    },

    elementClick: function () {
      document.addEventListener('click', function (ev) {
        var el = ev.target.closest('[data-calrs-link]');
        if (!el) return;
        ev.preventDefault();
        var raw = el.getAttribute('data-calrs-config');
        var cfg = {};
        if (raw) { try { cfg = JSON.parse(raw); } catch (e) {} }
        openModal(el.getAttribute('data-calrs-link'), cfg);
      });
    }
  };

  window.Calrs = Calrs;

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () { Calrs.elementClick(); });
  } else {
    Calrs.elementClick();
  }
})();
