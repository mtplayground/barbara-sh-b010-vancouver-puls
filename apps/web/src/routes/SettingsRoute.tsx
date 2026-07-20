import { FormEvent, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiClientError, apiClient } from "../api/client";
import type {
  ConnectInstagramRequest,
  InstagramAccountType,
  InstagramConnectionResponse,
  InstagramStatusResponse,
} from "../api/types";
import { useAuth } from "../auth/useAuth";
import { LoginRoute } from "./LoginRoute";

interface InstagramFormState {
  instagramAccountId: string;
  username: string;
  accountType: InstagramAccountType;
}

const defaultInstagramForm: InstagramFormState = {
  instagramAccountId: "",
  username: "",
  accountType: "business",
};

export function SettingsRoute() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const isAdmin = auth.roles.includes("admin");
  const [form, setForm] = useState<InstagramFormState>(defaultInstagramForm);
  const [message, setMessage] = useState<string | null>(null);

  const instagramQuery = useQuery({
    queryKey: ["settings", "instagram"],
    queryFn: apiClient.getInstagramStatus,
    enabled: isAdmin,
  });

  const connectMutation = useMutation({
    mutationFn: (connection: ConnectInstagramRequest) => apiClient.connectInstagram(connection),
    onSuccess: (status) => {
      setMessage(connectionMessage(status));
      seedFormFromStatus(status, setForm);
      void queryClient.setQueryData(["settings", "instagram"], status);
    },
  });

  const disconnectMutation = useMutation({
    mutationFn: apiClient.disconnectInstagram,
    onSuccess: (status) => {
      setMessage("Instagram publishing is disconnected.");
      void queryClient.setQueryData(["settings", "instagram"], status);
    },
  });

  useEffect(() => {
    if (instagramQuery.data) {
      seedFormFromStatus(instagramQuery.data, setForm);
    }
  }, [instagramQuery.data]);

  const status = instagramQuery.data;
  const account = status?.account ?? null;
  const connectionHealth = useMemo(() => connectionHealthLabel(status), [status]);
  const isSaving = connectMutation.isPending || disconnectMutation.isPending;
  const mutationError = connectMutation.error ?? disconnectMutation.error;

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
          Instagram connection settings are restricted to admins.
        </p>
      </div>
    );
  }

  function handleConnect(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (isSaving) {
      return;
    }

    const payload = instagramFormPayload(form);

    if (!payload.instagram_account_id && !status?.env_account_available) {
      return;
    }

    setMessage(null);
    connectMutation.mutate(payload);
  }

  function handleDisconnect() {
    if (isSaving || !status?.connected) {
      return;
    }

    const confirmed = window.confirm("Disconnect Instagram publishing from this workspace?");

    if (confirmed) {
      setMessage(null);
      disconnectMutation.mutate();
    }
  }

  return (
    <div className="space-y-6">
      <section className="border border-slate-200 bg-white p-6 shadow-sm">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
          <div>
            <p className="text-pine text-sm font-semibold uppercase">Settings</p>
            <h2 className="mt-2 text-2xl font-semibold text-slate-950">Instagram connection</h2>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-slate-600">
              Link the Instagram Business or Creator account used for approved Vancouver posts and
              monitor whether the publishing token is ready.
            </p>
          </div>

          <div className="grid gap-3 text-sm sm:grid-cols-3">
            <HealthMetric label="Connection" value={connectionHealth.connection} />
            <HealthMetric label="Token" value={connectionHealth.token} />
            <HealthMetric label="Account" value={connectionHealth.account} />
          </div>
        </div>
      </section>

      <section className="grid gap-6 xl:grid-cols-[minmax(320px,440px)_minmax(0,1fr)]">
        <form
          onSubmit={handleConnect}
          className="border border-slate-200 bg-white p-5 shadow-sm xl:self-start"
        >
          <div>
            <p className="text-sm font-semibold uppercase text-slate-500">Link account</p>
            <h3 className="mt-1 text-xl font-semibold text-slate-950">
              {account ? "Update Instagram details" : "Connect Instagram"}
            </h3>
            <p className="mt-2 text-sm leading-6 text-slate-600">
              The Graph API token stays server-side. Use the account ID from the linked
              Business/Creator profile, or leave it blank when the environment account is set.
            </p>
          </div>

          <div className="mt-5 space-y-4">
            <label className="block">
              <span className="text-sm font-medium text-slate-700">Instagram account ID</span>
              <input
                value={form.instagramAccountId}
                onChange={(event) => setForm({ ...form, instagramAccountId: event.target.value })}
                placeholder={
                  status?.env_account_available
                    ? "Using environment account ID"
                    : "17841400000000000"
                }
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">Username</span>
              <input
                value={form.username}
                onChange={(event) => setForm({ ...form, username: event.target.value })}
                placeholder="vancouverpuls"
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>

            <label className="block">
              <span className="text-sm font-medium text-slate-700">Account type</span>
              <select
                value={form.accountType}
                onChange={(event) =>
                  setForm({ ...form, accountType: event.target.value as InstagramAccountType })
                }
                className="focus:border-coral focus:ring-coral mt-1 h-11 w-full border border-slate-300 bg-white px-3 text-sm outline-none focus:ring-1"
              >
                <option value="business">Business</option>
                <option value="creator">Creator</option>
              </select>
            </label>
          </div>

          {mutationError ? (
            <p className="mt-4 text-sm font-medium text-red-700">{errorMessage(mutationError)}</p>
          ) : null}

          {message ? (
            <p className="mt-4 border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm font-medium text-emerald-900">
              {message}
            </p>
          ) : null}

          <div className="mt-5 flex flex-col gap-3 sm:flex-row">
            <button
              type="submit"
              disabled={
                isSaving ||
                !status?.configured ||
                !status?.token_available ||
                (!form.instagramAccountId.trim() && !status.env_account_available)
              }
              className="bg-pine hover:bg-pine/90 focus-visible:ring-coral h-11 flex-1 px-4 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60"
            >
              {connectMutation.isPending
                ? "Connecting..."
                : account
                  ? "Update connection"
                  : "Connect"}
            </button>
            <button
              type="button"
              onClick={handleDisconnect}
              disabled={isSaving || !status?.connected}
              className="h-11 border border-red-200 px-4 text-sm font-semibold text-red-700 hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-60"
            >
              {disconnectMutation.isPending ? "Disconnecting..." : "Disconnect"}
            </button>
          </div>
        </form>

        <div className="overflow-hidden border border-slate-200 bg-white shadow-sm">
          <div className="flex flex-col gap-2 border-b border-slate-200 px-5 py-4 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h3 className="text-base font-semibold text-slate-950">Connection health</h3>
              <p className="mt-1 text-sm text-slate-500">
                Publishing is available only when settings, token, and account are healthy.
              </p>
            </div>
            <button
              type="button"
              onClick={() => void instagramQuery.refetch()}
              disabled={instagramQuery.isFetching}
              className="border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 disabled:cursor-wait disabled:opacity-60"
            >
              {instagramQuery.isFetching ? "Refreshing..." : "Refresh"}
            </button>
          </div>

          {instagramQuery.isLoading ? (
            <p className="p-6 text-sm text-slate-600">Loading Instagram settings...</p>
          ) : null}

          {instagramQuery.isError ? (
            <p className="p-6 text-sm font-medium text-red-700">
              {errorMessage(instagramQuery.error)}
            </p>
          ) : null}

          {instagramQuery.isSuccess ? (
            <div className="divide-y divide-slate-100">
              <HealthRow
                label="Settings configured"
                healthy={status?.configured ?? false}
                detail={
                  status?.configured
                    ? "Graph API app settings are present."
                    : "Add Instagram app ID and Graph API settings to the server environment."
                }
              />
              <HealthRow
                label="Token available"
                healthy={status?.token_available ?? false}
                detail={
                  status?.token_available
                    ? "A server-side Graph API token is configured."
                    : "Set the secure Instagram access token before connecting."
                }
              />
              <HealthRow
                label="Environment account"
                healthy={status?.env_account_available ?? false}
                detail={
                  status?.env_account_available
                    ? "An account ID is available from the environment."
                    : "Enter an account ID manually when connecting."
                }
              />
              <HealthRow
                label="Workspace connection"
                healthy={status?.connected ?? false}
                detail={
                  status?.connected
                    ? "Approved posts can use this Instagram account."
                    : "No active Instagram account is linked."
                }
              />

              {account ? <AccountDetails account={account} /> : <EmptyConnection />}
            </div>
          ) : null}
        </div>
      </section>
    </div>
  );
}

