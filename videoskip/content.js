// VideoSeek content script.
//
// Two jobs: turn Shift + ←/→ into a jump of the configured step, and turn
// Shift + G into a timestamp box. No background worker, no network access, and
// no page mutation beyond the two Shadow DOM layers this file owns.
//
// The key handlers listen on `window` in the capture phase so they run before
// the player's own shortcuts and can stop them; they stand down while the user
// is typing, and while the jump box is open.

(() => {
  const DEFAULT_SETTINGS = { step: 30, enabled: true };
  const STEP_MIN = 1;
  const STEP_MAX = 600;
  const OPEN_KEY = 'g';
  const TOAST_MS = 1400;

  let settings = { ...DEFAULT_SETTINGS };
  /** The <video> the user last touched, so multi-player pages pick the right one. */
  let lastVideo = null;

  // --------------------------------------------------------------- settings

  function clampStep(value) {
    const number = Number(value);
    if (!Number.isFinite(number)) return DEFAULT_SETTINGS.step;
    return Math.min(STEP_MAX, Math.max(STEP_MIN, number));
  }

  function applySettings(stored) {
    if (stored && typeof stored.step === 'number') settings.step = clampStep(stored.step);
    if (stored && typeof stored.enabled === 'boolean') settings.enabled = stored.enabled;
  }

  // Reloading the extension invalidates the context and every chrome call then
  // throws; the page keeps working on the defaults instead of breaking.
  try {
    chrome.storage.sync.get(DEFAULT_SETTINGS, (stored) => {
      if (!chrome.runtime.lastError) applySettings(stored);
    });
    chrome.storage.onChanged.addListener((changes, area) => {
      if (area !== 'sync') return;
      const stored = {};
      for (const [key, change] of Object.entries(changes)) stored[key] = change.newValue;
      applySettings(stored);
    });
    chrome.runtime.onMessage.addListener((message, _sender, respond) => {
      if (!message || message.type !== 'open-jump') return;
      const video = activeVideo();
      if (video) {
        openJumpBox(video);
        respond({ ok: true });
      } else {
        respond({ ok: false, reason: 'no-video' });
      }
    });
  } catch {
    /* not running as an extension: the defaults are enough */
  }

  // ----------------------------------------------------------------- videos

  function visibleArea(video) {
    const rect = video.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) return 0;
    const style = window.getComputedStyle(video);
    if (style.visibility === 'hidden' || style.display === 'none' || style.opacity === '0') {
      return 0;
    }
    return rect.width * rect.height;
  }

  /** The player on screen: the last one the user interacted with, else the biggest. */
  function activeVideo() {
    if (lastVideo && lastVideo.isConnected && visibleArea(lastVideo) > 0) return lastVideo;

    let best = null;
    let bestArea = 0;
    for (const video of document.querySelectorAll('video')) {
      const area = visibleArea(video);
      if (area > bestArea) {
        best = video;
        bestArea = area;
      }
    }
    return best;
  }

  for (const type of ['play', 'pointerdown', 'focusin']) {
    document.addEventListener(
      type,
      (event) => {
        const target = event.target;
        if (!(target instanceof Element)) return;
        const video = target instanceof HTMLVideoElement ? target : target.closest('video');
        if (video) lastVideo = video;
      },
      true,
    );
  }

  // ------------------------------------------------------------------ seeks

  function clampTime(seconds, duration) {
    const upper =
      Number.isFinite(duration) && duration > 0
        ? Math.max(0, duration - 0.05)
        : Number.POSITIVE_INFINITY;
    return Math.max(0, Math.min(seconds, upper));
  }

  function jumpTo(video, seconds) {
    const target = clampTime(Number(seconds) || 0, video.duration);
    video.currentTime = target;
    lastVideo = video;
    showToast(formatTime(target));
    return target;
  }

  function jumpBy(video, delta) {
    const from = Number(video.currentTime) || 0;
    const target = clampTime(from + delta, video.duration);
    video.currentTime = target;
    lastVideo = video;
    showToast(`${delta < 0 ? '−' : '+'}${formatStep(delta)} → ${formatTime(target)}`);
    return target;
  }

  // ------------------------------------------------------------------ layers

  const SHARED_STYLE = `
    :host { all: initial; }
    * { box-sizing: border-box; }
    p { margin: 0; }
  `;

  function mountLayer(token, css, markup) {
    const host = document.createElement('div');
    host.setAttribute('data-videoseek', token);
    const shadow = host.attachShadow({ mode: 'closed' });
    shadow.innerHTML = `<style>${SHARED_STYLE}${css}</style>${markup}`;
    (document.body || document.documentElement).append(host);
    return { host, shadow };
  }

  const TOAST_STYLE = `
    .toast {
      position: fixed; left: 50%; bottom: 12vh; z-index: 2147483647;
      transform: translate(-50%, 8px); opacity: 0;
      padding: 8px 14px; border-radius: 999px; white-space: nowrap;
      background: rgba(20, 22, 26, .92); color: #e6e8eb; border: 1px solid #2b303a;
      font: 13px/1.4 system-ui, 'Segoe UI', 'Microsoft YaHei UI', sans-serif;
      transition: opacity .12s ease, transform .12s ease; pointer-events: none;
    }
    .toast.show { opacity: 1; transform: translate(-50%, 0); }
  `;

  let toast = null;
  let toastTimer = 0;

  function showToast(text) {
    if (!toast) {
      const layer = mountLayer('toast', TOAST_STYLE, '<div class="toast"></div>');
      toast = { host: layer.host, text: layer.shadow.querySelector('.toast') };
    }
    toast.text.textContent = text;
    toast.text.classList.add('show');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toast.text.classList.remove('show'), TOAST_MS);
  }

  const DIALOG_STYLE = `
    .backdrop {
      position: fixed; inset: 0; z-index: 2147483647;
      display: flex; align-items: flex-start; justify-content: center;
      padding-top: 16vh; background: rgba(8, 10, 14, .55);
      font: 13px/1.5 system-ui, 'Segoe UI', 'Microsoft YaHei UI', sans-serif;
    }
    .box {
      width: 340px; padding: 14px 16px; border-radius: 10px;
      background: #14161a; color: #e6e8eb; border: 1px solid #2b303a;
      box-shadow: 0 18px 48px rgba(0, 0, 0, .55);
    }
    .title { font-size: 12px; color: #8b93a1; }
    input {
      width: 100%; margin-top: 8px; padding: 8px 10px; border-radius: 7px;
      border: 1px solid #2b303a; background: #1b1e24; color: #e6e8eb;
      font: 16px/1.2 ui-monospace, Consolas, monospace; outline: none;
    }
    input:focus { border-color: #58a6ff; }
    .hint {
      display: flex; justify-content: space-between; gap: 8px;
      margin-top: 8px; font-size: 11px; color: #8b93a1;
    }
    .now { white-space: nowrap; }
    .status.error { color: #ff8a7a; }
  `;

  const DIALOG_MARKUP = `
    <div class="backdrop">
      <div class="box">
        <p class="title">跳到时间点：83 / 1:23 / 1:02:03 / +30 / -15</p>
        <input type="text" spellcheck="false" autocomplete="off" placeholder="1:30">
        <div class="hint"><span class="now"></span><span class="status">Enter 跳转 · Esc 取消</span></div>
      </div>
    </div>
  `;

  let dialog = null;

  function closeJumpBox() {
    if (!dialog) return;
    dialog.host.remove();
    dialog = null;
  }

  function openJumpBox(video) {
    closeJumpBox();
    const layer = mountLayer('jump', DIALOG_STYLE, DIALOG_MARKUP);
    const backdrop = layer.shadow.querySelector('.backdrop');
    const input = layer.shadow.querySelector('input');
    const now = layer.shadow.querySelector('.now');
    const status = layer.shadow.querySelector('.status');
    dialog = { host: layer.host };

    now.textContent = `当前位置 ${formatTime(video.currentTime)} · 总长 ${formatTime(video.duration)}`;

    function setStatus(text, isError) {
      status.textContent = text;
      status.classList.toggle('error', Boolean(isError));
    }

    function submit() {
      const parsed = parseTimestamp(input.value);
      if (!parsed) {
        setStatus('看不懂这个时间', true);
        input.select();
        return;
      }
      const target = parsed.relative ? video.currentTime + parsed.seconds : parsed.seconds;
      closeJumpBox();
      jumpTo(video, target);
    }

    input.addEventListener('input', () => setStatus('Enter 跳转 · Esc 取消', false));
    input.addEventListener('keydown', (event) => {
      // Keep typing away from the player's shortcuts, and from this script's own
      // window-level handler.
      event.stopPropagation();
      if (event.key === 'Escape') {
        event.preventDefault();
        closeJumpBox();
      } else if (event.key === 'Enter') {
        event.preventDefault();
        submit();
      }
    });
    input.addEventListener('keyup', (event) => event.stopPropagation());
    backdrop.addEventListener('pointerdown', (event) => {
      if (event.target === backdrop) closeJumpBox();
    });

    input.focus();
    input.select();
  }

  // ------------------------------------------------------------------- keys

  function isTypingTarget(target) {
    if (!(target instanceof Element)) return false;
    if (target.isContentEditable) return true;
    const tag = target.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
  }

  window.addEventListener(
    'keydown',
    (event) => {
      if (!settings.enabled || dialog) return;
      if (event.defaultPrevented || event.isComposing) return;
      if (event.altKey || event.ctrlKey || event.metaKey) return;
      if (!event.shiftKey || isTypingTarget(event.target)) return;

      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        const video = activeVideo();
        if (!video) return;
        event.preventDefault();
        event.stopPropagation();
        jumpBy(video, event.key === 'ArrowRight' ? settings.step : -settings.step);
        return;
      }

      if (event.key.toLowerCase() === OPEN_KEY) {
        const video = activeVideo();
        if (!video) return;
        event.preventDefault();
        event.stopPropagation();
        openJumpBox(video);
      }
    },
    true,
  );
})();
