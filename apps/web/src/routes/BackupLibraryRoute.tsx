import { FormEvent, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiClientError, apiClient } from "../api/client";
import type {
  BackupContentItemResponse,
  BackupContentKind,
  CalendarSlotResponse,
  CreateBackupContentItemRequest,
  UpdateBackupContentItemRequest,
} from "../api/types";
import { useAuth } from "../auth/useAuth";
import { LoginRoute } from "./LoginRoute";

const libraryLimit = 100;
const calendarDays = 14;
const backupKinds: BackupContentKind[] = ["did_you_know", "past_recap"];

interface BackupContentFormState {
  kind: BackupContentKind;
  title: string;
  body: string;
  sourceUrl: string;
  mediaRef: string;
  active: boolean;
}

type SlotSelections = Record<number, string>;

const emptyForm: BackupContentFormState = {
  kind: "did_you_know",
  title: "",
  body: "",
  sourceUrl: "",
  mediaRef: "",
  active: true,
};

export function BackupLibraryRoute() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const [query, setQuery] = useState("");
  const [showInactive, setShowInactive] = useState(false);
  const [editingItemId, setEditingItemId] = useState<number | null>(null);
  const [form, setForm] = useState<BackupContentFormState>(emptyForm);
  const [slotSelections, setSlotSelections] = useState<SlotSelections>({});
  const [message, setMessage] = useState<string | null>(null);

  const libraryQuery = useQuery({
    queryKey: ["backup-library", libraryLimit, showInactive],
    queryFn: () => apiClient.listBackupContentItems(libraryLimit, showInactive ? undefined : true),
    enabled: auth.isAuthenticated,
  });

  const calendarQuery = useQuery({
    queryKey: ["calendar", "backup-library", calendarDays],
    queryFn: () => apiClient.listCalendar(calendarDays),
    enabled: auth.isAuthenticated,
  });

  const items = useMemo(
    () => [...(libraryQuery.data?.items ?? [])].sort(sortBackupItems),
    [libraryQuery.data?.items],
  );
  const filteredItems = useMemo(() => filterItems(items, query), [items, query]);
  const activeItems = items.filter((item) => item.active).length;
  const factItems = items.filter((item) => item.kind === "did_you_know").length;
  const emptySlots = useMemo(
    () => (calendarQuery.data?.slots ?? []).filter((slot) => slot.is_empty && slot.is_upcoming),
    [calendarQuery.data?.slots],
  );

  const createMutation = useMutation({
    mutationFn: (item: CreateBackupContentItemRequest) => apiClient.createBackupContentItem(item),
    onSuccess: (created) => {
      setForm(emptyForm);
      setMessage(`${created.title} was added to the backup pool.`);
      void queryClient.invalidateQueries({ queryKey: ["backup-library"] });
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, item }: { id: number; item: UpdateBackupContentItemRequest }) =>
      apiClient.updateBackupContentItem(id, item),
    onSuccess: (updated) => {
      setEditingItemId(null);
      setForm(emptyForm);
      setMessage(`${updated.title} was updated.`);
      void queryClient.invalidateQueries({ queryKey: ["backup-library"] });
    },
  });

  const statusMutation = useMutation({
    mutationFn: ({ id, active }: { id: number; active: boolean }) =>
      apiClient.updateBackupContentItem(id, { active }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["backup-library"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: number) => apiClient.deleteBackupContentItem(id),
    onSuccess: (deleted) => {
      if (editingItemId === deleted.id) {
        cancelEditing();
      }
      setMessage(`${deleted.title} was removed from the backup pool.`);
      void queryClient.invalidateQueries({ queryKey: ["backup-library"] });
    },
  });

  const fillSlotMutation = useMutation({
    mutationFn: async ({
      item,
      slot,
    }: {
      item: BackupContentItemResponse;
      slot: CalendarSlotResponse;
    }) => {
      const draft = await apiClient.createDraft({
        manual_topic: item.title,
        manual_notes: backupDraftNotes(item),
      });
      const rendered = await apiClient.renderDraft(draft.id);
      const approved = await apiClient.approveDraft(rendered.draft.id);

      return apiClient.assignCalendarSlot({
        slot_date: slot.slot_date,
        slot_time: slot.slot_time,
        draft_id: approved.id,
      });
    },
    onSuccess: (slot) => {
      setMessage(
        `Backup item filled ${formatSlotDate(slot)} with draft #${slot.draft?.id ?? "new"}.`,
      );
      void queryClient.invalidateQueries({ queryKey: ["calendar"] });
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
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

  const isSaving = createMutation.isPending || updateMutation.isPending;
  const mutationError =
    createMutation.error ??
    updateMutation.error ??
    statusMutation.error ??
    deleteMutation.error ??
    fillSlotMutation.error;

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const payload = backupFormPayload(form);

    if (!payload.title || !payload.body || isSaving) {
      return;
    }

    setMessage(null);

    if (editingItemId) {
      updateMutation.mutate({ id: editingItemId, item: payload });
      return;
    }

    createMutation.mutate(payload);
  }

  function startEditing(item: BackupContentItemResponse) {
    setEditingItemId(item.id);
    setForm({
      kind: item.kind,
      title: item.title,
      body: item.body,
      sourceUrl: item.source_url ?? "",
      mediaRef: item.media_ref ?? "",
      active: item.active,
    });
    setMessage(null);
  }

  function cancelEditing() {
    setEditingItemId(null);
    setForm(emptyForm);
  }

  function deleteItem(item: BackupContentItemResponse) {
    const confirmed = window.confirm(`Remove ${item.title} from the backup pool?`);

    if (confirmed) {
      deleteMutation.mutate(item.id);
    }
  }

  function fillSlot(item: BackupContentItemResponse) {
    const selectedDate = slotSelections[item.id] ?? emptySlots[0]?.slot_date;
    const slot = emptySlots.find((candidate) => candidate.slot_date === selectedDate);

    if (!slot || fillSlotMutation.isPending) {
      return;
    }

    setMessage(null);
    fillSlotMutation.mutate({ item, slot });
  }

  return (
    <div className="space-y-6">
      <section className="border border-slate-200 bg-white p-6 shadow-sm">
        <div className="flex flex-col gap-5 xl:flex-row xl:items-start xl:justify-between">
          <div>
            <p className="text-pine text-sm font-semibold uppercase">Backup library</p>
            <h2 className="mt-2 text-2xl font-semibold text-slate-950">Slow-day content pool</h2>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-600">
              Keep evergreen Vancouver recaps and local facts ready, then use one to fill a flagged
              empty calendar slot.
            </p>
          </div>

          <div className="grid grid-cols-3 gap-3 text-sm">
            <BackupMetric label="Loaded" value={items.length.toString()} />
            <BackupMetric label="Active" value={activeItems.toString()} />
            <BackupMetric label="Gaps" value={emptySlots.length.toString()} />
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
                {editingItemId ? "Edit item" : "New backup item"}
              </p>
              <h3 className="mt-1 text-xl font-semibold text-slate-950">
                {editingItemId ? "Update evergreen content" : "Add evergreen content"}
              </h3>
            </div>
            {editingItemId ? (
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
              <span className="text-sm font-medium text-slate-700">Type</span>
              <select
                value={form.kind}
                onChange={(event) =>
                  setForm({ ...form, kind: event.target.value as BackupContentKind })
                }
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 bg-white px-3 text-sm outline-none focus:ring-1"
              >
                {backupKinds.map((kind) => (
                  <option key={kind} value={kind}>
                    {backupKindLabel(kind)}
                  </option>
                ))}
              </select>
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">Title</span>
              <input
                value={form.title}
                onChange={(event) => setForm({ ...form, title: event.target.value })}
                required
                placeholder="A rainy-day seawall recap"
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">Body</span>
              <textarea
                value={form.body}
                onChange={(event) => setForm({ ...form, body: event.target.value })}
                required
                rows={6}
                placeholder="What makes this worth knowing or doing in Vancouver?"
                className="focus:border-coral focus:ring-coral mt-1 w-full resize-y border border-slate-300 px-3 py-2 text-sm leading-6 outline-none focus:ring-1"
              />
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">Source URL</span>
              <input
                value={form.sourceUrl}
                onChange={(event) => setForm({ ...form, sourceUrl: event.target.value })}
                type="url"
                placeholder="https://example.com/local-history"
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">Media ref</span>
              <input
                value={form.mediaRef}
                onChange={(event) => setForm({ ...form, mediaRef: event.target.value })}
                placeholder="backup/library-image.jpg"
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <label className="flex items-center gap-2 text-sm text-slate-700">
              <input
                type="checkbox"
                checked={form.active}
                onChange={(event) => setForm({ ...form, active: event.target.checked })}
                className="text-coral focus:ring-coral h-4 w-4 border-slate-300"
              />
              Active in backup pool
            </label>
          </div>

          {formErrorMessage(createMutation.error ?? updateMutation.error) ? (
            <p className="mt-4 text-sm font-medium text-red-700">
              {formErrorMessage(createMutation.error ?? updateMutation.error)}
            </p>
          ) : null}

          <button
            type="submit"
            disabled={isSaving || !form.title.trim() || !form.body.trim()}
            className="bg-pine hover:bg-pine/90 focus-visible:ring-coral mt-5 w-full px-4 py-2 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {isSaving ? "Saving..." : editingItemId ? "Save item" : "Add item"}
          </button>
        </form>

        <section className="border border-slate-200 bg-white shadow-sm">
          <div className="flex flex-col gap-3 border-b border-slate-200 px-5 py-4 lg:flex-row lg:items-center lg:justify-between">
            <label className="min-w-0 flex-1">
              <span className="sr-only">Search backup library</span>
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search title, body, source URL, or media ref"
                className="focus:border-coral focus:ring-coral h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
              <label className="flex items-center gap-2 text-sm text-slate-700">
                <input
                  type="checkbox"
                  checked={showInactive}
                  onChange={(event) => setShowInactive(event.target.checked)}
                  className="text-coral focus:ring-coral h-4 w-4 border-slate-300"
                />
                Show inactive
              </label>
              <button
                type="button"
                onClick={() => {
                  void libraryQuery.refetch();
                  void calendarQuery.refetch();
                }}
                disabled={libraryQuery.isFetching || calendarQuery.isFetching}
                className="border border-slate-300 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-wait disabled:opacity-60"
              >
                {libraryQuery.isFetching || calendarQuery.isFetching ? "Refreshing..." : "Refresh"}
              </button>
            </div>
          </div>

          {message ? (
            <p className="mx-5 mt-5 border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm font-medium text-emerald-900">
              {message}
            </p>
          ) : null}

          {mutationError ? (
            <p className="mx-5 mt-5 text-sm font-medium text-red-700">
              {errorMessage(mutationError)}
            </p>
          ) : null}

          <div className="grid gap-3 border-b border-slate-100 p-5 text-sm lg:grid-cols-3">
            <BackupMetric label="Facts" value={factItems.toString()} />
            <BackupMetric label="Recaps" value={(items.length - factItems).toString()} />
            <BackupMetric label="Open slots" value={emptySlots.length.toString()} />
          </div>

          {libraryQuery.isLoading || calendarQuery.isLoading ? (
            <p className="p-6 text-sm text-slate-600">Loading backup library...</p>
          ) : null}

          {libraryQuery.isError ? (
            <p className="p-6 text-sm font-medium text-red-700">
              {errorMessage(libraryQuery.error)}
            </p>
          ) : null}

          {calendarQuery.isError ? (
            <p className="p-6 text-sm font-medium text-red-700">
              {errorMessage(calendarQuery.error)}
            </p>
          ) : null}

          {libraryQuery.isSuccess && items.length === 0 ? (
            <div className="p-6">
              <p className="text-base font-semibold text-slate-950">No backup items yet</p>
              <p className="mt-2 text-sm leading-6 text-slate-600">
                Add reusable Vancouver facts or recaps so slow days have material ready.
              </p>
            </div>
          ) : null}

          {libraryQuery.isSuccess && items.length > 0 && filteredItems.length === 0 ? (
            <div className="p-6">
              <p className="text-base font-semibold text-slate-950">No matching backup items</p>
              <p className="mt-2 text-sm text-slate-600">
                Clear the search field or include inactive items.
              </p>
            </div>
          ) : null}

          {filteredItems.length > 0 ? (
            <div className="divide-y divide-slate-100">
              {filteredItems.map((item) => (
                <BackupItem
                  key={item.id}
                  item={item}
                  emptySlots={emptySlots}
                  selectedSlotDate={slotSelections[item.id] ?? emptySlots[0]?.slot_date ?? ""}
                  isFilling={
                    fillSlotMutation.isPending && fillSlotMutation.variables?.item.id === item.id
                  }
                  isStatusUpdating={
                    statusMutation.isPending && statusMutation.variables?.id === item.id
                  }
                  onSelectSlot={(slotDate) =>
                    setSlotSelections((current) => ({ ...current, [item.id]: slotDate }))
                  }
                  onFill={() => fillSlot(item)}
                  onEdit={() => startEditing(item)}
                  onToggleActive={() =>
                    statusMutation.mutate({ id: item.id, active: !item.active })
                  }
                  onDelete={() => deleteItem(item)}
                />
              ))}
            </div>
          ) : null}
        </section>
      </section>
    </div>
  );
}

interface BackupMetricProps {
  label: string;
  value: string;
}

function BackupMetric({ label, value }: BackupMetricProps) {
  return (
    <div className="border border-slate-200 bg-slate-50 px-4 py-3">
      <p className="text-xs font-semibold uppercase text-slate-500">{label}</p>
      <p className="mt-1 text-2xl font-semibold text-slate-950">{value}</p>
    </div>
  );
}

interface BackupItemProps {
  item: BackupContentItemResponse;
  emptySlots: CalendarSlotResponse[];
  selectedSlotDate: string;
  isFilling: boolean;
  isStatusUpdating: boolean;
  onSelectSlot: (slotDate: string) => void;
  onFill: () => void;
  onEdit: () => void;
  onToggleActive: () => void;
  onDelete: () => void;
}

function BackupItem({
  item,
  emptySlots,
  selectedSlotDate,
  isFilling,
  isStatusUpdating,
  onSelectSlot,
  onFill,
  onEdit,
  onToggleActive,
  onDelete,
}: BackupItemProps) {
  return (
    <article className="p-5">
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_280px]">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="rounded-full bg-sky-100 px-2.5 py-1 text-xs font-semibold uppercase text-sky-800">
              {backupKindLabel(item.kind)}
            </span>
            <span
              className={[
                "rounded-full px-2.5 py-1 text-xs font-semibold uppercase",
                item.active ? "bg-emerald-100 text-emerald-800" : "bg-slate-100 text-slate-600",
              ].join(" ")}
            >
              {item.active ? "Active" : "Inactive"}
            </span>
            {item.media_ref ? (
              <span className="rounded-full bg-amber-100 px-2.5 py-1 text-xs font-semibold uppercase text-amber-800">
                Media
              </span>
            ) : null}
          </div>

          <h3 className="mt-3 text-xl font-semibold leading-7 text-slate-950">{item.title}</h3>
          <p className="mt-2 line-clamp-4 text-sm leading-6 text-slate-600">{item.body}</p>

          <dl className="mt-4 grid gap-2 text-xs text-slate-500 sm:grid-cols-3">
            <div>
              <dt className="font-semibold uppercase">Updated</dt>
              <dd className="mt-1">{formatDateTime(item.updated_at)}</dd>
            </div>
            <div>
              <dt className="font-semibold uppercase">Source</dt>
              <dd className="mt-1 truncate">{item.source_url ?? "Manual"}</dd>
            </div>
            <div>
              <dt className="font-semibold uppercase">Media ref</dt>
              <dd className="mt-1 truncate">{item.media_ref ?? "None"}</dd>
            </div>
          </dl>
        </div>

        <div className="grid gap-3 xl:self-start">
          <label className="block">
            <span className="text-sm font-medium text-slate-700">Flagged gap</span>
            <select
              value={selectedSlotDate}
              onChange={(event) => onSelectSlot(event.target.value)}
              disabled={emptySlots.length === 0 || !item.active}
              className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 bg-white px-3 text-sm outline-none focus:ring-1 disabled:cursor-not-allowed disabled:opacity-60"
            >
              {emptySlots.length === 0 ? <option value="">No upcoming gaps</option> : null}
              {emptySlots.map((slot) => (
                <option key={slot.slot_date} value={slot.slot_date}>
                  {formatSlotDate(slot)} at {formatSlotTime(slot.slot_time)}
                </option>
              ))}
            </select>
          </label>

          <button
            type="button"
            onClick={onFill}
            disabled={emptySlots.length === 0 || !item.active || isFilling}
            className="bg-pine hover:bg-pine/90 focus-visible:ring-coral px-4 py-2 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {isFilling ? "Filling slot..." : "Fill slot"}
          </button>

          <div className="grid grid-cols-3 gap-2">
            <button
              type="button"
              onClick={onEdit}
              className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50"
            >
              Edit
            </button>
            <button
              type="button"
              onClick={onToggleActive}
              disabled={isStatusUpdating}
              className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-wait disabled:opacity-60"
            >
              {item.active ? "Pause" : "Use"}
            </button>
            <button
              type="button"
              onClick={onDelete}
              className="border border-red-200 px-3 py-2 text-sm font-semibold text-red-700 hover:bg-red-50"
            >
              Remove
            </button>
          </div>
        </div>
      </div>
    </article>
  );
}

function backupFormPayload(
  form: BackupContentFormState,
): CreateBackupContentItemRequest & UpdateBackupContentItemRequest {
  return {
    kind: form.kind,
    title: form.title.trim(),
    body: form.body.trim(),
    source_url: optionalTrimmed(form.sourceUrl),
    media_ref: optionalTrimmed(form.mediaRef),
    active: form.active,
  };
}

function backupDraftNotes(item: BackupContentItemResponse): string {
  const parts = [
    `Backup content type: ${backupKindLabel(item.kind)}.`,
    item.body,
    item.source_url ? `Source: ${item.source_url}` : "",
    item.media_ref ? `Media reference: ${item.media_ref}` : "",
  ].filter(Boolean);

  return parts.join("\n\n");
}

function filterItems(
  items: BackupContentItemResponse[],
  query: string,
): BackupContentItemResponse[] {
  const normalizedQuery = query.trim().toLowerCase();

  if (!normalizedQuery) {
    return items;
  }

  return items.filter((item) =>
    [
      item.title,
      item.body,
      item.source_url ?? "",
      item.media_ref ?? "",
      backupKindLabel(item.kind),
      item.active ? "active" : "inactive",
    ]
      .join(" ")
      .toLowerCase()
      .includes(normalizedQuery),
  );
}

function sortBackupItems(
  left: BackupContentItemResponse,
  right: BackupContentItemResponse,
): number {
  if (left.active !== right.active) {
    return left.active ? -1 : 1;
  }

  return new Date(right.updated_at).getTime() - new Date(left.updated_at).getTime();
}

function optionalTrimmed(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function backupKindLabel(kind: BackupContentKind): string {
  switch (kind) {
    case "did_you_know":
      return "Did-you-know";
    case "past_recap":
      return "Past recap";
  }
}

function formatSlotDate(slot: CalendarSlotResponse): string {
  return new Intl.DateTimeFormat(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
  }).format(new Date(`${slot.slot_date}T00:00:00`));
}

function formatSlotTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(`2026-01-01T${value.slice(0, 5)}:00`));
}

function formatDateTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function formErrorMessage(error: unknown): string | null {
  if (!error) {
    return null;
  }

  return errorMessage(error);
}

function errorMessage(error: unknown): string {
  if (error instanceof ApiClientError || error instanceof Error) {
    return error.message;
  }

  return "Request failed";
}