interface HealthMetricProps {
  label: string;
  value: string;
}

function HealthMetric({ label, value }: HealthMetricProps) {
  return (
    <div className="border border-slate-200 bg-slate-50 px-4 py-3">
      <p className="text-xs font-semibold uppercase text-slate-500">{label}</p>
      <p className="mt-1 whitespace-nowrap text-xl font-semibold text-slate-950">{value}</p>
    </div>
  );
}

interface HealthRowProps {
  label: string;
  healthy: boolean;
  detail: string;
}

function HealthRow({ label, healthy, detail }: HealthRowProps) {
  return (
    <div className="flex flex-col gap-2 px-5 py-4 sm:flex-row sm:items-start sm:justify-between">
      <div>
        <p className="font-semibold text-slate-950">{label}</p>
        <p className="mt-1 text-sm text-slate-600">{detail}</p>
      </div>
      <span
        className={
          healthy
            ? "w-fit rounded-full bg-emerald-100 px-2.5 py-1 text-xs font-semibold uppercase text-emerald-800"
            : "w-fit rounded-full bg-amber-100 px-2.5 py-1 text-xs font-semibold uppercase text-amber-800"
        }
      >
        {healthy ? "Ready" : "Needs setup"}
      </span>
    </div>
  );
}

function AccountDetails({ account }: { account: InstagramConnectionResponse }) {
  return (
    <article className="bg-coral/5 px-5 py-5">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <p className="text-sm font-semibold uppercase text-slate-500">Linked account</p>
          <h4 className="mt-1 text-lg font-semibold text-slate-950">
            {account.username ? `@${account.username}` : account.instagram_account_id}
          </h4>
          <p className="mt-1 break-all text-sm text-slate-600">{account.instagram_account_id}</p>
        </div>
        <span className="w-fit rounded-full bg-white px-2.5 py-1 text-xs font-semibold uppercase text-slate-700">
          {accountTypeLabel(account.account_type)}
        </span>
      </div>

      <dl className="mt-4 grid gap-3 text-sm sm:grid-cols-2">
        <ConnectionDetail label="Graph API" value={account.graph_api_version} />
        <ConnectionDetail label="App ID" value={account.app_id} />
        <ConnectionDetail label="Token source" value={account.token_source} />
        <ConnectionDetail label="Token stored" value={account.token_configured ? "Yes" : "No"} />
        <ConnectionDetail label="Connected" value={formatDateTime(account.connected_at)} />
        <ConnectionDetail label="Updated" value={formatDateTime(account.updated_at)} />
      </dl>
    </article>
  );
}

