import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AudioFrame,
  AudioSession,
  Config,
  ConfigChanged,
  CursorPos,
  MonitorInfo,
  Runtime,
} from "./types";

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  setConfig: (config: Config) => invoke<Config>("set_config", { config }),
  getRuntime: () => invoke<Runtime>("get_runtime"),
  imagesDir: () => invoke<string>("images_dir"),
  monitors: () => invoke<MonitorInfo[]>("list_monitors"),
  sessions: () => invoke<AudioSession[]>("list_audio_sessions"),
  addDolls: (paths: string[]) => invoke<Config>("add_dolls", { paths }),
  removeDoll: (id: string) => invoke<Config>("remove_doll", { id }),
  replaceDollImage: (id: string, path: string) => invoke<Config>("replace_doll_image", { id, path }),
  setBackgroundImage: (path: string) => invoke<Config>("set_background_image", { path }),
  setEditMode: (on: boolean) => invoke<void>("set_edit_mode", { on }),
  exportPreset: (path: string) => invoke<void>("export_preset", { path }),
  importPreset: (path: string) => invoke<Config>("import_preset", { path }),
};

export function imageUrl(imagesDir: string, file: string): string {
  return convertFileSrc(`${imagesDir}\\${file}`);
}

export const onAudioFrame = (cb: (f: AudioFrame) => void): Promise<UnlistenFn> =>
  listen<AudioFrame>("audio-frame", (e) => cb(e.payload));

export const onConfigChanged = (cb: (c: ConfigChanged) => void): Promise<UnlistenFn> =>
  listen<ConfigChanged>("config-changed", (e) => cb(e.payload));

export const onCursor = (cb: (p: CursorPos) => void): Promise<UnlistenFn> =>
  listen<CursorPos>("cursor", (e) => cb(e.payload));

export const onRuntimeChanged = (cb: (r: Runtime) => void): Promise<UnlistenFn> =>
  listen<Runtime>("runtime-changed", (e) => cb(e.payload));
