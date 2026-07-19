import { PlaceholderRoute } from "./PlaceholderRoute";

export function SourcesRoute() {
  return (
    <PlaceholderRoute
      title="Sources"
      description="Source management routes are reserved for configured feeds and ingestion targets."
      items={["Source list", "Source settings", "Ingestion status"]}
    />
  );
}
