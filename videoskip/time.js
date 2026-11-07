// Timestamp helpers for VideoSeek.
//
// Loaded before content.js in every frame, and evaluated directly by
// tests/time-check.mjs, so this file stays free of chrome/DOM/module APIs:
// plain function declarations on the global scope.

/**
 * Parses a timestamp typed by the user.
 *
 * Accepts `83`, `1:23`, `1:23.5`, `1:02:03`, and the relative forms `+30` and
 * `-15` (seconds from the current position). Full-width characters produced by
 * a Chinese IME are normalised first.
 *
 * @param {string} text
 * @returns {{seconds: number, relative: boolean} | null} `null` when the text is
 *   not a timestamp, which is how the caller tells "invalid" from "0".
 */
function parseTimestamp(text) {
  if (typeof text !== 'string') return null;

  let body = text.trim();
  if (!body) return null;
  body = body.replace(/：/g, ':').replace(/[−－–—]/g, '-');

  let relative = false;
  let negative = false;
  if (body[0] === '+' || body[0] === '-') {
    relative = true;
    negative = body[0] === '-';
    body = body.slice(1);
  }

  const parts = body.split(':');
  if (parts.length === 0 || parts.length > 3) return null;

  let seconds = 0;
  for (const part of parts) {
    const piece = part.trim();
    if (!/^\d+(\.\d+)?$/.test(piece)) return null;
    seconds = seconds * 60 + Number(piece);
  }
  if (!Number.isFinite(seconds) || seconds > 24 * 3600) return null;

  return { seconds: negative ? -seconds : seconds, relative };
}

/**
 * Formats a position for the toast and the dialog, the way players do it:
 * `1:23` below an hour, `1:02:03` above.
 */
function formatTime(seconds) {
  const total = Math.max(0, Math.floor((Number(seconds) || 0) + 1e-6));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = total % 60;
  const pad = (value) => String(value).padStart(2, '0');
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(rest)}` : `${minutes}:${pad(rest)}`;
}

/** Formats a jump distance, keeping fractional steps readable. */
function formatStep(seconds) {
  const value = Math.abs(Number(seconds) || 0);
  return `${Number.isInteger(value) ? value : value.toFixed(1)}s`;
}
