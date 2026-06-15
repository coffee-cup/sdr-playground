// Shared synthetic spectrum + waterfall for the style-ref mockups. Fake data, no DSP.
// Each page sets window.THEME (palette) before loading this; class hooks are fixed:
//   #spec (spectrum canvas), #wf (waterfall canvas), #iwave (inspect waveform), #axis (dB gutter).
// Icons are injected into [data-icon] so every theme shares one intentional icon set.

const T = Object.assign(
  {
    cmap: [[0, 8, 8, 14], [0.4, 42, 31, 107], [0.65, 91, 79, 214], [0.82, 155, 123, 240], [1, 230, 222, 249]],
    line: "rgba(185,169,255,0.95)",
    fill: null,
    glow: null,
    grid: "rgba(255,255,255,0.05)",
    bg: "#0a0a12",
    axis: ["0", "−20", "−40", "−60", "−80"],
  },
  window.THEME || {}
);

const ICONS = {
  listen:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><circle cx="12" cy="12" r="1.8"/><path d="M8.5 8.5a5 5 0 0 0 0 7M15.5 8.5a5 5 0 0 1 0 7M6 6a8.5 8.5 0 0 0 0 12M18 6a8.5 8.5 0 0 1 0 12"/></svg>',
  library:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M6.5 4h11v16l-5.5-3.6L6.5 20z"/></svg>',
  recordings:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="2.2"/></svg>',
  settings:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M4 7h9M17 7h3M4 12h3M11 12h9M4 17h6M14 17h6"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="12" r="2"/><circle cx="12" cy="17" r="2"/></svg>',
  prev: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 6h2v12H7zM20 6v12l-9-6z"/></svg>',
  play: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>',
  pause: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 5h3.5v14H7zM13.5 5H17v14h-3.5z"/></svg>',
  next: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M15 6h2v12h-2zM4 6l9 6-9 6z"/></svg>',
  search:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><circle cx="11" cy="11" r="6.5"/><path d="m16 16 4 4"/></svg>',
};
document.querySelectorAll("[data-icon]").forEach((el) => {
  if (ICONS[el.dataset.icon]) el.innerHTML = ICONS[el.dataset.icon];
});

const lerp = (a, b, t) => a + (b - a) * t;
function cmap(t) {
  t = Math.max(0, Math.min(1, t));
  const c = T.cmap;
  for (let i = 1; i < c.length; i++) {
    if (t <= c[i][0]) {
      const a = c[i - 1], b = c[i], k = (t - a[0]) / (b[0] - a[0]);
      return [lerp(a[1], b[1], k), lerp(a[2], b[2], k), lerp(a[3], b[3], k)];
    }
  }
  const e = c[c.length - 1];
  return [e[1], e[2], e[3]];
}

const N = 512;
const peaks = [
  { c: 0.30, w: 0.013, a: 52 },
  { c: 0.50, w: 0.022, a: 66 },
  { c: 0.685, w: 0.009, a: 42 },
  { c: 0.83, w: 0.030, a: 48 },
];
function makeRow(t) {
  const a = new Float32Array(N);
  for (let i = 0; i < N; i++) a[i] = -96 + Math.random() * 5;
  for (const p of peaks) {
    const amp = p.a + Math.sin(t / 700 + p.c * 9) * 3;
    for (let i = 0; i < N; i++) {
      const x = i / N, d = (x - p.c) / p.w;
      a[i] = Math.max(a[i], -96 + amp * Math.exp(-d * d));
    }
  }
  return a;
}
const DBMIN = -96, DBMAX = -24;
const norm = (v) => Math.max(0, Math.min(1, (v - DBMIN) / (DBMAX - DBMIN)));

const spec = document.getElementById("spec");
const wf = document.getElementById("wf");
const iwave = document.getElementById("iwave");
let sctx, wctx, sw, sh, ww, wh;

