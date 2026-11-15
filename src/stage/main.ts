import { api, imageUrl, onAudioFrame, onConfigChanged, onCursor, onRuntimeChanged } from "../shared/api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StageRenderer } from "./renderer";

async function main() {
  const canvas = document.getElementById("stage") as HTMLCanvasElement;
  const [cfg, dir, runtime] = await Promise.all([api.getConfig(), api.imagesDir(), api.getRuntime()]);

  const renderer = new StageRenderer(canvas, {
    imageUrl: (file) => imageUrl(dir, file),
    onEdit: (c) => void api.setConfig(c),
  });
  renderer.setConfig(cfg);
  renderer.setEditing(runtime.editMode);

  await onAudioFrame((f) => renderer.onFrame(f));
  const self = getCurrentWindow().label;
  await onConfigChanged((c) => {
    if (c.origin !== self) renderer.setConfig(c.config);
  });
  await onRuntimeChanged((r) => renderer.setEditing(r.editMode));
  await onCursor((p) => renderer.setMouse(p));

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") void api.setEditMode(false);
  });
  window.addEventListener("contextmenu", (e) => e.preventDefault());
}

void main();
