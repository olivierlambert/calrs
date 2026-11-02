// First-day-of-week regression using a real browser and an isolated server.
// Requires Node >= 22 (built-in WebSocket) and Chrome/Chromium. No npm packages.
// Run from the repository root: node scripts/test-week-start-browser.mjs
// CALRS_BINARY and CHROME_BINARY can override the executable paths.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
import { createServer } from 'node:net'

const dir = await mkdtemp(join(tmpdir(), 'calrs-week-start-'))
const binary = resolve(process.env.CALRS_BINARY || 'target/debug/calrs')
const reserve = createServer()
await new Promise(r => reserve.listen(0, '127.0.0.1', r))
const port = reserve.address().port
await new Promise(r => reserve.close(r))
const base = `http://localhost:${port}`
const password = 'browser-test-password-330'
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
  await go('/auth/register')
  await submit('form', { name: 'Week Admin', email: 'week@example.test', password })
  const schedule = '0:10:00-11:00;1:09:00-17:00;2:09:00-17:00;3:09:00-17:00;4:09:00-17:00;5:09:00-17:00;6:10:00-11:00'
  const keys = ['sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat']
  const labels = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
  const select = day => evaluate(`(() => {
    const el = document.querySelector('#week_start');
    el.value = ${JSON.stringify(day)};
    el.dispatchEvent(new Event('change', {bubbles:true}));
  })()`)
  const rows = () => evaluate(`Array.from(document.querySelector('#schedule-container').children).map(row => row.querySelector('div').textContent)`)
  await go('/dashboard/settings')
  await submit('form:not([action])', { name: 'Week Admin', username: 'week-admin', avail_schedule: schedule, timezone: 'UTC', week_start: 'sun' })
  assert.deepEqual(await rows(), labels)
  const savedSchedule = await evaluate('document.querySelector("#avail_schedule").value')
  for (let i = 0; i < 7; i++) {
    await select(keys[i])
    assert.deepEqual(await rows(), labels.slice(i).concat(labels.slice(0, i)))
    assert.equal(await evaluate('document.querySelector("#avail_schedule").value'), savedSchedule)
  }

  await go('/dashboard/event-types/new')
  assert.deepEqual(await rows(), labels, 'new event inherits the owner’s Sunday')
  await submit('form:not([action])', { title: 'Week Test', slug: 'week-test', location_value: 'https://example.test/meeting', avail_schedule: schedule, min_notice_min: '0', timezone: 'UTC', week_start: '' })
  assert.equal(await path(), '/dashboard/event-types')
  const month = new Date()
  month.setMonth(month.getMonth() + 2, 1)
  const monthKey = `${month.getFullYear()}-${String(month.getMonth() + 1).padStart(2, '0')}`
  const pickView = view => evaluate(`document.querySelector('[data-view="${view}"]').click()`)
  const weekHeaders = () => evaluate(`Array.from(document.querySelectorAll('.week-day-header')).map(e => e.firstChild.textContent)`)
  const weekDates = () => evaluate(`Array.from(document.querySelectorAll('.week-day-col')).map(e => new URL(e.querySelector('a.week-slot-pill').href).searchParams.get('date'))`)

  for (let i = 0; i < 7; i++) {
    await go('/dashboard/event-types/week-test/edit')
    const original = await evaluate('document.querySelector("#avail_schedule").value')
    await select(keys[i])
    assert.deepEqual(await rows(), labels.slice(i).concat(labels.slice(0, i)))
    assert.equal(await evaluate('document.querySelector("#avail_schedule").value'), original)
    await submit('form:not([action])', { week_start: keys[i] })
    await go(`/u/week-admin/week-test?month=${monthKey}&tz=UTC`)
    await pickView('month')
    assert.deepEqual(await evaluate(`Array.from(document.querySelectorAll('.cal-weekday-full')).map(e => e.textContent)`), labels.slice(i).concat(labels.slice(0, i)))
    assert.equal(await evaluate(`Array.from(document.querySelector('#cal-grid').children).findIndex(e => e.dataset.date === '${monthKey}-01')`), (month.getDay() + 7 - i) % 7)
    await pickView('week')
    assert.deepEqual(await weekHeaders(), labels.slice(i).concat(labels.slice(0, i)))
    let dates = await weekDates()
    assert.equal(new Date(`${dates[0]}T12:00:00`).getDay(), i)
    for (let step = 0; step < 6; step++) {
      const before = dates[0]
      await evaluate(`document.querySelector('#main-cal-nav a:last-child').click()`)
      await until(async () => {
        try { return (await weekDates())[0] !== before } catch { return false }
      }, 'next week with slots across the month boundary')
      dates = await weekDates()
      assert.equal((new Date(`${dates[0]}T12:00:00`) - new Date(`${before}T12:00:00`)) / 86400000, 7)
      assert.deepEqual(await weekHeaders(), labels.slice(i).concat(labels.slice(0, i)))
    }
    await pickView('column')
    assert(await evaluate('document.querySelectorAll(".column-day").length > 0'))
  }
  // Change the saved preference without reloading the booking page, as if
  // the owner edited it in another tab. Month AJAX must rotate its headings.
  await pickView('month')
  const changed = await evaluate(`(async () => {
    const response = await fetch('/dashboard/event-types/week-test/edit');
    const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
    const fields = new URLSearchParams(new FormData(doc.querySelector('form:not([action])')));
    fields.set('week_start', 'tue');
    fields.set('_csrf', decodeURIComponent(document.cookie.match(/__Host-calrs_csrf=([^;]+)/)[1]));
    return (await fetch('/dashboard/event-types/week-test/edit', { method: 'POST', body: fields })).ok;
  })()`)
  assert(changed)
  await evaluate(`document.querySelector('#main-cal-nav a:last-child').click()`)
  await until(async () => (await evaluate(`document.querySelector('.cal-weekday-full').textContent`)) === 'Tue', 'updated first day after month AJAX')
  await pickView('week')
  assert.deepEqual(await weekHeaders(), labels.slice(2).concat(labels.slice(0, 2)))
  await screenshot('week-start')
  assert.deepEqual(jsErrors, [])
  console.log('PASS: all seven first-day settings, inherited editor order, unchanged hours, month alignment, week paging across months, column view, and no browser exceptions')
} finally {
  ws?.close()
  chrome?.kill('SIGTERM')
  server.kill('SIGTERM')
  await writeFile(join(dir, 'server.log'), serverLog)
  await writeFile(join(dir, 'chrome.log'), browserLog)
  console.log(`Browser test artifacts: ${dir}`)
}
