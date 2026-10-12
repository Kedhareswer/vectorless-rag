// Records the in-app "Add a model" guide clips and the full onboarding video
// from the real UI, with a stand-in backend (mock.js). Re-run after UI changes.
//
//   npm run dev                       # in another terminal (http://localhost:1420)
//   npm i --no-save playwright        # once; needs ffmpeg on PATH
//   node scripts/guides/record.mjs [installed|offline|cloud|onboarding]
//
// Clips go to src/assets/guides/*.webm, the walkthrough to docs/media/onboarding.mp4.
import { chromium } from 'playwright';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const ROOT = path.resolve(import.meta.dirname, '../..');
const URL = process.env.GUIDE_URL ?? 'http://localhost:1420/';
const MOCK = fs.readFileSync(path.join(import.meta.dirname, 'mock.js'), 'utf8');
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'tgg-guides-'));

/** Visible cursor, click ripple and caption bar — headless recordings show none of these. */
function overlays() {
  addEventListener('DOMContentLoaded', () => {
    const cursor = document.createElement('div');
    cursor.innerHTML =
      '<svg width="22" height="22" viewBox="0 0 24 24"><path d="M4 2l16 10-7 1.6L9.6 21z" fill="#1c1917" stroke="#fff" stroke-width="1.6" stroke-linejoin="round"/></svg>';
    Object.assign(cursor.style, { position: 'fixed', left: 0, top: 0, zIndex: 2147483647, pointerEvents: 'none', transform: 'translate(-60px,-60px)' });
    document.body.appendChild(cursor);
    addEventListener('mousemove', (e) => { cursor.style.transform = `translate(${e.clientX - 3}px,${e.clientY - 2}px)`; }, true);
    addEventListener('mousedown', (e) => {
      const r = document.createElement('div');
      Object.assign(r.style, {
        position: 'fixed', left: `${e.clientX - 15}px`, top: `${e.clientY - 15}px`, width: '30px', height: '30px',
        borderRadius: '50%', background: 'rgba(222,115,86,.4)', zIndex: 2147483646, pointerEvents: 'none',
        transition: 'transform .45s ease, opacity .45s ease',
      });
      document.body.appendChild(r);
      requestAnimationFrame(() => { r.style.transform = 'scale(1.9)'; r.style.opacity = '0'; });
      setTimeout(() => r.remove(), 500);
    }, true);

    const caption = document.createElement('div');
    caption.id = '__caption';
    Object.assign(caption.style, {
      position: 'fixed', left: '50%', bottom: '18px', transform: 'translateX(-50%)', zIndex: 2147483645,
      padding: '9px 16px', borderRadius: '999px', background: 'rgba(28,25,23,.9)', color: '#fff',
      font: '500 15px Inter, system-ui, sans-serif', boxShadow: '0 6px 20px rgba(0,0,0,.25)',
      pointerEvents: 'none', opacity: '0', transition: 'opacity .25s ease', whiteSpace: 'nowrap',
    });
    document.body.appendChild(caption);
  });
}

async function session({ width, height, servers, tourDone, hideVideos }) {
  const ctx = await (await browser()).newContext({
    viewport: { width, height },
    deviceScaleFactor: 1,
    colorScheme: 'light',
  });
  await ctx.addInitScript(([s, t]) => {
    localStorage.setItem('__servers', s);
    localStorage.setItem('__tourDone', t ? '1' : '0');
    localStorage.setItem('theme', 'light');
  }, [servers, tourDone]);
  await ctx.addInitScript(MOCK);
  await ctx.addInitScript(overlays);
  const page = await ctx.newPage();
  page.on('pageerror', (e) => console.error('page error:', e.message));
  await page.goto(URL);
  // Recordings must not contain the clips they are about to replace.
  if (hideVideos) await page.addStyleTag({ content: 'video { display: none !important; }' });
  await page.waitForTimeout(1200);
  await page.mouse.move(width / 2, height / 2);

  // Capture frames from Chromium directly; they are timed and encoded in finish().
  const frames = [];
  const cdp = await ctx.newCDPSession(page);
  cdp.on('Page.screencastFrame', ({ data, metadata, sessionId }) => {
    frames.push({ data, t: metadata.timestamp });
    cdp.send('Page.screencastFrameAck', { sessionId }).catch(() => {});
  });
  await cdp.send('Page.startScreencast', { format: 'jpeg', quality: 92 });
  return { ctx, page, cdp, frames };
}

let browserPromise;
function browser() {
  browserPromise ??= chromium.launch(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {});
  return browserPromise;
}

async function caption(page, text) {
  await page.evaluate((t) => {
    const el = document.getElementById('__caption');
    el.textContent = t;
    el.style.opacity = t ? '1' : '0';
  }, text);
}

