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
    const kept = (location.hash || "").replace(/^#/, "").split("&").filter(kv => kv && !/^(a|ar|at|afrom|ato|acmp)=/.test(kv));
    kept.push("a=" + (state.on ? 1 : 0));
    if (state.route) kept.push("ar=" + encodeURIComponent(state.route));
    if (LIVE && state.compare) kept.push("acmp=" + state.compare);
    // A picked period, or else the range: a shared link opens the same span.
    if (LIVE && state.period) kept.push("afrom=" + state.period.from, "ato=" + state.period.to);
    else if (LIVE) kept.push("at=" + state.range);
    history.replaceState(null, "", "#" + kept.join("&"));
  }
  // `afrom`/`ato` in a link: a period of dates, in Unix milliseconds.
  function periodFrom(h) {
    const from = Number(h.afrom), to = Number(h.ato);
    return from > 0 && to > from ? { from, to } : null;
  }

  // `acmp` in a link: what the view is compared with, the same span earlier by how much.
  const COMPARE = { prev: null, day: 86400000, week: 604800000 };
  function compareFrom(h) {
    return Object.prototype.hasOwnProperty.call(COMPARE, h.acmp) ? h.acmp : "";
  }
  // How far back the compared span is: the span's own length for "the period before".
  const compareOffset = data => state.compare === "prev" ? data.range_ms : COMPARE[state.compare];
  // A change against the compared period, as a percentage tag; `lowerIsBetter` for latency
  // and errors; `fmt` writes the compared value in the tag's tooltip. Nothing when either
  // side is missing or the base is zero.
  function delta(now, before, lowerIsBetter, fmt) {
    if (!state.base || now == null || before == null || !(before > 0)) return "";
    const pct = (now - before) * 100 / before;
    const was = `title="was ${escapeHtml(fmt(before))}"`;
    if (Math.abs(pct) < 0.5) return `<span class="delta" ${was}>±0%</span>`;
    const worse = lowerIsBetter ? pct > 0 : pct < 0;
    const text = (pct > 0 ? "+" : "−") + (Math.abs(pct) >= 10 ? Math.abs(pct).toFixed(0) : Math.abs(pct).toFixed(1)) + "%";
    return `<span class="delta ${worse ? "worse" : "better"}" ${was}>${text}</span>`;
  }

  // A drag across a chart in progress, and whether a refresh was held back for it.
  const drag = { on: false, missed: false };
  function endDrag() {
    drag.on = false;
    if (drag.missed) { drag.missed = false; render(); }
  }

  const hash = readHash();
  // The section a link opens on (`as`), until it has been shown.
  let OPEN_SECTION = /^apm-s-[a-z]+$/.test(hash.as || "") ? hash.as : "";
  const state = {
    on: hash.a !== "0",
    // A range in the link wins over the one this browser used last.
    range: RANGES.includes(Number(hash.at)) ? Number(hash.at) : Number(store.get("elephc-apm-range")) || 900,
    // A period of dates picked on the page, which wins over the range: null for the last
    // `range` seconds.
    period: periodFrom(hash),
    // The period the view is compared with (`COMPARE`), "" for none, and its series.
    compare: compareFrom(hash),
    base: null,
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
    // Deploys inside the span: a dashed rule on every chart, named on the one that asks.
    for (const d of state.data.deploys || []) {
      if (d.t < t0 || d.t > t1) continue;
      const dx = x(d.t);
      el("line", { x1: dx, x2: dx, y1: m.t, y2: h - m.b, class: "deploy" }, svg);
      if (opts.deployLabels) el("text", { x: dx + 3, y: m.t + 9, class: "deploy-label" }, svg).textContent = d.label || "deploy";
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
      // A compared period's line is dashed, with no area under it.
      if (area && !s.dash) el("path", { d: area, fill: s.color, "fill-opacity": 0.14, stroke: "none" }, svg);
      el("path", { d, fill: "none", stroke: s.color, "stroke-width": 2, "stroke-linejoin": "round", "stroke-linecap": "round", "stroke-dasharray": s.dash ? "5 4" : "none" }, svg);
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
    // Dragging across a chart picks that stretch as the period: every view follows it.
    if (LIVE) {
      const at = px => t0 + (Math.min(Math.max(px, m.l), w - m.r) - m.l) * (t1 - t0) / Math.max(1, w - m.l - m.r);
      const band = el("rect", { y: m.t, height: Math.max(0, h - m.t - m.b), class: "zoom", visibility: "hidden" }, svg);
      let start = null;
      const px = ev => ev.clientX - svg.getBoundingClientRect().left;
      hit.addEventListener("mousedown", ev => { start = px(ev); drag.on = true; ev.preventDefault(); });
      hit.addEventListener("mousemove", ev => {
        if (start == null) return;
        const now = px(ev);
        band.setAttribute("x", Math.min(start, now)); band.setAttribute("width", Math.abs(now - start));
        band.setAttribute("visibility", "visible");
      });
      hit.addEventListener("mouseup", ev => {
        if (start == null) return;
        const end = px(ev);
        const from = Math.round(at(Math.min(start, end))), to = Math.round(at(Math.max(start, end)));
        start = null;
        band.setAttribute("visibility", "hidden");
        // A click is a click: only a drag of a few pixels, over a second at least, picks a period.
        if (Math.abs(x(to) - x(from)) >= 6 && to - from >= 1000) { drag.on = false; drag.missed = false; setPeriod({ from, to }); }
        else endDrag();
      });
      // Leaving the chart mid-drag gives the drag up.
      hit.addEventListener("mouseleave", () => {
        if (start == null) return;
        start = null;
        band.setAttribute("visibility", "hidden");
        endDrag();
      });
    }
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
        rows += `<div class="row"><i style="background:${s.color}"></i><span>${escapeHtml(s.label)}</span> ${opts.fmt(v)}</div>`;
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
    // The compared period's figures, each tile's value followed by its change.
    const b = state.base && state.base.selected;
    const span = fmtSpan(data.measured_ms || data.range_ms);
    $("apm-k-rps").innerHTML = s ? escapeHtml(fmtRateUnit(s.rps)) + delta(s.rps, b && b.rps, false, fmtRateUnit) : "–";
    $("apm-k-rps-sub").textContent = s ? fmtCount(s.n) + " over " + span : "nothing recorded";
    $("apm-k-p95").innerHTML = s && s.n ? escapeHtml(fmtNs(s.p95_ns)) + delta(s.p95_ns, b && b.n && b.p95_ns, true, fmtNs) : "–";
    $("apm-k-p95-sub").textContent = s && s.n ? "p50 " + fmtNs(s.p50_ns) + " · p99 " + fmtNs(s.p99_ns) : " ";
    // An error rate compared from zero has no percentage: the base's own rate says it.
    $("apm-k-5xx").innerHTML = s && s.n ? escapeHtml(fmtPct(s.server_error_pct)) + delta(s.server_error_pct, b && b.n && b.server_error_pct, true, fmtPct) : "–";
    $("apm-k-5xx-sub").textContent = s && s.n ? "4xx " + fmtPct(s.client_error_pct) + (b && b.n ? " · was " + fmtPct(b.server_error_pct) : "") : " ";
    $("apm-k-mean").innerHTML = s && s.n ? escapeHtml(fmtNs(s.mean_ns)) + delta(s.mean_ns, b && b.n && b.mean_ns, true, fmtNs) : "–";
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
    // Each row's figures in the compared period, for the rate and p95 changes.
    const before = new Map(((state.base && state.base.routes) || []).map(r => [r.route, r]));
    const cells = (r, b) => `
      <td class="route" title="${escapeHtml(r.route)}">${escapeHtml(r.route)}</td>
      <td>${fmtCount(r.n)}</td><td>${fmtRateUnit(r.rps)}${delta(r.rps, b && b.rps, false, fmtRateUnit)}</td><td>${fmtNs(r.mean_ns)}</td>
      <td>${fmtNs(r.p50_ns)}</td><td>${fmtNs(r.p95_ns)}${delta(r.p95_ns, b && b.n && b.p95_ns, true, fmtNs)}</td><td>${fmtNs(r.p99_ns)}</td><td title="${r.mem_p95_bytes ? "p95 " + fmtBytes(r.mem_p95_bytes) : ""}">${r.mem_mean_bytes ? fmtBytes(r.mem_mean_bytes) : "–"}</td>
      <td>${fmtPct(r.client_error_pct)}</td><td class="${r.server_error_pct > 0 ? "err" : ""}">${fmtPct(r.server_error_pct)}</td>
      <td>${r.queries_per_txn ? r.queries_per_txn.toFixed(1) : "–"}</td>`;
    body.innerHTML = rows.length ? "" : `<tr><td colspan="12" style="text-align:left;color:var(--muted)">Nothing recorded in this range.</td></tr>`;
    for (const r of rows) {
      const tr = document.createElement("tr");
      if (r.route === data.route) tr.className = "selected";
      tr.innerHTML = cells(r, before.get(r.route)) + `<td><span class="share">${fmtPct(r.time_pct)}<span class="mini"><i style="width:${(r.time_pct / maxShare * 100).toFixed(1)}%"></i></span></span></td>`;
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
    foot.innerHTML = rows.length > 1 ? `<tr>${cells({ ...data.total, route: "All" }, state.base && state.base.total)}<td></td></tr>` : "";
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
    try {
      // The captures of the span in view: memory's and, by date, the store's. Asked for
      // even when this collector does not capture: the store can hold another run's, a
      // script's runs or the sampled profiles taken from the page (review).
      const res = await fetch("/api/traces?" + spanQuery(), { cache: "no-store" });
      const data = await res.json();
      const every = data.captureEvery;
      $("apm-traces-unit").textContent = every ? "one request captured every " + every + " s" : "";
      const rows = data.traces.filter(t => !state.route || t.route === state.route);
      const failed = rows.filter(t => t.failed).length;
      if (!rows.length && !every) {
        body.innerHTML = `<tr><td colspan="7" class="hint">Start the collector with <code>--capture-every 5</code> to keep the exact profile of the slowest requests of each route and of every failure.</td></tr>`;
        setNav("traces", "off", "Not capturing");
        return;
      }
      setNav("traces", String(rows.length), failed ? failed + " failed" : every ? "One captured every " + every + " s" : "Stored captures");
      body.innerHTML = rows.length ? "" : `<tr><td colspan="7" class="hint">No request captured${state.route ? " for this route" : ""} ${state.period ? "in this period" : "yet"}.</td></tr>`;
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
    const spanLabel = !LIVE ? "recorded runs"
      : state.period ? fmtTime(state.period.from, true) + " → " + fmtTime(state.period.to, true)
      : "last " + fmtSpan(data.range_ms);
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
    // A drag across a chart is not redrawn under the pointer: the live refresh
    // rebuilt the charts every second and dropped the drag's start (the
    // missed refresh runs when the drag ends).
    if (drag.on) { drag.missed = true; return; }
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
    // The compared period, shifted onto this one: a dashed line beside throughput and p95.
    const compared = overlay(points, state.base, compareOffset(data));
    const was = COMPARE_LABEL[state.compare];
    const rps = state.base ? [...RPS, { key: "rps_b", label: was, color: "var(--muted)", dash: true }] : RPS;
    const lat = state.base ? [...LAT, { key: "p95_b", label: "p95 " + was, color: "var(--muted)", dash: true }] : LAT;
    renderTiles(data);
    renderDeploys(data);
    key($("apm-l-lat"), lat);
    key($("apm-l-err"), ERR);
    chart($("apm-c-rps"), compared, rps, { area: true, fmt: fmtRate, label: "Transactions per second over time", deployLabels: true });
    chart($("apm-c-lat"), compared, lat, { fmt: fmtNs, label: "Latency percentiles over time" });
    chart($("apm-c-err"), points, ERR, { cap: 100, fmt: v => +v.toFixed(2) + "%", label: "Error rate over time" });
    renderTable(data);
    renderScripts(data);
    renderApps(data);
    renderErrors(data);
    renderResources(data, points);
    renderNav(data);
    // A link naming a section (`as=apm-s-apps`) opens on it, once it has drawn: a shared
    // link to the Apps or the Scripts lands there, not on the overview.
    if (OPEN_SECTION) {
      const section = $(OPEN_SECTION);
      if (section && !section.hidden) {
        OPEN_SECTION = "";
        pinned = section.id;
        section.scrollIntoView({ block: "start" });
      }
    }
    spy();
  }

  // A cron job's runs are a few an hour, which a throughput chart flattens into
  // dots. Each script gets a strip with one mark per run instead: its height the
  // run's duration, a failed run full height and wider, so it reads without its
  // colour. A row charts its script; the charted script also gets its runs one
  // by one, with the last few listed.
  // How a mark's failed runs exited: `exit 255`, `exit 255 ×3 · exit 1`; plain "failed"
  // for runs stored before the codes were kept.
  function exitText(r) {
    const exits = (r.exits || []).filter(e => e[0] !== 0);
    if (!exits.length) return r.n > 1 ? r.failed + " of " + r.n + " failed" : "failed";
    const codes = exits.map(e => "exit " + e[0] + (e[1] > 1 ? " ×" + e[1] : "")).join(" · ");
    return r.n > r.failed ? codes + " (" + r.failed + " of " + r.n + ")" : codes;
  }

  function renderScripts(data) {
    const scripts = data.runs || [];
    const section = $("apm-s-scripts");
    const shown = scripts.length > 0;
    section.hidden = !shown;
    $("apm-nav-scripts").hidden = !shown;
    if (!shown) return;
    const to = data.now, from = to - data.range_ms;
    const span = Math.max(1, to - from);
    // The totals cover every run in the period, not only the last ones drawn (review).
    const runsOf = s => s.total != null ? s.total : s.runs.reduce((n, r) => n + r.n, 0);
    const failedOf = s => s.failed != null ? s.failed : s.runs.reduce((n, r) => n + r.failed, 0);
    const allFailed = scripts.reduce((n, s) => n + failedOf(s), 0);
    // The badge counts what needs a look, the failed runs; with none it counts the scripts.
    const many = scripts.length + " script" + (scripts.length === 1 ? "" : "s");
    setNav("scripts", String(allFailed || scripts.length),
      allFailed ? many + " · " + allFailed + " failed run" + (allFailed === 1 ? "" : "s") : many + ", every run succeeded", allFailed ? "fail" : "");
    const list = $("apm-scripts");
    list.innerHTML = "";
    for (const s of scripts) {
      const runs = runsOf(s), failed = failedOf(s);
      const top = Math.max(1, ...s.runs.map(r => r.mean));
      // The p95 of the runs' own durations, not of the folded minutes' means (review).
      const p95 = s.p95;
      const last = s.runs[s.runs.length - 1];
      const marks = s.runs.map(r => {
        const x = Math.max(0, Math.min(1, (r.t - from) / span)) * 476 + 2;
        // A stored minute can fold several runs: all of them failed reads as a failed
        // run; only some of them, as a run with a red cap.
        const bad = r.failed > 0 && r.failed >= r.n, some = r.failed > 0 && !bad;
        const h = bad ? 34 : Math.max(3, 30 * r.mean / top);
        const cap = some ? `<rect x="${(x - 2).toFixed(1)}" y="${(35 - h).toFixed(1)}" width="4" height="${Math.min(h, 5).toFixed(1)}" rx="2" class="bad"></rect>` : "";
        return `<rect x="${(x - (bad ? 3.5 : 2)).toFixed(1)}" y="${(35 - h).toFixed(1)}" width="${bad ? 7 : 4}" height="${h.toFixed(1)}" rx="2" class="${bad ? "bad" : "ok"}"><title>${escapeHtml(fmtTime(r.t, true))} · ${escapeHtml(fmtNs(r.mean))}${r.n > 1 ? " (mean of " + r.n + ")" : ""}${r.failed ? " · " + escapeHtml(exitText(r)) : ""}</title></rect>` + cap;
      }).join("");
      const row = document.createElement("button");
      row.type = "button";
      row.className = "script-row" + (state.route === s.name ? " sel" : "");
      row.innerHTML = `<span class="sn"><span class="n">${escapeHtml(s.name.replace(/^CLI /, ""))}</span>` +
        `<span class="m2">${fmtCount(runs)} run${runs === 1 ? "" : "s"}${s.points > s.runs.length ? " · the last " + s.runs.length + " drawn" : ""}</span></span>` +
        `<svg viewBox="0 0 480 36" preserveAspectRatio="none" role="img" aria-label="${escapeHtml(s.name)}: one mark per run"><line x1="0" x2="480" y1="35.5" y2="35.5"></line>${marks}</svg>` +
        `<span class="sf${failed ? " bad" : ""}">${failed ? failed + " failed" : "none failed"}</span>` +
        `<span class="sp">p95 ${fmtNs(p95)}</span>` +
        `<span class="sl">${escapeHtml(fmtTime(last.t, data.range_ms > 86400000))}</span>`;
      row.addEventListener("click", () => setRoute(s.name === state.route ? "" : s.name));
      list.appendChild(row);
    }
    renderScriptDetail(data, scripts.find(s => s.name === state.route), from, span);
  }

  // The charted script's runs one by one: a dot per run at its duration, a cross
  // for a failed one, and the last runs listed.
  function renderScriptDetail(data, script, from, span) {
    const host = $("apm-script-detail");
    host.hidden = !script;
    if (!script) { host.innerHTML = ""; return; }
    const runs = script.runs;
    const top = niceMax(Math.max(1, ...runs.map(r => r.mean)));
    const W = 1000, H = 200, L = 64, T = 12, B = 22;
    const x = t => L + Math.max(0, Math.min(1, (t - from) / span)) * (W - L - 8);
    const y = v => T + (H - T - B) * (1 - v / top);
    const grid = [0, 0.25, 0.5, 0.75, 1].map(f => `<line class="grid" x1="${L}" x2="${W - 8}" y1="${y(top * f).toFixed(1)}" y2="${y(top * f).toFixed(1)}"></line>` +
      `<text class="axis" x="${L - 8}" y="${(y(top * f) + 4).toFixed(1)}" text-anchor="end">${escapeHtml(fmtNs(top * f))}</text>`).join("");
    const path = runs.map((r, i) => (i ? "L" : "M") + x(r.t).toFixed(1) + " " + y(r.mean).toFixed(1)).join("");
    const dots = runs.map(r => {
      const cx = x(r.t), cy = y(r.mean);
      if (r.failed && r.failed < r.n) {
        return `<circle class="some" cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="5"><title>${escapeHtml(fmtTime(r.t, true))} · ${escapeHtml(fmtNs(r.mean))} mean · ${escapeHtml(exitText(r))}</title></circle>`;
      }
      return r.failed
        ? `<path class="bad" d="M${(cx - 6).toFixed(1)} ${(cy - 6).toFixed(1)}l12 12M${(cx + 6).toFixed(1)} ${(cy - 6).toFixed(1)}l-12 12"><title>${escapeHtml(fmtTime(r.t, true))} · ${escapeHtml(fmtNs(r.mean))} · ${escapeHtml(exitText(r))}</title></path>`
        : `<circle class="ok" cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="4.5"><title>${escapeHtml(fmtTime(r.t, true))} · ${escapeHtml(fmtNs(r.mean))}</title></circle>`;
    }).join("");
    const lastRuns = runs.slice(-8).reverse().map(r =>
      `<tr><td>${escapeHtml(fmtTime(r.t, true))}</td>` +
      `<td><span class="rs ${r.failed ? "bad" : "ok"}">${r.failed ? escapeHtml(exitText(r)) : (r.n > 1 ? r.n + " ok" : "ok")}</span></td>` +
      `<td>${escapeHtml(fmtNs(r.mean))}${r.n > 1 ? " mean" : ""}</td><td>${r.n ? fmtCount(Math.round(r.q / r.n)) : "–"}</td></tr>`).join("");
    host.innerHTML = `<h3>${escapeHtml(script.name)}<span class="unit">duration of each run</span></h3>` +
      `<div class="key"><span><i style="background:var(--apm-one)"></i>run</span><span><i style="background:var(--err-5xx)"></i>failed</span></div>` +
      `<svg viewBox="0 0 ${W} ${H}" class="runs" role="img" aria-label="Duration of each run of ${escapeHtml(script.name)}">${grid}` +
      `<path class="trend" d="${path}"></path>${dots}</svg>` +
      `<div class="table-wrap"><table class="last-runs"><thead><tr><th>Ended</th><th>Status</th><th>Duration</th><th>Queries</th></tr></thead>` +
      `<tbody>${lastRuns}</tbody></table></div>`;
  }

  // Programs that send to a listening collector — an app on a phone, a library in
  // its host, a consumer: their versions, newest first, with a flag on one that
  // fails clearly more than the version before it; then each instance, sending,
  // silent or gone.
  function renderApps(data) {
    const apps = data.apps;
    const section = $("apm-s-apps");
    const shown = !!(apps && apps.instances && apps.instances.length);
    section.hidden = !shown;
    $("apm-nav-apps").hidden = !shown;
    if (!shown) return;
    const sending = apps.instances.filter(i => i.state === "sending").length;
    const silent = apps.instances.filter(i => i.state === "silent").length;
    setNav("apps", String(sending), sending + " sending" + (silent ? " · " + silent + " silent" : ""), silent ? "fail" : "");
    // Versions keep one colour each, in the order they appeared: the newest the brand's.
    const versions = apps.versions.slice().sort((a, b) => b.first - a.first);
    const colour = {};
    // A version is its whole build id; its label, or when it has none its build time, is
    // what is shown: two unlabelled builds stay two rows (review).
    const buildName = v => v.build || (v.built ? "built " + fmtTime(v.built * 1000, true) : "unlabelled build");
    versions.forEach((v, i) => { colour[v.program + "\u0000" + v.id] = i < 3 ? "var(--ver-" + (i + 1) + ")" : "var(--muted)"; });
    const dot = v => `<i class="vdot" style="background:${colour[v.program + "\u0000" + v.id] || "var(--muted)"}"></i>`;
    const programs = new Set(versions.map(v => v.program));
    const name = v => (programs.size > 1 ? escapeHtml(v.program) + " " : "") + escapeHtml(buildName(v));
    const total = Math.max(1, versions.reduce((n, v) => n + v.instances, 0));
    // Each version's error rate over the span, the three newest in their colours: when a
    // new build started failing, against its deploy's dashed rule.
    const charted = versions.filter(v => (v.slices || []).some(s => s[0] > 0)).slice(0, 3);
    const slices = charted.length ? charted[0].slices.length : 0;
    const step = data.range_ms / Math.max(1, slices);
    const vpoints = Array.from({ length: slices }, (_, i) => {
      const p = { t: data.now - data.range_ms + (i + 0.5) * step, n: 0 };
      charted.forEach((v, k) => {
        const [calls, failed] = v.slices[i];
        p.n += calls;
        p["v" + k] = calls ? 100 * failed / calls : null;
      });
      return p;
    });
    const vseries = charted.map((v, k) => ({ key: "v" + k, label: (programs.size > 1 ? v.program + " " : "") + buildName(v), color: colour[v.program + "\u0000" + v.id] }));
    key($("apm-l-apps"), vseries);
    chart($("apm-c-apps"), vpoints, vseries, { cap: 100, fmt: v => +v.toFixed(2) + "%", label: "Error rate of each version over time", deployLabels: true });
    const vbody = document.querySelector("#apm-versions tbody");
    vbody.innerHTML = "";
    versions.forEach((v, i) => {
      // The version before it, of the same program: a new build that fails
      // twice as often, by half a point or more, is flagged with the ratio.
      const before = versions.slice(i + 1).find(o => o.program === v.program);
      const ratio = before && before.errors > 0 ? v.errors / before.errors : null;
      const flagged = before && v.errors >= 0.5 && v.errors - before.errors >= 0.5 && (ratio === null || ratio >= 2);
      const flag = flagged ? `<span class="vflag">${ratio ? "×" + ratio.toFixed(1) : "new"}</span>` : "";
      const tr = document.createElement("tr");
      tr.className = "fixed";
      tr.innerHTML = `<td><span class="vname">${dot(v)}<span><b>${name(v)}</b><small>first seen ${escapeHtml(fmtTime(v.first, true))}</small></span></span></td>` +
        `<td><span class="share"><span>${fmtCount(v.instances)}</span><span class="mini"><i style="width:${(100 * v.instances / total).toFixed(1)}%"></i></span></span></td>` +
        `<td>${fmtCount(v.n)}</td><td>${fmtRateUnit(v.rps)}</td>` +
        `<td${flagged ? ' class="err"' : ""}>${fmtPct(v.errors)}${flag}</td><td>${fmtNs(v.p95)}</td><td>${v.mem ? fmtBytes(v.mem) : "–"}</td>`;
      vbody.appendChild(tr);
    });
    const order = { sending: 0, silent: 1, gone: 2 };
    const instances = apps.instances.slice().sort((a, b) => order[a.state] - order[b.state] || b.last - a.last);
    const LISTED = 50;
    $("apm-instances-unit").textContent = sending + " sending · " + silent + " silent · " + (apps.instances.length - sending - silent) + " gone" +
      (instances.length > LISTED ? " · the " + LISTED + " most recent listed" : "");
    const ibody = document.querySelector("#apm-instances tbody");
    ibody.innerHTML = "";
    for (const inst of instances.slice(0, LISTED)) {
      const v = versions.find(o => o.program === inst.program && o.id === inst.id) || inst;
      const tr = document.createElement("tr");
      tr.className = "fixed";
      tr.innerHTML = `<td class="route" title="${escapeHtml(inst.source)}">${escapeHtml(inst.source)}</td>` +
        `<td><span class="istate ${inst.state}">${inst.state}</span></td>` +
        `<td><span class="vname">${dot(v)}${escapeHtml(v.id === inst.id && v !== inst ? buildName(v) : inst.build || "unlabelled build")}</span></td>` +
        `<td>${escapeHtml(fmtTime(inst.last, true))}</td><td>${fmtCount(inst.n)}</td>` +
        `<td${inst.errors >= 1 ? ' class="err"' : ""}>${fmtPct(inst.errors)}</td><td>${inst.mem ? fmtBytes(inst.mem) : "–"}</td>`;
      ibody.appendChild(tr);
    }
  }

  // The errors failed transactions ended with, grouped by exception and place: a row charts
  // its transaction, like the transactions table's.
  function renderErrors(data) {
    const errors = data.errors;
    const section = $("apm-s-errors");
    const shown = !!(errors && errors.list && errors.list.length);
    section.hidden = !shown;
    $("apm-nav-errors").hidden = !shown;
    if (!shown) return;
    const more = errors.groups > errors.list.length ? " · the " + errors.list.length + " most frequent shown" : "";
    $("apm-errors-unit").textContent = fmtCount(errors.total) + " in " + errors.groups + " group" + (errors.groups === 1 ? "" : "s") + more;
    setNav("errors", fmtCount(errors.total), errors.groups + " group" + (errors.groups === 1 ? "" : "s"), "err");
    const body = document.querySelector("#apm-errors tbody");
    body.innerHTML = "";
    for (const e of errors.list) {
      const tr = document.createElement("tr");
      // The class's own name first: the namespace is in the tooltip.
      const short = e.class.slice(e.class.lastIndexOf("\\") + 1);
      const file = e.at.replace(/^.*\//, "");
      tr.innerHTML = `<td class="route" title="${escapeHtml(e.class)}">${escapeHtml(short)}</td>` +
        `<td class="route" title="${escapeHtml(e.at)}">${escapeHtml(file)}</td>` +
        `<td class="route" title="${escapeHtml(e.route)}">${escapeHtml(e.route)}</td>` +
        `<td class="err">${fmtCount(e.n)}</td><td>${e.last ? fmtTime(e.last, data.range_ms > 86400000) : "–"}</td>` +
        `<td>${e.trace ? `<code title="${escapeHtml(e.trace)}">${escapeHtml(e.trace.slice(0, 8))}…</code>` : "–"}</td>`;
      if (LIVE) tr.addEventListener("click", () => setRoute(e.route === state.route ? "" : e.route));
      body.appendChild(tr);
    }
  }

  // What the compared line is called on the charts.
  const COMPARE_LABEL = { prev: "before", day: "a day earlier", week: "a week earlier" };
  // The compared period's throughput and p95 on this period's points: each point takes the
  // compared point `offset` earlier, within half a step. A step either side has none.
  function overlay(points, base, offset) {
    if (!base || !base.points || points.length < 2 || offset == null) return points;
    const half = (points[1].t - points[0].t) / 2;
    const earlier = base.points.filter(q => !q.gap);
    let j = 0;
    return points.map(p => {
      if (p.gap) return p;
      while (j < earlier.length && earlier[j].t + offset < p.t - half) j++;
      const q = earlier[j];
      if (!q || Math.abs(q.t + offset - p.t) > half) return p;
      return { ...p, rps_b: q.rps, p95_b: q.n ? q.p95 : null };
    });
  }

  // The deploys of the span: each one a button that compares the time since it with as
  // long before it.
  function renderDeploys(data) {
    const host = $("apm-deploys");
    const list = data.deploys || [];
    host.hidden = !LIVE || !list.length;
    if (host.hidden) return;
    const services = new Set(list.map(d => d.service));
    host.innerHTML = "<span>Deploys</span>";
    for (const d of list) {
      const button = document.createElement("button");
      button.type = "button";
      const name = d.label || "build of " + fmtTime(d.built * 1000, true);
      button.textContent = name + " · " + fmtTime(d.t, true) + (services.size > 1 ? " · " + d.service : "");
      button.title = "Compare the time since this deploy with as long before it";
      button.addEventListener("click", () => compareDeploy(d));
      host.appendChild(button);
    }
  }
  function compareDeploy(d) {
    const now = state.data ? state.data.now : Date.now();
    const length = Math.max(60000, Math.min(now - d.t, state.data ? state.data.range_ms : 900000));
    state.compare = "prev";
    setPeriod({ from: d.t, to: d.t + length });
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
      const routeQuery = state.route ? "&route=" + encodeURIComponent(state.route) : "";
      const res = await fetch("/api/series?" + spanQuery() + routeQuery, { cache: "no-store" });
      const data = await res.json();
      // The compared period: the same span, as far back as the comparison says.
      let base = null;
      if (state.compare) {
        const offset = compareOffset(data);
        const from = data.now - data.range_ms - offset, to = data.now - offset;
        const earlier = await fetch("/api/series?from=" + from + "&to=" + to + routeQuery, { cache: "no-store" });
        base = await earlier.json();
      }
      state.data = data;
      state.base = base;
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

  // The span the API is asked for: the picked period, or the last `range` seconds.
  function spanQuery() {
    return state.period ? "from=" + state.period.from + "&to=" + state.period.to : "range=" + state.range;
  }
  // A `datetime-local` value is the viewer's local time, to the minute.
  const toLocalInput = ms => {
    const d = new Date(ms - new Date(ms).getTimezoneOffset() * 60000);
    return d.toISOString().slice(0, 16);
  };
  function showPeriod() {
    const form = $("apm-period");
    form.classList.toggle("on", !!state.period);
    const span = state.period || { from: (state.data ? state.data.now : Date.now()) - state.range * 1000, to: state.data ? state.data.now : Date.now() };
    $("apm-from").value = toLocalInput(span.from);
    $("apm-to").value = toLocalInput(span.to);
    document.querySelectorAll("#apm-ranges button").forEach(b =>
      b.setAttribute("aria-pressed", String(!state.period && Number(b.dataset.range) === state.range)));
    $("apm-compare").value = state.compare;
    $("apm-compare-box").classList.toggle("on", !!state.compare);
  }
  function setRange(range) {
    state.range = range;
    state.period = null;
    store.set("elephc-apm-range", String(range));
    writeHash();
    showPeriod();
    load();
  }
  // A period of dates: the tiles, charts, table and captures all show it.
  function setPeriod(period) {
    state.period = period;
    writeHash();
    showPeriod();
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
    $("apm-period").addEventListener("submit", ev => {
      ev.preventDefault();
      const from = new Date($("apm-from").value).getTime(), to = new Date($("apm-to").value).getTime();
      if (from > 0 && to > from) setPeriod({ from, to });
    });
    $("apm-compare").addEventListener("change", ev => {
      state.compare = compareFrom({ acmp: ev.target.value });
      writeHash();
      showPeriod();
      load();
    });
  } else {
    // An export holds one span — every recorded run — so there is no range to pick, nor
    // another period to compare it with.
    $("apm-ranges").style.display = "none";
    $("apm-period").style.display = "none";
    $("apm-compare-box").style.display = "none";
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
    // A link to another range, or to a period, is followed too (review).
    const period = periodFrom(next);
    const range = Number(next.at);
    if (LIVE && period && (!state.period || period.from !== state.period.from || period.to !== state.period.to)) setPeriod(period);
    else if (LIVE && !period && RANGES.includes(range) && (range !== state.range || state.period)) setRange(range);
    const compare = compareFrom(next);
    if (LIVE && compare !== state.compare) { state.compare = compare; showPeriod(); if (on) load(); }
    if (on !== state.on) setMode(on);
  });
  if (LIVE) {
    if (state.period) setPeriod(state.period);
    else setRange(RANGES.includes(state.range) ? state.range : 900);
    try {
      const events = new EventSource("/api/stream");
      // A long range reads days of stored history; a new window every second
      // changes it by a second, so it is refreshed every ten.
      let lastLoad = 0;
      events.onmessage = () => {
        if (!state.on) return;
        const now = Date.now();
        // A period that ended does not change: nothing new to read.
        if (state.period && state.period.to < now - 60000) return;
        if ((state.period || state.range > 3600) && now - lastLoad < 10000) return;
        lastLoad = now;
        load();
      };
    } catch (e) {
      setInterval(() => { if (state.on) load(); }, 2000);
    }
  }
})();
