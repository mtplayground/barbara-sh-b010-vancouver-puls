import { PlaceholderRoute } from "./PlaceholderRoute";

export function CalendarRoute() {
  return (
    <PlaceholderRoute
      title="Calendar"
      description="Scheduled publishing slots and backup content placement will be organized here."
      items={["Scheduled posts", "Open slots", "Publishing status"]}
    />
  );
}
