import { Navigate, Route, Routes } from "react-router-dom";

import { AppLayout } from "../layout/AppLayout";
import { CalendarRoute } from "./CalendarRoute";
import { DashboardRoute } from "./DashboardRoute";
import { DraftsRoute } from "./DraftsRoute";
import { InboxRoute } from "./InboxRoute";
import { SettingsRoute } from "./SettingsRoute";
import { SourcesRoute } from "./SourcesRoute";

export function AppRoutes() {
  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route index element={<DashboardRoute />} />
        <Route path="sources" element={<SourcesRoute />} />
        <Route path="inbox" element={<InboxRoute />} />
        <Route path="drafts" element={<DraftsRoute />} />
        <Route path="calendar" element={<CalendarRoute />} />
        <Route path="settings" element={<SettingsRoute />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
