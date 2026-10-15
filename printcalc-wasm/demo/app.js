// printcalc in the browser. Every number comes from the wasm build of
// libprintcalc; this file is state, DOM and formatting only.
//
// The pure helpers (`slotsFromSummary`, `buildJob`, `parseMargins`,
// formatters) are exported so they can be exercised from Node without a
// DOM; `main()` wires them to the page.

import init, {
  parse_slice_bytes,
  parse_job,
  auto_job,
  calculate,
  builtin_printers,
  builtin_filaments,
  printcalc_wasm_version,
} from './pkg/printcalc_wasm.js';

export const DEFAULTS = {
  electricity_cost_per_kwh: 0.14,
  labor_rate_per_hour: 15,
  margin_percentages: [50, 60, 70],
  currency_symbol: '$',
};

const STORAGE_KEY = 'printcalc.defaults.v1';
const FILE_HINT = '.gcode, .bgcode, .gcode.3mf, .sl1 or .sl1s, or a job .toml plus the sliced file it names. Nothing leaves your browser.';

// ---------------------------------------------------------------- pure

/** 'resin' if any slot reports ml, else 'fdm'. */
export function processOf(summary) {
  const resin = summary.plates.some((p) => p.filament_usage.some((u) => (u.ml ?? 0) > 0));
  return resin ? 'resin' : 'fdm';
}

/** Mirrors SliceSummary::slots: one entry per used slot, totals across plates. */
export function slotsFromSummary(summary) {
  const map = new Map();
  for (const plate of summary.plates) {
    for (const u of plate.filament_usage) {
      const g = u.grams ?? 0;
      const ml = u.ml ?? 0;
      if (g <= 0 && ml <= 0) continue;
      const s = map.get(u.slot_id) ?? {
        slot_id: u.slot_id, material_type: null, color: null, profile_name: null,
        vendor: null, price: null, grams: 0, ml: 0,
      };
      for (const k of ['material_type', 'color', 'profile_name', 'vendor', 'price']) {
        if (s[k] == null && u[k] != null && u[k] !== '') s[k] = u[k];
      }
      s.grams += g;
      s.ml += ml;
      map.set(u.slot_id, s);
    }
  }
  return [...map.values()].sort((a, b) =>
    a.slot_id.localeCompare(b.slot_id, undefined, { numeric: true }));
}

/** "50, 60, 70" -> [50, 60, 70]; throws on anything that can't price. */
export function parseMargins(text) {
  const parts = String(text).split(',').map((s) => s.trim()).filter(Boolean);
  if (parts.length === 0) throw new Error('Enter at least one margin, like 50, 60, 70.');
  const margins = parts.map((s) => {
    const v = Number(s);
    if (!Number.isFinite(v)) throw new Error(`"${s}" isn't a number.`);
    if (v < 0 || v >= 100) throw new Error(`A margin must be 0 to 99, not ${s}. Margins are the share of the price that isn't cost.`);
    return v;
  });
  return margins;
}

/**
 * The Job JSON the library costs, from UI state.
 * state = { name, slice, plate|null, printer, materials: {slot: profile},
 *           quantity, laborMinutes, laborRate|null, hardware: [{name,cost,quantity}],
 *           electricity|null, margins|null, efficiency|null }
 */
export function buildJob(state) {
  const source = { summary: state.slice };
  if (state.plate != null) source.plate = state.plate;
  const materials = {};
  for (const [slot, profile] of Object.entries(state.materials)) {
    if (profile) materials[slot] = { profile };
  }
  // Same rule as Job::add_hardware: names unique, last wins, first position kept.
  const hardware = [];
  for (const item of state.hardware) {
    const name = item.name.trim();
    if (!name) continue;
    const entry = { name, cost: Number(item.cost) || 0, quantity: Math.max(1, Math.trunc(Number(item.quantity) || 1)) };
    const i = hardware.findIndex((h) => h.name === name);
    if (i >= 0) hardware[i] = entry; else hardware.push(entry);
  }
  const labor = { minutes: Math.max(0, Number(state.laborMinutes) || 0) };
  if (state.laborRate != null) labor.rate_per_hour = state.laborRate;
  const job = {
    name: state.name,
    source,
    printer: { profile: state.printer },
    materials,
    quantity: Math.max(1, Math.trunc(Number(state.quantity) || 1)),
    labor,
    hardware,
  };
  if (state.electricity != null) job.electricity_cost_per_kwh_override = state.electricity;
  if (state.margins != null) job.margin_percentages_override = state.margins;
  if (state.efficiency != null) job.material_efficiency_factor_override = state.efficiency;
  return job;
}

