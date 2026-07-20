import { FormEvent, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiClientError, apiClient } from "../api/client";
import type { CalendarSlotResponse, DraftResponse } from "../api/types";
import { useAuth } from "../auth/useAuth";
import { LoginRoute } from "./LoginRoute";

const calendarDays = 14;

interface SlotDraftSelection {
  draftId: string;
  slotTime: string;
}

type SlotSelections = Record<string, SlotDraftSelection>;

export function CalendarRoute() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const [startDate, setStartDate] = useState(() => formatDateInput(new Date()));
  const [slotSelections, setSlotSelections] = useState<SlotSelections>({});
  const [message, setMessage] = useState<string | null>(null);

  const calendarQuery = useQuery({
    queryKey: ["calendar", startDate, calendarDays],
    queryFn: () => apiClient.listCalendar(calendarDays, startDate),
    enabled: auth.isAuthenticated,
  });

  const draftsQuery = useQuery({
    queryKey: ["drafts", 100],
    queryFn: () => apiClient.listDrafts(100),
    enabled: auth.isAuthenticated,
  });

  const approvedDrafts = useMemo(
    () =>
      (draftsQuery.data?.drafts ?? [])
        .filter((draft) => draft.status === "approved")
        .sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()),
    [draftsQuery.data?.drafts],
  );
  const scheduledSlots = calendarQuery.data?.slots.filter((slot) => !slot.is_empty).length ?? 0;
  const emptyUpcomingSlots = calendarQuery.data?.empty_upcoming_slots ?? 0;

  const assignMutation = useMutation({
    mutationFn: ({
      slotDate,
      draftId,
      slotTime,
    }: {
      slotDate: string;
      draftId: number;
      slotTime: string;
    }) =>
      apiClient.assignCalendarSlot({
        slot_date: slotDate,
        slot_time: slotTime,
        draft_id: draftId,
      }),
    onSuccess: (slot) => {
      setMessage(
        `Draft #${slot.draft?.id ?? "selected"} was scheduled for ${formatSlotDate(slot)}.`,
      );
      setSlotSelections((current) => {
        const next = { ...current };
        delete next[slot.slot_date];
        return next;
      });
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

  const mutationError = assignMutation.error;

  function handlePreviousWindow() {
    setStartDate((current) => shiftDate(current, -calendarDays));
    setMessage(null);
  }

  function handleToday() {
    setStartDate(formatDateInput(new Date()));
    setMessage(null);
  }

  function handleNextWindow() {
    setStartDate((current) => shiftDate(current, calendarDays));
    setMessage(null);
  }

  function updateSlotSelection(slotDate: string, patch: Partial<SlotDraftSelection>) {
    setSlotSelections((current) => ({
      ...current,
      [slotDate]: {
        draftId: current[slotDate]?.draftId ?? "",
        slotTime: current[slotDate]?.slotTime ?? "09:00:00",
        ...patch,
      },
    }));
  }

  function assignSlot(slot: CalendarSlotResponse, event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const selection = slotSelections[slot.slot_date];
    const draftId = Number(selection?.draftId);

    if (!Number.isInteger(draftId) || draftId < 1 || assignMutation.isPending) {
      return;
    }

    setMessage(null);
    assignMutation.mutate({
      slotDate: slot.slot_date,
      draftId,
      slotTime: selection?.slotTime || slot.slot_time,
    });
  }

  return (
    <div className="space-y-6">
      <section className="border border-slate-200 bg-white p-6 shadow-sm">
        <div className="flex flex-col gap-5 xl:flex-row xl:items-start xl:justify-between">
          <div>
            <p className="text-pine text-sm font-semibold uppercase">Calendar</p>
            <h2 className="mt-2 text-2xl font-semibold text-slate-950">Content scheduler</h2>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-600">
              Keep one approved Vancouver post assigned to each publishing day and catch upcoming
              gaps before they reach the feed.
            </p>
          </div>

          <div className="grid grid-cols-3 gap-3 text-sm">
            <CalendarMetric label="Window" value={`${calendarDays}d`} />
            <CalendarMetric label="Scheduled" value={scheduledSlots.toString()} />
            <CalendarMetric label="Gaps" value={emptyUpcomingSlots.toString()} />
          </div>
        </div>
      </section>

      <section className="border border-slate-200 bg-white shadow-sm">
        <div className="flex flex-col gap-3 border-b border-slate-200 px-5 py-4 lg:flex-row lg:items-center lg:justify-between">
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={handlePreviousWindow}
              className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50"
            >
              Previous
            </button>
            <button
              type="button"
              onClick={handleToday}
              className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50"
            >
              Today
            </button>
            <button
              type="button"
              onClick={handleNextWindow}
              className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50"
            >
              Next
            </button>
          </div>

          <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
            <label className="flex items-center gap-2 text-sm font-medium text-slate-700">
              Start
              <input
                type="date"
                value={startDate}
                onChange={(event) => {
                  setStartDate(event.target.value);
                  setMessage(null);
                }}
                className="focus:border-coral focus:ring-coral h-10 border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>
            <button
              type="button"
              onClick={() => {
                void calendarQuery.refetch();
                void draftsQuery.refetch();
              }}
              disabled={calendarQuery.isFetching || draftsQuery.isFetching}
              className="border border-slate-300 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-wait disabled:opacity-60"
            >
              {calendarQuery.isFetching || draftsQuery.isFetching ? "Refreshing..." : "Refresh"}
            </button>
          </div>
        </div>

        {calendarQuery.isLoading || draftsQuery.isLoading ? (
          <p className="p-6 text-sm text-slate-600">Loading calendar...</p>
        ) : null}

        {calendarQuery.isError ? (
          <p className="p-6 text-sm font-medium text-red-700">
            {errorMessage(calendarQuery.error)}
          </p>
        ) : null}

        {draftsQuery.isError ? (
          <p className="p-6 text-sm font-medium text-red-700">{errorMessage(draftsQuery.error)}</p>
        ) : null}

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

        {calendarQuery.isSuccess ? (
          <div className="grid gap-4 p-5 xl:grid-cols-[minmax(0,1fr)_340px]">
            <div className="grid gap-3">
              {calendarQuery.data.slots.map((slot) => (
                <CalendarSlot
                  key={slot.slot_date}
                  slot={slot}
                  approvedDrafts={approvedDrafts}
                  selection={slotSelections[slot.slot_date]}
                  isAssigning={
                    assignMutation.isPending &&
                    assignMutation.variables?.slotDate === slot.slot_date
                  }
                  onSelectionChange={(patch) => updateSlotSelection(slot.slot_date, patch)}
                  onAssign={(event) => assignSlot(slot, event)}
                />
              ))}
            </div>

            <aside className="border border-slate-200 bg-slate-50 p-4 xl:self-start">
              <div>
                <p className="text-sm font-semibold uppercase text-slate-500">Approved drafts</p>
                <p className="mt-1 text-2xl font-semibold text-slate-950">
                  {approvedDrafts.length}
                </p>
              </div>

              <div className="mt-4 space-y-3">
                {approvedDrafts.length === 0 ? (
                  <p className="text-sm leading-6 text-slate-600">
                    No approved drafts are waiting for a slot.
                  </p>
                ) : (
                  approvedDrafts
                    .slice(0, 6)
                    .map((draft) => <ApprovedDraftSummary key={draft.id} draft={draft} />)
                )}
              </div>
            </aside>
          </div>
        ) : null}
      </section>
    </div>
  );
}

interface CalendarMetricProps {
  label: string;
  value: string;
}

function CalendarMetric({ label, value }: CalendarMetricProps) {
  return (
    <div className="border border-slate-200 bg-slate-50 px-4 py-3">
      <p className="text-xs font-semibold uppercase text-slate-500">{label}</p>
      <p className="mt-1 text-2xl font-semibold text-slate-950">{value}</p>
    </div>
  );
}

interface CalendarSlotProps {
  slot: CalendarSlotResponse;
  approvedDrafts: DraftResponse[];
  selection?: SlotDraftSelection;
  isAssigning: boolean;
  onSelectionChange: (patch: Partial<SlotDraftSelection>) => void;
  onAssign: (event: FormEvent<HTMLFormElement>) => void;
}

function CalendarSlot({
  slot,
  approvedDrafts,
  selection,
  isAssigning,
  onSelectionChange,
  onAssign,
}: CalendarSlotProps) {
  const selectedDraftId = selection?.draftId ?? "";
  const selectedSlotTime = selection?.slotTime ?? slot.slot_time;
  const canAssign = slot.is_empty && selectedDraftId !== "" && !isAssigning;

  return (
    <article
      className={[
        "border p-4 shadow-sm",
        slot.is_empty && slot.is_upcoming ? "border-coral bg-coral/5" : "border-slate-200 bg-white",
      ].join(" ")}
    >
      <div className="grid gap-4 lg:grid-cols-[180px_minmax(0,1fr)]">
        <div>
          <p className="text-sm font-semibold text-slate-950">{formatSlotDate(slot)}</p>
          <p className="mt-1 text-sm text-slate-500">{formatSlotTime(slot.slot_time)}</p>
          <div className="mt-3">
            {slot.is_empty ? (
              <span className="rounded-full bg-amber-100 px-2.5 py-1 text-xs font-semibold uppercase text-amber-800">
                Gap
              </span>
            ) : (
              <span className="rounded-full bg-emerald-100 px-2.5 py-1 text-xs font-semibold uppercase text-emerald-800">
                Scheduled
              </span>
            )}
          </div>
        </div>

        {slot.draft ? (
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <span className="rounded-full bg-slate-100 px-2.5 py-1 text-xs font-semibold uppercase text-slate-700">
                Draft #{slot.draft.id}
              </span>
              <span className="rounded-full bg-sky-100 px-2.5 py-1 text-xs font-semibold uppercase text-sky-800">
                {slot.draft.status}
              </span>
            </div>
            <p className="mt-3 line-clamp-2 text-sm font-semibold leading-6 text-slate-950">
              {slot.draft.caption_en}
            </p>
            <p className="mt-2 line-clamp-2 text-sm leading-6 text-slate-600">
              {slot.draft.caption_zh}
            </p>
          </div>
        ) : (
          <form onSubmit={onAssign} className="grid gap-3 md:grid-cols-[minmax(0,1fr)_140px_110px]">
            <label className="block min-w-0">
              <span className="text-sm font-medium text-slate-700">Draft</span>
              <select
                value={selectedDraftId}
                onChange={(event) => onSelectionChange({ draftId: event.target.value })}
                disabled={approvedDrafts.length === 0}
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 bg-white px-3 text-sm outline-none focus:ring-1 disabled:cursor-not-allowed disabled:opacity-60"
              >
                <option value="">Select approved draft</option>
                {approvedDrafts.map((draft) => (
                  <option key={draft.id} value={draft.id}>
                    #{draft.id} {headline(draft)}
                  </option>
                ))}
              </select>
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">Time</span>
              <input
                type="time"
                step="60"
                value={toTimeInputValue(selectedSlotTime)}
                onChange={(event) => onSelectionChange({ slotTime: `${event.target.value}:00` })}
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <button
              type="submit"
              disabled={!canAssign}
              className="bg-pine hover:bg-pine/90 focus-visible:ring-coral self-end px-4 py-2 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60"
            >
              {isAssigning ? "Assigning..." : "Assign"}
            </button>
          </form>
        )}
      </div>
    </article>
  );
}

function ApprovedDraftSummary({ draft }: { draft: DraftResponse }) {
  return (
    <article className="border border-slate-200 bg-white p-3">
      <p className="text-xs font-semibold uppercase text-slate-500">Draft #{draft.id}</p>
      <p className="mt-2 line-clamp-2 text-sm font-semibold leading-5 text-slate-950">
        {draft.caption_en}
      </p>
      <p className="mt-2 text-xs text-slate-500">Approved {formatDateTime(draft.updated_at)}</p>
    </article>
  );
}

function shiftDate(value: string, days: number): string {
  const date = new Date(`${value}T00:00:00`);
  date.setDate(date.getDate() + days);
  return formatDateInput(date);
}

function formatDateInput(value: Date): string {
  const year = value.getFullYear();
  const month = `${value.getMonth() + 1}`.padStart(2, "0");
  const day = `${value.getDate()}`.padStart(2, "0");
  return `${year}-${month}-${day}`;
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
  }).format(new Date(`2026-01-01T${toTimeInputValue(value)}:00`));
}

function formatDateTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function toTimeInputValue(value: string): string {
  return value.slice(0, 5);
}

function headline(draft: DraftResponse): string {
  const normalized = draft.caption_en.split(/\s+/).filter(Boolean).join(" ");

  if (normalized.length <= 48) {
    return normalized;
  }

  return `${normalized.slice(0, 48).trimEnd()}...`;
}

function errorMessage(error: unknown): string {
  if (error instanceof ApiClientError || error instanceof Error) {
    return error.message;
  }

  return "Request failed";
}
