// The editor page. It holds no edit state of its own: every change goes to the Rust core
// through `run()` (the same commands the HTTP API accepts), and the page redraws from the
// "state" events the core sends back. Only player things (playhead, selection, zoom) live here.

import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

type Rect = { x: number; y: number; w: number; h: number };
type Range = { start: number; end: number };
type FileInfo = { path: string; duration: number; width: number; height: number; fps: number; has_audio: boolean };
type Job = { id: number; kind: string; status: string; progress: number; output: string; error: string | null };
type State = {
  file: FileInfo | null;
  edit: { source: string; crop: Rect | null; deleted: Range[] } | null;
  kept: Range[];
  kept_duration: number;
  output_size: { w: number; h: number } | null;
  can_undo: boolean;
  can_redo: boolean;
  jobs: Job[];
  recording: { seconds: number } | null;
  preview?: string | null;
};

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const video = $<HTMLVideoElement>("video");
const overlay = $("overlay");
const cropEl = $("crop");
const track = $("track");
const scroll = $("scroll");

let S: State = { file: null, edit: null, kept: [], kept_duration: 0, output_size: null, can_undo: false, can_redo: false, jobs: [], recording: null };
let loadedPath = ""; // the video open in the editor
let loadedSrc = ""; // the file the player is showing (maybe its preview copy)
let draft: Rect | null = null; // crop being dragged, not yet sent
let ratio: number | null = null; // locked crop shape, from the menu
let selection: Range | null = null;
let zoom = 1;
let previewCrop = false;
const seenJobs = new Set<number>();

// ---------- talking to the core ----------

async function run(command: object): Promise<any> {
  try {
    return await invoke("cmd", { command });
  } catch (e) {
    toast(String(e), true);
    throw e;
  }
}

// ---------- debug log ----------
// Every click, key, player event and error goes to the core's debug log, a temporary folder
// deleted on quit (its path is `debug_log` in api.json). See crates/core/src/debuglog.rs.

const logQueue: object[] = [];
function logEvent(e: object) {
  logQueue.push({ ms: Math.round(performance.now()), ...e });
  if (logQueue.length === 1)
    setTimeout(() => invoke("cmd", { command: { cmd: "log", events: logQueue.splice(0) } }).catch(() => {}), 250);
}

/** A short readable name for an element: `button#play "Play"`. */
function describe(t: EventTarget | null): string {
  if (!(t instanceof Element)) return String(t);
  const id = t.id ? `#${t.id}` : "";
  const cls = typeof t.className === "string" && t.className ? `.${t.className.trim().split(/\s+/).join(".")}` : "";
  const text = (t.textContent ?? "").trim().slice(0, 40);
  return `${t.tagName.toLowerCase()}${id}${cls}${text ? ` "${text}"` : ""}`;
}

for (const type of ["pointerdown", "pointerup", "click", "dblclick", "contextmenu"])
  document.addEventListener(type, (e) => {
    const p = e as PointerEvent;
    logEvent({ event: type, target: describe(e.target), x: Math.round(p.clientX), y: Math.round(p.clientY), button: p.button });
  }, true);
document.addEventListener("keydown", (e) => {
  logEvent({ event: "keydown", key: e.key, mods: [e.metaKey && "meta", e.ctrlKey && "ctrl", e.altKey && "alt", e.shiftKey && "shift"].filter(Boolean), target: describe(e.target), repeat: e.repeat });
}, true);
for (const type of ["loadedmetadata", "play", "playing", "pause", "waiting", "stalled", "seeking", "seeked", "ended", "error", "emptied"])
  video.addEventListener(type, () => {
    logEvent({ event: `video.${type}`, time: +video.currentTime.toFixed(3), paused: video.paused, ready: video.readyState, error: video.error?.message });
  });
window.addEventListener("error", (e) => logEvent({ event: "js_error", message: e.message, where: `${e.filename}:${e.lineno}` }));
window.addEventListener("unhandledrejection", (e) => logEvent({ event: "js_rejection", reason: String(e.reason) }));