/**
 * Basename of the sliced file a default/full-tier job expects, or null
 * when the job carries its slice data (bundled tier).
 */
export function jobSliceName(job) {
  const path = job?.source?.path;
  if (typeof path !== 'string' || path === '') return null;
  return path.split(/[\\/]/).pop();
}

/** Case-insensitive basename comparison, so "Cube.GCODE" satisfies "cube.gcode". */
export function sameFileName(a, b) {
  const base = (n) => String(n ?? '').split(/[\\/]/).pop().toLowerCase();
  return base(a) !== '' && base(a) === base(b);
}

/**
 * A copy of `job` with `slice` embedded as its source, keeping the plate
 * selection. Turns a default/full-tier job into what `calculate` accepts
 * once the user has supplied the sliced file it named.
 */
export function attachSlice(job, slice) {
  const source = { summary: slice };
  if (typeof job?.source?.plate === 'number') source.plate = job.source.plate;
  return { ...job, source };
}

/**
 * The profile behind a ProfileRef: embedded ones come straight back; a
 * linked name is looked up in `presets` (the built-in list), which is the
 * only library a browser has. Throws when the name isn't a preset.
 */
export function resolveRef(ref, presets, what) {
  if (ref == null) return null;
  if (ref.profile) return ref.profile;
  const found = presets.find((p) => p.name === ref.ref);
  if (!found) {
    throw new Error(`${what} links "${ref.ref}", which isn't a built-in preset. There's no profile library in the browser; save the job with --retier full so the profile is embedded.`);
  }
  return { ...found };
}

