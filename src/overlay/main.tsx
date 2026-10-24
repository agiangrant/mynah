import React from "react";
import ReactDOM from "react-dom/client";
import { Overlay } from "./Overlay";
import "./overlay.css";

if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("mock")) {
  await import("../dev/mock");
}

document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