function applyState(st: State) {
  S = st;
  render();
}

// ---------- helpers ----------

function fmt(t: number): string {
  t = Math.max(0, t);
  const m = Math.floor(t / 60);
  const s = t - m * 60;
  const h = Math.floor(m / 60);
  const ss = s.toFixed(1).padStart(4, "0");
  return h ? `${h}:${String(m % 60).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const duration = () => S.file?.duration ?? 0;
const fullFrame = (): Rect => ({ x: 0, y: 0, w: S.file!.width, h: S.file!.height });
const currentCrop = (): Rect => draft ?? S.edit?.crop ?? fullFrame();

let toastTimer = 0;
function toast(msg: string, error = false, action?: { label: string; fn: () => void }) {
  const el = $("toast");
  el.replaceChildren(Object.assign(document.createElement("span"), { textContent: msg }));
  if (action) {
    const b = Object.assign(document.createElement("button"), { textContent: action.label });
    b.onclick = action.fn;
    el.append(b);
  }
  el.classList.toggle("error", error);
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (el.hidden = true), error ? 9000 : 6000);
}

function dialog(title: string, body: HTMLElement[], buttons: { label: string; primary?: boolean; fn?: () => void | boolean }[]) {
  const panel = $("dialog-body");
  const h = Object.assign(document.createElement("h2"), { textContent: title });
  const row = document.createElement("div");
  row.className = "row";
  for (const b of buttons) {
    const el = Object.assign(document.createElement("button"), { textContent: b.label });
    if (b.primary) el.className = "primary";
    el.onclick = () => {
      if (b.fn?.() === false) return;
      $("dialog").hidden = true;
    };
    row.append(el);
  }
  panel.replaceChildren(h, ...body, row);
  $("dialog").hidden = false;
}

function field(label: string, input: HTMLElement): HTMLElement {
  const l = document.createElement("label");
  l.append(label, input);
  return l;
}

function numInput(v: number): HTMLInputElement {
  return Object.assign(document.createElement("input"), { type: "number", value: String(v), min: "0", step: "2" });
}

// ---------- rendering ----------

function render() {
  const has = !!S.file;
  $("empty").hidden = has;
  $("viewer").hidden = !has;
  $("timeline").hidden = !has;
  $("edit-tools").hidden = !has;

  if (S.file && S.file.path !== loadedPath) loadFile(S.file);
  else if (S.file && playerSrc() !== loadedSrc) switchSource();
  if (!S.file) loadedPath = loadedSrc = "";

  $<HTMLButtonElement>("undo").disabled = !S.can_undo;
  $<HTMLButtonElement>("redo").disabled = !S.can_redo;
  $<HTMLButtonElement>("save-fast").disabled = !!S.edit?.crop;
  if (S.output_size) $("size").textContent = `${S.output_size.w} × ${S.output_size.h}`;
  $("result").textContent = has ? `Result: ${fmt(S.kept_duration)}` : "";

  $("rec-banner").hidden = !S.recording;
  $("record").hidden = !!S.recording;
  if (S.recording) $("rec-time").textContent = fmt(S.recording.seconds).replace(/\.\d$/, "");

  renderCrop();
  renderCuts();
  renderSelection();
  renderJobs();
}

/** What the player shows: the smaller preview copy of a big video once it's ready, else the video itself. */
function playerSrc(): string {
  return S.preview ?? S.file?.path ?? "";
}

/** Swaps in the preview copy when it's ready, keeping the playhead and play/pause. */
function switchSource() {
  const [t, paused] = [video.currentTime, video.paused];
  loadedSrc = playerSrc();
  logEvent({ event: "switch_source", src: loadedSrc, time: t });
  video.src = convertFileSrc(loadedSrc);
  video.addEventListener("loadedmetadata", () => {
    video.currentTime = t;
    if (!paused) video.play().catch(() => {});
  }, { once: true });
}

async function loadFile(f: FileInfo) {
  loadedPath = f.path;
  selection = null;
  zoom = 1;
  $("video-msg").hidden = true;
  loadedSrc = playerSrc();
  video.src = convertFileSrc(loadedSrc);
  layout();
  $("thumbs").style.backgroundImage = "";
  try {
    const count = Math.max(10, Math.round(scroll.clientWidth / 110));
    const r = await run({ cmd: "thumbnails", count, height: 90 });
    if (loadedPath === f.path) $("thumbs").style.backgroundImage = `url(${r.image})`;
  } catch {
    /* already shown as a toast */
  }
}

/** Places the crop overlay exactly over the picture inside the letterboxed <video>. */
function layout() {
  if (!S.file) return;
  const vw = video.clientWidth, vh = video.clientHeight;
  const scale = Math.min(vw / S.file.width, vh / S.file.height);
  const w = S.file.width * scale, h = S.file.height * scale;
  Object.assign(overlay.style, { left: `${(vw - w) / 2}px`, top: `${(vh - h) / 2}px`, width: `${w}px`, height: `${h}px` });
  track.style.width = `${zoom * 100}%`;
  renderCrop();
}

function scale(): number {
  return S.file ? overlay.clientWidth / S.file.width : 1;
}

function renderCrop() {
  if (!S.file) return;
  const c = currentCrop(), k = scale();
  Object.assign(cropEl.style, { left: `${c.x * k}px`, top: `${c.y * k}px`, width: `${c.w * k}px`, height: `${c.h * k}px` });
  const [l, t, r, b] = [c.x * k, c.y * k, (c.x + c.w) * k, (c.y + c.h) * k];
  const box = (id: string, left: number, top: number, w: number, h: number) =>
    Object.assign($(id).style, { left: `${left}px`, top: `${top}px`, width: `${Math.max(0, w)}px`, height: `${Math.max(0, h)}px` });
  const W = overlay.clientWidth, H = overlay.clientHeight;
  box("dim-t", 0, 0, W, t);
  box("dim-b", 0, b, W, H - b);
  box("dim-l", 0, t, l, b - t);
  box("dim-r", r, t, W - r, b - t);
  if (draft) $("size").textContent = `${Math.round(draft.w) & ~1} × ${Math.round(draft.h) & ~1}`;
}

const pct = (t: number) => `${(t / (duration() || 1)) * 100}%`;

function renderCuts() {
  const cuts = $("cuts");
  cuts.replaceChildren(
    ...(S.edit?.deleted ?? []).map((r) => {
      const d = document.createElement("div");
      Object.assign(d.style, { left: pct(r.start), width: `calc(${pct(r.end)} - ${pct(r.start)})` });
      d.title = `Deleted ${fmt(r.start)} to ${fmt(r.end)}. Double-click to bring it back.`;
      d.ondblclick = (e) => {
        e.stopPropagation();
        run({ cmd: "restore_range", start: r.start, end: r.end });
      };
      return d;
    }),
  );
}

function renderSelection() {
  const el = $("sel");
  const overlapsCut = !!selection && (S.edit?.deleted ?? []).some((r) => r.start < selection!.end && r.end > selection!.start);
  el.hidden = !selection;
  $("del-sel").hidden = !selection;
  $("restore-sel").hidden = !overlapsCut;
  $("sel-info").textContent = selection ? `Selected ${fmt(selection.start)} to ${fmt(selection.end)} (${fmt(selection.end - selection.start)})` : "";
  if (selection) Object.assign(el.style, { left: pct(selection.start), width: `calc(${pct(selection.end)} - ${pct(selection.start)})` });
  report();
}

function renderJobs() {
  // A save takes the progress bar over a preview copy being made.
  const running =
    S.jobs.find((j) => j.kind !== "preview" && j.status === "running") ??
    S.jobs.find((j) => j.kind === "preview" && j.status === "running");
  $("progress").hidden = !running;
  if (running) {
    const what =
      running.kind === "preview" ? "Preparing a smooth preview"
      : running.kind === "copy" ? "Preparing to copy"
      : `Saving ${running.output.split(/[\\/]/).pop()}`;
    $("progress-label").textContent = `${what}… ${Math.round(running.progress * 100)}%`;
    $("progress-fill").style.width = `${running.progress * 100}%`;
    $("cancel").onclick = () => run({ cmd: "cancel", job: running.id });
  }
  for (const j of S.jobs) {
    if (j.kind === "preview" || j.status === "running" || seenJobs.has(j.id)) continue;
    seenJobs.add(j.id);
    if (j.kind === "copy") {
      if (j.status === "done") toast(copiedMsg);
      else if (j.status === "failed") toast(`Couldn't copy: ${j.error}`, true);
    } else if (j.status === "done") {
      toast(`Saved ${j.output.split(/[\\/]/).pop()}`, false, { label: "Show in folder", fn: () => revealItemInDir(j.output) });
    } else if (j.status === "failed") {
      const pre = Object.assign(document.createElement("pre"), { textContent: j.error ?? "" });
      const p = Object.assign(document.createElement("div"), { textContent: "Couldn't save the video. Details:" });
      dialog("Save failed", [p, pre], [{ label: "OK", primary: true }]);
    }
  }
}

