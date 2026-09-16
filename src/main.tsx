import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { IpcProvider } from "./app/ipc";
import "./styles/base.css";
import "./styles/shell.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("找不到根节点 #root");
}

createRoot(container).render(
  <StrictMode>
    <IpcProvider>
      <App />
    </IpcProvider>
  </StrictMode>,
);
