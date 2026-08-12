import "@fontsource-variable/ibm-plex-sans";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { App } from "./App";
import { AppErrorBoundary } from "./components/system/AppErrorBoundary";
import { recordFrontendIssue } from "./lib/runtime";
import { initializeTheme } from "./stores/ui-store";
import "./styles/global.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

initializeTheme();

window.addEventListener("error", () => {
  void recordFrontendIssue("unhandledError").catch(() => undefined);
});
window.addEventListener("unhandledrejection", () => {
  void recordFrontendIssue("unhandledRejection").catch(() => undefined);
});

const root = document.getElementById("root");

if (!root) {
  throw new Error("MineTrace root element was not found");
}

createRoot(root).render(
  <StrictMode>
    <AppErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <App />
        </BrowserRouter>
      </QueryClientProvider>
    </AppErrorBoundary>
  </StrictMode>,
);