export function formatDuration(seconds) {
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(h > 0 ? 2 : 1, '0');
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${h}h${mm}m${ss}s` : `${mm}m${ss}s`;
}

export function formatAmount(grams, ml) {
  if (grams > 0 && ml > 0) return `${grams.toFixed(2)}g + ${ml.toFixed(2)}ml`;
  if (grams > 0) return `${grams.toFixed(2)}g`;
  if (ml > 0) return `${ml.toFixed(2)}ml`;
  return '0g';
}

export const money = (cur, v) => `${cur}${v.toFixed(2)}`;

/** The CLI's exact/guess label. */
export const confidenceLabel = (c) => (c >= 0.999 ? 'exact' : 'guess');

/** Slot as the CLI describes it: "slot 1 (Generic PLA @System #26A69A)". */
export function slotLabel(usage) {
  const bits = [usage.profile_name ?? usage.material_type, usage.color].filter(Boolean);
  return bits.length ? `slot ${usage.slot_id} (${bits.join(' ')})` : `slot ${usage.slot_id}`;
}

export function loadDefaults(storage) {
  try {
    const raw = storage?.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULTS };
    const saved = JSON.parse(raw);
    return { ...DEFAULTS, ...saved };
  } catch {
    return { ...DEFAULTS };
  }
}

export function saveDefaults(storage, defaults) {
  try { storage?.setItem(STORAGE_KEY, JSON.stringify(defaults)); } catch { /* private window etc. */ }
}

// ---------------------------------------------------------------- DOM

const $ = (id) => document.getElementById(id);
const el = (tag, attrs = {}, ...children) => {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'class') node.className = v;
    else if (k === 'text') node.textContent = v;
    else if (k === 'html') node.innerHTML = v;
    else if (k.startsWith('on')) node.addEventListener(k.slice(2), v);
    else if (v === true) node.setAttribute(k, '');
    else if (v !== false && v != null) node.setAttribute(k, v);
  }
  for (const c of children) if (c != null) node.append(c);
  return node;
};

const WEAR_FIELDS = [
  ['purchase_price', 'Purchase price', 1],
  ['additional_upfront_cost', 'Extra upfront', 1],
  ['estimated_life_years', 'Life, years', 0.5],
  ['estimated_uptime_percent', 'Uptime 0 to 1', 0.05],
  ['annual_maintenance_cost', 'Maintenance /yr', 1],
  ['power_consumption_watts', 'Watts', 1],
  ['cost_buffer_factor', 'Buffer factor', 0.05],
  ['print_time_rate_override', 'Flat rate /h', 0.1],
];

async function main() {
  await init();
  $('version').textContent = `v${printcalc_wasm_version()}`;

  const presets = {
    printers: JSON.parse(builtin_printers()),
    filaments: JSON.parse(builtin_filaments()),
  };

  const defaults = loadDefaults(globalThis.localStorage);

  /** Everything the receipt is computed from. */
  const state = {
    name: 'job',
    slice: null,
    slots: [],          // from auto_job or slotsFromSummary
    detected: null,     // { printer, slots } from auto_job, or null when loaded from a job
    sliceName: null,    // filename the current slice came from
    loaded: null,       // { job, slice, wants } when state came from a job .toml
    pendingJob: null,   // { job, filename, wants }: a path-tier job waiting for its sliced file
    process: 'fdm',
    plate: null,
    printer: null,      // PrinterProfile or null
    printerCustom: false,
    materials: {},      // slot_id -> MaterialProfile
    quantity: 1,
    laborMinutes: 0,
    laborRate: null,
    hardware: [],
    efficiency: null,
    lastJob: null,
    lastBreakdown: null,
  };

  // ---- errors
  const showError = (msg) => { const e = $('error'); e.textContent = msg; e.hidden = !msg; };

  // ---- defaults panel
  $('electricity').value = defaults.electricity_cost_per_kwh;
  $('default-labor-rate').value = defaults.labor_rate_per_hour;
  $('margins').value = defaults.margin_percentages.join(', ');
  $('currency').value = defaults.currency_symbol;

  const readDefaults = () => {
    const d = { ...defaults };
    const e = Number($('electricity').value);
    if (Number.isFinite(e) && e >= 0) d.electricity_cost_per_kwh = e;
    const r = Number($('default-labor-rate').value);
    if (Number.isFinite(r) && r >= 0) d.labor_rate_per_hour = r;
    d.margin_percentages = parseMargins($('margins').value);
    d.currency_symbol = $('currency').value || '$';
    return d;
  };

  // ---- file loading
  const loadSliceBytes = async (bytes, filename) => {
    const slice = JSON.parse(parse_slice_bytes(bytes, filename));
    const auto = JSON.parse(auto_job(filename.replace(/\.[^.]+$/, ''), JSON.stringify(slice), null, null, 1.5));
    state.name = filename.replace(/\.(gcode\.3mf|[^.]+)$/, '');
    state.slice = slice;
    state.sliceName = filename;
    state.loaded = null;
    state.process = processOf(slice);
    state.detected = { printer: auto.printer, slots: auto.slots };
    state.slots = auto.slots.map((s) => s.usage);
    state.plate = null;
    state.printerCustom = false;
    state.printer = auto.printer ? auto.printer.profile : null;
    state.materials = {};
    for (const s of auto.slots) {
      const p = s.matched?.profile ?? s.from_file ?? null;
      state.materials[s.usage.slot_id] = p;
    }
    $('file-name').textContent = filename;
    renderSetup();
    recalc();
  };

  // A job .toml either carries its slice data (bundled tier) or names a
  // sliced file by path (default/full). The browser can't read that
  // path, so the job waits in `state.pendingJob` until the user supplies
  // the file: dropped alongside, dropped afterwards, or already open.
  const loadJobToml = async (text, filename) => {
    const job = JSON.parse(parse_job(text));
    const wants = jobSliceName(job);
    if (!wants) {
      applyJob(job, filename, null);
      return;
    }
    state.pendingJob = { job, filename, wants };
    if (state.slice && sameFileName(state.sliceName, wants)) {
      applyJob(attachSlice(job, state.slice), filename, state.sliceName);
      return;
    }
    $('file-name').textContent = filename;
    $('file-meta').textContent = `waiting for ${wants}`;
    renderPending();
  };

  // The sliced file a pending job asked for. Any sliced file is accepted
  // (the job may have been written on another machine, under another
  // name); a mismatch is noted in "What the file says" rather than refused.
  const attachSliceFile = (bytes, filename) => {
    const pending = state.pendingJob;
    const slice = JSON.parse(parse_slice_bytes(bytes, filename));
    applyJob(attachSlice(pending.job, slice), pending.filename, filename);
  };

  /** Loads a job whose source is embedded into the UI state. */
  function applyJob(job, filename, sliceName) {
    const printer = resolveRef(job.printer, presets.printers, 'The printer');
    const mats = job.materials ?? (job.material ? { 1: job.material } : {});
    const wants = state.pendingJob?.filename === filename ? state.pendingJob.wants : null;
    state.pendingJob = null;
    state.name = job.name;
    state.slice = job.source.summary;
    state.sliceName = sliceName;
    state.loaded = { job: filename, slice: sliceName, wants };
    state.process = processOf(state.slice);
    state.detected = null;
    state.slots = slotsFromSummary(state.slice);
    state.plate = typeof job.source.plate === 'number' ? job.source.plate : null;
    state.printer = printer;
    state.printerCustom = !presets.printers.some((p) => p.name === state.printer.name);
    state.materials = {};
    for (const s of state.slots) {
      const ref = mats[s.slot_id] ?? job.default_material ?? null;
      state.materials[s.slot_id] = resolveRef(ref, presets.filaments, `The material for slot ${s.slot_id}`);
    }
    state.quantity = job.quantity ?? 1;
    state.laborMinutes = job.labor?.minutes ?? 0;
    state.laborRate = job.labor?.rate_per_hour ?? null;
    state.hardware = (job.hardware ?? []).map((h) => ({ name: h.name, cost: h.cost, quantity: h.quantity ?? 1 }));
    state.efficiency = job.material_efficiency_factor_override ?? null;
    if (job.electricity_cost_per_kwh_override != null) $('electricity').value = job.electricity_cost_per_kwh_override;
    if (job.margin_percentages_override) $('margins').value = job.margin_percentages_override.join(', ');
    $('file-name').textContent = sliceName ? `${filename} + ${sliceName}` : filename;
    renderPending();
    renderSetup();
    recalc();
  }

  function renderPending() {
    const p = state.pendingJob;
    $('pending').hidden = !p;
    if (!p) return;
    $('pending-job').textContent = p.filename;
    $('pending-wants').textContent = p.wants;
  }
  $('pending-choose').addEventListener('click', () => $('file-input').click());
  $('pending-cancel').addEventListener('click', () => {
    state.pendingJob = null;
    renderPending();
    $('file-name').textContent = state.sliceName ?? state.loaded?.job ?? 'Drop a sliced file here';
    if (!state.slice) $('file-meta').textContent = FILE_HINT;
    else renderSetup();
  });

  const isToml = (file) => /\.toml$/i.test(file.name);

  // Several files at once are fine: the job .toml goes first so a sliced
  // file dropped with it lands in that job instead of starting a new one.
  const handleFiles = async (list) => {
    const files = [...(list ?? [])].filter(Boolean);
    if (files.length === 0) return;
    files.sort((a, b) => Number(isToml(b)) - Number(isToml(a)));
    showError('');
    try {
      for (const file of files) {
        if (isToml(file)) await loadJobToml(await file.text(), file.name);
        else if (state.pendingJob) attachSliceFile(new Uint8Array(await file.arrayBuffer()), file.name);
        else await loadSliceBytes(new Uint8Array(await file.arrayBuffer()), file.name);
      }
    } catch (e) {
      showError(String(e?.message ?? e));
    }
  };
  const handleFile = (file) => handleFiles(file ? [file] : []);

  for (const id of ['file-input', 'job-input']) {
    $(id).addEventListener('change', (ev) => { handleFiles(ev.target.files); ev.target.value = ''; });
  }
  const drop = $('drop');
  for (const evt of ['dragenter', 'dragover']) drop.addEventListener(evt, (e) => { e.preventDefault(); drop.classList.add('ring-2', 'ring-act'); });
  for (const evt of ['dragleave', 'drop']) drop.addEventListener(evt, (e) => { e.preventDefault(); drop.classList.remove('ring-2', 'ring-act'); });
  drop.addEventListener('drop', (e) => handleFiles(e.dataTransfer?.files));

  // ---- setup rendering (structure changes only on file load)
  function renderSetup() {
    $('setup').hidden = false;
    const s = state.slice;
    const totalSeconds = s.plates.reduce((a, p) => a + p.print_time_seconds, 0);
    const grams = state.slots.reduce((a, u) => a + u.grams, 0);
    const ml = state.slots.reduce((a, u) => a + u.ml, 0);
    $('file-meta').textContent = `${s.plates.length} plate${s.plates.length === 1 ? '' : 's'}, ${formatDuration(totalSeconds)}, ${formatAmount(grams, ml)}`;

    // Detected
    const dl = $('detected');
    dl.replaceChildren();
    const row = (k, v) => dl.append(el('dt', { class: 'text-muted', text: k }), el('dd', { class: 'font-mono' }, v));
    if (s.meta?.slicer) row('Slicer', s.meta.slicer);
    const model = s.meta?.printer_profile ?? s.plates.find((p) => p.printer_model)?.printer_model;
    if (state.detected) {
      const m = state.detected.printer;
      row('Printer', m
        ? `${m.matched_on} → ${m.profile.name} (${confidenceLabel(m.confidence)})`
        : (model ? `${model}: no matching preset, pick one below` : 'not recorded, pick one below'));
      for (const slot of state.detected.slots) {
        const what = slot.matched
          ? `${slot.matched.profile.name} (${confidenceLabel(slot.matched.confidence)})`
          : slot.from_file
            ? `priced from the file at ${money(currency(), slot.from_file.price)}/${state.process === 'resin' ? 'L' : 'kg'}`
            : 'no matching preset, pick one below';
        row(slotLabel(slot.usage), what);
      }
    } else {
      if (model) row('Printer', model);
      const from = state.loaded ?? {};
      row('Loaded from', from.slice ? `job ${from.job} with ${from.slice}` : `job ${from.job}; slice data is bundled in it`);
      if (from.wants && from.slice && !sameFileName(from.slice, from.wants)) {
        row('Note', `the job named ${from.wants}; costing ${from.slice} instead`);
      }
    }

    // Printer select
    const sel = $('printer-select');
    sel.replaceChildren();
    const kindMatch = presets.printers.filter((p) => p.kind === state.process);
    const groups = groupBy(kindMatch, (p) => p.name.split('-')[0]);
    sel.append(el('option', { value: '', text: 'Choose a printer', disabled: true }));
    for (const [vendor, list] of groups) {
      const og = el('optgroup', { label: vendorLabel(vendor) });
      for (const p of list) og.append(el('option', { value: p.name, text: p.name }));
      sel.append(og);
    }
    sel.append(el('option', { value: '__custom', text: 'Custom numbers' }));
    syncPrinterSelect();
    $('printer-note').textContent = state.process === 'resin' ? 'Resin file: resin printers only' : '';
    renderPrinterFields();

    // Plate picker
    const pw = $('plate-wrap');
    pw.hidden = s.plates.length <= 1;
    if (!pw.hidden) {
      const ps = $('plate');
      ps.replaceChildren(el('option', { value: '', text: 'All plates' }));
      for (const p of s.plates) ps.append(el('option', { value: String(p.plate_index), text: `Plate ${p.plate_index}` }));
      ps.value = state.plate == null ? '' : String(state.plate);
    }

    // Slots
    renderSlots();

    // Batch
    $('quantity').value = state.quantity;
    $('labor-minutes').value = state.laborMinutes;
    $('labor-rate').value = state.laborRate ?? '';
    $('efficiency').value = state.efficiency ?? '';
    renderHardware();
  }

  function syncPrinterSelect() {
    const sel = $('printer-select');
    if (!state.printer) sel.value = '';
    else if (state.printerCustom) sel.value = '__custom';
    else sel.value = presets.printers.some((p) => p.name === state.printer.name) ? state.printer.name : '__custom';
  }

  function renderPrinterFields() {
    const wrap = $('printer-fields');
    wrap.replaceChildren();
    if (!state.printer) return;
    for (const [key, label, step] of WEAR_FIELDS) {
      const input = el('input', {
        class: 'num', type: 'number', step, min: 0, id: `pf-${key}`,
        value: state.printer[key] ?? '',
        placeholder: key === 'print_time_rate_override' ? 'computed' : '',
        onchange: (e) => {
          const v = e.target.value === '' ? null : Number(e.target.value);
          if (key === 'print_time_rate_override') {
            if (v == null) delete state.printer.print_time_rate_override; else state.printer.print_time_rate_override = v;
          } else if (v != null && Number.isFinite(v)) {
            state.printer[key] = v;
          }
          markPrinterCustom();
          recalc();
        },
      });
      wrap.append(el('div', {}, el('label', { class: 'lbl', for: `pf-${key}`, text: label }), input));
    }
  }

  function markPrinterCustom() {
    if (!state.printerCustom) {
      state.printer = { ...state.printer, name: `${state.printer.name}-custom` };
      state.printerCustom = true;
      syncPrinterSelect();
    }
  }

  $('printer-select').addEventListener('change', (e) => {
    const v = e.target.value;
    if (v === '__custom') {
      if (!state.printer) state.printer = blankPrinter(state.process);
      markPrinterCustom();
      $('printer-fields').hidden = false;
      $('printer-toggle').setAttribute('aria-expanded', 'true');
    } else {
      const p = presets.printers.find((x) => x.name === v);
      if (p) { state.printer = { ...p }; state.printerCustom = false; }
    }
    renderPrinterFields();
    recalc();
  });
  $('printer-toggle').addEventListener('click', () => {
    const f = $('printer-fields');
    f.hidden = !f.hidden;
    $('printer-toggle').setAttribute('aria-expanded', String(!f.hidden));
  });

  function renderSlots() {
    const wrap = $('slots');
    wrap.replaceChildren();
    const kind = state.process === 'resin' ? 'resin' : 'filament';
    const options = presets.filaments.filter((f) => f.kind === kind);
    const groups = groupBy(options, (f) => f.name.split('-')[0]);
    const unit = kind === 'resin' ? 'L' : 'kg';

    for (const usage of state.slots) {
      const slotId = usage.slot_id;
      const det = state.detected?.slots.find((d) => d.usage.slot_id === slotId);
      const current = state.materials[slotId];

      const sel = el('select', { class: 'field', id: `mat-${slotId}` });
      sel.append(el('option', { value: '', text: 'Choose a material', disabled: true }));
      if (det?.from_file) sel.append(el('option', { value: '__file', text: `From the file: ${money(currency(), det.from_file.price)}/${unit}` }));
      for (const [vendor, list] of groups) {
        const og = el('optgroup', { label: vendorLabel(vendor) });
        for (const f of list) og.append(el('option', { value: f.name, text: `${f.name}  ${money('$', f.price)}/${unit}` }));
        sel.append(og);
      }
      sel.append(el('option', { value: '__custom', text: 'Custom price' }));

      const price = el('input', { class: 'num', type: 'number', min: 0, step: 0.01, id: `price-${slotId}`, 'aria-label': `Price per ${unit}` });
      const eff = el('input', { class: 'num', type: 'number', min: 1, step: 0.05, id: `eff-${slotId}`, 'aria-label': 'Waste allowance' });

      const syncInputs = () => {
        const m = state.materials[slotId];
        price.value = m ? m.price : '';
        eff.value = m ? m.efficiency_factor : '';
        if (!m) sel.value = '';
        else if (det?.from_file && m === det.from_file) sel.value = '__file';
        else if (presets.filaments.some((f) => f.name === m.name)) sel.value = m.name;
        else sel.value = '__custom';
      };
      syncInputs();

      sel.addEventListener('change', () => {
        const v = sel.value;
        if (v === '__file') state.materials[slotId] = det.from_file;
        else if (v === '__custom') state.materials[slotId] = customMaterial(state.materials[slotId], kind, usage);
        else state.materials[slotId] = { ...presets.filaments.find((f) => f.name === v) };
        syncInputs();
        recalc();
      });
      const edit = (key) => (e) => {
        const v = Number(e.target.value);
        if (!Number.isFinite(v)) return;
        const m = state.materials[slotId] ?? customMaterial(null, kind, usage);
        const isPreset = presets.filaments.some((f) => f.name === m.name && f.price === m.price && f.efficiency_factor === m.efficiency_factor);
        state.materials[slotId] = { ...m, name: isPreset || m === det?.from_file ? `${m.name}-custom` : m.name, [key]: v };
        syncInputs();
        recalc();
      };
      price.addEventListener('change', edit('price'));
      eff.addEventListener('change', edit('efficiency_factor'));

      const swatch = /^#[0-9a-f]{6}$/i.test(usage.color ?? '')
        ? el('span', { class: 'inline-block w-3 h-3 rounded-sm border border-rule align-[-1px] mr-1.5', style: `background:${usage.color}` })
        : null;
      wrap.append(el('div', { class: 'grid gap-2 sm:grid-cols-[1fr_7rem_6rem]' },
        el('div', { class: 'sm:col-span-3 text-sm' },
          swatch,
          el('span', { class: 'font-mono', text: slotLabel(usage) }),
          el('span', { class: 'text-muted', text: `  ${formatAmount(usage.grams, usage.ml)}` })),
        el('div', {}, el('label', { class: 'lbl', for: `mat-${slotId}`, text: 'Profile' }), sel),
        el('div', {}, el('label', { class: 'lbl', for: `price-${slotId}`, text: `Price /${unit}` }), price),
        el('div', {}, el('label', { class: 'lbl', for: `eff-${slotId}`, text: 'Waste ×' }), eff),
      ));
    }
  }

  function renderHardware() {
    const wrap = $('hardware');
    wrap.replaceChildren();
    if (state.hardware.length === 0) {
      wrap.append(el('p', { class: 'text-sm text-muted', text: 'None yet.' }));
      return;
    }
    const names = state.hardware.map((h) => h.name.trim());
    state.hardware.forEach((item, i) => {
      const dup = item.name.trim() && names.indexOf(item.name.trim()) !== i;
      const bind = (key, input) => input.addEventListener('input', () => {
        item[key] = key === 'name' ? input.value : Number(input.value);
        if (key === 'name') renderHardware(); else recalc();
      });
      const name = el('input', { class: `field ${dup ? 'border-bad' : ''}`, type: 'text', value: item.name, placeholder: 'M3 brass insert', 'aria-label': 'Item name', 'aria-invalid': dup ? 'true' : null });
      const cost = el('input', { class: 'num', type: 'number', min: 0, step: 0.01, value: item.cost, 'aria-label': 'Cost each' });
      const qty = el('input', { class: 'num', type: 'number', min: 1, step: 1, value: item.quantity, 'aria-label': 'Count per unit' });
      bind('name', name); bind('cost', cost); bind('quantity', qty);
      const remove = el('button', { class: 'btn-ghost', type: 'button', text: 'Remove', 'aria-label': `Remove ${item.name || 'item'}`,
        onclick: () => { state.hardware.splice(i, 1); renderHardware(); recalc(); } });
      wrap.append(el('div', { class: 'grid grid-cols-[1fr_5.5rem_4.5rem_auto] gap-2 items-center' }, name, cost, qty, remove));
      if (dup) wrap.append(el('p', { class: 'text-xs text-bad -mt-1', text: `"${item.name.trim()}" is listed twice; the later entry wins.` }));
    });
  }
  $('hardware-add').addEventListener('click', () => {
    state.hardware.push({ name: '', cost: 0, quantity: 1 });
    renderHardware();
    $('hardware').querySelector('input')?.focus();
    // Focus the new row's name field.
    const inputs = $('hardware').querySelectorAll('input[type=text]');
    inputs[inputs.length - 1]?.focus();
  });

  // ---- scalar inputs
  $('quantity').addEventListener('input', (e) => { state.quantity = Number(e.target.value) || 1; recalc(); });
  $('labor-minutes').addEventListener('input', (e) => { state.laborMinutes = Number(e.target.value) || 0; recalc(); });
  $('labor-rate').addEventListener('input', (e) => { state.laborRate = e.target.value === '' ? null : Number(e.target.value); recalc(); });
  $('efficiency').addEventListener('input', (e) => { state.efficiency = e.target.value === '' ? null : Number(e.target.value); recalc(); });
  $('plate').addEventListener('change', (e) => { state.plate = e.target.value === '' ? null : Number(e.target.value); recalc(); });
  for (const id of ['electricity', 'default-labor-rate', 'margins', 'currency']) {
    $(id).addEventListener('input', () => { recalc(); });
  }

  // ---- compute + receipt
  function currency() {
    return $('currency').value || defaults.currency_symbol || '$';
  }

  function recalc() {
    if (!state.slice) return;
    let d;
    try {
      d = readDefaults();
      saveDefaults(globalThis.localStorage, d);
      Object.assign(defaults, d);
    } catch (e) {
      renderReceiptMessage(String(e.message));
      return;
    }
    if (!state.printer) { renderReceiptMessage('Choose a printer to see the cost.'); return; }
    const missing = state.slots.filter((u) => !state.materials[u.slot_id]);
    if (missing.length) { renderReceiptMessage(`Choose a material for ${missing.map(slotLabel).join(', ')}.`); return; }

    const job = buildJob({
      name: state.name, slice: state.slice, plate: state.plate, printer: state.printer,
      materials: state.materials, quantity: state.quantity, laborMinutes: state.laborMinutes,
      laborRate: state.laborRate, hardware: state.hardware, electricity: null, margins: null,
      efficiency: state.efficiency,
    });
    try {
      const breakdown = JSON.parse(calculate(JSON.stringify(job), JSON.stringify(d)));
      state.lastJob = job;
      state.lastBreakdown = breakdown;
      renderReceipt(breakdown, d.currency_symbol);
      showError('');
    } catch (e) {
      renderReceiptMessage(String(e?.message ?? e));
    }
  }

  function renderReceiptMessage(msg) {
    $('receipt-empty').hidden = false;
    $('receipt-empty').textContent = msg;
    $('receipt').hidden = true;
    $('receipt-actions').hidden = true;
  }

  function renderReceipt(b, cur) {
    const r = $('receipt');
    r.replaceChildren();
    $('receipt-empty').hidden = true;
    r.hidden = false;
    $('receipt-actions').hidden = false;
    const batch = b.quantity > 1;

    const line = (label, right, cls = '') => el('div', { class: `flex items-baseline ${cls}` },
      el('span', { class: 'whitespace-pre', text: label }), el('span', { class: 'leader' }), el('span', { class: 'whitespace-nowrap', text: right }));
    const heading = (t) => el('div', { class: 'text-muted mt-4 mb-1 first:mt-0', text: t });

    if (b.plate_breakdown.length > 1) {
      r.append(heading('plates'));
      for (const p of b.plate_breakdown) r.append(line(`Plate ${p.plate_index}`, `${formatDuration(p.print_time_seconds)} · ${formatAmount(p.grams, p.ml)}`));
    }
    if (b.material_breakdown.length) {
      r.append(heading('material breakdown'));
      for (const m of b.material_breakdown) {
        const unit = m.unit === 'g' ? 'kg' : 'L';
        r.append(line(m.filament_name, `${m.amount.toFixed(2)}${m.unit} @ ${money(cur, m.unit_price)}/${unit} = ${money(cur, m.cost)}`, 'text-material'));
      }
    }
    if (b.hardware_breakdown.length) {
      r.append(heading('hardware breakdown'));
      for (const h of b.hardware_breakdown) {
        const each = batch ? `x${h.quantity} @ ${money(cur, h.unit_cost)} × ${b.quantity}` : `x${h.quantity} @ ${money(cur, h.unit_cost)}`;
        r.append(line(h.name, `${each} = ${money(cur, h.cost)}`, 'text-hardware'));
      }
    }
    r.append(heading(batch ? `summary  (${b.quantity} units)` : 'summary'));
    r.append(line('Material', money(cur, b.material_cost), 'text-material'));
    r.append(line('Printer wear', money(cur, b.wear_cost), 'text-machine'));
    r.append(line('Electricity', money(cur, b.electricity_cost), 'text-machine'));
    r.append(line('Labor', money(cur, b.labor_cost), 'text-labor'));
    r.append(line('Hardware', money(cur, b.hardware_cost), 'text-hardware'));
    r.append(el('div', { class: 'border-t border-rule my-1.5' }));
    r.append(el('div', { class: 'flex items-baseline text-act font-semibold text-lg' },
      el('span', { text: 'TOTAL' }), el('span', { class: 'leader' }), el('span', { text: money(cur, b.total_cost) })));
    if (batch) r.append(line('  per unit', money(cur, b.unit_cost), 'text-act'));

    const prices = el('div', { class: 'mt-4 text-act' });
    prices.append(el('div', { class: 'text-muted', text: `sale price at ${b.suggested_prices.map((p) => `${Math.round(p.margin_percent)}%`).join(' / ')} margin` }));
    prices.append(el('div', { class: 'flex flex-wrap gap-x-4 font-semibold', },
      ...b.suggested_prices.map((p) => el('span', { text: money(cur, p.price) }))));
    if (batch) {
      prices.append(el('div', { class: 'text-muted mt-1', text: 'per unit' }));
      prices.append(el('div', { class: 'flex flex-wrap gap-x-4' }, ...b.suggested_prices.map((p) => el('span', { text: money(cur, p.unit_price) }))));
    }
    r.append(prices);
    r.append(el('div', { class: 'mt-3 text-xs text-muted font-sans', text: `${formatDuration(b.print_time_seconds)} per unit at ${money(cur, b.print_time_rate)}/h machine rate` }));
  }

  // ---- downloads
  const download = (name, text) => {
    const blob = new Blob([text], { type: 'application/json' });
    const a = el('a', { href: URL.createObjectURL(blob), download: name });
    document.body.append(a); a.click(); a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  };
  $('dl-breakdown').addEventListener('click', () => state.lastBreakdown && download(`${state.name}-cost.json`, JSON.stringify(state.lastBreakdown, null, 2)));
  $('dl-job').addEventListener('click', () => state.lastJob && download(`${state.name}.job.json`, JSON.stringify(state.lastJob, null, 2)));

  // ---- helpers
  function groupBy(list, keyFn) {
    const m = new Map();
    for (const x of list) { const k = keyFn(x); if (!m.has(k)) m.set(k, []); m.get(k).push(x); }
    return [...m.entries()].sort(([a], [b]) => a.localeCompare(b));
  }
  function vendorLabel(prefix) {
    return { bambu: 'Bambu Lab', prusa: 'Prusa', prusament: 'Prusament', generic: 'Generic' }[prefix] ?? prefix;
  }
  function blankPrinter(process) {
    return {
      name: 'custom', kind: process, purchase_price: 800, additional_upfront_cost: 0, estimated_life_years: 5,
      estimated_uptime_percent: 0.5, annual_maintenance_cost: 80, power_consumption_watts: 160, cost_buffer_factor: 1.3,
    };
  }
  function customMaterial(from, kind, usage) {
    const base = from ?? { name: usage.profile_name ?? usage.material_type ?? `slot-${usage.slot_id}`, kind, price: 20, efficiency_factor: 1.5 };
    return { ...base, kind, name: base.name.endsWith('-custom') ? base.name : `${base.name}-custom` };
  }

  // ?load=<url> fetches a sliced file at startup — handy for demos and screenshots.
  const params = new URLSearchParams(location.search);
  const url = params.get('load');
  if (url) {
    try {
      const res = await fetch(url);
      if (!res.ok) throw new Error(`${res.status} fetching ${url}`);
      const name = decodeURIComponent(url.split('/').pop());
      await handleFile(new File([await res.arrayBuffer()], name));
    } catch (e) { showError(String(e.message ?? e)); }
  }
}

if (typeof document !== 'undefined') main();
