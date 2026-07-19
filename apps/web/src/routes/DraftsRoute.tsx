import { PlaceholderRoute } from "./PlaceholderRoute";

export function DraftsRoute() {
  return (
    <PlaceholderRoute
      title="Drafts"
      description="Draft editing and approval workflows will be mounted under this route."
      items={["Generated drafts", "Brand assets", "Approval queue"]}
    />
  );
}
