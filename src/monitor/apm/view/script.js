// Transactions view (monitor --apm), inserted after the call-graph script. It
// reads DATA.apm: { mode: "live", target } when a collector serves this page
// (the data comes from /api/series and /api/stream), or { mode: "static",
// target, all, routes } when the series were embedded at export time.
(function () {
  "use strict";
  const APM = (typeof DATA !== "undefined" && DATA.apm) || null;
  if (!APM) return;
  const LIVE = APM.mode === "live";
  const LAT = [
    { key: "p50", label: "p50", color: "var(--lat-p50)" },
    { key: "p95", label: "p95", color: "var(--lat-p95)" },
    { key: "p99", label: "p99", color: "var(--lat-p99)" },
  ];
  const ERR = [
    { key: "e4", label: "4xx", color: "var(--err-4xx)" },
    { key: "e5", label: "5xx / failed", color: "var(--err-5xx)" },
  ];
  const RPS = [{ key: "rps", label: "per second", color: "var(--apm-one)" }];
  const CPU = [{ key: "cpu_pct", label: "CPU", color: "var(--apm-one)" }];
  // Memory is charted in MiB so the axis lands on round megabytes.
  const MEM = [{ key: "mem_mib", label: "memory", color: "var(--apm-one)" }];
  const $ = id => document.getElementById(id);
  const store = {
    get(key) { try { return localStorage.getItem(key); } catch (e) { return null; } },
    set(key, value) { try { localStorage.setItem(key, value); } catch (e) {} },
  };

  // --- URL state: `a` (1 = transactions, 0 = profile), `ar` (route) and `at`
  // (range, seconds). The call-graph view owns the other keys and keeps these
  // when it rewrites.
  const RANGES = [300, 900, 3600, 21600, 86400, 604800];
  // A hand-edited link can carry a lone `%`, which decodeURIComponent throws
  // on; uncaught, it stopped the whole view before it drew anything.
  function safeDecode(text) {
    try { return decodeURIComponent(text); } catch (_) { return text; }
  }
  function readHash() {
    const out = {};
    (location.hash || "").replace(/^#/, "").split("&").forEach(kv => {
      if (!kv) return;
      const i = kv.indexOf("=");
      out[i < 0 ? kv : kv.slice(0, i)] = i < 0 ? "" : safeDecode(kv.slice(i + 1));
    });
    return out;
  }
  function writeHash() {
    const kept = (location.hash || "").replace(/^#/, "").split("&").filter(kv => kv && !/^(a|ar|at)=/.test(kv));
    kept.push("a=" + (state.on ? 1 : 0));
    if (state.route) kept.push("ar=" + encodeURIComponent(state.route));
    if (LIVE) kept.push("at=" + state.range);
    history.replaceState(null, "", "#" + kept.join("&"));
  }

  const hash = readHash();
  const state = {
    on: hash.a !== "0",
    // A range in the link wins over the one this browser used last.
    range: RANGES.includes(Number(hash.at)) ? Number(hash.at) : Number(store.get("elephc-apm-range")) || 900,
    route: hash.ar || "",
    // The kind of transaction the table shows ("" for every kind).
    kind: "",
    sort: { key: "time_pct", dir: -1 },
    data: null,
  };

  function fmtNs(ns) {
    if (ns == null) return "–";
    if (ns === 0) return "0";
    if (ns >= 1e9) return (ns / 1e9).toFixed(2) + " s";
    if (ns >= 1e6) return (ns / 1e6).toFixed(ns >= 1e8 ? 0 : 1) + " ms";
    if (ns >= 1e3) return (ns / 1e3).toFixed(0) + " µs";
    return ns + " ns";
  }
  const fmtRate = v => v == null ? "–" : v >= 100 ? v.toFixed(0) : v >= 10 ? v.toFixed(1) : v.toFixed(2);
  // A cron job's runs are a few an hour: say so instead of "0.00/s".
  const fmtRateUnit = v => v == null ? "–" : v >= 0.1 || v === 0 ? fmtRate(v) + "/s" : v * 60 >= 0.1 ? fmtRate(v * 60) + "/min" : fmtRate(v * 3600) + "/h";
  const fmtPct = v => v == null ? "–" : v === 0 ? "0%" : v < 0.1 ? "<0.1%" : v.toFixed(1) + "%";
  const fmtCount = n => n == null ? "–" : n.toLocaleString();
  // The unit follows the ROUNDED figure, so a value just under a unit reads
  // "1.00 GB", not "1024 MB".
  function fmtBytes(b) {
    if (b == null) return "–";
    const kb = b / 1024;
    if (Math.round(kb) < 1024) return Math.round(kb) + " KB";
    const mb = kb / 1024;
    const mbText = mb.toFixed(mb < 10 ? 1 : 0);
    if (+mbText < 1024) return mbText + " MB";
    return (mb / 1024).toFixed(2) + " GB";
  }
  const fmtPctCpu = v => v == null ? "–" : (v >= 10 ? v.toFixed(0) : +v.toFixed(1)) + "%";
  // CPU as a share of every core the service may use (`cpus`); a service that
  // did not say how many it has is read against one core.
  const cpuPct = (cores, cpus) => cores == null ? null : cores * 100 / (cpus > 0 ? cpus : 1);
  function fmtTime(ms, long) {
    const d = new Date(ms);
    const hm = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
    return long ? d.toLocaleDateString([], { month: "short", day: "numeric" }) + " " + hm : hm;
  }
  function fmtSpan(ms) {
    const s = ms / 1000;
    if (s < 120) return s.toFixed(0) + " s";
    if (s < 7200) return (s / 60).toFixed(0) + " min";
    if (s < 172800) return (s / 3600).toFixed(1) + " h";
    return (s / 86400).toFixed(1) + " d";
  }
  function niceMax(v) {
    if (!(v > 0)) return 1;
    const p = Math.pow(10, Math.floor(Math.log10(v)));
    for (const m of [1, 2, 2.5, 5, 10]) if (m * p >= v) return m * p;
    return 10 * p;
  }
  const escapeHtml = text => String(text).replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const SVG = "http://www.w3.org/2000/svg";
  function el(name, attrs, parent) {
    const node = document.createElementNS(SVG, name);
    for (const k in attrs) node.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(node);
    return node;
  }
  function key(container, series) {
    if (!container) return;
    container.innerHTML = "";
    for (const s of series) {
      const span = document.createElement("span");
      const i = document.createElement("i");
      i.style.background = s.color;
      span.append(i, s.label);
      container.appendChild(span);
    }
  }

  // One line chart: shared time axis, one y scale, gaps break the line, and a
  // hover crosshair that survives the redraw a new window triggers.
  function chart(host, points, series, opts) {
    host.innerHTML = "";
    const w = host.clientWidth, h = host.clientHeight;
    if (!w || !h) return;
    // Room on the right for the direct labels, sized to the longest one.
    const longest = series.length > 1 ? Math.max(...series.map(s => s.label.length)) : 0;
    const m = { l: 46, r: series.length > 1 ? 10 + longest * 6.5 : 10, t: 8, b: 22 };
    const usable = points.filter(p => !p.gap);
    const values = [];
    for (const p of usable) for (const s of series) if (p[s.key] != null) values.push(p[s.key]);
    if (!values.length) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.textContent = usable.length ? "nothing in this range" : "no data yet";
      host.appendChild(empty);
      return;
    }
    const max = Math.min(opts.cap || Infinity, niceMax(Math.max(...values) * 1.05));
    const t0 = state.data.now - state.data.range_ms, t1 = state.data.now;
    const x = t => m.l + (w - m.l - m.r) * (t - t0) / Math.max(1, t1 - t0);
    const y = v => m.t + (h - m.t - m.b) * (1 - v / max);
    const svg = el("svg", { viewBox: `0 0 ${w} ${h}`, role: "img", "aria-label": opts.label }, host);
    for (let k = 0; k <= 4; k++) {
      const v = max * k / 4, yy = y(v);
      el("line", { x1: m.l, x2: w - m.r, y1: yy, y2: yy, class: k ? "grid" : "baseline" }, svg);
      el("text", { x: m.l - 6, y: yy + 4, "text-anchor": "end", class: "axis" }, svg).textContent = opts.fmt(v);
    }
    const long = state.data.range_ms > 86400000;
    const ticks = Math.max(1, Math.min(4, Math.floor((w - m.l - m.r) / (long ? 120 : 70))));
    for (let k = 0; k <= ticks; k++) {
      const t = t0 + (t1 - t0) * k / ticks;
      el("text", { x: x(t), y: h - 6, "text-anchor": k === 0 ? "start" : k === ticks ? "end" : "middle", class: "axis" }, svg)
        .textContent = fmtTime(t, long);
    }
    for (const s of series) {
      let d = "", area = "", run = [];
      const flush = () => {
        if (!run.length) return;
        d += run.map((p, i) => (i ? "L" : "M") + x(p.t).toFixed(1) + " " + y(p[s.key]).toFixed(1)).join("");
        if (opts.area) {
          area += "M" + x(run[0].t).toFixed(1) + " " + y(0) + run.map(p => "L" + x(p.t).toFixed(1) + " " + y(p[s.key]).toFixed(1)).join("") +
            "L" + x(run[run.length - 1].t).toFixed(1) + " " + y(0) + "Z";
        }
        run = [];
      };
      for (const p of points) { if (p.gap || p[s.key] == null) flush(); else run.push(p); }
      flush();
      if (area) el("path", { d: area, fill: s.color, "fill-opacity": 0.14, stroke: "none" }, svg);
      el("path", { d, fill: "none", stroke: s.color, "stroke-width": 2, "stroke-linejoin": "round", "stroke-linecap": "round" }, svg);
      points.forEach((p, i) => {
        if (p.gap || p[s.key] == null) return;
        const joined = q => q && !q.gap && q[s.key] != null;
        if (!joined(points[i - 1]) && !joined(points[i + 1])) el("circle", { cx: x(p.t), cy: y(p[s.key]), r: 3, fill: s.color }, svg);
      });
    }
    if (series.length > 1) {
      const labels = series
        .map(s => ({ s, last: [...usable].reverse().find(p => p[s.key] != null) }))
        .filter(entry => entry.last)
        .map(entry => ({ label: entry.s.label, y: y(entry.last[entry.s.key]) + 4 }))
        .sort((a, b) => a.y - b.y);
      for (let i = 1; i < labels.length; i++) labels[i].y = Math.max(labels[i].y, labels[i - 1].y + 12);
      const overflow = labels.length ? labels[labels.length - 1].y - (h - m.b + 4) : 0;
      for (const label of labels) el("text", { x: w - m.r + 4, y: label.y - Math.max(0, overflow), class: "direct" }, svg).textContent = label.label;
    }
    const cross = el("line", { y1: m.t, y2: h - m.b, class: "cross", visibility: "hidden" }, svg);
    const dots = series.map(s => el("circle", { r: 4, fill: s.color, stroke: "var(--panel)", "stroke-width": 2, visibility: "hidden" }, svg));
    const tip = document.createElement("div");
    tip.className = "ptip";
    host.appendChild(tip);
    const hit = el("rect", { x: m.l, y: 0, width: Math.max(0, w - m.l - m.r), height: h, fill: "transparent" }, svg);
    const show = px => {
      let best = null, bestDx = Infinity;
      for (const p of usable) { const dx = Math.abs(x(p.t) - px); if (dx < bestDx) { bestDx = dx; best = p; } }
      if (!best) return;
      const bx = x(best.t);
      cross.setAttribute("x1", bx); cross.setAttribute("x2", bx); cross.setAttribute("visibility", "visible");
      let rows = "";
      series.forEach((s, i) => {
        const v = best[s.key];
        if (v == null) { dots[i].setAttribute("visibility", "hidden"); return; }
        dots[i].setAttribute("cx", bx); dots[i].setAttribute("cy", y(v)); dots[i].setAttribute("visibility", "visible");
        rows += `<div class="row"><i style="background:${s.color}"></i><span>${s.label}</span> ${opts.fmt(v)}</div>`;
      });
      tip.innerHTML = `<div class="t">${fmtTime(best.t, long)} · ${fmtCount(best.n)}</div>` + rows;
      tip.style.display = "block";
      tip.style.left = (bx + 12 + tip.offsetWidth > w ? bx - 12 - tip.offsetWidth : bx + 12) + "px";
      tip.style.top = "8px";
    };
    hit.addEventListener("mousemove", ev => { host.hoverX = ev.clientX - svg.getBoundingClientRect().left; show(host.hoverX); });
    hit.addEventListener("mouseleave", () => {
      host.hoverX = null;
      tip.style.display = "none";
      cross.setAttribute("visibility", "hidden");
      dots.forEach(dot => dot.setAttribute("visibility", "hidden"));
    });
    if (host.hoverX != null) show(host.hoverX);
  }

  function renderTiles(data) {
    const s = data.selected;
    const span = fmtSpan(data.measured_ms || data.range_ms);
    $("apm-k-rps").textContent = s ? fmtRateUnit(s.rps) : "–";
    $("apm-k-rps-sub").textContent = s ? fmtCount(s.n) + " over " + span : "nothing recorded";
    $("apm-k-p95").textContent = s && s.n ? fmtNs(s.p95_ns) : "–";
    $("apm-k-p95-sub").textContent = s && s.n ? "p50 " + fmtNs(s.p50_ns) + " · p99 " + fmtNs(s.p99_ns) : " ";
    $("apm-k-5xx").textContent = s && s.n ? fmtPct(s.server_error_pct) : "–";
    $("apm-k-5xx-sub").textContent = s && s.n ? "4xx " + fmtPct(s.client_error_pct) : " ";
    $("apm-k-mean").textContent = s && s.n ? fmtNs(s.mean_ns) : "–";
    $("apm-k-mean-sub").textContent = s && s.n && !data.route ? "across " + data.routes.length + " transaction" + (data.routes.length === 1 ? "" : "s") : " ";
  }

  // What kind of entry into the program a transaction is, by its name: a
  // web request, a script run, a message a consumer handled, a call a host
  // made into a library export, or a boundary named with --apm-transaction.
  const KINDS = [
    { key: "http", label: "HTTP" },
    { key: "msg", label: "Messages", prefix: "MSG " },
    { key: "call", label: "Calls", prefix: "CALL " },
    { key: "txn", label: "Boundaries", prefix: "TXN " },
    { key: "cli", label: "Scripts", prefix: "CLI " },
  ];
  function kindOf(name) {
    // Several sources share the page: a row reads "<source> · <transaction>"
    // (label_sources), and its kind is the transaction's.
    const at = name.lastIndexOf(" · ");
    if (at >= 0) name = name.slice(at + 3);
    const kind = KINDS.find(k => k.prefix && name.startsWith(k.prefix));
    return kind ? kind.key : "http";
  }

  // The kind chips, shown once the table holds more than one kind.
  function renderKinds(routes) {
    const present = KINDS.filter(k => routes.some(r => kindOf(r.route) === k.key));
    const host = $("apm-kinds");
    if (present.length < 2) {
      host.hidden = true;
      state.kind = "";
      return;
    }
    if (state.kind && !present.some(k => k.key === state.kind)) state.kind = "";
    host.hidden = false;
    host.innerHTML = "";
    for (const k of [{ key: "", label: "All" }, ...present]) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = k.label;
      button.setAttribute("aria-pressed", String(state.kind === k.key));
      button.addEventListener("click", () => { state.kind = k.key; renderTable(lastData); });
      host.appendChild(button);
    }
  }

  let lastData = null;
  function renderTable(data) {
    lastData = data;
    const body = document.querySelector("#apm-routes tbody"), foot = document.querySelector("#apm-routes tfoot");
    renderKinds(data.routes);
    const rows = data.routes.filter(r => !state.kind || kindOf(r.route) === state.kind);
    const { key: sortKey, dir } = state.sort;
    rows.sort((a, b) => (a[sortKey] < b[sortKey] ? -1 : a[sortKey] > b[sortKey] ? 1 : 0) * dir);
    const maxShare = Math.max(1, ...rows.map(r => r.time_pct));
    const cells = r => `
      <td class="route" title="${escapeHtml(r.route)}">${escapeHtml(r.route)}</td>
      <td>${fmtCount(r.n)}</td><td>${fmtRateUnit(r.rps)}</td><td>${fmtNs(r.mean_ns)}</td>
      <td>${fmtNs(r.p50_ns)}</td><td>${fmtNs(r.p95_ns)}</td><td>${fmtNs(r.p99_ns)}</td>
      <td>${fmtPct(r.client_error_pct)}</td><td class="${r.server_error_pct > 0 ? "err" : ""}">${fmtPct(r.server_error_pct)}</td>
      <td>${r.queries_per_txn ? r.queries_per_txn.toFixed(1) : "–"}</td>`;
    body.innerHTML = rows.length ? "" : `<tr><td colspan="11" style="text-align:left;color:var(--muted)">Nothing recorded in this range.</td></tr>`;
    for (const r of rows) {
      const tr = document.createElement("tr");
      if (r.route === data.route) tr.className = "selected";
      tr.innerHTML = cells(r) + `<td><span class="share">${fmtPct(r.time_pct)}<span class="mini"><i style="width:${(r.time_pct / maxShare * 100).toFixed(1)}%"></i></span></span></td>`;
      // An exported page carries the series of its busiest routes only: a
      // quieter row cannot be charted alone, and says so rather than
      // silently charting everything (review).
      if (!LIVE && !(APM.routes && APM.routes[r.route])) {
        tr.classList.add("fixed");
        tr.title = "Only the busiest routes can be charted alone in an exported page";
      } else {
        tr.addEventListener("click", () => setRoute(r.route === state.route ? "" : r.route));
      }
      body.appendChild(tr);
    }
    foot.innerHTML = rows.length > 1 ? `<tr>${cells({ ...data.total, route: "All" })}<td></td></tr>` : "";
    document.querySelectorAll("#apm-routes th").forEach(th => {
      th.setAttribute("aria-sort", th.dataset.key === sortKey ? (dir < 0 ? "descending" : "ascending") : "none");
    });
  }

  // The captured slow and failed requests: each one opens as its own exact
  // call graph in the Profile view.
  async function loadTraces() {
    if (!LIVE) return;
    const card = $("apm-s-traces");
    card.hidden = false;
    $("apm-nav-traces").hidden = false;
    const body = document.querySelector("#apm-traces tbody");
    if (!APM.captureEvery) {
      $("apm-traces-unit").textContent = "";
      body.innerHTML = `<tr><td colspan="7" class="hint">Start the collector with <code>--capture-every 5</code> to keep the exact profile of the slowest requests of each route and of every failure.</td></tr>`;
      setNav("traces", "off", "Not capturing");
      return;
    }
    try {
      const res = await fetch("/api/traces", { cache: "no-store" });
      const data = await res.json();
      $("apm-traces-unit").textContent = "one request captured every " + data.captureEvery + " s";
      const rows = data.traces.filter(t => !state.route || t.route === state.route);
      const failed = rows.filter(t => t.failed).length;
      setNav("traces", String(rows.length), failed ? failed + " failed" : "One captured every " + data.captureEvery + " s");
      body.innerHTML = rows.length ? "" : `<tr><td colspan="7" class="hint">No request captured${state.route ? " for this route" : ""} yet.</td></tr>`;
      for (const t of rows.slice(0, 20)) {
        const tr = document.createElement("tr");
        tr.innerHTML = `<td class="route" title="${escapeHtml(t.route)}">${escapeHtml(t.route)}</td>
          <td>${fmtNs(t.duration_ns)}</td>
          <td class="${t.failed ? "err" : ""}">${t.status == null ? "–" : t.status}${t.failed ? " ✗" : ""}</td>
          <td>${fmtTime(t.t, false)}</td><td>${fmtCount(t.functions)}</td><td>${fmtCount(t.queries)}</td>
          <td><a href="/?trace=${t.id}#a=0">Open profile →</a></td>`;
        body.appendChild(tr);
      }
    } catch (e) {}
  }

  // What a budget's metric and operator read as on the page.
  const METRIC_LABELS = {
    p50: "p50 latency", p95: "p95 latency", p99: "p99 latency", mean: "mean latency",
    error_pct: "5xx rate", client_error_pct: "4xx rate", rps: "throughput", queries: "queries per request",
  };
  const OPS = { "<=": "≤", ">=": "≥", "==": "=", "<": "<", ">": ">" };
  // A budget's figure with its unit, a space before words and milliseconds.
  function fmtBudget(v, unit) {
    const n = +(+v).toFixed(2);
    const u = (unit || "").trim();
    return u === "%" || u === "/s" ? n + u : n + (u ? " " + u : "");
  }

  // Budgets (--alert): the Checks panel, broken ones first; the sidebar
  // says how many are broken.
  async function loadAlerts() {
    if (!LIVE) return;
    try {
      const res = await fetch("/api/alerts", { cache: "no-store" });
      const data = await res.json();
      $("apm-s-budgets").hidden = false;
      $("apm-nav-budgets").hidden = false;
      const box = $("apm-budgets");
      box.innerHTML = "";
      if (!data.alerts.length) {
        box.innerHTML = `<div class="knone">No budget set. Start the collector with <code>--alert 'p95:GET /products&lt;=100'</code> to watch one.</div>`;
        $("apm-budgets-unit").textContent = "";
        setNav("budgets", "none", "No budget set");
        return;
      }
      const rows = [...data.alerts].sort((a, b) => Number(b.firing) - Number(a.firing));
      for (const a of rows) {
        const row = document.createElement("div");
        row.className = "krow" + (a.firing ? " firing" : a.value == null ? " idle" : "");
        row.title = a.rule;
        const value = a.value == null ? "no traffic" : fmtBudget(a.value, a.unit);
        // A budget that never changed state has no "since" to give.
        const since = a.since ? (a.firing ? "broken since " : "ok since ") + fmtTime(a.since, false) : "–";
        row.innerHTML = `<span class="kt">${a.firing ? "Broken" : a.value == null ? "Idle" : "OK"}</span>` +
          `<span class="kmetric">${escapeHtml(METRIC_LABELS[a.metric] || a.metric)}</span>` +
          `<span class="kroute">${escapeHtml(a.route == null ? "all routes" : a.route)}</span>` +
          `<span class="kbudget">${escapeHtml((OPS[a.op] || a.op) + " " + fmtBudget(a.threshold, a.unit))}</span>` +
          `<span class="kval">${escapeHtml(value)}</span>` +
          `<span class="ksince">${escapeHtml(since)}</span>`;
        box.appendChild(row);
      }
      box.insertAdjacentHTML("afterbegin", '<div class="krow khead"><span>State</span><span>Metric</span><span>Route</span>' +
        '<span>Budget</span><span>Now</span><span>Since</span></div>');
      const broken = rows.filter(a => a.firing).length;
      $("apm-budgets-unit").textContent = broken ? broken + " of " + rows.length + " broken" : "all " + rows.length + " kept";
      setNav("budgets", broken ? broken + " broken" : "OK", rows.length + " budget" + (rows.length === 1 ? "" : "s"), broken ? "fail" : "pass");
    } catch (e) {}
  }

  // --- the sections sidebar: a row per section with its headline figure.
  function setNav(section, value, sub, tone) {
    const pv = $("apm-v-" + section), m2 = $("apm-n-" + section);
    if (pv) { pv.textContent = value; pv.className = "pv" + (tone ? " " + tone : ""); }
    if (m2 && sub != null) m2.textContent = sub;
  }
  function renderNav(data) {
    const s = data.selected;
    setNav("overview", s ? fmtRateUnit(s.rps) : "–",
      s && s.n ? "p95 " + fmtNs(s.p95_ns) + " · " + fmtPct(s.server_error_pct) + " errors" : "Nothing recorded");
    setNav("transactions", String(data.routes.length), data.route ? "One charted: " + data.route : "Per route or script");
    const res = data.resources;
    $("apm-nav-resources").hidden = !res;
    if (res) setNav("resources", fmtPctCpu(cpuPct(res.cores, res.cores_available)), fmtBytes(res.memory_bytes) + " · " + res.processes + " process" + (res.processes === 1 ? "" : "es"));
    const spanLabel = LIVE ? "last " + fmtSpan(data.range_ms) : "recorded runs";
    $("apm-side-span").textContent = spanLabel;
  }
  const main = $("apm-main");
  // A section picked in the sidebar stays selected until the reader scrolls
  // by hand: the last sections cannot always reach the top of the view, and a
  // refresh that adds rows above would otherwise move the selection off it.
  let pinned = null;
  document.querySelectorAll("#apm-nav .navrow").forEach(row => row.addEventListener("click", () => {
    const target = $(row.dataset.section);
    if (!target) return;
    pinned = row.dataset.section;
    spy();
    target.scrollIntoView({ behavior: "smooth", block: "start" });
  }));
  const unpin = () => { pinned = null; };
  ["wheel", "touchstart", "keydown", "mousedown"].forEach(kind => main.addEventListener(kind, unpin, { passive: true }));
  // The row of the section at the top of the view is the selected one.
  function spy() {
    if (pinned && !$(pinned).hidden) {
      document.querySelectorAll("#apm-nav .navrow").forEach(row => row.classList.toggle("sel", row.dataset.section === pinned));
      return;
    }
    const top = main.getBoundingClientRect().top + 40;
    let current = null;
    document.querySelectorAll("#apm .sec").forEach(sec => {
      if (!sec.hidden && sec.getBoundingClientRect().top <= top) current = sec.id;
    });
    // Scrolled to the bottom: the last section is the one being read.
    if (main.scrollTop + main.clientHeight >= main.scrollHeight - 4) {
      const shown = [...document.querySelectorAll("#apm .sec")].filter(sec => !sec.hidden);
      if (shown.length) current = shown[shown.length - 1].id;
    }
    current = current || "apm-s-overview";
    document.querySelectorAll("#apm-nav .navrow").forEach(row => row.classList.toggle("sel", row.dataset.section === current));
  }
  main.addEventListener("scroll", spy, { passive: true });

  function render() {
    const data = state.data;
    if (!data || !state.on) return;
    const status = $("apm-status");
    const banner = $("apm-banner");
    if (data.error) {
      status.className = "status down"; status.querySelector("b").textContent = "unreachable";
      banner.className = "banner on";
      banner.innerHTML = "<b>Service unreachable.</b> " + escapeHtml(data.error) + " — the collector keeps retrying every second.";
    } else {
      status.className = data.live ? "status live" : "status";
      status.querySelector("b").textContent = data.live ? "live · " + data.target : (LIVE ? "stored history" : "recorded runs · " + data.target);
      banner.className = "banner";
    }
    $("apm-filter").hidden = !data.route;
    $("apm-filter-name").textContent = data.route || "";
    const points = data.points.map(p => {
      if (p.gap) return p;
      const n = p.n || 0;
      return { ...p, e4: n ? p.status[3] * 100 / n : null, e5: n ? p.status[4] * 100 / n : null };
    });
    renderTiles(data);
    key($("apm-l-lat"), LAT);
    key($("apm-l-err"), ERR);
    chart($("apm-c-rps"), points, RPS, { area: true, fmt: fmtRate, label: "Transactions per second over time" });
    chart($("apm-c-lat"), points, LAT, { fmt: fmtNs, label: "Latency percentiles over time" });
    chart($("apm-c-err"), points, ERR, { cap: 100, fmt: v => +v.toFixed(2) + "%", label: "Error rate over time" });
    renderTable(data);
    renderResources(data, points);
    renderNav(data);
    spy();
  }

  function renderResources(data, points) {
    const section = $("apm-s-resources");
    section.hidden = !data.resources;
    if (!data.resources) return;
    const available = data.resources.cores_available;
    const cpu = points.map(p => p.gap ? p : { ...p, cpu_pct: cpuPct(p.cpu, p.cpus || available) });
    const usable = cpu.filter(p => !p.gap && p.cpu_pct != null);
    const peakCpu = usable.length ? Math.max(...usable.map(p => p.cpu_pct)) : null;
    $("apm-u-cpu").textContent = available > 0 ? "% of all " + available + " cores" : "% of one core";
    const mems = points.filter(p => !p.gap && p.mem != null).map(p => p.mem);
    const lastMem = mems.length ? mems[mems.length - 1] : null;
    $("apm-l-cpu").textContent = "average " + fmtPctCpu(cpuPct(data.resources.cores, available)) + " · peak " + fmtPctCpu(peakCpu) +
      " · " + data.resources.cores.toFixed(2) + " cores busy on average";
    $("apm-l-mem").textContent = "now " + fmtBytes(lastMem) + " · peak " + fmtBytes(mems.length ? Math.max(...mems) : null) +
      " · " + data.resources.processes + " process" + (data.resources.processes === 1 ? "" : "es");
    // All cores busy is 100%, so the axis stops there — unless the service
    // used more: a quota of 1.5 CPUs counts as one whole core, and its full
    // use reads 150%, which a capped axis drew off the chart (review).
    chart($("apm-c-cpu"), cpu, CPU, { area: true, cap: available > 0 && !(peakCpu > 100) ? 100 : undefined, fmt: v => (v >= 10 ? v.toFixed(0) : +v.toFixed(1)) + "%", label: "CPU of the service over time, as a share of all its cores" });
    const mem = points.map(p => p.gap ? p : { ...p, mem_mib: p.mem == null ? null : p.mem / 1048576 });
    chart($("apm-c-mem"), mem, MEM, { area: true, fmt: v => v === 0 ? "0" : fmtBytes(v * 1048576), label: "Memory of the service over time" });
  }

  let inflight = false, again = false;
  async function load() {
    if (!LIVE) {
      // A route the page holds no series for (a link to it) charts nothing
      // alone: the selection is dropped rather than shown over every route.
      if (state.route && !(APM.routes && APM.routes[state.route])) state.route = "";
      state.data = (state.route && APM.routes[state.route]) || APM.all;
      render();
      return;
    }
    if (inflight) { again = true; return; }
    inflight = true;
    try {
      const q = "range=" + state.range + (state.route ? "&route=" + encodeURIComponent(state.route) : "");
      const res = await fetch("/api/series?" + q, { cache: "no-store" });
      state.data = await res.json();
      render();
      loadTraces();
      loadAlerts();
    } catch (e) {
      const status = $("apm-status");
      status.className = "status down"; status.querySelector("b").textContent = "collector offline";
    } finally {
      inflight = false;
      if (again) { again = false; load(); }
    }
  }

  function setRange(range) {
    state.range = range;
    store.set("elephc-apm-range", String(range));
    writeHash();
    document.querySelectorAll("#apm-ranges button").forEach(b => b.setAttribute("aria-pressed", String(Number(b.dataset.range) === range)));
    load();
  }
  function setRoute(route) {
    state.route = route;
    writeHash();
    load();
  }

  // --- the mode switch in the header: Transactions | Profile.
  const h1 = document.querySelector("header h1");
  const modes = document.createElement("div");
  modes.id = "apm-modes";
  modes.setAttribute("role", "group");
  modes.setAttribute("aria-label", "View");
  modes.innerHTML = '<button type="button" data-on="1">Transactions</button><button type="button" data-on="0">Profile</button>';
  if (h1) h1.insertAdjacentElement("afterend", modes);
  const hasProfile = (DATA.frames || []).some(f => f.nodes && f.nodes.length);
  const capture = $("apm-capture");
  function setMode(on) {
    state.on = on;
    document.body.classList.toggle("apm-on", on);
    modes.querySelectorAll("button").forEach(b => b.setAttribute("aria-pressed", String((b.dataset.on === "1") === on)));
    // A served page can ask the service for a profile; a static one already
    // holds the run's.
    // A store browsed or programs that push leave no service to ask (review).
    capture.classList.toggle("show", LIVE && APM.capture && !on);
    capture.classList.toggle("center", !hasProfile);
    writeHash();
    if (on) { load(); }
    else { window.dispatchEvent(new Event("resize")); }
  }
  modes.querySelectorAll("button").forEach(b => b.addEventListener("click", () => setMode(b.dataset.on === "1")));
  $("apm-capture-text").textContent = hasProfile
    ? "Showing the last captured profile."
    : "No profile yet: sample the service's CPU for a few seconds.";
  if (APM.trace) {
    // This page shows one captured request's exact profile.
    const t = APM.trace;
    $("apm-capture-text").innerHTML = `Exact profile of <b>${escapeHtml(t.route)}</b> — ${fmtNs(t.duration_ns)}` +
      (t.status == null ? "" : `, status ${t.status}`) + `, captured ${fmtTime(t.t, false)}. <a href="/#a=0">Sampled profiles</a>`;
    $("apm-capture-btn").style.display = "none";
  }
  $("apm-capture-btn").addEventListener("click", async () => {
    const button = $("apm-capture-btn");
    button.disabled = true;
    button.textContent = "Sampling…";
    try {
      const res = await fetch("/api/capture", { method: "POST", cache: "no-store" });
      if (!res.ok) throw new Error(await res.text());
      location.reload();
    } catch (e) {
      button.disabled = false;
      button.textContent = "Capture a profile";
      $("apm-capture-text").textContent = "Capture failed: " + (e.message || e);
    }
  });

  // Emptying the service's route table, when a service is attached. Two clicks
  // — the first arms, the second resets — rather than a browser dialog; the
  // arming wears off after a few seconds.
  const reset = $("apm-reset");
  if (LIVE && APM && APM.capture && !APM.trace) {
    reset.hidden = false;
    let disarm = null;
    reset.addEventListener("click", async () => {
      if (!reset.classList.contains("confirm")) {
        reset.classList.add("confirm");
        reset.textContent = "Click again to empty it";
        disarm = setTimeout(() => { reset.classList.remove("confirm"); reset.textContent = "Reset the route table"; }, 4000);
        return;
      }
      clearTimeout(disarm);
      reset.disabled = true;
      try {
        const res = await fetch("/api/reset-routes", { method: "POST", cache: "no-store" });
        const text = (await res.text()).trim();
        if (!res.ok) throw new Error(text);
        $("apm-routes-unit").textContent = text + " — it fills again from the next requests";
      } catch (e) {
        $("apm-routes-unit").textContent = "Reset failed: " + (e.message || e);
      }
      reset.disabled = false;
      reset.classList.remove("confirm");
      reset.textContent = "Reset the route table";
    });
  }

  if (LIVE) {
    $("apm-ranges").querySelectorAll("button").forEach(b => b.addEventListener("click", () => setRange(Number(b.dataset.range))));
  } else {
    // An export holds one span — every recorded run — so there is no range to pick.
    $("apm-ranges").style.display = "none";
  }
  $("apm-clear").addEventListener("click", () => setRoute(""));
  document.querySelectorAll("#apm-routes th").forEach(th => th.addEventListener("click", () => {
    const k = th.dataset.key;
    state.sort = { key: k, dir: state.sort.key === k ? -state.sort.dir : (k === "route" ? 1 : -1) };
    if (state.data) renderTable(state.data);
  }));
  let resizeTimer;
  window.addEventListener("resize", () => { clearTimeout(resizeTimer); resizeTimer = setTimeout(render, 120); });

  setMode(state.on);
  // Back/forward, or an edited address, changes the hash without reloading:
  // follow it, or the view and the URL disagree.
  window.addEventListener("hashchange", () => {
    const next = readHash();
    const on = next.a !== "0";
    const route = next.ar || "";
    if (route !== state.route) { state.route = route; if (on) load(); }
    // A link to another range is followed too (review).
    const range = Number(next.at);
    if (LIVE && RANGES.includes(range) && range !== state.range) setRange(range);
    if (on !== state.on) setMode(on);
  });
  if (LIVE) {
    setRange(RANGES.includes(state.range) ? state.range : 900);
    try {
      const events = new EventSource("/api/stream");
      // A long range reads days of stored history; a new window every second
      // changes it by a second, so it is refreshed every ten.
      let lastLoad = 0;
      events.onmessage = () => {
        if (!state.on) return;
        const now = Date.now();
        if (state.range > 3600 && now - lastLoad < 10000) return;
        lastLoad = now;
        load();
      };
    } catch (e) {
      setInterval(() => { if (state.on) load(); }, 2000);
    }
  }
})();
