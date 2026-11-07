// Settings live in chrome.storage.sync, so the step follows the user between
// browsers; content.js picks changes up through storage.onChanged without a
// reload.

const DEFAULT_SETTINGS = { step: 30, enabled: true };
const STEP_MIN = 1;
const STEP_MAX = 600;

const stepInput = document.getElementById('step');
const enabledInput = document.getElementById('enabled');
const jumpButton = document.getElementById('jump');
const statusLine = document.getElementById('status');

function clampStep(value) {
  const number = Math.round(Number(value));
  if (!Number.isFinite(number)) return DEFAULT_SETTINGS.step;
  return Math.min(STEP_MAX, Math.max(STEP_MIN, number));
}

function setStatus(text) {
  statusLine.textContent = text;
}

function save() {
  const step = clampStep(stepInput.value);
  stepInput.value = String(step);
  chrome.storage.sync.set({ step, enabled: enabledInput.checked }, () => {
    setStatus(chrome.runtime.lastError ? '保存失败' : '已保存');
  });
}

chrome.storage.sync.get(DEFAULT_SETTINGS, (stored) => {
  stepInput.value = String(clampStep(stored.step));
  enabledInput.checked = stored.enabled !== false;
});

stepInput.addEventListener('change', save);
enabledInput.addEventListener('change', save);

jumpButton.addEventListener('click', () => {
  chrome.tabs.query({ active: true, currentWindow: true }, ([tab]) => {
    if (!tab || !tab.id) {
      setStatus('找不到当前标签页');
      return;
    }
    chrome.tabs.sendMessage(tab.id, { type: 'open-jump' }, (response) => {
      if (chrome.runtime.lastError) {
        setStatus('这个页面用不了（扩展脚本没有注入）');
        return;
      }
      if (!response || !response.ok) {
        setStatus('当前页面没有找到视频');
        return;
      }
      window.close();
    });
  });
});
