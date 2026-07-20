import { Navigate, Route, Routes } from "react-router-dom";

import { AppLayout } from "../layout/AppLayout";
import { BackupLibraryRoute } from "./BackupLibraryRoute";
import { CalendarRoute } from "./CalendarRoute";
import { DashboardRoute } from "./DashboardRoute";
import { DraftsRoute } from "./DraftsRoute";
import { InboxRoute } from "./InboxRoute";
import { LoginRoute } from "./LoginRoute";
import { SettingsRoute } from "./SettingsRoute";
import { SourcesRoute } from "./SourcesRoute";
import { UsersRoute } from "./UsersRoute";

export function AppRoutes() {
  return (
    <Routes>
      <Route path="login" element={<LoginRoute />} />
      <Route element={<AppLayout />}>
        <Route index element={<DashboardRoute />} />
        <Route path="sources" element={<SourcesRoute />} />
        <Route path="inbox" element={<InboxRoute />} />
        <Route path="drafts" element={<DraftsRoute />} />
        <Route path="calendar" element={<CalendarRoute />} />
        <Route path="backup-library" element={<BackupLibraryRoute />} />
        <Route path="users" element={<UsersRoute />} />
        <Route path="settings" element={<SettingsRoute />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