function EmptyConnection() {
  return (
    <div className="px-5 py-6">
      <p className="text-base font-semibold text-slate-950">No Instagram account linked</p>
      <p className="mt-2 max-w-2xl text-sm leading-6 text-slate-600">
        Connect the Business or Creator account once the server-side app settings and token are
        configured.
      </p>
    </div>
  );
}

interface ConnectionDetailProps {
  label: string;
  value: string;
}

function ConnectionDetail({ label, value }: ConnectionDetailProps) {
  return (
    <div>
      <dt className="text-xs font-semibold uppercase text-slate-500">{label}</dt>
      <dd className="mt-1 break-all font-medium text-slate-800">{value}</dd>
    </div>
  );
}

function seedFormFromStatus(
  status: InstagramStatusResponse,
  setForm: (form: InstagramFormState) => void,
) {
  if (!status.account) {
    return;
  }

  setForm({
    instagramAccountId: status.account.instagram_account_id,
    username: status.account.username ?? "",
    accountType: status.account.account_type,
  });
}

function instagramFormPayload(form: InstagramFormState): ConnectInstagramRequest {
  return {
    instagram_account_id: nullableTrim(form.instagramAccountId) ?? undefined,
    username: nullableTrim(form.username) ?? undefined,
    account_type: form.accountType,
  };
}

function nullableTrim(value: string): string | null {
  const trimmed = value.trim();

  return trimmed.length > 0 ? trimmed : null;
}

function connectionMessage(status: InstagramStatusResponse): string {
  if (status.account?.username) {
    return `Instagram account @${status.account.username} is connected.`;
  }

  if (status.account?.instagram_account_id) {
    return `Instagram account ${status.account.instagram_account_id} is connected.`;
  }

  return "Instagram account is connected.";
}

function connectionHealthLabel(status: InstagramStatusResponse | undefined) {
  if (!status) {
    return {
      connection: "Checking",
      token: "Checking",
      account: "Checking",
    };
  }

  return {
    connection: status.connected ? "Connected" : "Disconnected",
    token: status.token_available ? "Ready" : "Missing",
    account: status.account
      ? accountTypeLabel(status.account.account_type)
      : status.env_account_available
        ? "Env ready"
        : "Missing",
  };
}

function accountTypeLabel(type: InstagramAccountType): string {
  return type === "business" ? "Business" : "Creator";
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
