// Booking questions (#210): the event type editor and the booking form in a
// real browser and an isolated server.
// Requires Node >= 22 (built-in WebSocket) and Chrome/Chromium. No npm packages.
// Run from the repository root: node scripts/test-questions-browser.mjs
// CALRS_BINARY and CHROME_BINARY can override the executable paths.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
import { createServer } from 'node:net'

const dir = await mkdtemp(join(tmpdir(), 'calrs-questions-'))
const binary = resolve(process.env.CALRS_BINARY || 'target/debug/calrs')
const reserve = createServer()
await new Promise(r => reserve.listen(0, '127.0.0.1', r))
const port = reserve.address().port
await new Promise(r => reserve.close(r))
const base = `http://localhost:${port}`
const password = 'questions-browser-password'
const server = spawn(binary, ['--data-dir', join(dir, 'data'), 'serve', '--port', String(port)])
let serverLog = ''
server.stdout.on('data', d => { serverLog += d })
server.stderr.on('data', d => { serverLog += d })
let chrome, ws
let browserLog = ''
const pause = ms => new Promise(r => setTimeout(r, ms))
async function until(fn, description, timeout = 15000) {
  const deadline = Date.now() + timeout
  while (Date.now() < deadline) {
    const result = await fn()
    if (result) return result
    await pause(100)
  }
  throw new Error(`Timed out: ${description}`)
}

