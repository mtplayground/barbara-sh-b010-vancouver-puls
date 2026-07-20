import { FormEvent, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiClientError, apiClient } from "../api/client";
import type { DraftResponse, DraftStatus } from "../api/types";
import { useAuth } from "../auth/useAuth";
import { LoginRoute } from "./LoginRoute";

const draftLimit = 75;

interface DraftFormState {
  captionEn: string;
  captionZh: string;
}

interface ManualDraftFormState {
  topic: string;
  notes: string;
}

const emptyDraftForm: DraftFormState = {
  captionEn: "",
  captionZh: "",
};

const emptyManualDraftForm: ManualDraftFormState = {
  topic: "",
  notes: "",
};

type PreviewMode = "post" | "reel";

export function DraftsRoute() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const sourceItemId = parsePositiveNumber(searchParams.get("item"));
  const [selectedDraftId, setSelectedDraftId] = useState<number | null>(null);
  const [form, setForm] = useState<DraftFormState>(emptyDraftForm);
  const [manualForm, setManualForm] = useState<ManualDraftFormState>(emptyManualDraftForm);
  const [previewMode, setPreviewMode] = useState<PreviewMode>("post");
  const [message, setMessage] = useState<string | null>(null);

  const draftsQuery = useQuery({
    queryKey: ["drafts", draftLimit],
    queryFn: () => apiClient.listDrafts(draftLimit),
    enabled: auth.isAuthenticated,
  });

  const drafts = useMemo(
    () => [...(draftsQuery.data?.drafts ?? [])].sort(sortDrafts),
    [draftsQuery.data?.drafts],
  );
  const selectedDraft = drafts.find((draft) => draft.id === selectedDraftId) ?? drafts[0] ?? null;
  const sourceLinkedDrafts = drafts.filter((draft) => draft.source_item_id !== null).length;
  const renderedDrafts = drafts.filter(
    (draft) => draft.rendered_post_asset_ref && draft.rendered_reel_asset_ref,
  ).length;
  const dirty =
    selectedDraft !== null &&
    (form.captionEn !== selectedDraft.caption_en || form.captionZh !== selectedDraft.caption_zh);
  const previewDataUri = useMemo(
    () => previewSvgDataUri(form.captionEn, form.captionZh, previewMode),
    [form.captionEn, form.captionZh, previewMode],
  );

  const createFromItemMutation = useMutation({
    mutationFn: (itemId: number) => apiClient.createDraft({ source_item_id: itemId }),
    onSuccess: (created) => {
      setSelectedDraftId(created.id);
      setForm(draftToForm(created));
      setMessage(`Draft #${created.id} was generated from item #${created.source_item_id}.`);
      setSearchParams({});
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    },
  });

  const createManualMutation = useMutation({
    mutationFn: (draft: ManualDraftFormState) =>
      apiClient.createDraft({
        manual_topic: draft.topic,
        manual_notes: draft.notes || undefined,
      }),
    onSuccess: (created) => {
      setSelectedDraftId(created.id);
      setForm(draftToForm(created));
      setManualForm(emptyManualDraftForm);
      setMessage(`Draft #${created.id} was generated.`);
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    },
  });

  const saveMutation = useMutation({
    mutationFn: ({ id, state }: { id: number; state: DraftFormState }) =>
      apiClient.updateDraft(id, {
        caption_en: state.captionEn,
        caption_zh: state.captionZh,
      }),
    onSuccess: (updated) => {
      setSelectedDraftId(updated.id);
      setForm(draftToForm(updated));
      setMessage(`Draft #${updated.id} was saved.`);
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    },
  });

  const regenerateMutation = useMutation({
    mutationFn: (id: number) => apiClient.regenerateDraft(id),
    onSuccess: (updated) => {
      setSelectedDraftId(updated.id);
      setForm(draftToForm(updated));
      setMessage(`Draft #${updated.id} was regenerated.`);
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    },
  });

  const renderMutation = useMutation({
    mutationFn: (id: number) => apiClient.renderDraft(id),
    onSuccess: (rendered) => {
      setSelectedDraftId(rendered.draft.id);
      setForm(draftToForm(rendered.draft));
      setMessage(`Draft #${rendered.draft.id} assets were rendered.`);
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    },
  });

  const approveMutation = useMutation({
    mutationFn: (id: number) => apiClient.updateDraft(id, { status: "approved" }),
    onSuccess: (updated) => {
      setSelectedDraftId(updated.id);
      setForm(draftToForm(updated));
      setMessage(`Draft #${updated.id} was approved.`);
      void queryClient.invalidateQueries({ queryKey: ["drafts"] });
    },
  });

  useEffect(() => {
    if (!selectedDraftId && drafts.length > 0) {
      setSelectedDraftId(drafts[0].id);
    }
  }, [drafts, selectedDraftId]);

  useEffect(() => {
    if (selectedDraft) {
      setForm(draftToForm(selectedDraft));
    }
  }, [selectedDraft]);

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

  const selectedCanApprove =
    selectedDraft !== null &&
    selectedDraft.status !== "approved" &&
    Boolean(selectedDraft.rendered_post_asset_ref && selectedDraft.rendered_reel_asset_ref);
  const mutationError =
    createFromItemMutation.error ??
    createManualMutation.error ??
    saveMutation.error ??
    regenerateMutation.error ??
    renderMutation.error ??
    approveMutation.error;

  function handleManualSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!manualForm.topic.trim() || createManualMutation.isPending) {
      return;
    }

    setMessage(null);
    createManualMutation.mutate(manualForm);
  }

  function saveSelectedDraft() {
    if (!selectedDraft || !dirty || saveMutation.isPending) {
      return;
    }

    setMessage(null);
    saveMutation.mutate({ id: selectedDraft.id, state: form });
  }

  function regenerateSelectedDraft() {
    if (!selectedDraft || regenerateMutation.isPending) {
      return;
    }

    setMessage(null);
    regenerateMutation.mutate(selectedDraft.id);
  }

  function renderSelectedDraft() {
    if (!selectedDraft || renderMutation.isPending || dirty) {
      return;
    }

    setMessage(null);
    renderMutation.mutate(selectedDraft.id);
  }

  function approveSelectedDraft() {
    if (!selectedDraft || !selectedCanApprove || approveMutation.isPending || dirty) {
      return;
    }

    setMessage(null);
    approveMutation.mutate(selectedDraft.id);
  }

  return (
    <div className="space-y-6">
      <section className="border border-slate-200 bg-white p-6 shadow-sm">
        <div className="flex flex-col gap-5 lg:flex-row lg:items-start lg:justify-between">
          <div>
            <p className="text-pine text-sm font-semibold uppercase">Drafts</p>
            <h2 className="mt-2 text-2xl font-semibold text-slate-950">Bilingual draft editor</h2>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-600">
              Review generated captions, keep English and Chinese versions aligned, and render the
              final feed assets before approval.
            </p>
          </div>

          <div className="grid grid-cols-3 gap-3 text-sm">
            <DraftMetric label="Drafts" value={drafts.length.toString()} />
            <DraftMetric label="From items" value={sourceLinkedDrafts.toString()} />
            <DraftMetric label="Rendered" value={renderedDrafts.toString()} />
          </div>
        </div>
      </section>

      {sourceItemId ? (
        <section className="border border-amber-200 bg-amber-50 p-5 shadow-sm">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <p className="text-sm font-semibold uppercase text-amber-800">
                Inbox item #{sourceItemId}
              </p>
              <h3 className="mt-1 text-lg font-semibold text-slate-950">Generate source draft</h3>
            </div>
            <button
              type="button"
              onClick={() => createFromItemMutation.mutate(sourceItemId)}
              disabled={createFromItemMutation.isPending}
              className="bg-pine hover:bg-pine/90 focus-visible:ring-coral px-4 py-2 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-wait disabled:opacity-60"
            >
              {createFromItemMutation.isPending ? "Generating..." : "Generate draft"}
            </button>
          </div>
        </section>
      ) : null}

      <section className="grid gap-6 xl:grid-cols-[300px_minmax(0,1fr)]">
        <aside className="space-y-4 xl:self-start">
          <form
            onSubmit={handleManualSubmit}
            className="border border-slate-200 bg-white p-5 shadow-sm"
          >
            <p className="text-sm font-semibold uppercase text-slate-500">Manual draft</p>
            <label className="mt-4 block">
              <span className="text-sm font-medium text-slate-700">Topic</span>
              <input
                value={manualForm.topic}
                onChange={(event) => setManualForm({ ...manualForm, topic: event.target.value })}
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
                placeholder="Weekend seawall closure"
              />
            </label>
            <label className="mt-4 block">
              <span className="text-sm font-medium text-slate-700">Notes</span>
              <textarea
                value={manualForm.notes}
                onChange={(event) => setManualForm({ ...manualForm, notes: event.target.value })}
                className="focus:border-coral focus:ring-coral mt-1 min-h-24 w-full resize-y border border-slate-300 px-3 py-2 text-sm outline-none focus:ring-1"
              />
            </label>
            <button
              type="submit"
              disabled={createManualMutation.isPending || !manualForm.topic.trim()}
              className="bg-pine hover:bg-pine/90 focus-visible:ring-coral mt-4 h-11 w-full px-4 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60"
            >
              {createManualMutation.isPending ? "Generating..." : "Generate"}
            </button>
          </form>

          <div className="border border-slate-200 bg-white shadow-sm">
            <div className="flex items-center justify-between border-b border-slate-200 px-4 py-3">
              <h3 className="text-sm font-semibold uppercase text-slate-600">Queue</h3>
              <button
                type="button"
                onClick={() => void draftsQuery.refetch()}
                disabled={draftsQuery.isFetching}
                className="text-sm font-semibold text-slate-700 hover:text-slate-950 disabled:cursor-wait disabled:opacity-60"
              >
                {draftsQuery.isFetching ? "Refreshing" : "Refresh"}
              </button>
            </div>

            {draftsQuery.isLoading ? (
              <p className="p-4 text-sm text-slate-600">Loading drafts...</p>
            ) : null}

            {draftsQuery.isError ? (
              <p className="p-4 text-sm font-medium text-red-700">
                {errorMessage(draftsQuery.error)}
              </p>
            ) : null}

            {draftsQuery.isSuccess && drafts.length === 0 ? (
              <p className="p-4 text-sm text-slate-600">No drafts yet.</p>
            ) : null}

            {drafts.length > 0 ? (
              <div className="divide-y divide-slate-100">
                {drafts.map((draft) => (
                  <button
                    key={draft.id}
                    type="button"
                    onClick={() => setSelectedDraftId(draft.id)}
                    className={[
                      "block w-full px-4 py-3 text-left hover:bg-slate-50",
                      selectedDraft?.id === draft.id ? "bg-slate-50" : "",
                    ].join(" ")}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-sm font-semibold text-slate-950">
                        Draft #{draft.id}
                      </span>
                      <StatusBadge status={draft.status} />
                    </div>
                    <p className="mt-2 line-clamp-2 text-sm leading-5 text-slate-600">
                      {draft.caption_en}
                    </p>
                    <p className="mt-2 text-xs text-slate-500">
                      {formatDateTime(draft.updated_at)}
                    </p>
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        </aside>

        <div className="grid min-w-0 gap-6 2xl:grid-cols-[minmax(0,1fr)_420px]">
          <section className="min-w-0 border border-slate-200 bg-white shadow-sm">
            {selectedDraft ? (
              <>
                <div className="border-b border-slate-200 px-5 py-4">
                  <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                    <div>
                      <div className="flex flex-wrap items-center gap-2">
                        <StatusBadge status={selectedDraft.status} />
                        {selectedDraft.source_item_id ? (
                          <span className="rounded-full bg-sky-100 px-2.5 py-1 text-xs font-semibold uppercase text-sky-800">
                            Item #{selectedDraft.source_item_id}
                          </span>
                        ) : (
                          <span className="rounded-full bg-slate-100 px-2.5 py-1 text-xs font-semibold uppercase text-slate-700">
                            Manual
                          </span>
                        )}
                      </div>
                      <h3 className="mt-3 text-xl font-semibold text-slate-950">
                        Draft #{selectedDraft.id}
                      </h3>
                      <p className="mt-1 text-sm text-slate-500">
                        Updated {formatDateTime(selectedDraft.updated_at)}
                      </p>
                    </div>

                    <div className="flex flex-wrap gap-2">
                      <button
                        type="button"
                        onClick={regenerateSelectedDraft}
                        disabled={regenerateMutation.isPending}
                        className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-wait disabled:opacity-60"
                      >
                        {regenerateMutation.isPending ? "Regenerating..." : "Regenerate"}
                      </button>
                      <button
                        type="button"
                        onClick={saveSelectedDraft}
                        disabled={!dirty || saveMutation.isPending}
                        className="bg-pine hover:bg-pine/90 focus-visible:ring-coral px-3 py-2 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60"
                      >
                        {saveMutation.isPending ? "Saving..." : "Save"}
                      </button>
                    </div>
                  </div>
                </div>

                <div className="grid gap-5 p-5">
                  <label className="block">
                    <span className="text-sm font-semibold text-slate-700">English caption</span>
                    <textarea
                      value={form.captionEn}
                      onChange={(event) => setForm({ ...form, captionEn: event.target.value })}
                      className="focus:border-coral focus:ring-coral mt-2 min-h-52 w-full resize-y border border-slate-300 px-3 py-3 text-sm leading-6 outline-none focus:ring-1"
                    />
                  </label>

                  <label className="block">
                    <span className="text-sm font-semibold text-slate-700">中文 caption</span>
                    <textarea
                      value={form.captionZh}
                      onChange={(event) => setForm({ ...form, captionZh: event.target.value })}
                      className="focus:border-coral focus:ring-coral mt-2 min-h-44 w-full resize-y border border-slate-300 px-3 py-3 text-sm leading-6 outline-none focus:ring-1"
                    />
                  </label>

                  {message ? (
                    <p className="border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm font-medium text-emerald-900">
                      {message}
                    </p>
                  ) : null}

                  {mutationError ? (
                    <p className="text-sm font-medium text-red-700">
                      {errorMessage(mutationError)}
                    </p>
                  ) : null}

                  <div className="grid gap-3 border-t border-slate-200 pt-5 sm:grid-cols-3">
                    <AssetRef label="Post asset" value={selectedDraft.rendered_post_asset_ref} />
                    <AssetRef label="Reel asset" value={selectedDraft.rendered_reel_asset_ref} />
                    <AssetRef label="Updated by" value={selectedDraft.updated_by_sub} />
                  </div>
                </div>
              </>
            ) : (
              <div className="p-6">
                <p className="text-base font-semibold text-slate-950">No draft selected</p>
                <p className="mt-2 text-sm text-slate-600">Create or select a draft to edit.</p>
              </div>
            )}
          </section>

          <section className="border border-slate-200 bg-white shadow-sm 2xl:self-start">
            <div className="border-b border-slate-200 px-5 py-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold uppercase text-slate-500">Preview</p>
                  <h3 className="mt-1 text-lg font-semibold text-slate-950">Rendered asset</h3>
                </div>
                <div className="flex border border-slate-300">
                  {(["post", "reel"] as PreviewMode[]).map((mode) => (
                    <button
                      key={mode}
                      type="button"
                      onClick={() => setPreviewMode(mode)}
                      className={[
                        "px-3 py-2 text-sm font-semibold capitalize",
                        previewMode === mode
                          ? "bg-slate-950 text-white"
                          : "bg-white text-slate-700",
                      ].join(" ")}
                    >
                      {mode}
                    </button>
                  ))}
                </div>
              </div>
            </div>

            <div className="p-5">
              <div className="bg-slate-950 p-3">
                <img
                  src={previewDataUri}
                  alt=""
                  className={[
                    "mx-auto block w-full max-w-sm bg-white object-contain",
                    previewMode === "post" ? "aspect-[4/5]" : "aspect-[9/16]",
                  ].join(" ")}
                />
              </div>

              <div className="mt-4 grid gap-2 sm:grid-cols-2 2xl:grid-cols-1">
                <button
                  type="button"
                  onClick={renderSelectedDraft}
                  disabled={!selectedDraft || dirty || renderMutation.isPending}
                  className="border border-slate-300 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {renderMutation.isPending ? "Rendering..." : "Render assets"}
                </button>
                <button
                  type="button"
                  onClick={approveSelectedDraft}
                  disabled={!selectedCanApprove || approveMutation.isPending || dirty}
                  className="bg-coral hover:bg-coral/90 focus-visible:ring-coral px-4 py-2 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {approveMutation.isPending ? "Approving..." : "Approve"}
                </button>
              </div>
            </div>
          </section>
        </div>
      </section>
    </div>
  );
}

interface DraftMetricProps {
  label: string;
  value: string;
}

function DraftMetric({ label, value }: DraftMetricProps) {
  return (
    <div className="border border-slate-200 bg-slate-50 px-4 py-3">
      <p className="text-xs font-semibold uppercase text-slate-500">{label}</p>
      <p className="mt-1 text-2xl font-semibold text-slate-950">{value}</p>
    </div>
  );
}

interface StatusBadgeProps {
  status: DraftStatus;
}

function StatusBadge({ status }: StatusBadgeProps) {
  const className =
    status === "approved"
      ? "bg-emerald-100 text-emerald-800"
      : status === "draft"
        ? "bg-amber-100 text-amber-800"
        : "bg-slate-100 text-slate-700";

  return (
    <span className={`rounded-full px-2.5 py-1 text-xs font-semibold uppercase ${className}`}>
      {statusLabel(status)}
    </span>
  );
}

interface AssetRefProps {
  label: string;
  value: string | null;
}

function AssetRef({ label, value }: AssetRefProps) {
  return (
    <div className="min-w-0 border border-slate-200 bg-slate-50 px-3 py-2">
      <p className="text-xs font-semibold uppercase text-slate-500">{label}</p>
      <p className="mt-1 truncate text-sm font-medium text-slate-800">{value ?? "None"}</p>
    </div>
  );
}

function draftToForm(draft: DraftResponse): DraftFormState {
  return {
    captionEn: draft.caption_en,
    captionZh: draft.caption_zh,
  };
}

function sortDrafts(a: DraftResponse, b: DraftResponse): number {
  return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
}

function parsePositiveNumber(value: string | null): number | null {
  if (!value) {
    return null;
  }

  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

function statusLabel(status: DraftStatus): string {
  return status.replace(/_/g, " ");
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

function previewSvgDataUri(captionEn: string, captionZh: string, mode: PreviewMode): string {
  const width = 1080;
  const height = mode === "post" ? 1350 : 1920;
  const title = wrapSvgText(headlineFrom(captionEn, 62), 20, 3);
  const subtitle = wrapSvgText(headlineFrom(captionZh, 36), 15, 2);
  const en = wrapSvgText(captionEn, 42, 4);
  const zh = wrapSvgText(captionZh, 25, 3);
  const skylineY = mode === "post" ? 490 : 720;
  const captionY = height - 330;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
<defs>
<linearGradient id="sky" x1="0" x2="1" y1="0" y2="1"><stop offset="0%" stop-color="#00d2ff"/><stop offset="45%" stop-color="#39ff88"/><stop offset="100%" stop-color="#ff3d7f"/></linearGradient>
<linearGradient id="panel" x1="0" x2="0" y1="0" y2="1"><stop offset="0%" stop-color="#fff7d6"/><stop offset="100%" stop-color="#ffffff"/></linearGradient>
</defs>
<rect width="${width}" height="${height}" fill="url(#sky)"/>
<circle cx="${width - 130}" cy="120" r="245" fill="#ffe600" opacity="0.86"/>
<path d="M0 ${skylineY - 140} C170 ${skylineY - 264}, 250 ${skylineY - 210}, 372 ${skylineY - 140} C540 ${skylineY - 330}, 620 ${skylineY - 264}, 782 ${skylineY - 140} C895 ${skylineY - 210}, 984 ${skylineY - 330}, ${width} ${skylineY - 140} L${width} ${skylineY} L0 ${skylineY} Z" fill="#ffffff" opacity="0.76"/>
<g transform="translate(0 ${skylineY})"><rect x="0" y="200" width="${width}" height="118" fill="#101827"/><rect x="52" y="96" width="98" height="222" fill="#16213a"/><rect x="178" y="20" width="116" height="298" fill="#243b72"/><rect x="332" y="138" width="126" height="180" fill="#172554"/><rect x="486" y="58" width="92" height="260" fill="#111827"/><rect x="626" y="0" width="140" height="318" fill="#2563eb"/><path d="M0 274 C230 230 398 338 606 282 C785 235 895 220 ${width} 246 L${width} 318 L0 318 Z" fill="#00f5d4" opacity="0.86"/><path d="M90 258 C270 232 430 282 612 250 C795 218 944 212 1030 230" fill="none" stroke="#fffb00" stroke-width="20" stroke-linecap="round"/><circle cx="182" cy="230" r="17" fill="#ff3d7f"/><circle cx="638" cy="244" r="17" fill="#ff3d7f"/></g>
<rect x="54" y="58" width="${width - 108}" height="${skylineY - 74}" rx="38" fill="url(#panel)" opacity="0.97"/>
<rect x="54" y="58" width="${width - 108}" height="18" rx="9" fill="#ff3d7f"/>
<text x="92" y="126" font-family="Inter, Arial, sans-serif" font-size="24" font-weight="900" fill="#111827">VANCOUVER</text>
${svgTextLines(title, 88, 220, mode === "post" ? 74 : 78, "#101827", 900)}
<rect x="88" y="${mode === "post" ? 440 : 470}" width="${width - 176}" height="134" rx="24" fill="#00d2ff" opacity="0.95"/>
${svgTextLines(subtitle, 108, mode === "post" ? 492 : 526, mode === "post" ? 42 : 44, "#06111f", 800)}
<rect x="70" y="${captionY - 74}" width="${width - 140}" height="248" rx="32" fill="#111827" opacity="0.91"/>
<text x="108" y="${captionY - 24}" font-family="Inter, Arial, sans-serif" font-size="22" font-weight="900" fill="#39ff88">EN</text>
${svgTextLines(en, 162, captionY - 24, mode === "post" ? 31 : 34, "#ffffff", 700)}
<text x="108" y="${captionY + 120}" font-family="Inter, Arial, sans-serif" font-size="22" font-weight="900" fill="#ffe600">中文</text>
${svgTextLines(zh, 162, captionY + 120, mode === "post" ? 31 : 34, "#ffffff", 700)}
</svg>`;

  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function headlineFrom(caption: string, maxChars: number): string {
  const normalized = caption.split(/\s+/).filter(Boolean).join(" ");
  const firstSentence = normalized.split(/[.!?。！？]/)[0]?.trim() || normalized;
  return truncateChars(firstSentence || "Draft preview", maxChars);
}

function wrapSvgText(value: string, maxCharsPerLine: number, maxLines: number): string[] {
  const words = value.split(/\s+/).filter(Boolean);
  const lines: string[] = [];
  let current = "";

  for (const word of words.length > 0 ? words : ["Draft"]) {
    const candidate = current ? `${current} ${word}` : word;

    if (candidate.length > maxCharsPerLine && current) {
      lines.push(current);
      current = word;

      if (lines.length === maxLines) {
        lines[maxLines - 1] = truncateChars(`${lines[maxLines - 1]} ${word}`, maxCharsPerLine);
        return lines;
      }
    } else {
      current = candidate;
    }
  }

  if (current) {
    lines.push(current);
  }

  if (lines.length > maxLines) {
    lines.length = maxLines;
  }

  const last = lines[lines.length - 1];
  if (last) {
    lines[lines.length - 1] = truncateChars(last, maxCharsPerLine);
  }

  return lines;
}

function truncateChars(value: string, maxChars: number): string {
  if (value.length <= maxChars) {
    return value;
  }

  return `${value.slice(0, maxChars).trimEnd()}...`;
}

function svgTextLines(
  lines: string[],
  x: number,
  y: number,
  size: number,
  fill: string,
  weight: number,
): string {
  return lines
    .map(
      (line, index) =>
        `<text x="${x}" y="${y + index * (size + 9)}" font-family="Inter, Arial, sans-serif" font-size="${size}" font-weight="${weight}" fill="${fill}">${escapeXml(line)}</text>`,
    )
    .join("");
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}