// ---------- player ----------

function togglePlay() {
  logEvent({ event: "togglePlay", file: !!S.file, paused: video.paused, time: +video.currentTime.toFixed(3) });
  if (!S.file) return;
  if (video.paused) {
    const last = S.kept[S.kept.length - 1];
    if (last && video.currentTime >= last.end - 0.05) video.currentTime = S.kept[0].start;
    video.play().catch((e) => logEvent({ event: "play_failed", error: String(e) }));
  } else video.pause();
}

function seek(t: number) {
  video.currentTime = clamp(t, 0, duration());
}

/** Changes text only when it differs. Replacing a button's text between mouse down and up makes WebKit drop the click. */
function setText(el: HTMLElement, text: string) {
  if (el.textContent !== text) el.textContent = text;
}

/** Every frame: move the playhead, and skip over deleted parts while playing. */
function tick() {
  if (S.file) {
    const t = video.currentTime;
    if (!video.paused) {
      const cut = S.edit?.deleted.find((r) => t >= r.start && t < r.end - 0.01);
      if (cut) {
        if (cut.end >= duration() - 0.01) {
          video.pause();
          video.currentTime = cut.start;
        } else video.currentTime = cut.end;
      }
    }
    $("playhead").style.left = pct(video.currentTime);
    setText($("time"), `${fmt(video.currentTime)} / ${fmt(duration())}`);
    setText($("play"), video.paused ? "Play" : "Pause");
    report();
  }
  requestAnimationFrame(tick);
}

