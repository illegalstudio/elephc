#!/usr/bin/env python3
"""Serve a small local dashboard for the benchmark time-series.

Reads .benchmarks/series.json, embeds it into a self-contained HTML page
(Chart.js + fonts from CDN), serves it on localhost with the stdlib http.server,
and opens a browser. No dependencies beyond Python 3; the charts/fonts need
internet access to their CDNs, but the data itself is inlined into the page.

Usage:

    python3 .benchmarks/dashboard.py            # serve + open browser
    python3 .benchmarks/dashboard.py --port 9000
    python3 .benchmarks/dashboard.py --no-open  # just serve
"""

from __future__ import annotations

import argparse
import json
import webbrowser
from functools import partial
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

SERIES_PATH = Path(__file__).resolve().parent / "series.json"

PAGE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>elephc · benchmark telemetry</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Instrument+Serif:ital@0;1&family=JetBrains+Mono:wght@400;500;700&display=swap" rel="stylesheet">
<script src="https://cdn.jsdelivr.net/npm/chart.js@4"></script>
<style>
  :root {
    --bg:        #08090b;
    --bg-tint:   #0e1217;
    --panel:     rgba(255,255,255,.022);
    --panel-bd:  rgba(255,255,255,.075);
    --fg:        #eef2f4;
    --muted:     #67737f;
    --faint:     rgba(255,255,255,.055);
    --c1:        #ff7a45;   /* sum_loop      */
    --c2:        #2bd4bd;   /* array_sum     */
    --c3:        #a98bff;   /* string_concat */
    --signal:    #f3b14b;   /* UI accent     */
  }
  * { box-sizing: border-box; }
  html { scroll-behavior: smooth; }
  body {
    margin: 0; min-height: 100vh; color: var(--fg);
    background: var(--bg);
    font-family: "JetBrains Mono", ui-monospace, Menlo, monospace;
    font-size: 13px; letter-spacing: -.01em;
    -webkit-font-smoothing: antialiased;
    overflow-x: hidden;
  }
  /* atmosphere: radial glows + a fine measurement grid */
  body::before {
    content: ""; position: fixed; inset: 0; z-index: -2; pointer-events: none;
    background:
      radial-gradient(900px 520px at 78% -8%, rgba(43,212,189,.10), transparent 60%),
      radial-gradient(820px 520px at 12% 4%,  rgba(255,122,69,.09),  transparent 58%),
      radial-gradient(1200px 800px at 50% 120%, rgba(169,139,255,.07), transparent 55%),
      var(--bg-tint);
  }
  body::after {
    content: ""; position: fixed; inset: 0; z-index: -1; pointer-events: none;
    opacity: .35;
    background-image:
      linear-gradient(var(--faint) 1px, transparent 1px),
      linear-gradient(90deg, var(--faint) 1px, transparent 1px);
    background-size: 46px 46px;
    -webkit-mask-image: radial-gradient(circle at 50% 30%, #000 0%, transparent 78%);
            mask-image: radial-gradient(circle at 50% 30%, #000 0%, transparent 78%);
  }
  /* film grain */
  .grain {
    position: fixed; inset: -50%; z-index: 0; pointer-events: none; opacity: .035;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='.85' numOctaves='2'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
  }
  .wrap { position: relative; z-index: 1; max-width: 1180px; margin: 0 auto; padding: 60px 40px 80px; }

  /* ---- masthead ---- */
  .masthead {
    display: flex; align-items: flex-end; justify-content: space-between;
    gap: 28px; padding-bottom: 26px; border-bottom: 1px solid var(--panel-bd);
  }
  .brand { display: flex; align-items: baseline; gap: 16px; }
  .brand .mark {
    font-family: "Instrument Serif", serif; font-size: 64px; line-height: .82;
    font-style: italic; letter-spacing: -.02em;
  }
  .brand .mark .e { color: var(--signal); }
  .brand .tag {
    text-transform: uppercase; letter-spacing: .42em; font-size: 10px;
    color: var(--muted); padding-bottom: 9px;
  }
  .report { text-align: right; font-size: 11px; color: var(--muted); line-height: 1.85; }
  .report b { color: var(--fg); font-weight: 500; }
  .report .live {
    display: inline-flex; align-items: center; gap: 7px; color: var(--c2);
  }
  .report .live::before {
    content: ""; width: 7px; height: 7px; border-radius: 50%;
    background: var(--c2); box-shadow: 0 0 0 0 rgba(43,212,189,.6);
    animation: pulse 2.4s ease-out infinite;
  }
  @keyframes pulse {
    0% { box-shadow: 0 0 0 0 rgba(43,212,189,.55); }
    70%,100% { box-shadow: 0 0 0 9px rgba(43,212,189,0); }
  }

  /* ---- readout gauges ---- */
  .readouts {
    display: grid; grid-template-columns: repeat(3, 1fr); gap: 18px;
    margin: 34px 0 30px;
  }
  .gauge {
    position: relative; overflow: hidden;
    background: var(--panel); border: 1px solid var(--panel-bd);
    border-radius: 14px; padding: 20px 22px 18px;
    backdrop-filter: blur(6px);
  }
  .gauge::before {
    content: ""; position: absolute; left: 0; top: 0; bottom: 0; width: 3px;
    background: var(--ck); box-shadow: 0 0 22px 1px var(--ck);
  }
  .gauge .name {
    display: flex; align-items: center; gap: 9px;
    font-size: 12px; color: var(--fg); margin-bottom: 16px;
  }
  .gauge .name .id { color: var(--muted); margin-left: auto; font-size: 10px; letter-spacing: .14em; text-transform: uppercase; }
  .gauge .reading { display: flex; align-items: baseline; gap: 8px; }
  .gauge .val {
    font-family: "Instrument Serif", serif; font-size: 56px; line-height: .85;
    letter-spacing: -.01em; font-variant-numeric: tabular-nums;
  }
  .gauge .unit { font-size: 13px; color: var(--muted); }
  .spark { margin: 16px 0 12px; height: 38px; }
  .spark svg { display: block; width: 100%; height: 38px; overflow: visible; }
  .gauge .foot { display: flex; align-items: center; justify-content: space-between; font-size: 11px; }
  .delta { display: inline-flex; align-items: center; gap: 6px; padding: 3px 9px; border-radius: 999px; font-weight: 500; }
  .delta.up   { color: var(--c1); background: rgba(255,122,69,.10); }
  .delta.down { color: var(--c2); background: rgba(43,212,189,.10); }
  .range { color: var(--muted); font-variant-numeric: tabular-nums; }

  /* ---- chart panels ---- */
  .panel {
    background: var(--panel); border: 1px solid var(--panel-bd);
    border-radius: 14px; padding: 22px 24px 20px; margin-bottom: 20px;
    backdrop-filter: blur(6px);
  }
  .panel header {
    display: flex; align-items: baseline; justify-content: space-between;
    margin-bottom: 18px; gap: 16px;
  }
  .panel h2 {
    margin: 0; font-family: "Instrument Serif", serif; font-size: 25px;
    font-weight: 400; letter-spacing: .01em;
  }
  .panel .hint { font-size: 11px; color: var(--muted); }
  .legend { display: flex; gap: 18px; flex-wrap: wrap; }
  .legend button {
    background: none; border: 0; color: var(--fg); cursor: pointer;
    font-family: inherit; font-size: 11px; display: inline-flex; align-items: center; gap: 8px;
    padding: 0; opacity: 1; transition: opacity .2s;
  }
  .legend button.off { opacity: .3; }
  .legend .swatch { width: 16px; height: 3px; border-radius: 2px; box-shadow: 0 0 9px 0 currentColor; }
  .canvas-box { position: relative; height: 360px; }

  footer {
    margin-top: 34px; padding-top: 20px; border-top: 1px solid var(--panel-bd);
    color: var(--muted); font-size: 11px; line-height: 1.9;
  }
  footer code { color: var(--fg); background: rgba(255,255,255,.05); padding: 1px 6px; border-radius: 5px; }

  ::selection { background: rgba(243,177,75,.28); }
  ::-webkit-scrollbar { width: 11px; height: 11px; }
  ::-webkit-scrollbar-thumb { background: rgba(255,255,255,.10); border-radius: 6px; border: 3px solid transparent; background-clip: content-box; }

  /* page-load reveal */
  @keyframes rise { from { opacity: 0; transform: translateY(16px); } to { opacity: 1; transform: none; } }
  .reveal { animation: rise .8s cubic-bezier(.2,.75,.2,1) both; }

  @media (max-width: 820px) {
    .wrap { padding: 38px 20px 60px; }
    .readouts { grid-template-columns: 1fr; }
    .masthead { flex-direction: column; align-items: flex-start; gap: 18px; }
    .report { text-align: left; }
    .brand .mark { font-size: 52px; }
  }
</style>
</head>
<body>
  <div class="grain"></div>
  <div class="wrap">
    <header class="masthead reveal">
      <div class="brand">
        <div class="mark"><span class="e">e</span>lephc</div>
        <div class="tag">benchmark&nbsp;telemetry</div>
      </div>
      <div class="report" id="report"></div>
    </header>

    <section class="readouts" id="readouts"></section>

    <section class="panel reveal" style="animation-delay:.32s">
      <header>
        <div>
          <h2>Median execution time</h2>
          <div class="hint">milliseconds · lower is better · compiled <code style="all:unset">elephc</code> binary</div>
        </div>
        <div class="legend" id="legend-abs"></div>
      </header>
      <div class="canvas-box"><canvas id="absChart"></canvas></div>
    </section>

    <section class="panel reveal" style="animation-delay:.44s">
      <header>
        <div>
          <h2>Distance from C</h2>
          <div class="hint">× slower than the <code style="all:unset">-O2</code> C baseline on the same runner</div>
        </div>
        <div class="legend" id="legend-ratio"></div>
      </header>
      <div class="canvas-box"><canvas id="ratioChart"></canvas></div>
    </section>

    <footer class="reveal" style="animation-delay:.56s" id="footer"></footer>
  </div>

<script>
const PAYLOAD = __DATA__;
const META = { sum_loop:   { color: "#ff7a45", label: "sum_loop" },
               array_sum:  { color: "#2bd4bd", label: "array_sum" },
               string_concat: { color: "#a98bff", label: "string_concat" } };

const series = PAYLOAD.series;
const cases  = Object.keys(META).filter(c => series.some(p => p.cases[c]));
const labels = series.map(p => p.date);
const fmtDate = d => d.slice(5);                       // MM-DD
const val = (p, c, k) => (p.cases[c] ? p.cases[c][k] : null);

/* ---------- masthead report ---------- */
document.getElementById("report").innerHTML =
  `<span class="live">recovered from CI</span><br>` +
  `<b>${PAYLOAD.points}</b> samples · branch <b>${PAYLOAD.branch}</b><br>` +
  `<b>${labels[0]}</b> → <b>${labels[labels.length-1]}</b><br>` +
  `${PAYLOAD.sampling}`;

document.getElementById("footer").innerHTML =
  `Source <code>.benchmarks/series.json</code>, recovered from GitHub Actions artifacts ` +
  `(<code>${PAYLOAD.workflow || "Benchmark Suite"}</code>). ` +
  `Refresh with <code>python3 .benchmarks/fetch_series.py</code>. ` +
  `Each point is one CI median on a shared runner — read the <em>shape</em>, not sub-millisecond deltas.`;

/* ---------- helpers ---------- */
function hexA(hex, a) {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${n>>16&255},${n>>8&255},${n&255},${a})`;
}
function sparkSVG(vals, color) {
  const w = 100, h = 38, lo = Math.min(...vals), hi = Math.max(...vals), sp = (hi - lo) || 1;
  const pts = vals.map((v, i) => [ i / (vals.length - 1) * w, h - ((v - lo) / sp) * (h - 6) - 3 ]);
  const line = pts.map(p => `${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(" ");
  const area = `0,${h} ${line} ${w},${h}`;
  const last = pts[pts.length - 1];
  const gid = "g" + Math.round(lo * 1e4) + color.slice(1);
  return `<svg viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">
    <defs><linearGradient id="${gid}" x1="0" x2="0" y1="0" y2="1">
      <stop offset="0" stop-color="${hexA(color,.32)}"/><stop offset="1" stop-color="${hexA(color,0)}"/>
    </linearGradient></defs>
    <polygon points="${area}" fill="url(#${gid})"/>
    <polyline points="${line}" fill="none" stroke="${color}" stroke-width="1.6"
      stroke-linejoin="round" stroke-linecap="round" style="filter:drop-shadow(0 0 4px ${hexA(color,.7)})"/>
    <circle cx="${last[0].toFixed(1)}" cy="${last[1].toFixed(1)}" r="2.4" fill="${color}"/>
  </svg>`;
}

/* ---------- readout gauges ---------- */
const readoutsEl = document.getElementById("readouts");
const counters = [];
cases.forEach((c, i) => {
  const vals = series.map(p => val(p, c, "elephc_ms")).filter(v => v != null);
  const first = vals[0], last = vals[vals.length - 1];
  const min = Math.min(...vals), max = Math.max(...vals);
  const pct = (last - first) / first * 100;
  const worse = pct > 0;
  const color = META[c].color;
  const el = document.createElement("div");
  el.className = "gauge reveal";
  el.style.setProperty("--ck", color);
  el.style.animationDelay = (0.10 + i * 0.09) + "s";
  el.innerHTML = `
    <div class="name"><span style="color:${color}">${META[c].label}</span>
      <span class="id">kernel ${String(i+1).padStart(2,"0")}</span></div>
    <div class="reading"><span class="val" data-to="${last.toFixed(2)}">0.00</span><span class="unit">ms</span></div>
    <div class="spark">${sparkSVG(vals, color)}</div>
    <div class="foot">
      <span class="delta ${worse ? "up" : "down"}">${worse ? "▲" : "▼"} ${Math.abs(pct).toFixed(0)}%
        <span style="color:var(--muted);font-weight:400">vs start</span></span>
      <span class="range">${min.toFixed(2)} – ${max.toFixed(2)}</span>
    </div>`;
  readoutsEl.appendChild(el);
  counters.push(el.querySelector(".val"));
});

/* count-up once the cards reveal */
function countUp(el) {
  const to = parseFloat(el.dataset.to), dur = 1050, t0 = performance.now();
  const ease = x => 1 - Math.pow(1 - x, 4);
  (function tick(now) {
    const k = Math.min(1, (now - t0) / dur);
    el.textContent = (to * ease(k)).toFixed(2);
    if (k < 1) requestAnimationFrame(tick);
  })(t0);
}
setTimeout(() => counters.forEach(countUp), 380);

/* ---------- Chart.js theming ---------- */
Chart.defaults.font.family = "'JetBrains Mono', monospace";
Chart.defaults.font.size = 11;
Chart.defaults.color = "#67737f";

/* per-series colored glow under each line */
const glow = {
  id: "glow",
  beforeDatasetDraw(chart, args) {
    const ds = chart.data.datasets[args.index];
    chart.ctx.save();
    chart.ctx.shadowColor = ds.borderColor;
    chart.ctx.shadowBlur = 14;
  },
  afterDatasetDraw(chart) { chart.ctx.restore(); },
};

function gradientFill(color) {
  return (ctx) => {
    const { chart } = ctx;
    const { ctx: c, chartArea } = chart;
    if (!chartArea) return hexA(color, .12);
    const g = c.createLinearGradient(0, chartArea.top, 0, chartArea.bottom);
    g.addColorStop(0, hexA(color, .26));
    g.addColorStop(1, hexA(color, 0));
    return g;
  };
}

function mkDataset(c, data) {
  const color = META[c].color;
  return {
    label: META[c].label, data,
    borderColor: color, backgroundColor: gradientFill(color),
    fill: true, tension: .32, borderWidth: 2,
    pointRadius: 0, pointHoverRadius: 5,
    pointBackgroundColor: color, pointHoverBorderColor: "#08090b", pointHoverBorderWidth: 2,
    spanGaps: true,
  };
}

const axes = (yTitle, suffix) => ({
  x: {
    ticks: { maxRotation: 0, autoSkipPadding: 28, callback(v) { return fmtDate(labels[v]); } },
    grid: { color: "rgba(255,255,255,.04)", drawTicks: false },
    border: { color: "rgba(255,255,255,.10)" },
  },
  y: {
    title: { display: true, text: yTitle, color: "#67737f", font: { size: 10 } },
    grace: "12%", grid: { color: "rgba(255,255,255,.045)", drawTicks: false },
    border: { display: false },
    ticks: { padding: 8, callback(v) { return v + suffix; } },
  },
});

const tooltip = (suffix) => ({
  backgroundColor: "rgba(8,9,11,.94)", borderColor: "rgba(255,255,255,.12)", borderWidth: 1,
  titleColor: "#eef2f4", bodyColor: "#c3ccd2", padding: 12, cornerRadius: 9,
  usePointStyle: true, boxPadding: 6, titleFont: { weight: "700" },
  callbacks: { label: (x) => `  ${x.dataset.label}  ${x.parsed.y?.toFixed(2)}${suffix}` },
});

function buildChart(canvasId, datasets, yTitle, suffix) {
  return new Chart(document.getElementById(canvasId), {
    type: "line",
    data: { labels: labels.map((_, i) => i), datasets },
    options: {
      responsive: true, maintainAspectRatio: false,
      interaction: { mode: "index", intersect: false },
      animation: { duration: 1200, easing: "easeOutQuart" },
      layout: { padding: { top: 6, right: 6 } },
      plugins: { legend: { display: false }, tooltip: tooltip(suffix) },
      scales: axes(yTitle, suffix),
    },
    plugins: [glow],
  });
}

const absChart = buildChart("absChart",
  cases.map(c => mkDataset(c, series.map(p => val(p, c, "elephc_ms")))), "ms", " ms");

const ratioChart = buildChart("ratioChart",
  cases.map(c => mkDataset(c, series.map(p => {
    const e = val(p, c, "elephc_ms"), cc = val(p, c, "c_ms");
    return (e != null && cc) ? e / cc : null;
  }))), "× C", "×");

/* ---------- custom interactive legends ---------- */
function buildLegend(containerId, chart) {
  const box = document.getElementById(containerId);
  chart.data.datasets.forEach((ds, i) => {
    const b = document.createElement("button");
    b.innerHTML = `<span class="swatch" style="background:${ds.borderColor};color:${ds.borderColor}"></span>${ds.label}`;
    b.onclick = () => {
      const vis = chart.isDatasetVisible(i);
      chart.setDatasetVisibility(i, !vis);
      b.classList.toggle("off", vis);
      chart.update();
    };
    box.appendChild(b);
  });
}
buildLegend("legend-abs", absChart);
buildLegend("legend-ratio", ratioChart);
</script>
</body>
</html>
"""


def build_page(series_path: Path) -> bytes:
    """Read the series JSON and inline it into the dashboard HTML."""
    data = json.loads(series_path.read_text())
    html = PAGE.replace("__DATA__", json.dumps(data))
    return html.encode("utf-8")


class Handler(BaseHTTPRequestHandler):
    """Serve the rendered dashboard for any GET; silence default logging."""

    def __init__(self, *args, page: bytes, **kwargs):
        self._page = page
        super().__init__(*args, **kwargs)

    def do_GET(self) -> None:
        """Return the embedded dashboard HTML for every path."""
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(self._page)))
        self.end_headers()
        self.wfile.write(self._page)

    def log_message(self, *args) -> None:
        """Suppress the noisy per-request stderr logging."""


def main() -> None:
    """Parse args, render the page, serve it, and optionally open a browser."""
    p = argparse.ArgumentParser(description="Local benchmark dashboard.")
    p.add_argument("--port", type=int, default=8000, help="Port to listen on (default: 8000).")
    p.add_argument("--no-open", action="store_true", help="Don't open a browser automatically.")
    args = p.parse_args()

    if not SERIES_PATH.exists():
        raise SystemExit(f"missing {SERIES_PATH} — run fetch_series.py first")

    page = build_page(SERIES_PATH)
    server = HTTPServer(("127.0.0.1", args.port), partial(Handler, page=page))
    url = f"http://127.0.0.1:{args.port}/"
    print(f"benchmark dashboard at {url}  (Ctrl-C to stop)")
    if not args.no_open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nstopped")


if __name__ == "__main__":
    main()
