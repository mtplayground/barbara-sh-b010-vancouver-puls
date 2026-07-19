import { useQuery } from "@tanstack/react-query";

import { apiClient } from "./api/client";
import type { StorageHealthResponse } from "./api/types";

export function App() {
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
    <main className="bg-paper min-h-screen text-slate-950">
      <section className="mx-auto flex min-h-screen w-full max-w-5xl flex-col justify-center px-6 py-12">
        <div className="max-w-3xl">
          <p className="text-pine mb-3 text-sm font-semibold uppercase tracking-wide">
            Service health
          </p>
          <h1 className="text-4xl font-semibold leading-tight sm:text-5xl">
            API connectivity is wired through the client layer.
          </h1>
          <p className="mt-5 text-lg leading-8 text-slate-700">
            The frontend fetches backend status through a typed API client managed by React Query.
          </p>
        </div>

        <div className="mt-10 grid gap-4 md:grid-cols-3">
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
        </div>
      </section>
    </main>
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
        <h2 className="text-base font-semibold text-slate-950">{label}</h2>
        <span className={statusClassName(displayStatus)}>{displayStatus}</span>
      </div>
      <p className="mt-3 min-h-6 text-sm text-slate-600">
        {error ? error.message : (detail ?? "Waiting for the backend response.")}
      </p>
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
