import { FormEvent, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiClientError, apiClient } from "../api/client";
import type {
  ContentSourceKind,
  CreateSourceRequest,
  SourceResponse,
  UpdateSourceRequest,
} from "../api/types";
import { useAuth } from "../auth/useAuth";
import { LoginRoute } from "./LoginRoute";

const sourceKinds: ContentSourceKind[] = ["rss", "website", "instagram", "manual"];

interface SourceFormState {
  name: string;
  kind: ContentSourceKind;
  url: string;
  externalId: string;
  enabled: boolean;
}

const emptySourceForm: SourceFormState = {
  name: "",
  kind: "rss",
  url: "",
  externalId: "",
  enabled: true,
};

export function SourcesRoute() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const isAdmin = auth.roles.includes("admin");
  const [form, setForm] = useState<SourceFormState>(emptySourceForm);
  const [editingSourceId, setEditingSourceId] = useState<number | null>(null);
  const [formMessage, setFormMessage] = useState<string | null>(null);

  const sourcesQuery = useQuery({
    queryKey: ["admin", "sources"],
    queryFn: apiClient.listSources,
    enabled: isAdmin,
  });

  const sortedSources = useMemo(
    () => [...(sourcesQuery.data?.sources ?? [])].sort(sortSources),
    [sourcesQuery.data?.sources],
  );

  const createMutation = useMutation({
    mutationFn: (source: CreateSourceRequest) => apiClient.createSource(source),
    onSuccess: (created) => {
      setForm(emptySourceForm);
      setFormMessage(`${created.name} was added to source polling.`);
      void queryClient.invalidateQueries({ queryKey: ["admin", "sources"] });
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, source }: { id: number; source: UpdateSourceRequest }) =>
      apiClient.updateSource(id, source),
    onSuccess: (updated) => {
      setEditingSourceId(null);
      setForm(emptySourceForm);
      setFormMessage(`${updated.name} was updated.`);
      void queryClient.invalidateQueries({ queryKey: ["admin", "sources"] });
    },
  });

  const statusMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) =>
      apiClient.updateSource(id, { enabled }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["admin", "sources"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: number) => apiClient.deleteSource(id),
    onSuccess: (deleted) => {
      if (editingSourceId === deleted.id) {
        setEditingSourceId(null);
        setForm(emptySourceForm);
      }
      setFormMessage(`${deleted.name} was removed.`);
      void queryClient.invalidateQueries({ queryKey: ["admin", "sources"] });
    },
  });

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

  if (!isAdmin) {
    return (
      <div className="border border-slate-200 bg-white p-6 shadow-sm">
        <p className="text-coral text-sm font-semibold uppercase">Access denied</p>
        <h2 className="mt-2 text-xl font-semibold text-slate-950">Admin permissions required</h2>
        <p className="mt-2 max-w-2xl text-sm text-slate-600">
          Source creation, polling changes, and removals are restricted to admins.
        </p>
      </div>
    );
  }

  const activeSources = sortedSources.filter((source) => source.enabled).length;
  const isSaving = createMutation.isPending || updateMutation.isPending;
  const formError = createMutation.error ?? updateMutation.error;

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const payload = sourceFormPayload(form);

    if (!payload.name || (!payload.url && !payload.external_id) || isSaving) {
      return;
    }

    setFormMessage(null);

    if (editingSourceId) {
      updateMutation.mutate({ id: editingSourceId, source: payload });
      return;
    }

    createMutation.mutate(payload);
  }

  function startEditing(source: SourceResponse) {
    setEditingSourceId(source.id);
    setForm({
      name: source.name,
      kind: source.kind,
      url: source.url ?? "",
      externalId: source.external_id ?? "",
      enabled: source.enabled,
    });
    setFormMessage(null);
  }

  function cancelEditing() {
    setEditingSourceId(null);
    setForm(emptySourceForm);
  }

  function deleteSource(source: SourceResponse) {
    const confirmed = window.confirm(`Remove ${source.name} from source polling?`);

    if (confirmed) {
      deleteMutation.mutate(source.id);
    }
  }

  return (
    <div className="space-y-6">
      <section className="border border-slate-200 bg-white p-6 shadow-sm">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
          <div>
            <p className="text-pine text-sm font-semibold uppercase">Sources</p>
            <h2 className="mt-2 text-2xl font-semibold text-slate-950">
              Vancouver source management
            </h2>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-600">
              Add feeds and accounts for scheduled ingestion, pause polling, and keep source records
              current.
            </p>
          </div>

          <div className="grid grid-cols-2 gap-3 text-sm sm:grid-cols-3">
            <SourceMetric label="Total" value={sortedSources.length.toString()} />
            <SourceMetric label="Polling" value={activeSources.toString()} />
            <SourceMetric
              label="Paused"
              value={(sortedSources.length - activeSources).toString()}
            />
          </div>
        </div>
      </section>

      <section className="grid gap-6 xl:grid-cols-[minmax(320px,420px)_minmax(0,1fr)]">
        <form
          onSubmit={handleSubmit}
          className="border border-slate-200 bg-white p-5 shadow-sm xl:self-start"
        >
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-sm font-semibold uppercase text-slate-500">
                {editingSourceId ? "Edit source" : "New source"}
              </p>
              <h3 className="mt-1 text-xl font-semibold text-slate-950">
                {editingSourceId ? "Update source details" : "Add a polling source"}
              </h3>
            </div>
            {editingSourceId ? (
              <button
                type="button"
                onClick={cancelEditing}
                className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50"
              >
                Cancel
              </button>
            ) : null}
          </div>

          <div className="mt-5 space-y-4">
            <label className="block">
              <span className="text-sm font-medium text-slate-700">Name</span>
              <input
                value={form.name}
                onChange={(event) => setForm({ ...form, name: event.target.value })}
                required
                placeholder="Vancouver Parks events"
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">Source type</span>
              <select
                value={form.kind}
                onChange={(event) =>
                  setForm({ ...form, kind: event.target.value as ContentSourceKind })
                }
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 bg-white px-3 text-sm outline-none focus:ring-1"
              >
                {sourceKinds.map((kind) => (
                  <option key={kind} value={kind}>
                    {sourceKindLabel(kind)}
                  </option>
                ))}
              </select>
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">URL</span>
              <input
                value={form.url}
                onChange={(event) => setForm({ ...form, url: event.target.value })}
                type="url"
                placeholder="https://example.com/events.xml"
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">External ID</span>
              <input
                value={form.externalId}
                onChange={(event) => setForm({ ...form, externalId: event.target.value })}
                placeholder="@vancouverevents"
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <label className="flex items-center gap-3 text-sm font-medium text-slate-700">
              <input
                checked={form.enabled}
                onChange={(event) => setForm({ ...form, enabled: event.target.checked })}
                type="checkbox"
                className="focus:ring-coral h-4 w-4 border-slate-300 text-slate-950"
              />
              Poll this source on the scheduled ingestion run
            </label>
          </div>

          {formError ? (
            <p className="mt-4 text-sm font-medium text-red-700">{errorMessage(formError)}</p>
          ) : null}

          {formMessage ? (
            <p className="mt-4 border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm font-medium text-emerald-900">
              {formMessage}
            </p>
          ) : null}

          <button
            type="submit"
            disabled={isSaving}
            className="bg-pine hover:bg-pine/90 focus-visible:ring-coral mt-5 h-11 w-full px-4 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {isSaving ? "Saving..." : editingSourceId ? "Save source" : "Add source"}
          </button>
        </form>

        <div className="overflow-hidden border border-slate-200 bg-white shadow-sm">
          <div className="flex flex-col gap-2 border-b border-slate-200 px-5 py-4 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h3 className="text-base font-semibold text-slate-950">Configured sources</h3>
              <p className="mt-1 text-sm text-slate-500">
                Polling status follows each source's enabled setting.
              </p>
            </div>
            <button
              type="button"
              onClick={() => void sourcesQuery.refetch()}
              disabled={sourcesQuery.isFetching}
              className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-wait disabled:opacity-60"
            >
              {sourcesQuery.isFetching ? "Refreshing..." : "Refresh"}
            </button>
          </div>

          {sourcesQuery.isLoading ? (
            <p className="p-6 text-sm text-slate-600">Loading sources...</p>
          ) : null}

          {sourcesQuery.isError ? (
            <p className="p-6 text-sm font-medium text-red-700">
              {errorMessage(sourcesQuery.error)}
            </p>
          ) : null}

          {sourcesQuery.isSuccess && sortedSources.length === 0 ? (
            <div className="p-6">
              <p className="text-base font-semibold text-slate-950">No sources configured</p>
              <p className="mt-2 text-sm leading-6 text-slate-600">
                Add an RSS feed, website, Instagram account, or manual source to begin polling
                Vancouver event and news material.
              </p>
            </div>
          ) : null}

          {sourcesQuery.isSuccess && sortedSources.length > 0 ? (
            <div className="divide-y divide-slate-100">
              {sortedSources.map((source) => (
                <SourceRow
                  key={source.id}
                  source={source}
                  isEditing={editingSourceId === source.id}
                  isStatusPending={
                    statusMutation.isPending && statusMutation.variables?.id === source.id
                  }
                  isDeletePending={
                    deleteMutation.isPending && deleteMutation.variables === source.id
                  }
                  onEdit={() => startEditing(source)}
                  onToggle={() =>
                    statusMutation.mutate({ id: source.id, enabled: !source.enabled })
                  }
                  onDelete={() => deleteSource(source)}
                />
              ))}
            </div>
          ) : null}

          {statusMutation.isError ? (
            <p className="border-t border-slate-200 p-4 text-sm font-medium text-red-700">
              {errorMessage(statusMutation.error)}
            </p>
          ) : null}

          {deleteMutation.isError ? (
            <p className="border-t border-slate-200 p-4 text-sm font-medium text-red-700">
              {errorMessage(deleteMutation.error)}
            </p>
          ) : null}
        </div>
      </section>
    </div>
  );
}

