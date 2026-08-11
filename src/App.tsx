import { lazy, Suspense } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { AppShell } from "./components/layout/AppShell";
import { InstancesPage } from "./pages/InstancesPage";
import { LibraryPage } from "./pages/LibraryPage";
import { NotFoundPage } from "./pages/NotFoundPage";
import { OverviewPage } from "./pages/OverviewPage";
import { ScanPage } from "./pages/ScanPage";
import { SessionsPage } from "./pages/SessionsPage";
import { SettingsPage } from "./pages/SettingsPage";

const ProfilePage = lazy(async () => {
  const module = await import("./pages/ProfilePage");
  return { default: module.ProfilePage };
});

export function App() {
  return (
    <Routes>
      <Route element={<AppShell />}>
        <Route index element={<Navigate to="/overview" replace />} />
        <Route path="overview" element={<OverviewPage />} />
        <Route path="profile" element={<Suspense fallback={<div className="route-loading" aria-label="Loading profile" />}><ProfilePage /></Suspense>} />
        <Route path="dashboard" element={<Navigate to="/overview" replace />} />
        <Route path="sessions" element={<SessionsPage />} />
        <Route path="instances" element={<InstancesPage />} />
        <Route path="worlds" element={<LibraryPage kind="worlds" />} />
        <Route path="servers" element={<LibraryPage kind="servers" />} />
        <Route path="versions" element={<LibraryPage kind="versions" />} />
        <Route path="scan" element={<ScanPage />} />
        <Route path="settings" element={<SettingsPage />} />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}