let lastReport = "";
let reportTimer = 0;
/** Tells the core what the player is doing, so API clients can see it in `state.ui`. */
function report() {
  if (reportTimer) return;
  reportTimer = window.setTimeout(() => {
    reportTimer = 0;
    const ui = { playhead: video.currentTime, playing: !video.paused, selection, preview_crop: previewCrop, zoom, crop_shape: ratio };
    const s = JSON.stringify(ui);
    if (s !== lastReport) {
      lastReport = s;
      invoke("cmd", { command: { cmd: "ui_report", ui } }).catch(() => {});
    }
  }, 200);
}

video.addEventListener("error", () => {
  const m = $("video-msg");
  m.textContent = "This video can't be previewed here, but you can still crop, cut and save it.";
  m.hidden = false;
});

// ---------- crop dragging ----------

cropEl.addEventListener("pointerdown", (e) => {
  if (!S.file || previewCrop) return;
  e.preventDefault();
  const handle = (e.target as HTMLElement).dataset.h ?? "move";
  const start = currentCrop();
  const x0 = e.clientX, y0 = e.clientY;
  const k = scale();
  const W = S.file.width, H = S.file.height;
  cropEl.setPointerCapture(e.pointerId);

  const move = (ev: PointerEvent) => {
    const dx = (ev.clientX - x0) / k, dy = (ev.clientY - y0) / k;
    let { x, y, w, h } = start;
    const min = 16;
    if (handle === "move") {
      x = clamp(start.x + dx, 0, W - w);
      y = clamp(start.y + dy, 0, H - h);
    } else {
      if (handle.includes("w")) { x = clamp(start.x + dx, 0, start.x + start.w - min); w = start.x + start.w - x; }
      if (handle.includes("e")) w = clamp(start.w + dx, min, W - start.x);
      if (handle.includes("n")) { y = clamp(start.y + dy, 0, start.y + start.h - min); h = start.y + start.h - y; }
      if (handle.includes("s")) h = clamp(start.h + dy, min, H - start.y);
      const r = ratio ?? (ev.shiftKey ? start.w / start.h : null);
      if (r) ({ x, y, w, h } = keepShape({ x, y, w, h }, start, handle, r, W, H));
    }
    draft = { x, y, w, h };
    renderCrop();
  };
  const up = () => {
    cropEl.removeEventListener("pointermove", move);
    cropEl.removeEventListener("pointerup", up);
    if (draft) {
      const d = draft;
      const rect = { x: Math.round(d.x), y: Math.round(d.y), w: Math.round(d.w), h: Math.round(d.h) };
      run({ cmd: "set_crop", rect }).finally(() => {
        draft = null;
        render();
      });
    }
  };
  cropEl.addEventListener("pointermove", move);
  cropEl.addEventListener("pointerup", up);
});

