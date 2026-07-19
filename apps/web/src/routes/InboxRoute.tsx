import { PlaceholderRoute } from "./PlaceholderRoute";

export function InboxRoute() {
  return (
    <PlaceholderRoute
      title="Inbox"
      description="Ingested items will land here for review, deduplication, and draft selection."
      items={["New items", "Needs review", "Accepted ideas"]}
    />
  );
}
