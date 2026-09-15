import { createApp } from "vue";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import NoteWindow from "./NoteWindow.vue";
import Settings from "./Settings.vue";
import "./styles.css";

// One bundle serves every window; the window label says which UI to show.
const label = getCurrentWebviewWindow().label;
const noteId = label.startsWith("note-") ? label.slice("note-".length) : null;

(noteId ? createApp(NoteWindow, { id: noteId }) : createApp(Settings)).mount("#root");