async function click(page, locator, pause = 450) {
  await locator.waitFor({ state: 'visible' });
  const box = await locator.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 22 });
  await page.waitForTimeout(180);
  await page.mouse.down();
  await page.mouse.up();
  await page.waitForTimeout(pause);
}

async function hover(page, locator, pause = 600) {
  const box = await locator.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 22 });
  await page.waitForTimeout(pause);
}

async function typeInto(page, locator, text) {
  await click(page, locator, 150);
  await page.keyboard.type(text, { delay: 45 });
  await page.waitForTimeout(400);
}

/** A terminal window drawn over the app, for the one step that happens outside it. */
async function terminal(page, lines) {
  await page.evaluate(async (ls) => {
    const wrap = document.createElement('div');
    wrap.id = '__terminal';
    Object.assign(wrap.style, {
      position: 'fixed', inset: '0', zIndex: 2147483600, display: 'flex', alignItems: 'center', justifyContent: 'center',
      background: 'rgba(28,25,23,.55)',
    });
    wrap.innerHTML = `<div style="width:640px;border-radius:10px;overflow:hidden;box-shadow:0 20px 50px rgba(0,0,0,.4);font:14px/1.6 'JetBrains Mono',ui-monospace,monospace">
      <div style="background:#2d2a27;color:#a8a29e;padding:8px 12px;font:12px Inter,system-ui">Terminal</div>
      <pre id="__term" style="margin:0;min-height:220px;padding:14px 16px;background:#1c1917;color:#f4f3ee;white-space:pre-wrap"></pre></div>`;
    document.body.appendChild(wrap);
    const out = document.getElementById('__term');
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    for (const [kind, text] of ls) {
      if (kind === 'cmd') {
        out.textContent += '$ ';
        for (const ch of text) { out.textContent += ch; await wait(55); }
        out.textContent += '\n';
        await wait(400);
      } else {
        out.textContent += `${text}\n`;
        await wait(kind === 'slow' ? 900 : 300);
      }
    }
  }, lines);
  await page.waitForTimeout(1200);
  await page.evaluate(() => document.getElementById('__terminal').remove());
}

