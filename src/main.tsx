import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import { attachConsole } from "@tauri-apps/plugin-log";
import { router } from "./router";
import "./index.css";

if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
  void attachConsole().catch(() => {});
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);
