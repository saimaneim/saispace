import "@fontsource-variable/plus-jakarta-sans";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { applyAccent, readAccent } from "./lib/accent";

applyAccent(readAccent());

const rootElement = document.getElementById("root");
if (rootElement) {
  ReactDOM.createRoot(rootElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
