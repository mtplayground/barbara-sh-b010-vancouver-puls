import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";

import { ApiClientError, apiClient } from "../api/client";
import type { IngestedItemResponse } from "../api/types";
import { useAuth } from "../auth/useAuth";
import { LoginRoute } from "./LoginRoute";

const itemLimit = 75;

export function InboxRoute() {
  const auth = useAuth();
  const [query, setQuery] = useState("");

  const itemsQuery = useQuery({
    queryKey: ["inbox", "items", itemLimit],
    queryFn: () => apiClient.listInboxItems(itemLimit),
    enabled: auth.isAuthenticated,
  });

  const items = useMemo(() => itemsQuery.data?.items ?? [], [itemsQuery.data?.items]);
  const filteredItems = useMemo(() => filterItems(items, query), [items, query]);
  const itemsWithMedia = items.filter((item) => item.media_ref).length;
  const todayItems = items.filter((item) => isToday(item.ingested_at)).length;

  if (auth.status === "unknown") {
    return (
      <div className="border border-slate-200 bg-white p-6 shadow-sm">
        <p className="text-sm font-medium text-slate-600">Checking session...</p>
      </div>
    );
  }

  if (auth.status === "anonymous") {
    return <LoginRoute />;
  }

  return (
    <div className="space-y-6">
      <section className="border border-slate-200 bg-white p-6 shadow-sm">
        <div className="flex flex-col gap-5 lg:flex-row lg:items-start lg:justify-between">
          <div>
            <p className="text-pine text-sm font-semibold uppercase">Inbox</p>
            <h2 className="mt-2 text-2xl font-semibold text-slate-950">Fresh ingested items</h2>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-600">
              Browse normalized Vancouver event and news material, inspect source links, and start
              drafting from promising items.
            </p>
          </div>

          <div className="grid grid-cols-3 gap-3 text-sm">
            <InboxMetric label="Loaded" value={items.length.toString()} />
            <InboxMetric label="Today" value={todayItems.toString()} />
            <InboxMetric label="Media" value={itemsWithMedia.toString()} />
          </div>
        </div>
      </section>

      <section className="border border-slate-200 bg-white shadow-sm">
        <div className="flex flex-col gap-3 border-b border-slate-200 px-5 py-4 lg:flex-row lg:items-center lg:justify-between">
          <label className="min-w-0 flex-1">
            <span className="sr-only">Search ingested items</span>
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search title, summary, link, or source number"
              className="focus:border-coral focus:ring-coral h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
            />
          </label>

          <button
            type="button"
            onClick={() => void itemsQuery.refetch()}
            disabled={itemsQuery.isFetching}
            className="border border-slate-300 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-wait disabled:opacity-60"
          >
            {itemsQuery.isFetching ? "Refreshing..." : "Refresh"}
          </button>
        </div>

        {itemsQuery.isLoading ? (
          <p className="p-6 text-sm text-slate-600">Loading ingested items...</p>
        ) : null}

        {itemsQuery.isError ? (
          <p className="p-6 text-sm font-medium text-red-700">{errorMessage(itemsQuery.error)}</p>
        ) : null}

        {itemsQuery.isSuccess && items.length === 0 ? (
          <div className="p-6">
            <p className="text-base font-semibold text-slate-950">No ingested items yet</p>
            <p className="mt-2 text-sm leading-6 text-slate-600">
              Fresh items appear here after the scheduled ingestion service polls enabled sources.
            </p>
          </div>
        ) : null}

        {itemsQuery.isSuccess && items.length > 0 && filteredItems.length === 0 ? (
          <div className="p-6">
            <p className="text-base font-semibold text-slate-950">No matching items</p>
            <p className="mt-2 text-sm text-slate-600">
              Clear the search field to return to the latest ingested material.
            </p>
          </div>
        ) : null}

        {itemsQuery.isSuccess && filteredItems.length > 0 ? (
          <div className="divide-y divide-slate-100">
            {filteredItems.map((item) => (
              <InboxItem key={item.id} item={item} />
            ))}
          </div>
        ) : null}
      </section>
    </div>
  );
}

interface InboxMetricProps {
  label: string;
  value: string;
}

function InboxMetric({ label, value }: InboxMetricProps) {
  return (
    <div className="border border-slate-200 bg-slate-50 px-4 py-3">
      <p className="text-xs font-semibold uppercase text-slate-500">{label}</p>
      <p className="mt-1 text-2xl font-semibold text-slate-950">{value}</p>
    </div>
  );
}

interface InboxItemProps {
  item: IngestedItemResponse;
}

function InboxItem({ item }: InboxItemProps) {
  const freshness = item.source_published_at ?? item.ingested_at;
  const hasCachedMedia = item.media_ref?.startsWith("cached/") ?? false;

  return (
    <article className="p-5">
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_180px]">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="rounded-full bg-emerald-100 px-2.5 py-1 text-xs font-semibold uppercase text-emerald-800">
              Fresh
            </span>
            <span className="rounded-full bg-slate-100 px-2.5 py-1 text-xs font-semibold uppercase text-slate-700">
              Source #{item.source_id}
            </span>
            {item.media_ref ? (
              <span className="rounded-full bg-amber-100 px-2.5 py-1 text-xs font-semibold uppercase text-amber-800">
                {hasCachedMedia ? "Cached media" : "Media link"}
              </span>
            ) : null}
          </div>

          <h3 className="mt-3 text-xl font-semibold leading-7 text-slate-950">{item.title}</h3>

          {item.summary ? (
            <p className="mt-2 line-clamp-3 text-sm leading-6 text-slate-600">{item.summary}</p>
          ) : null}

          <dl className="mt-4 grid gap-2 text-xs text-slate-500 sm:grid-cols-3">
            <div>
              <dt className="font-semibold uppercase">Published</dt>
              <dd className="mt-1">{formatDateTime(freshness)}</dd>
            </div>
            <div>
              <dt className="font-semibold uppercase">Ingested</dt>
              <dd className="mt-1">{formatDateTime(item.ingested_at)}</dd>
            </div>
            <div>
              <dt className="font-semibold uppercase">Dedup</dt>
              <dd className="mt-1 font-mono">{item.dedup_key.slice(0, 10)}</dd>
            </div>
          </dl>
        </div>

        <div className="flex flex-col gap-2 lg:items-stretch">
          <Link
            to={`/drafts?item=${encodeURIComponent(item.id)}`}
            className="bg-pine hover:bg-pine/90 focus-visible:ring-coral px-4 py-2 text-center text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2"
          >
            Start draft
          </Link>
          <a
            href={item.link}
            target="_blank"
            rel="noreferrer"
            className="border border-slate-300 px-4 py-2 text-center text-sm font-semibold text-slate-700 hover:bg-slate-50"
          >
            Open source
          </a>
        </div>
      </div>
    </article>
  );
}

function filterItems(items: IngestedItemResponse[], query: string): IngestedItemResponse[] {
  const normalizedQuery = query.trim().toLowerCase();

  if (!normalizedQuery) {
    return items;
  }

  return items.filter((item) => {
    const haystack = [
      item.title,
      item.summary ?? "",
      item.link,
      item.media_ref ?? "",
      item.dedup_key,
      `source ${item.source_id}`,
    ]
      .join(" ")
      .toLowerCase();

    return haystack.includes(normalizedQuery);
  });
}

function isToday(value: string): boolean {
  const date = new Date(value);
  const now = new Date();

  return (
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate()
  );
}

function formatDateTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function errorMessage(error: unknown): string {
  if (error instanceof ApiClientError || error instanceof Error) {
    return error.message;
  }

  return "Request failed";
}
