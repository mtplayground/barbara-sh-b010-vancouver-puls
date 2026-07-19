import { useQuery } from "@tanstack/react-query";

import { apiClient } from "../api/client";
import type { StorageHealthResponse } from "../api/types";
import { useAuth } from "../auth/useAuth";

export function DashboardRoute() {
  const auth = useAuth();
  const apiHealth = useQuery({
    queryKey: ["api-health"],
    queryFn: apiClient.getHealth,
  });
  const databaseHealth = useQuery({
    queryKey: ["database-health"],
    queryFn: apiClient.getDatabaseHealth,
  });
  const storageHealth = useQuery({
    queryKey: ["storage-health"],
    queryFn: apiClient.getStorageHealth,
  });

  return (
    <div className="space-y-6">
      <section className="bg-white px-5 py-5 shadow-sm">
        <p className="text-pine text-sm font-semibold uppercase tracking-wide">Overview</p>
        <h2 className="mt-2 text-3xl font-semibold tracking-normal">Workspace dashboard</h2>
        <p className="mt-3 max-w-3xl text-base leading-7 text-slate-700">
          Routing, navigation, API status checks, and auth state scaffolding are ready for the
          feature slices that follow.
        </p>
      </section>

      <section className="grid gap-4 md:grid-cols-3">
        <StatusPanel
          label="API"
          detail={apiHealth.data ? `service: ${apiHealth.data.service}` : undefined}
          error={apiHealth.error}
          isLoading={apiHealth.isLoading}
          status={apiHealth.data?.status}
        />
        <StatusPanel
          label="Database"
          detail={databaseHealth.data ? `engine: ${databaseHealth.data.database}` : undefined}
          error={databaseHealth.error}
          isLoading={databaseHealth.isLoading}
          status={databaseHealth.data?.status}
        />
        <StatusPanel
          label="Storage"
          detail={storageDetail(storageHealth.data)}
          error={storageHealth.error}
          isLoading={storageHealth.isLoading}
          status={storageHealth.data?.status}
        />
      </section>

      <section className="grid gap-4 md:grid-cols-2">
        <SummaryPanel
          label="Current auth state"
          value={auth.status}
          detail="Session loading will be wired by the auth task."
        />
        <SummaryPanel
          label="Assigned roles"
          value={auth.roles.length > 0 ? auth.roles.join(", ") : "none"}
          detail="Role data is reserved for the user-management workflow."
        />
      </section>
    </div>
  );
}

interface StatusPanelProps {
  label: string;
  status?: "ok" | "disabled";
  detail?: string;
  error: Error | null;
  isLoading: boolean;
}

function StatusPanel({ label, status, detail, error, isLoading }: StatusPanelProps) {
  const displayStatus = isLoading ? "checking" : error ? "error" : status;

  return (
    <article className="border-coral border-l-4 bg-white px-5 py-4 shadow-sm">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-base font-semibold text-slate-950">{label}</h3>
        <span className={statusClassName(displayStatus)}>{displayStatus}</span>
      </div>
      <p className="mt-3 min-h-6 text-sm text-slate-600">
        {error ? error.message : (detail ?? "Waiting for the backend response.")}
      </p>
    </article>
  );
}

interface SummaryPanelProps {
  label: string;
  value: string;
  detail: string;
}

function SummaryPanel({ label, value, detail }: SummaryPanelProps) {
  return (
    <article className="bg-white px-5 py-4 shadow-sm">
      <p className="text-sm font-medium text-slate-500">{label}</p>
      <p className="mt-2 text-xl font-semibold text-slate-950">{value}</p>
      <p className="mt-2 text-sm leading-6 text-slate-600">{detail}</p>
    </article>
  );
}

function storageDetail(storage?: StorageHealthResponse): string | undefined {
  if (!storage) {
    return undefined;
  }

  if (storage.status === "disabled") {
    return "object storage is not configured";
  }

  return storage.prefix
    ? `bucket: ${storage.bucket} / ${storage.prefix}`
    : `bucket: ${storage.bucket}`;
}

function statusClassName(status: StatusPanelProps["status"] | "checking" | "error"): string {
  const baseClassName = "rounded-full px-2.5 py-1 text-xs font-semibold uppercase";

  switch (status) {
    case "ok":
      return `${baseClassName} bg-emerald-100 text-emerald-800`;
    case "disabled":
      return `${baseClassName} bg-slate-100 text-slate-700`;
    case "error":
      return `${baseClassName} bg-rose-100 text-rose-800`;
    default:
      return `${baseClassName} bg-amber-100 text-amber-800`;
  }
}