try {
  await until(async () => {
    if (server.exitCode !== null) throw new Error(`Server stopped: ${serverLog}`)
    try { return (await fetch(`${base}/auth/login`)).ok } catch { return false }
  }, 'server startup')
  chrome = spawn(process.env.CHROME_BINARY || 'google-chrome', [
    '--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
    '--remote-debugging-port=0', `--user-data-dir=${join(dir, 'chrome')}`, 'about:blank',
  ])
  chrome.stderr.on('data', d => { browserLog += d })
  const debugPort = await until(async () => {
    if (chrome.exitCode !== null) throw new Error(`Chrome stopped: ${browserLog}`)
    try { return (await readFile(join(dir, 'chrome', 'DevToolsActivePort'), 'utf8')).split('\n')[0] } catch { return false }
  }, 'Chrome startup')
  const pages = await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json()
  ws = new WebSocket(pages.find(p => p.type === 'page').webSocketDebuggerUrl)
  await new Promise((r, reject) => { ws.onopen = r; ws.onerror = reject })
  let nextId = 0
  const pending = new Map()
  const loads = new Set()
  ws.onmessage = event => {
    const message = JSON.parse(event.data)
    if (message.id) {
      const p = pending.get(message.id)
      if (!p) return
      pending.delete(message.id)
      message.error ? p.reject(new Error(JSON.stringify(message.error))) : p.resolve(message.result)
    }
    if (message.method === 'Page.loadEventFired') {
      for (const r of loads) r()
      loads.clear()
    }
  }
  const call = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++nextId
    pending.set(id, { resolve, reject })
    ws.send(JSON.stringify({ id, method, params }))
  })
  await call('Page.enable')
  await call('Runtime.enable')
  await call('Network.enable')
  await call('Network.setExtraHTTPHeaders', { headers: { 'Accept-Language': 'en' } })
  const evaluate = async expression => {
    const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails))
    return result.result.value
  }
  const navigation = async action => {
    let timer, done
    const loaded = new Promise((r, reject) => {
      done = r
      loads.add(r)
      timer = setTimeout(() => { loads.delete(r); reject(new Error('Navigation timed out')) }, 15000)
    })
    try { await action(); await loaded } finally { clearTimeout(timer); loads.delete(done) }
  }
  const go = path => navigation(() => call('Page.navigate', { url: base + path }))
  const text = () => evaluate('document.body.innerText')
  const path = () => evaluate('location.pathname')
  const submit = (selector, fields, button) => navigation(() => evaluate(`(() => {
    const form = document.querySelector(${JSON.stringify(selector)});
    if (!form) throw new Error('Missing form: ' + ${JSON.stringify(selector)});
    for (const [name, value] of Object.entries(${JSON.stringify(fields)})) {
      const input = form.elements.namedItem(name);
      if (!input) throw new Error('Missing field: ' + name);
      input.value = value;
    }
    const button = ${JSON.stringify(button || null)};
    if (!form.reportValidity()) throw new Error('Invalid form');
    form.requestSubmit(button ? form.querySelector(button) : undefined);
  })()`))
  const screenshot = async name => {
    const { data } = await call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true })
    await writeFile(join(dir, `${name}.png`), Buffer.from(data, 'base64'))
  }
  const jsErrors = []
  const originalMessage = ws.onmessage
  ws.onmessage = event => {
    const message = JSON.parse(event.data)
    if (message.method === 'Runtime.exceptionThrown') jsErrors.push(message.params.exceptionDetails)
    originalMessage(event)
  }
  const click = selector => evaluate(`(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) throw new Error('Missing: ' + ${JSON.stringify(selector)});
    el.click();
  })()`)
  const type = (selector, value) => evaluate(`(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) throw new Error('Missing: ' + ${JSON.stringify(selector)});
    el.value = ${JSON.stringify(value)};
    el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', {bubbles:true}));
  })()`)
  const saved = () => evaluate('JSON.parse(document.querySelector("#questions").value)')
  const labelsShown = () => evaluate(`Array.from(document.querySelectorAll('#q-list .q-label')).map(e => e.textContent)`)

  await go('/auth/register')
  await submit('form', { name: 'Questions Admin', email: 'questions@example.test', password })
  await go('/dashboard/settings')
  await submit('form:not([action])', { name: 'Questions Admin', username: 'q-admin', timezone: 'UTC' })

  await go('/dashboard/event-types/new')
  await evaluate(`document.querySelector('#booking-form-section').open = true`)
  assert.deepEqual(await saved(), [])
  // Three questions: a required short text, a choice, a consent.
  await click('#q-add')
  assert.equal(await evaluate('document.activeElement.id'), 'q-ed-0-kind', 'focus moves to the new question')
  await type('#q-ed-0-label', 'Licence plate')
  await click('#q-ed-0-required')
  assert.equal(await evaluate('document.activeElement.id'), 'q-ed-0-required', 'focus stays on the toggled box')
  await click('#q-add')
  await type('#q-ed-1-kind', 'single_choice')
  await type('#q-ed-1-label', 'Where are the tyres?')
  await evaluate(`(() => {
    const inputs = document.querySelectorAll('#q-list .q-opt input');
    inputs[0].value = 'At the shop'; inputs[0].dispatchEvent(new Event('input', {bubbles:true}));
    inputs[1].value = 'I bring them'; inputs[1].dispatchEvent(new Event('input', {bubbles:true}));
  })()`)
  await click('#q-add')
  await type('#q-ed-2-kind', 'checkbox')
  await type('#q-ed-2-label', 'I agree to the privacy policy')
  await click('#q-ed-2-required')
  await click('#q-ed-2-consent')
  await type('#q-ed-2-link', 'https://example.test/privacy')
  await screenshot('questions-editor')
  // Keyboard-style reorder: the buttons, not drag and drop. Focus follows.
  await evaluate(`document.querySelectorAll('#q-list .q-row')[2].querySelector('.q-up').click()`)
  assert.deepEqual(await labelsShown(), ['Licence plate', 'I agree to the privacy policy', 'Where are the tyres?'])
  assert(await evaluate(`document.activeElement.classList.contains('q-up')`), 'focus stays on Move up')
  await evaluate(`document.querySelectorAll('#q-list .q-row')[0].querySelector('.q-down').click()`)
  assert.deepEqual(await labelsShown(), ['I agree to the privacy policy', 'Licence plate', 'Where are the tyres?'])
  // Add one and remove it again.
  await click('#q-add')
  await evaluate(`document.querySelectorAll('#q-list .q-row')[3].querySelector('.ui-btn--danger').click()`)
  const qs = await saved()
  assert.equal(qs.length, 3)
  assert.deepEqual(qs.map(q => q.kind), ['checkbox', 'short_text', 'single_choice'])
  assert.equal(qs[0].consent, true)
  assert.equal(qs[0].required, true)
  assert.equal(qs[0].link, 'https://example.test/privacy')
  assert.equal(qs[1].required, true)
  assert.deepEqual(qs[2].options.map(o => o.label), ['At the shop', 'I bring them'])
  await submit('form:not([action])', { title: 'Tyres', slug: 'tyres', location_value: 'https://example.test/meeting', min_notice_min: '0', timezone: 'UTC',
    avail_schedule: '0:09:00-17:00;1:09:00-17:00;2:09:00-17:00;3:09:00-17:00;4:09:00-17:00;5:09:00-17:00;6:09:00-17:00' })
  assert.equal(await path(), '/dashboard/event-types')

  await go('/dashboard/event-types/tyres/edit')
  const stored = await saved()
  assert.equal(stored.length, 3)
  assert(stored.every(q => q.id), 'saved questions come back with their ids')
  await evaluate(`document.querySelector('#booking-form-section').open = true`)
  await evaluate(`document.querySelectorAll('#q-list .q-edit-btn')[2].click()`)
  assert(await evaluate(`document.querySelector('#q-ed-2-kind').disabled`), 'a saved question keeps its kind')
  assert((await evaluate(`document.querySelector('#q-ed-2-kind-note').textContent`)).includes('where_are_the_tyres'), 'the key is shown')

  const date = new Date(Date.now() + 3 * 86400000).toISOString().slice(0, 10)
  await go(`/u/q-admin/tyres/book?date=${date}&time=10:00&tz=UTC`)
  await screenshot('questions-booking-form')
  assert(await evaluate(`document.querySelector('fieldset.q-group legend').textContent.includes('Where are the tyres?')`))
  // The required consent blocks the native submit until ticked.
  const blocked = await evaluate(`(() => {
    const f = document.querySelector('form');
    f.elements.namedItem('name').value = 'Guest';
    f.elements.namedItem('email').value = 'guest@example.test';
    document.querySelector('#q1').value = 'AB-123';
    return f.checkValidity();
  })()`)
  assert.equal(blocked, false)
  await navigation(() => evaluate(`(() => {
    document.querySelector('#q0').click();
    document.querySelector('#q2-o1').click();
    const f = document.querySelector('form');
    if (!f.reportValidity()) throw new Error('Invalid form');
    f.requestSubmit();
  })()`))
  const confirmation = await text()
  assert(confirmation.includes('AB-123'), confirmation)
  assert(confirmation.includes('You agreed'), confirmation)
  assert(confirmation.includes('I bring them'), confirmation)
  await go('/dashboard/bookings')
  await evaluate(`document.querySelector('.booking-answers').open = true`)
  await screenshot('questions-bookings')
  assert((await text()).includes('Agreed'))
  assert.deepEqual(jsErrors, [])
  console.log('PASS: editor add, edit, reorder with buttons, remove, save and reload; kind locked once saved; booking form fieldsets, native required consent, answers on the confirmation and bookings pages; no browser exceptions')
} finally {
  ws?.close()
  chrome?.kill('SIGTERM')
  server.kill('SIGTERM')
  await writeFile(join(dir, 'server.log'), serverLog)
  await writeFile(join(dir, 'chrome.log'), browserLog)
  console.log(`Browser test artifacts: ${dir}`)
}
