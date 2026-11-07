// Checks the timestamp parser without a browser. time.js is a plain script, so
// it loads into a bare VM context and the assertions run against its globals.
//
//   node tests/time-check.mjs

import { readFileSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const context = vm.createContext({});
vm.runInContext(readFileSync(path.join(here, '..', 'time.js'), 'utf8'), context, {
  filename: 'time.js',
});

const parse = context.parseTimestamp;
const format = context.formatTime;

let failures = 0;

function check(label, actual, expected) {
  if (JSON.stringify(actual) === JSON.stringify(expected)) return;
  failures += 1;
  console.error(`FAIL ${label}: got ${JSON.stringify(actual)}, want ${JSON.stringify(expected)}`);
}

// Absolute positions.
check('seconds', parse('83'), { seconds: 83, relative: false });
check('mm:ss', parse('1:23'), { seconds: 83, relative: false });
check('hh:mm:ss', parse('1:02:03'), { seconds: 3723, relative: false });
check('fractional seconds', parse('1:23.5'), { seconds: 83.5, relative: false });
check('surrounding spaces', parse('  2:00  '), { seconds: 120, relative: false });
check('full-width colon', parse('1：23'), { seconds: 83, relative: false });
check('zero', parse('0'), { seconds: 0, relative: false });

// Relative jumps.
check('forward', parse('+30'), { seconds: 30, relative: true });
check('backward', parse('-15'), { seconds: -15, relative: true });
check('minus sign from an IME', parse('−15'), { seconds: -15, relative: true });
check('relative with a colon', parse('+1:30'), { seconds: 90, relative: true });

// Everything else is invalid, which the caller shows as an error.
for (const bad of ['', '   ', 'abc', '1:2:3:4', '1:xx', ':', '1:23s', '-', 0, null, undefined]) {
  check(`rejects ${JSON.stringify(bad)}`, parse(bad), null);
}

// Formatting.
check('below an hour', format(83), '1:23');
check('above an hour', format(3723), '1:02:03');
check('zero', format(0), '0:00');
check('negative clamps to zero', format(-5), '0:00');
check('fractions truncate', format(59.9), '0:59');

if (failures > 0) {
  console.error(`${failures} check(s) failed`);
  process.exit(1);
}
console.log('time.js: all checks passed');