/** Forces a width/height ratio while resizing, anchored on the opposite side or corner. */
function keepShape(c: Rect, start: Rect, handle: string, r: number, W: number, H: number): Rect {
  let { w, h } = c;
  if (handle === "n" || handle === "s") w = h * r;
  else h = w / r;
  // Shrink to fit the frame if needed.
  const right = handle.includes("w") ? start.x + start.w : null;
  const bottom = handle.includes("n") ? start.y + start.h : null;
  const cx = start.x + start.w / 2, cy = start.y + start.h / 2;
  const maxW = handle.length === 1 && (handle === "n" || handle === "s") ? 2 * Math.min(cx, W - cx) : right !== null ? right : W - start.x;
  const maxH = handle.length === 1 && (handle === "e" || handle === "w") ? 2 * Math.min(cy, H - cy) : bottom !== null ? bottom : H - start.y;
  const f = Math.min(1, maxW / w, maxH / h);
  w *= f;
  h *= f;
  let x = right !== null ? right - w : start.x;
  let y = bottom !== null ? bottom - h : start.y;
  if (handle === "n" || handle === "s") x = cx - w / 2;
  if (handle === "e" || handle === "w") y = cy - h / 2;
  return { x, y, w, h };
}

function setShape(value: string) {
  ratio = value === "free" ? null : Number(value);
  if (!ratio || !S.file) return;
  // Biggest rectangle of that shape, centered on the current crop.
  const W = S.file.width, H = S.file.height;
  const c = currentCrop();
  let w = W, h = W / ratio;
  if (h > H) { h = H; w = H * ratio; }
  const x = clamp(c.x + c.w / 2 - w / 2, 0, W - w);
  const y = clamp(c.y + c.h / 2 - h / 2, 0, H - h);
  run({ cmd: "set_crop", rect: { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) } });
}