function fit() {
  const dpr = window.devicePixelRatio || 1;
  let r = spec.getBoundingClientRect();
  spec.width = r.width * dpr; spec.height = r.height * dpr;
  sctx = spec.getContext("2d"); sctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  sw = r.width; sh = r.height;

  r = wf.getBoundingClientRect();
  wf.width = Math.max(2, r.width * dpr); wf.height = Math.max(2, r.height * dpr);
  wctx = wf.getContext("2d"); ww = wf.width; wh = wf.height;
  wctx.fillStyle = T.bg; wctx.fillRect(0, 0, ww, wh);
  for (let k = 0; k < wh; k++) pushRow(makeRow(k * 33));

  const ax = document.getElementById("axis");
  if (ax && !ax.dataset.done) {
    ax.dataset.done = 1;
    T.axis.forEach((label, idx) => {
      const s = document.createElement("span");
      s.textContent = label;
      s.style.top = (idx * 0.2 * sh + 4) + "px";
      ax.appendChild(s);
    });
  }
}

function pushRow(rowDb) {
  const img = wctx.getImageData(0, 0, ww, wh - 1);
  wctx.putImageData(img, 0, 1);
  const line = wctx.createImageData(ww, 1);
  for (let x = 0; x < ww; x++) {
    const bin = Math.floor((x / ww) * N);
    const [r, g, b] = cmap(norm(rowDb[bin]));
    const o = x * 4;
    line.data[o] = r; line.data[o + 1] = g; line.data[o + 2] = b; line.data[o + 3] = 255;
  }
  wctx.putImageData(line, 0, 0);
}

function tracePath(ctx, rowDb, w, h) {
  ctx.beginPath();
  for (let x = 0; x < w; x++) {
    const bin = Math.floor((x / w) * N);
    const y = h - norm(rowDb[bin]) * h;
    x ? ctx.lineTo(x, y) : ctx.moveTo(x, y);
  }
}
function drawSpec(rowDb) {
  sctx.clearRect(0, 0, sw, sh);
  sctx.strokeStyle = T.grid; sctx.lineWidth = 1;
  for (let f = 0.2; f < 1; f += 0.2) {
    const y = sh - f * sh;
    sctx.beginPath(); sctx.moveTo(0, y); sctx.lineTo(sw, y); sctx.stroke();
  }
  if (T.graticule) {
    for (let gx = 0.1; gx < 1; gx += 0.1) {
      const x = gx * sw;
      sctx.beginPath(); sctx.moveTo(x, 0); sctx.lineTo(x, sh); sctx.stroke();
    }
  }
  if (T.fill) {
    tracePath(sctx, rowDb, sw, sh);
    sctx.lineTo(sw, sh); sctx.lineTo(0, sh); sctx.closePath();
    sctx.fillStyle = T.fill; sctx.fill();
  }
  if (T.glow) { sctx.shadowColor = T.glow; sctx.shadowBlur = 8; }
  tracePath(sctx, rowDb, sw, sh);
  sctx.strokeStyle = T.line; sctx.lineWidth = T.lineWidth || 1.4; sctx.stroke();
  sctx.shadowBlur = 0;
}

function drawWave() {
  if (!iwave) return;
  const dpr = window.devicePixelRatio || 1, r = iwave.getBoundingClientRect();
  iwave.width = r.width * dpr; iwave.height = r.height * dpr;
  const c = iwave.getContext("2d"); c.setTransform(dpr, 0, 0, dpr, 0, 0);
  c.clearRect(0, 0, r.width, r.height);
  if (T.graticule) {
    c.strokeStyle = T.grid; c.lineWidth = 1;
    for (let gy = 0.25; gy < 1; gy += 0.25) {
      c.beginPath(); c.moveTo(0, gy * r.height); c.lineTo(r.width, gy * r.height); c.stroke();
    }
  }
  if (T.glow) { c.shadowColor = T.glow; c.shadowBlur = 7; }
  c.strokeStyle = T.line; c.lineWidth = T.lineWidth || 1.4;
  c.beginPath();
  for (let x = 0; x < r.width; x++) {
    const y = r.height / 2 + Math.sin(x / 11) * Math.sin(x / 53) * r.height * 0.33;
    x ? c.lineTo(x, y) : c.moveTo(x, y);
  }
  c.stroke(); c.shadowBlur = 0;
}

let t = 0, last = 0;
function frame(now) {
  if (now - last > 42) {
    last = now; t += 33;
    const r = makeRow(t);
    pushRow(r); drawSpec(r);
  }
  requestAnimationFrame(frame);
}
window.addEventListener("resize", () => { fit(); drawWave(); });
fit(); drawWave(); requestAnimationFrame(frame);
