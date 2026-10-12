// Real-browser language, persistence and storage-error regressions. Node >= 22, no npm packages.
// Build with cargo xtask web, then run with --chrome /usr/bin/chromium --url http://127.0.0.1:8765/
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const arg = (name, fallback) => { const i = process.argv.indexOf(`--${name}`); return i < 0 ? fallback : process.argv[i + 1]; };
const url = arg("url", "http://127.0.0.1:8765/");
const out = arg("out", "target/web/language");
const port = Number(arg("port", "9334"));
mkdirSync(out, { recursive: true });
const proc = spawn(arg("chrome", "google-chrome"), [
  "--headless=new", "--no-sandbox", "--disable-dev-shm-usage", "--no-first-run",
  "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--autoplay-policy=no-user-gesture-required",
  `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "fc-language-"))}`, "about:blank",
], { stdio: "ignore" });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let ws, nextId = 0;
const pending = new Map();
const report = { url, steps: [], screenshots: [] };

async function connect() {
  for (let i = 0; i < 100; ++i) {
    try {
      const page = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((t) => t.type === "page");
      if (page) {
        ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
        ws.onmessage = ({ data }) => {
          const message = JSON.parse(data), request = pending.get(message.id);
          if (request) {
            pending.delete(message.id); clearTimeout(request.timer);
            message.error ? request.reject(new Error(message.error.message)) : request.resolve(message.result);
          }
        };
        return;
      }
    } catch {}
    await sleep(100);
  }
  throw new Error("Chrome did not start");
}
function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++nextId, timer = setTimeout(() => { pending.delete(id); reject(new Error(`timeout: ${method}`)); }, 120000);
    pending.set(id, { resolve, reject, timer }); ws.send(JSON.stringify({ id, method, params }));
  });
}
async function js(expression) {
  // Keep asynchronous API promises reachable while CDP awaits them, including during WASM GC.
  const r = await send("Runtime.evaluate", { expression: `window.__languageEvaluation = (${expression})`, awaitPromise: true, returnByValue: true, timeout: 120000 });
  assert.equal(r.exceptionDetails, undefined, r.exceptionDetails?.exception?.description);
  return r.result.value;
}
async function ready() {
  const start = Date.now();
  while (Date.now() - start < 120000) {
    if (await js("!window.__beforeLanguageReload && !!window.filmcraftLoad?.readyMs")) return;
    const error = await js("window.filmcraftLoad?.error || window.filmcraftLoad?.fatal");
    assert.ok(!error, String(error)); await sleep(100);
  }
  throw new Error("FilmCraft did not become ready");
}
const request = (m, p = {}) => js(`filmcraft.request(${JSON.stringify(m)}, ${JSON.stringify(p)})`);
const execute = (c, p = {}) => js(`filmcraft.execute(${JSON.stringify(c)}, ${JSON.stringify(p)})`);
const state = () => js("filmcraft.inspect().then(v => v.ui)");
async function click(id) {
  // A centred egui window settles its size on following passes. Resolve the stable control ID
  // after layout, then use the browser's pointer events to exercise an actual user click.
  await request("ui.inspect"); await request("ui.inspect");
  await sleep(150);
  const element = (await request("ui.elements", { prefix: id })).find((e) => e.id === id);
  assert.ok(element, `no visible element ${id}`);
  const [left, top, width, height] = element.rect;
  const x = left + width / 2, y = top + height / 2;
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", buttons: 1, clickCount: 1, x, y });
  await sleep(50);
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", buttons: 0, clickCount: 1, x, y });
  await sleep(150);
  await request("ui.inspect"); await request("ui.inspect");
}
async function reload() { await js("window.__beforeLanguageReload = true"); await send("Page.reload"); await ready(); }
async function shot(name) {
  await sleep(150);
  const path = join(out, `${name}.png`), r = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(path, Buffer.from(r.data, "base64")); report.screenshots.push(path);
}
try {
  await connect(); await send("Page.enable");
  await send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 1000, deviceScaleFactor: 1, mobile: false });
  await send("Page.navigate", { url: `${url}?fresh&webgl` }); await ready();
  const before = await execute("sequence.inspect");
  for (const [name, code] of [
    ["korean", "ko"], ["english", "en"], ["japanese", "ja"], ["spanish", "es"], ["portuguese", "pt-br"],
    ["ukrainian", "uk"], ["chinese", "zh-cn"], ["german", "de"], ["russian", "ru"], ["french", "fr"], ["indonesian", "id"],
  ]) {
    const previousLanguage = (await state()).language;
    const previousPreference = await js("localStorage.getItem('filmcraft.interfaceLanguage')");
    try {
      await request("ui.menu.invoke", { id: `app.language.${name}` });
    } catch (error) {
      // The original default build has no Simplified Chinese face. Keep testing the refusal
      // path when none is supplied; an explicit CJK build must pass the positive path below.
      if (name !== "chinese" || !/no Chinese font/.test(String(error))) throw error;
      assert.equal((await state()).language, previousLanguage);
      assert.equal(await js("localStorage.getItem('filmcraft.interfaceLanguage')"), previousPreference);
      report.steps.push("chinese: missing optional font refuses selection and preserves the current choice");
      continue;
    }
    assert.equal((await state()).language, code);
    assert.equal(await js("localStorage.getItem('filmcraft.interfaceLanguage')"), code);
    const menu = await request("ui.menu.list");
    for (const item of menu.filter((it) => it.id.startsWith("app.language."))) assert.equal(item.checked, item.id === `app.language.${name}`);
    assert.deepEqual(await execute("sequence.inspect"), before, "language selection changed the project");
    if (code === "ko") { assert.equal(menu.find((it) => it.id === "file.saveAs").label, "다른 이름으로 저장…"); await shot("01-korean-editor"); }
    await reload(); assert.equal((await state()).language, code, `${code} did not persist after refresh`);
    report.steps.push(`menu, saved preference and refresh: ${code}`);
    console.log(`passed: ${code}`);
  }
  await request("ui.menu.invoke", { id: "app.settings.general" });
  await click("settings.general.interfaceLanguage"); await shot("02-language-options");
  await click("settings.general.interfaceLanguage.ko");
  assert.equal((await state()).settings.values.general.interfaceLanguage, "ko");
  await click("settings.cancel"); assert.equal((await state()).language, "id");
  await request("ui.menu.invoke", { id: "app.settings.general" });
  await click("settings.general.interfaceLanguage");
  await click("settings.general.interfaceLanguage.ko");
  assert.equal((await state()).settings.values.general.interfaceLanguage, "ko");
  await click("settings.ok"); await reload(); assert.equal((await state()).language, "ko");
  await request("ui.menu.invoke", { id: "app.settings.general" }); await shot("03-korean-settings");
  await click("settings.cancel"); await request("ui.panel.show", { panel: "Effects" });
  for (const query of ["흐림", "gaussian"]) {
    await request("ui.set", { effectsSearch: query });
    await request("ui.inspect"); await request("ui.inspect");
    assert.ok((await request("ui.elements", { prefix: "effects.item." })).some((e) => e.id === "effects.item.gaussian_blur"));
  }
  await shot("04-korean-effect-search"); report.steps.push("Settings OK/Cancel and Korean/English search with stable automation IDs");
  await js("localStorage.setItem('filmcraft.interfaceLanguage', 'invalid-code')"); await reload(); assert.equal((await state()).language, "en");
  // The fallback preference is already `system`; choose English explicitly before testing a
  // change back to System Language, so this exercises a real settings change.
  await request("ui.menu.invoke", { id: "app.language.english" });
  const locale = await send("Page.addScriptToEvaluateOnNewDocument", { source: "Object.defineProperty(navigator, 'languages', {get: () => ['ko-KR', 'en-US']})" });
  await execute("prefs.set", { key: "general.interfaceLanguage", value: "system" }); await reload();
  assert.equal((await state()).language, "ko"); assert.equal(await js("localStorage.getItem('filmcraft.interfaceLanguage')"), "system");
  await request("ui.menu.invoke", { id: "app.language.spanish" }); await reload(); assert.equal((await state()).language, "es");
  await send("Page.removeScriptToEvaluateOnNewDocument", { identifier: locale.identifier });
  report.steps.push("invalid saved value, system Korean locale and explicit preference precedence");
  await send("Page.addScriptToEvaluateOnNewDocument", { source: "Object.defineProperty(window, 'localStorage', {get() {throw new DOMException('Storage disabled', 'SecurityError')}})" });
  await reload(); await request("ui.menu.invoke", { id: "app.language.korean" });
  assert.equal((await state()).language, "ko"); assert.match((await state()).status, /저장하지 못했습니다/);
  assert.ok(!(await js("window.filmcraftLoad.fatal || null")));
  report.steps.push("disabled storage reports a translated failure and leaves editing usable"); report.ok = true;
} catch (error) {
  report.ok = false; report.error = String(error.stack || error);
  try { await shot("99-failure"); } catch {}
} finally {
  writeFileSync(join(out, "report.json"), JSON.stringify(report, null, 2)); console.log(JSON.stringify(report, null, 2));
  ws?.close(); proc.kill(); for (const request of pending.values()) clearTimeout(request.timer);
}
process.exit(report.ok ? 0 : 1);