function editSize() {
  if (!S.file) return;
  const c = currentCrop();
  const [w, h, x, y] = [c.w, c.h, c.x, c.y].map(numInput);
  const apply = () => {
    run({ cmd: "set_crop", rect: { w: +w.value, h: +h.value, x: +x.value, y: +y.value } });
  };
  dialog(
    "Exact crop (pixels)",
    [field("Width", w), field("Height", h), field("Left", x), field("Top", y)],
    [{ label: "Cancel" }, { label: "Apply", primary: true, fn: apply }],
  );
}

// ---------- timeline ----------

function timeAt(clientX: number): number {
  const r = track.getBoundingClientRect();
  return clamp(((clientX - r.left) / r.width) * duration(), 0, duration());
}

/** Snaps to the playhead, the ends, and deleted-part edges when within 6 pixels. */
function snap(t: number): number {
  const pxPerSec = track.clientWidth / (duration() || 1);
  const targets = [0, duration(), video.currentTime, ...(S.edit?.deleted ?? []).flatMap((r) => [r.start, r.end])];
  let best = t;
  for (const s of targets) if (Math.abs(s - t) * pxPerSec < 6 && Math.abs(s - t) < Math.abs(best - t) + 1e-9) best = s;
  return best;
}

track.addEventListener("pointerdown", (e) => {
  if (!S.file || e.button !== 0) return;
  e.preventDefault();
  track.setPointerCapture(e.pointerId);
  const scrub = e.target === $("ruler");
  const t0 = snap(timeAt(e.clientX));
  const x0 = e.clientX;
  let dragging = false;
  if (scrub) {
    video.pause();
    seek(timeAt(e.clientX));
  }
  const move = (ev: PointerEvent) => {
    if (scrub) return seek(timeAt(ev.clientX));
    if (!dragging && Math.abs(ev.clientX - x0) < 4) return;
    if (!dragging) video.pause();
    dragging = true;
    const t = snap(timeAt(ev.clientX));
    selection = { start: Math.min(t0, t), end: Math.max(t0, t) };
    renderSelection();
    seek(t); // show the frame under the moving end, to find where the range should stop
  };
  const up = () => {
    track.removeEventListener("pointermove", move);
    track.removeEventListener("pointerup", up);
    if (!scrub && !dragging) {
      selection = null;
      renderSelection();
      seek(timeAt(x0));
    }
  };
  track.addEventListener("pointermove", move);
  track.addEventListener("pointerup", up);
});

scroll.addEventListener(
  "wheel",
  (e) => {
    if (!(e.ctrlKey || e.metaKey) || !S.file) return;
    e.preventDefault();
    const before = timeAt(e.clientX);
    zoom = clamp(zoom * Math.exp(-e.deltaY / 200), 1, 200);
    track.style.width = `${zoom * 100}%`;
    // Keep the time under the mouse in place.
    const r = scroll.getBoundingClientRect();
    scroll.scrollLeft = (before / duration()) * track.clientWidth - (e.clientX - r.left);
    report();
  },
  { passive: false },
);

function deleteSelection() {
  if (!selection) return;
  const s = selection;
  selection = null;
  run({ cmd: "delete_range", start: s.start, end: s.end });
  renderSelection();
}

function restoreSelection() {
  if (!selection) return;
  run({ cmd: "restore_range", start: selection.start, end: selection.end });
}

function markSelection(which: "start" | "end") {
  const t = video.currentTime;
  const s = selection ?? { start: which === "start" ? t : 0, end: which === "end" ? t : duration() };
  selection = which === "start" ? { start: t, end: Math.max(t, s.end) } : { start: Math.min(t, s.start), end: t };
  if (selection.end - selection.start < 0.001) selection = null;
  renderSelection();
}

// ---------- files, saving, recording ----------

async function openFile() {
  const path = await openDialog({
    multiple: false,
    filters: [{ name: "Video", extensions: ["mp4", "mov", "m4v", "mkv", "webm", "avi", "mts", "m2ts", "3gp", "wmv", "flv", "ts"] }],
  });
  if (typeof path === "string") run({ cmd: "open", path });
}

