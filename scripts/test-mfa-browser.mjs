// Functional MFA test using a real Chrome browser and an isolated calrs server.
// Requires Node >= 22 (built-in WebSocket) and Chrome/Chromium. No npm packages.
// Run from the repository root: node scripts/test-mfa-browser.mjs
// CALRS_BINARY and CHROME_BINARY can override the executable paths.
import assert from 'node:assert/strict'
import { createHmac } from 'node:crypto'
import { spawn, execFileSync } from 'node:child_process'
import { mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
import { createServer } from 'node:net'

const dir = await mkdtemp(join(tmpdir(), 'calrs-mfa-browser-'))
const binary = resolve(process.env.CALRS_BINARY || 'target/debug/calrs')
const reserve = createServer()
await new Promise(r => reserve.listen(0, '127.0.0.1', r))
const port = reserve.address().port
await new Promise(r => reserve.close(r))
const base = `http://localhost:${port}`
const password = 'browser-test-password-213'
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
  const logout = async () => {
    // Exercise the server's real CSRF-protected logout endpoint.
    await evaluate(`fetch('/auth/logout', {method:'POST',headers:{'Content-Type':'application/x-www-form-urlencoded'},body:'_csrf='+encodeURIComponent(document.cookie.match(/__Host-calrs_csrf=([^;]+)/)[1])})`)
    await go('/auth/login')
  }
  const login = email => submit('form[action="/auth/login"]', { email, password })
  const otp = (base32, offset = 0) => {
    let bits = ''
    for (const c of base32) bits += 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c).toString(2).padStart(5, '0')
    const key = Buffer.from(bits.match(/.{8}/g).map(b => parseInt(b, 2)))
    const step = BigInt(Math.floor(Date.now() / 30000) + offset)
    const message = Buffer.alloc(8)
    message.writeBigUInt64BE(step)
    const digest = createHmac('sha1', key).update(message).digest()
    const i = digest[19] & 15
    return String((digest.readUInt32BE(i) & 0x7fffffff) % 1000000).padStart(6, '0')
  }
  const recoveryCodes = () => evaluate('[...document.querySelectorAll(".recovery-code")].map(n => n.textContent)')

  // Register the first local admin. Optional MFA preserves password-only login.
  await go('/auth/register')
  await submit('form', { name: 'MFA Browser Admin', email: 'admin@example.test', password })
  assert.equal(await path(), '/dashboard')
  // Check optional enrollment, password reauthentication and disable on a
  // separate account, keeping each account below its real rate limit.
  await logout()
  await go('/auth/register')
  await submit('form', { name: 'Optional MFA', email: 'optional@example.test', password })
  await go('/dashboard/settings/mfa')
  await submit('form[action="/dashboard/settings/mfa"]', { password: 'wrong-password' }, '[value="enable"]')
  assert.match(await text(), /Verification failed/)
  await go('/dashboard/settings/mfa')
  await submit('form[action="/dashboard/settings/mfa"]', { password }, '[value="enable"]')
  const optionalSecret = await evaluate('document.querySelector("#mfa-secret").textContent')
  await submit('form[action="/auth/mfa"]', { code: otp(optionalSecret, -1) })
  const optionalCodes = await recoveryCodes()
  assert.equal(optionalCodes.length, 10)
  await go('/dashboard/settings/mfa')
  await submit('form[action="/dashboard/settings/mfa"]', { password, code: optionalCodes[0] }, '[value="disable"]')
  await go('/dashboard/settings/mfa')
  assert.match(await text(), /is disabled/)
  await logout()
  await login('optional@example.test')
  assert.equal(await path(), '/dashboard')
  await logout()
  await login('admin@example.test')
  assert.equal(await path(), '/dashboard')
  await go('/dashboard/settings')
  assert(await evaluate(`Boolean(document.querySelector('a[href="/dashboard/settings/mfa"]'))`), "MFA link is visible in settings")
  await go('/dashboard/settings/mfa')
  assert.match(await text(), /is disabled/)
  await submit('form[action="/dashboard/settings/mfa"]', { password }, '[value="enable"]')
  assert.equal(await path(), '/auth/mfa')
  const secret = await evaluate('document.querySelector("#mfa-secret").textContent')
  assert.match(secret, /^[A-Z2-7]{32}$/)
  assert.equal(await evaluate(`document.querySelector('img[src^="data:image/png"]').naturalWidth > 0`), true)
  await screenshot('01-enrollment')
  // Prove a wrong code does not activate MFA, then complete with the previous step.
  await submit('form[action="/auth/mfa"]', { code: '000000' })
  assert.match(await text(), /Verification failed/)
  await submit('form[action="/auth/mfa"]', { code: otp(secret, -1) })
  let codes = await recoveryCodes()
  assert.equal(codes.length, 10)
  // Recovery codes are intentionally not captured in screenshots or logs.
  await go('/dashboard/settings/mfa')
  assert.match(await text(), /is enabled/)
  await screenshot('02-settings')
  await logout()
  await login('admin@example.test')
  assert.equal(await path(), '/auth/mfa')
  const cookies = (await call('Network.getCookies', { urls: [base] })).cookies
  assert(!cookies.some(c => c.name === '__Host-calrs_session'))
  const challengeCookie = cookies.find(c => c.name === '__Host-calrs_mfa')
  assert(challengeCookie.httpOnly && challengeCookie.secure)
  await screenshot('03-login-challenge')
  await submit('form[action="/auth/mfa"]', { code: otp(secret) })
  assert.equal(await path(), '/dashboard')
  // Recovery login works once, then replay fails even with the right password.
  await logout()
  await login('admin@example.test')
  await submit('form[action="/auth/mfa"]', { code: codes[0] })
  assert.equal(await path(), '/dashboard')
  await logout()
  await login('admin@example.test')
  await submit('form[action="/auth/mfa"]', { code: codes[0] })
  assert.match(await text(), /Verification failed/)
  await submit('form[action="/auth/mfa"]', { code: codes[1] })
  assert.equal(await path(), '/dashboard')
  // Regeneration shows a replacement set and invalidates all old codes.
  await go('/dashboard/settings/mfa')
  await submit('form[action="/dashboard/settings/mfa"]', { password, code: codes[2] }, '[value="regenerate"]')
  const oldCodes = codes
  codes = await recoveryCodes()
  assert.equal(codes.length, 10)
  assert(codes.every(c => !oldCodes.includes(c)))
  // Require MFA. Ordinary auth settings saves must leave the policy unchanged.
  await go('/dashboard/admin')
  await submit('form[action="/dashboard/admin/mfa"]', { policy: 'required', password, code: codes[0] })
  assert.equal(await path(), '/dashboard/admin')
  assert.equal(await evaluate('document.querySelector("#mfa-policy").value'), 'required')
  await submit('form[action="/dashboard/admin/auth"]', {})
  assert.equal(await evaluate('document.querySelector("#mfa-policy").value'), 'required')
  await go('/dashboard/settings/mfa')
  assert.match(await text(), /cannot disable/)
  assert.equal(await evaluate('Boolean(document.querySelector("button[value=disable]"))'), false)
  await screenshot('04-required-settings')
  await logout()
  // New users must finish enrollment before they can reach the dashboard.
  await go('/auth/register')
  await submit('form', { name: 'MFA Browser Member', email: 'member@example.test', password })
  assert.equal(await path(), '/auth/mfa')
  // Suspending and re-enabling the account must invalidate its old challenge.
  execFileSync(binary, ['--data-dir', join(dir, 'data'), 'user', 'disable', 'member@example.test'])
  execFileSync(binary, ['--data-dir', join(dir, 'data'), 'user', 'enable', 'member@example.test'])
  await go('/auth/mfa')
  assert.match(await text(), /Verification failed/)
  assert.equal(await evaluate('Boolean(document.querySelector("#mfa-secret"))'), false)
  await go('/auth/login')
  await login('member@example.test')
  const memberSecret = await evaluate('document.querySelector("#mfa-secret").textContent')
  await go('/dashboard')
  assert.equal(await path(), '/auth/login')
  await go('/auth/mfa')
  await submit('form[action="/auth/mfa"]', { code: otp(memberSecret, -1) })
  assert.equal((await recoveryCodes()).length, 10)
  await go('/dashboard')
  assert.equal(await path(), '/dashboard')
  // Server-operator recovery revokes sessions without weakening the policy.
  execFileSync(binary, ['--data-dir', join(dir, 'data'), 'user', 'reset-mfa', 'member@example.test'])
  await go('/dashboard')
  assert.equal(await path(), '/auth/login')
  await login('member@example.test')
  assert.equal(await path(), '/auth/mfa')
  assert(await evaluate('Boolean(document.querySelector("#mfa-secret"))'))
  console.log('PASS: enrollment, password reauthentication, disable, QR rendering, TOTP login, recovery login/replay, regeneration, mandatory policy, keep-current save, registration gate, suspension revocation, CLI reset')
  console.log(`Screenshots and isolated test data: ${dir}`)
} catch (e) {
  console.error(`Functional test failed; logs in ${dir}`)
  throw e
} finally {
  ws?.close()
  chrome?.kill('SIGTERM')
  server.kill('SIGTERM')
  await writeFile(join(dir, 'server.log'), serverLog)
  await writeFile(join(dir, 'chrome.log'), browserLog)
}