interface SourceMetricProps {
  label: string;
  value: string;
}

function SourceMetric({ label, value }: SourceMetricProps) {
  return (
    <div className="border border-slate-200 bg-slate-50 px-4 py-3">
      <p className="text-xs font-semibold uppercase text-slate-500">{label}</p>
      <p className="mt-1 text-2xl font-semibold text-slate-950">{value}</p>
    </div>
  );
}

interface SourceRowProps {
  source: SourceResponse;
  isEditing: boolean;
  isStatusPending: boolean;
  isDeletePending: boolean;
  onEdit: () => void;
  onToggle: () => void;
  onDelete: () => void;
}

function SourceRow({
  source,
  isEditing,
  isStatusPending,
  isDeletePending,
  onEdit,
  onToggle,
  onDelete,
}: SourceRowProps) {
  return (
    <article className={["p-5", isEditing ? "bg-coral/5" : "bg-white"].join(" ")}>
      <div className="flex flex-col gap-4 xl:flex-row xl:items-start xl:justify-between">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h4 className="truncate text-lg font-semibold text-slate-950">{source.name}</h4>
            <span className="rounded-full bg-slate-100 px-2.5 py-1 text-xs font-semibold uppercase text-slate-700">
              {sourceKindLabel(source.kind)}
            </span>
            <span className={source.enabled ? enabledBadgeClass : pausedBadgeClass}>
              {source.enabled ? "Polling" : "Paused"}
            </span>
          </div>
          <p className="mt-2 break-all text-sm text-slate-600">
            {source.url ?? source.external_id ?? "No endpoint"}
          </p>
          <div className="mt-3 grid gap-2 text-xs text-slate-500 sm:grid-cols-2">
            <p>Updated {formatDateTime(source.updated_at)}</p>
            <p>Created {formatDateTime(source.created_at)}</p>
          </div>
        </div>

        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={onToggle}
            disabled={isStatusPending}
            className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-wait disabled:opacity-60"
          >
            {isStatusPending ? "Saving..." : source.enabled ? "Pause" : "Resume"}
          </button>
          <button
            type="button"
            onClick={onEdit}
            className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50"
          >
            Edit
          </button>
          <button
            type="button"
            onClick={onDelete}
            disabled={isDeletePending}
            className="border border-red-200 px-3 py-2 text-sm font-semibold text-red-700 hover:bg-red-50 disabled:cursor-wait disabled:opacity-60"
          >
            {isDeletePending ? "Removing..." : "Remove"}
          </button>
        </div>
      </div>
    </article>
  );
}

const enabledBadgeClass =
  "rounded-full bg-emerald-100 px-2.5 py-1 text-xs font-semibold uppercase text-emerald-800";
const pausedBadgeClass =
  "rounded-full bg-amber-100 px-2.5 py-1 text-xs font-semibold uppercase text-amber-800";

function sourceFormPayload(form: SourceFormState): CreateSourceRequest {
  return {
    name: form.name.trim(),
    kind: form.kind,
    url: nullableTrim(form.url),
    external_id: nullableTrim(form.externalId),
    enabled: form.enabled,
  };
}

function nullableTrim(value: string): string | null {
  const trimmed = value.trim();

  return trimmed.length > 0 ? trimmed : null;
}

function sortSources(left: SourceResponse, right: SourceResponse): number {
  if (left.enabled !== right.enabled) {
    return left.enabled ? -1 : 1;
  }

  return left.name.localeCompare(right.name);
}

function sourceKindLabel(kind: ContentSourceKind): string {
  switch (kind) {
    case "rss":
      return "RSS";
    case "website":
      return "Website";
    case "instagram":
      return "Instagram";
    case "manual":
      return "Manual";
  }
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