const copiedMsg = "Copied. Paste it into a chat or email (Cmd/Ctrl+V).";

/** Puts the result on the clipboard; an edited video is saved to a scratch file first. */
async function copyVideo() {
  if (!S.edit) return;
  video.pause();
  const r = await run({ cmd: "copy" });
  if (r.copied) toast(copiedMsg);
}

async function save(mode: "exact" | "fast" = "exact", ask = false) {
  if (!S.edit) return;
  if (mode === "exact" && !S.edit.crop && !S.edit.deleted.length) {
    toast("Nothing to save yet: drag the crop box or delete part of the timeline first.");
    return;
  }
  let output: string | null = null;
  if (ask) {
    const base = S.edit.source.replace(/\.[^./\\]+$/, "");
    output = await saveDialog({ defaultPath: `${base}-cropped.mp4`, filters: [{ name: "MP4 video", extensions: ["mp4"] }] });
    if (!output) return;
  }
  video.pause();
  await run({ cmd: "export", output, mode });
}

async function recordDialog() {
  const { sources, system_audio } = await run({ cmd: "sources" });
  const screens = sources.filter((s: any) => s.kind === "screen");
  const mics = sources.filter((s: any) => s.kind === "mic");
  const opt = (value: string, label: string) => Object.assign(document.createElement("option"), { value, textContent: label });
  const screenSel = document.createElement("select");
  screenSel.append(...screens.map((s: any) => opt(s.id, s.name)));
  const micSel = document.createElement("select");
  micSel.append(opt("", "No microphone"), ...mics.map((s: any) => opt(s.id, s.name)));
  const sysSel = document.createElement("select");
  sysSel.append(opt("", "Don't record it"), opt("1", "Record it too"));
  const fpsSel = document.createElement("select");
  fpsSel.append(opt("30", "30 frames/sec"), opt("60", "60 frames/sec"));
  let last: any = {};
  try {
    last = JSON.parse(localStorage.getItem("record") ?? "{}");
  } catch {}
  for (const [sel, v] of [[screenSel, last.screen], [micSel, last.mic], [sysSel, last.system], [fpsSel, last.fps]] as const)
    if (v !== undefined && [...sel.options].some((o) => o.value === v)) sel.value = v;
  const note = Object.assign(document.createElement("div"), {
    textContent: "The window hides while recording. Stop from the REC item in the menu bar (or tray).",
  });
  note.style.color = "var(--muted)";
  const start = async () => {
    const choice = { screen: screenSel.value, mic: micSel.value, system: sysSel.value, fps: fpsSel.value };
    try {
      localStorage.setItem("record", JSON.stringify(choice));
    } catch {}
    const cd = $("countdown");
    cd.hidden = false;
    for (const n of [3, 2, 1]) {
      cd.textContent = String(n);
      await new Promise((r) => setTimeout(r, 1000));
    }
    cd.hidden = true;
    try {
      await invoke("cmd", { command: { cmd: "record_start", screen: choice.screen || null, mic: choice.mic || null, system_audio: system_audio && !!choice.system, fps: Number(choice.fps) } });
    } catch (e) {
      const pre = Object.assign(document.createElement("pre"), { textContent: String(e) });
      dialog("Recording didn't start", [pre], [{ label: "OK", primary: true }]);
    }
  };
  if (!screens.length) {
    dialog("No screen found", [Object.assign(document.createElement("div"), { textContent: "No screens were found to record." })], [{ label: "OK" }]);
    return;
  }
  dialog("Record the screen", [field("Screen", screenSel), field("Microphone", micSel), ...(system_audio ? [field("Computer's sound", sysSel)] : []), field("Smoothness", fpsSel), note], [
    { label: "Cancel" },
    { label: "Start recording", primary: true, fn: () => void start() },
  ]);
}

// ---------- wiring ----------