async function finish({ ctx, cdp, frames }, out, encode) {
  await cdp.send('Page.stopScreencast');
  await ctx.close();
  // Frames arrive only when the screen changes: hold each one until the next.
  const dir = fs.mkdtempSync(path.join(TMP, 'frames-'));
  const list = ['ffconcat version 1.0'];
  frames.forEach((f, i) => {
    const file = path.join(dir, `${String(i).padStart(5, '0')}.jpg`);
    fs.writeFileSync(file, Buffer.from(f.data, 'base64'));
    const next = frames[i + 1]?.t ?? f.t + 1;
    list.push(`file '${file}'`, `duration ${Math.max(next - f.t, 0.001).toFixed(4)}`);
  });
  list.push(`file '${path.join(dir, `${String(frames.length - 1).padStart(5, '0')}.jpg`)}'`);
  const listFile = path.join(dir, 'frames.txt');
  fs.writeFileSync(listFile, list.join('\n'));
  fs.mkdirSync(path.dirname(out), { recursive: true });
  const common = ['-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', listFile, '-an', '-r', '30'];
  const args = encode === 'clip'
    ? [...common, '-vf', 'scale=800:-2', '-c:v', 'libvpx-vp9', '-crf', '40', '-b:v', '0', '-row-mt', '1', out]
    : [...common, '-c:v', 'libx264', '-crf', '24', '-preset', 'slow', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', out];
  execFileSync('ffmpeg', args);
  console.log(`${path.relative(ROOT, out)}  ${(fs.statSync(out).size / 1024).toFixed(0)} KB`);
}

const CLIP = { width: 1000, height: 680, tourDone: true, hideVideos: true };
const addModelButton = (page) => page.getByRole('main').getByRole('button', { name: 'Add a model' }).first();
const dialog = (page) => page.getByRole('dialog', { name: 'Add a model' });

const recordings = {
  async installed() {
    const s = await session({ ...CLIP, servers: 'later' });
    const { page } = s;
    await caption(page, '1. Install Ollama, then download a model');
    await terminal(page, [
      ['cmd', 'ollama pull llama3.2:3b'],
      ['out', 'pulling manifest'],
      ['slow', 'pulling dde5aa3fc5ff: 100% ▕████████████████▏ 2.0 GB'],
      ['out', 'verifying sha256 digest'],
      ['out', 'writing manifest'],
      ['out', 'success'],
    ]);
    await caption(page, '2. In TGG, click Add a model');
    await click(page, addModelButton(page), 900);
    await caption(page, '3. Choose “A model you already have”');
    await click(page, dialog(page).getByRole('tab', { name: /A model you already have/ }), 900);
    await caption(page, '4. Click Check again — your models appear');
    await page.evaluate(() => { window.__ollamaRunning = true; });
    await click(page, dialog(page).getByRole('button', { name: /Check again/ }), 1200);
    await caption(page, '5. Pick a model. That’s it — it answers offline');
    await click(page, dialog(page).getByRole('button', { name: /llama3.2:3b/ }), 900);
    await caption(page, '');
    await hover(page, page.getByRole('button', { name: /llama3.2:3b/ }).first(), 1800);
    await finish(s, path.join(ROOT, 'src/assets/guides/installed.webm'), 'clip');
  },

  async offline() {
    const s = await session({ ...CLIP, servers: 'none' });
    const { page } = s;
    await caption(page, '1. Click Add a model');
    await click(page, addModelButton(page), 900);
    await caption(page, '2. Choose “Built-in offline model”');
    await click(page, dialog(page).getByRole('tab', { name: /Built-in offline model/ }), 900);
    await caption(page, '3. Click Download and use');
    await click(page, dialog(page).getByRole('button', { name: /Download and use/ }), 900);
    await caption(page, '4. Start the one-time ~700 MB download');
    await click(page, page.getByRole('button', { name: 'Download', exact: true }).last(), 300);
    await page.getByRole('button', { name: 'Close' }).waitFor({ timeout: 15000 });
    await caption(page, '5. Done — answers are written on this computer');
    await page.waitForTimeout(1400);
    await click(page, page.getByRole('button', { name: 'Close' }), 1800);
    await caption(page, '');
    await page.waitForTimeout(800);
    await finish(s, path.join(ROOT, 'src/assets/guides/offline.webm'), 'clip');
  },

  async cloud() {
    const s = await session({ ...CLIP, servers: 'none' });
    const { page } = s;
    await caption(page, '1. Click Add a model');
    await click(page, addModelButton(page), 900);
    await caption(page, '2. Choose “Cloud provider” and a provider');
    await click(page, dialog(page).getByRole('tab', { name: /Cloud provider/ }), 900);
    await hover(page, dialog(page).locator('select'), 700);
    await caption(page, '3. Get a key from the provider’s website');
    await click(page, dialog(page).getByRole('button', { name: /Get a key/ }), 1000);
    await caption(page, '4. Paste the key — it is kept in your OS keychain');
    await typeInto(page, dialog(page).getByPlaceholder('Paste your API key'), 'sk-ant-api03-demo-key-0000');
    await caption(page, '5. Save and use. Switch models any time from the chat box');
    await click(page, dialog(page).getByRole('button', { name: /Save and use/ }), 900);
    await caption(page, '');
    await hover(page, page.getByRole('button', { name: /claude/i }).first(), 1800);
    await finish(s, path.join(ROOT, 'src/assets/guides/cloud.webm'), 'clip');
  },

  async onboarding() {
    const s = await session({ width: 1280, height: 800, servers: 'ollama', tourDone: false, hideVideos: false });
    const { page } = s;
    const tour = page.getByRole('dialog', { name: /./ }).first();
    await page.waitForTimeout(3500);
    await click(page, tour.getByRole('button', { name: /Next/ }), 4500);
    await click(page, tour.getByRole('button', { name: /Next/ }), 5000);
    await click(page, tour.getByRole('button', { name: 'Get started' }), 1500);

    await caption(page, 'Ask a question — TGG shows how to add a model the moment you need one');
    const input = page.getByRole('textbox', { name: 'Message input' });
    await typeInto(page, input, 'How does the contract renew, and how can we exit?');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(1000);
    await caption(page, 'Each option has a short recording and steps');
    await page.waitForTimeout(3500);
    await click(page, dialog(page).getByRole('tab', { name: /Built-in offline model/ }), 3000);
    await click(page, dialog(page).getByRole('tab', { name: /Cloud provider/ }), 3000);
    await caption(page, 'Ollama is already installed here — use one of its models');
    await click(page, dialog(page).getByRole('tab', { name: /A model you already have/ }), 2500);
    await click(page, dialog(page).getByRole('button', { name: /llama3.2:3b/ }), 1200);

    await caption(page, 'Add a document to the chat');
    await click(page, page.getByRole('main').getByRole('button', { name: /Add document/ }).last(), 2000);
    await caption(page, 'Ask again — every step is shown, then the answer');
    await click(page, input, 200);
    await page.keyboard.press('Enter');
    await page.waitForTimeout(2500);
    await caption(page, '');
    await page.getByRole('button', { name: 'Copy answer' }).or(page.getByText('Termination for breach')).first().waitFor({ timeout: 20000 });
    await page.waitForTimeout(3000);
    await finish(s, path.join(ROOT, 'docs/media/onboarding.mp4'), 'full');
  },
};

const only = process.argv[2];
try {
  for (const [name, record] of Object.entries(recordings)) {
    if (only && name !== only) continue;
    await record();
  }
} finally {
  if (browserPromise) await (await browserPromise).close();
  fs.rmSync(TMP, { recursive: true, force: true });
}
