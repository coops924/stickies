import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import NoteWindow from "./NoteWindow";
import Settings from "./Settings";
import "./styles.css";

// One bundle serves every window; the window label says which UI to show.
const label = getCurrentWebviewWindow().label;
const noteId = label.startsWith("note-") ? label.slice("note-".length) : null;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{noteId ? <NoteWindow id={noteId} /> : <Settings />}</React.StrictMode>,
);
