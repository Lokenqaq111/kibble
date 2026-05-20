import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";

import idle from "./assets/cat/idle.svg";
import mouthOpen from "./assets/cat/mouth_open.svg";
import chewing from "./assets/cat/chewing.svg";
import swallow from "./assets/cat/swallow.svg";
import happy from "./assets/cat/happy.svg";
import confused from "./assets/cat/confused.svg";
import dead from "./assets/cat/dead.svg";

type State =
  | "idle"
  | "mouth_open"
  | "chewing"
  | "swallow"
  | "happy"
  | "confused"
  | "dead";

const sprites: Record<State, string> = {
  idle,
  mouth_open: mouthOpen,
  chewing,
  swallow,
  happy,
  confused,
  dead,
};

const IMAGE_EXTS = new Set([
  "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "tif", "tiff", "bmp",
]);

type Mode = "setup" | "ready";

const cat = document.getElementById("cat") as HTMLImageElement;
const win = document.getElementById("window") as HTMLDivElement;
const scrollEl = document.getElementById("scroll") as HTMLDivElement;
const noteEl = document.getElementById("note") as HTMLTextAreaElement;

let state: State = "idle";
let mode: Mode = "ready";
let pendingFiles: string[] = [];
let busy = false;

function setState(next: State) {
  state = next;
  cat.src = sprites[next];
  cat.classList.remove("chewing", "swallowing", "happy");
  if (next === "chewing") cat.classList.add("chewing");
  if (next === "swallow") cat.classList.add("swallowing");
  if (next === "happy") cat.classList.add("happy");
}

function isImage(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_EXTS.has(ext);
}

function openScroll(placeholder: string) {
  noteEl.placeholder = placeholder;
  noteEl.value = "";
  scrollEl.classList.add("open");
  setTimeout(() => noteEl.focus(), 50);
}

function closeScroll() {
  scrollEl.classList.remove("open");
  noteEl.blur();
}

function enterSetupMode() {
  mode = "setup";
  setState("dead");
  openScroll("repo path? e.g. /Users/you/kibble-data");
}

function enterReadyMode() {
  mode = "ready";
  closeScroll();
  setState("idle");
}

async function submitRepoPath(raw: string) {
  if (busy) return;
  const value = raw.trim();
  if (!value) return;
  busy = true;
  try {
    await invoke("set_repo_path", { path: value });
    enterReadyMode();
    setState("happy");
    setTimeout(() => setState("idle"), 400);
  } catch (e) {
    console.error(e);
    noteEl.value = "";
    noteEl.placeholder = `${e}`;
  } finally {
    busy = false;
  }
}

async function finishIngest(note: string) {
  if (busy) return;
  busy = true;
  const files = pendingFiles;
  pendingFiles = [];

  closeScroll();
  setState("swallow");
  setTimeout(() => {
    if (state === "swallow") setState("idle");
  }, 500);

  try {
    await invoke("ingest", { files, note });
    setState("happy");
    setTimeout(() => setState("idle"), 300);
  } catch (e) {
    console.error(e);
    setState("confused");
    setTimeout(() => setState("idle"), 3000);
  } finally {
    busy = false;
  }
}

noteEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    if (mode === "setup") {
      submitRepoPath(noteEl.value);
    } else {
      finishIngest(noteEl.value.trim());
    }
  }
});

win.addEventListener("mousedown", (e) => {
  const target = e.target as Node;
  const inScroll = scrollEl.contains(target);
  const scrollOpen = scrollEl.classList.contains("open");

  if (scrollOpen && !inScroll && mode === "ready") {
    finishIngest(noteEl.value.trim());
    return;
  }

  if (!inScroll && e.button === 0) {
    getCurrentWindow().startDragging().catch(() => {});
  }
});

window.addEventListener("keydown", (e) => {
  const meta = e.metaKey || e.ctrlKey;
  if (meta && (e.key === "w" || e.key === "W" || e.key === "q" || e.key === "Q")) {
    e.preventDefault();
    getCurrentWindow().close().catch(() => {});
  }
});

async function bootstrap() {
  setState("idle");

  const webview = getCurrentWebview();
  await webview.onDragDropEvent((event) => {
    if (mode !== "ready") return;
    if (event.payload.type === "enter" || event.payload.type === "over") {
      if (state === "idle") setState("mouth_open");
      win.classList.add("dragover");
    } else if (event.payload.type === "leave") {
      win.classList.remove("dragover");
      if (state === "mouth_open") setState("idle");
    } else if (event.payload.type === "drop") {
      win.classList.remove("dragover");
      const paths = (event.payload.paths as string[]).filter(isImage);
      if (paths.length === 0) {
        setState("idle");
        return;
      }
      pendingFiles = paths;
      setState("chewing");
      openScroll("note? (optional, Enter to send)");
    }
  });

  try {
    await invoke("startup_check");
    enterReadyMode();
  } catch (e) {
    console.error("kibble:", e);
    enterSetupMode();
  }
}

bootstrap();
