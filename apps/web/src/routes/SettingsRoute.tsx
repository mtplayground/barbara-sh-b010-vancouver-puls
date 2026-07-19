import { PlaceholderRoute } from "./PlaceholderRoute";

export function SettingsRoute() {
  return (
    <PlaceholderRoute
      title="Settings"
      description="Account, role, source, and Instagram connection settings will be grouped here."
      items={["Users and roles", "Instagram connection", "Notification settings"]}
    />
  );
}
