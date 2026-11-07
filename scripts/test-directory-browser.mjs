// Booking directory and favorites in a real browser and an isolated server.
// Requires Node >= 22 (built-in WebSocket) and Chrome/Chromium. No npm packages.
// Run from the repository root: node scripts/test-directory-browser.mjs
// CALRS_BINARY and CHROME_BINARY can override the executable paths.
import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
import { createServer } from 'node:net'

const dir = await mkdtemp(join(tmpdir(), 'calrs-directory-'))
const binary = resolve(process.env.CALRS_BINARY || 'target/debug/calrs')
const reserve = createServer()
await new Promise(r => reserve.listen(0, '127.0.0.1', r))
const port = reserve.address().port
await new Promise(r => reserve.close(r))
const base = `http://localhost:${port}`
const password = 'directory-browser-password'
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
    const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true, userGesture: true })
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

  const sql = statements => {
    const result = spawnSync('python3', ['-c', `import sqlite3,sys
with sqlite3.connect(sys.argv[1]) as db:
 db.executescript(sys.argv[2])`, join(dir, 'data', 'calrs.db'), statements], { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
  }
  await call('Page.bringToFront')
  await call('Emulation.setDeviceMetricsOverride', { width: 1280, height: 960, deviceScaleFactor: 1, mobile: false })
  await go('/auth/register')
  await submit('form', { name: 'Directory Viewer', email: 'directory@example.test', password })
  sql(`
    INSERT INTO users (id,email,name,username,role,enabled) VALUES ('andy','andy@example.test','Andy Lewis','andy-lewis','user',1);
    INSERT INTO accounts (id,name,email,timezone,user_id) VALUES ('andy-account','Andy','andy@example.test','UTC','andy');
    INSERT INTO event_types (id,account_id,slug,title,duration_min,visibility) VALUES
      ('consulting','andy-account','consulting','Consulting',60,'public'),
      ('tam','andy-account','tam','TAM',30,'internal'),
      ('private','andy-account','private','Hidden private meeting',30,'private');
    INSERT INTO teams (id,name,slug,visibility) VALUES ('sales','Sales France','sales-fr','public');
    INSERT INTO event_types (id,account_id,team_id,slug,title,duration_min,visibility) VALUES
      ('demo','andy-account','sales','demo-fr','Démo FR',45,'public');
  `)
  const rows = () => evaluate(`Array.from(document.querySelectorAll('.directory-entry')).map(row => row.dataset.eventId)`)
  const click = selector => evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`)
  const star = async (id, desired) => {
    await click(`[data-event-id="${id}"] .directory-star`)
    await until(() => evaluate(`(() => {
      const button = document.querySelector('[data-event-id="${id}"] .directory-star');
      return button && !button.disabled && button.getAttribute('aria-pressed') === '${desired}';
    })()`), `favorite ${id}: ${desired}`)
  }
  await go('/dashboard/directory')
  assert.deepEqual(await rows(), ['consulting', 'tam', 'demo'])
  await submit('form[method=get]', { q: 'Andy TAM' })
  assert.deepEqual(await rows(), ['tam'])
  await star('tam', true)
  await go('/dashboard/directory?q=Sales')
  assert.deepEqual(await rows(), ['demo'])
  await star('demo', true)
  await go('/dashboard/directory?q=Consulting')
  await star('consulting', true)
  await go('/dashboard')
  assert.deepEqual(await rows(), ['consulting', 'tam', 'demo'])
  await screenshot('dashboard-favorites')

  await call('Page.bringToFront')
  await call('Browser.grantPermissions', { origin: base, permissions: ['clipboardReadWrite', 'clipboardSanitizedWrite'] })
  await click('[data-event-id="consulting"] .directory-share')
  await until(() => evaluate(`document.querySelector('[data-event-id="consulting"] .directory-status').textContent === 'Copied!'`), 'public link copied')
  assert.equal(await evaluate('navigator.clipboard.readText()'), `${base}/u/andy-lewis/consulting`)
  await click('[data-event-id="tam"] .directory-share')
  await until(() => evaluate(`document.querySelector('[data-event-id="tam"] .directory-status').textContent === 'Copied!'`), 'internal invite copied')
  const firstInvite = await evaluate('navigator.clipboard.readText()')
  assert(firstInvite.startsWith(`${base}/u/andy-lewis/tam?invite=`))
  await click('[data-event-id="tam"] .directory-share')
  await until(() => evaluate(`!document.querySelector('[data-event-id="tam"] .directory-share').disabled`), 'second invite completed')
  assert.notEqual(await evaluate('navigator.clipboard.readText()'), firstInvite)

  // Clipboard denial keeps a newly generated invite available for manual copy.
  await evaluate(`Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async () => { throw new Error('denied'); } } })`)
  await click('[data-event-id="tam"] .directory-share')
  await until(() => evaluate(`!document.querySelector('[data-event-id="tam"] .directory-manual').hidden`), 'manual copy fallback')
  assert((await evaluate(`document.querySelector('[data-event-id="tam"] .directory-manual input').value`)).includes('/tam?invite='))
  assert.equal(await evaluate(`document.querySelector('[data-event-id="tam"] .directory-share').disabled`), false)

  // A failed save neither changes the star nor leaves the button stuck.
  await evaluate(`window.realFetch = window.fetch; window.fetch = async () => new Response('', {status: 500})`)
  await click('[data-event-id="consulting"] .directory-star')
  await until(() => evaluate(`document.querySelector('[data-event-id="consulting"] .directory-status').textContent.includes('could not')`), 'failed save message')
  assert.equal(await evaluate(`document.querySelector('[data-event-id="consulting"] .directory-star').getAttribute('aria-pressed')`), 'true')
  assert.equal(await evaluate(`document.querySelector('[data-event-id="consulting"] .directory-star').disabled`), false)
  await evaluate('window.fetch = window.realFetch')

  sql(`UPDATE event_types SET title='TAM customer session', slug='tam-updated' WHERE id='tam';`)
  await go('/dashboard')
  assert((await text()).includes('TAM customer session'))
  assert.equal(await evaluate(`document.querySelector('[data-event-id="tam"] .directory-share').dataset.url`), '/u/andy-lewis/tam-updated')
  sql(`UPDATE event_types SET visibility='private' WHERE id='tam'; UPDATE event_types SET enabled=0 WHERE id='consulting';`)
  await go('/dashboard')
  assert.deepEqual(await rows(), ['demo'])
  await click('[data-event-id="demo"] .directory-star')
  await until(async () => (await rows()).length === 0, 'remove last visible favorite')
  assert.equal(await evaluate(`document.querySelector('.directory-empty').hidden`), false)
  sql(`UPDATE event_types SET visibility='internal' WHERE id='tam'; UPDATE event_types SET enabled=1 WHERE id='consulting';`)

  await go('/dashboard/directory?favorites=true')
  assert.deepEqual(await rows(), ['consulting', 'tam'])
  await go('/dashboard/directory')
  await screenshot('directory-desktop')
  await call('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 1, mobile: true })
  await screenshot('directory-mobile')
  assert.equal(await evaluate('document.documentElement.scrollWidth <= window.innerWidth'), true, 'mobile horizontal overflow')
  await star('demo', true)
  sql(`UPDATE users SET language='fr' WHERE email='directory@example.test';`)
  await go('/dashboard/directory')
  assert((await text()).includes('Annuaire des rendez-vous'))
  assert((await text()).includes('Générer un lien'))
  await screenshot('directory-french-mobile')
  assert.deepEqual(jsErrors, [])
  console.log('PASS: search, independent favorites, persistence, dashboard, clipboard, fresh invites, fallback, save failures, live rename and visibility, mobile layout, French, no browser exceptions')
} finally {
  ws?.close()
  chrome?.kill('SIGTERM')
  server.kill('SIGTERM')
  await writeFile(join(dir, 'server.log'), serverLog)
  await writeFile(join(dir, 'chrome.log'), browserLog)
  console.log(`Browser test artifacts: ${dir}`)
}