$("open").onclick = $("open2").onclick = openFile;
$("record").onclick = $("record2").onclick = recordDialog;
$("rec-stop").onclick = () => run({ cmd: "record_stop" });
$("size").onclick = editSize;
$<HTMLSelectElement>("ratio").onchange = (e) => setShape((e.target as HTMLSelectElement).value);
$("reset-crop").onclick = () => {
  ratio = null;
  $<HTMLSelectElement>("ratio").value = "free";
  run({ cmd: "set_crop", rect: null });
};
$("undo").onclick = () => run({ cmd: "undo" });
$("redo").onclick = () => run({ cmd: "redo" });
$("copy").onclick = () => copyVideo();
$("save").onclick = () => save();
$("save-more").onclick = (e) => {
  e.stopPropagation();
  $("save-menu").hidden = !$("save-menu").hidden;
};
document.addEventListener("click", () => ($("save-menu").hidden = true));
$("save-as").onclick = () => save("exact", true);
$("save-fast").onclick = () => save("fast");
$("play").onclick = togglePlay;
$("del-sel").onclick = deleteSelection;
$("restore-sel").onclick = restoreSelection;
$("preview-crop").onclick = () => togglePreview();

function togglePreview() {
  previewCrop = !previewCrop;
  document.body.classList.toggle("preview", previewCrop);
  report();
}

document.addEventListener("keydown", (e) => {
  if ((e.target as HTMLElement).tagName === "INPUT" || !$("dialog").hidden) return;
  const mod = e.metaKey || e.ctrlKey;
  const k = e.key.toLowerCase();
  if (mod && k === "o") return e.preventDefault(), openFile();
  if (!S.file) return;
  const frame = 1 / (S.file.fps || 30);
  if (mod && k === "z") return e.preventDefault(), run({ cmd: e.shiftKey ? "redo" : "undo" });
  if (mod && k === "y") return e.preventDefault(), run({ cmd: "redo" });
  if (mod && k === "s") return e.preventDefault(), save();
  if (mod && k === "c" && !window.getSelection()?.toString()) return e.preventDefault(), copyVideo();
  if (mod) return;
  switch (e.key) {
    case " ": e.preventDefault(); togglePlay(); break;
    case "ArrowLeft": e.preventDefault(); video.pause(); seek(video.currentTime - (e.shiftKey ? 1 : frame)); break;
    case "ArrowRight": e.preventDefault(); video.pause(); seek(video.currentTime + (e.shiftKey ? 1 : frame)); break;
    case "Delete": case "Backspace": e.preventDefault(); deleteSelection(); break;
    case "Escape": selection = null; renderSelection(); break;
    default:
      if (k === "i") markSelection("start");
      else if (k === "o") markSelection("end");
      else if (k === "c") togglePreview();
      else if (k === "r") restoreSelection();
  }
});

new ResizeObserver(layout).observe(video);

getCurrentWebview().onDragDropEvent((e) => {
  const empty = $("empty");
  if (e.payload.type === "over" || e.payload.type === "enter") empty.classList.add("over");
  else empty.classList.remove("over");
  if (e.payload.type === "drop" && e.payload.paths.length) run({ cmd: "open", path: e.payload.paths[0] });
});

// Player commands sent through the API.
listen<any>("ui", ({ payload: p }) => {
  if (p.cmd === "play") { if (video.paused) togglePlay(); }
  else if (p.cmd === "pause") video.pause();
  else if (p.cmd === "seek") seek(p.time);
  else if (p.cmd === "select") { selection = { start: Math.min(p.start, p.end), end: Math.max(p.start, p.end) }; renderSelection(); }
  else if (p.cmd === "clear_selection") { selection = null; renderSelection(); }
});
listen<State>("state", (e) => applyState(e.payload));
listen<string>("error", (e) => toast(e.payload, true));

run({ cmd: "state" }).then((st) => {
  for (const j of st.jobs) seenJobs.add(j.id);
  applyState(st);
});
requestAnimationFrame(tick);
