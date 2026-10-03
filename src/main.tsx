import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { desktop } from "./api";
import App from "./App";
import DesktopFrame from "./DesktopFrame";
import Overlay from "./Overlay";
import "./styles.css";

const overlay = desktop
  ? getCurrentWindow().label === "overlay"
  : import.meta.env.DEV && new URLSearchParams(location.search).has("overlay");
document.documentElement.dataset.window = overlay ? "overlay" : "main";

class ErrorBoundary extends React.Component<
  React.PropsWithChildren,
  { error: string }
> {
  state = { error: "" };
  static getDerivedStateFromError(error: Error) {
    return { error: error.message };
  }
  render() {
    if (this.state.error)
      return (
        <div className="fatal-error">
          <h1>Не удалось открыть интерфейс</h1>
          <p>{this.state.error}</p>
          <button onClick={() => window.location.reload()}>
            Перезагрузить окно
          </button>
        </div>
      );
    return this.props.children;
  }
}
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary>
      {overlay ? (
        <Overlay />
      ) : (
        <DesktopFrame>
          <App />
        </DesktopFrame>
      )}
    </ErrorBoundary>
  </React.StrictMode>,
);
