import { FormEvent, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiClientError, apiClient } from "../api/client";
import type { AdminUserResponse, CreateInviteResponse, UserRole } from "../api/types";
import { useAuth } from "../auth/useAuth";
import { LoginRoute } from "./LoginRoute";

const roleOptions: UserRole[] = ["admin", "editor"];

export function UsersRoute() {
  const auth = useAuth();
  const queryClient = useQueryClient();
  const [email, setEmail] = useState("");
  const [inviteResult, setInviteResult] = useState<CreateInviteResponse | null>(null);
  const isAdmin = auth.roles.includes("admin");

  const usersQuery = useQuery({
    queryKey: ["admin", "users"],
    queryFn: apiClient.listAdminUsers,
    enabled: isAdmin,
  });

  const inviteMutation = useMutation({
    mutationFn: (inviteEmail: string) => apiClient.inviteEditor(inviteEmail),
    onSuccess: (result) => {
      setInviteResult(result);
      setEmail("");
      void queryClient.invalidateQueries({ queryKey: ["admin", "users"] });
    },
  });

  const roleMutation = useMutation({
    mutationFn: ({ sub, role }: { sub: string; role: UserRole }) =>
      apiClient.updateUserRole(sub, role),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["admin", "users"] });
      void queryClient.invalidateQueries({ queryKey: ["auth", "session"] });
    },
  });

  const sortedUsers = useMemo(() => usersQuery.data?.users ?? [], [usersQuery.data?.users]);

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
          User invitations and role changes are restricted to admins.
        </p>
      </div>
    );
  }

  function handleInvite(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmedEmail = email.trim();

    if (!trimmedEmail || inviteMutation.isPending) {
      return;
    }

    setInviteResult(null);
    inviteMutation.mutate(trimmedEmail);
  }

  return (
    <div className="space-y-6">
      <div className="border border-slate-200 bg-white p-6 shadow-sm">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
          <div>
            <p className="text-pine text-sm font-semibold uppercase">Administration</p>
            <h2 className="mt-2 text-2xl font-semibold text-slate-950">Users and roles</h2>
          </div>

          <form
            onSubmit={handleInvite}
            className="flex w-full flex-col gap-3 sm:flex-row lg:w-auto"
          >
            <label className="min-w-0 flex-1 lg:w-80">
              <span className="sr-only">Editor email</span>
              <input
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                type="email"
                required
                placeholder="editor@example.com"
                className="focus:border-coral focus:ring-coral h-11 w-full border border-slate-300 px-3 text-sm outline-none focus:ring-1"
              />
            </label>
            <button
              type="submit"
              disabled={inviteMutation.isPending}
              className="bg-pine hover:bg-pine/90 focus-visible:ring-coral h-11 px-4 text-sm font-semibold text-white shadow-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60"
            >
              {inviteMutation.isPending ? "Sending..." : "Invite editor"}
            </button>
          </form>
        </div>

        {inviteMutation.isError ? (
          <p className="mt-4 text-sm font-medium text-red-700">
            {errorMessage(inviteMutation.error)}
          </p>
        ) : null}

        {inviteResult ? (
          <div className="mt-4 border border-emerald-200 bg-emerald-50 p-4 text-sm text-emerald-950">
            <p className="font-semibold">{inviteDeliveryMessage(inviteResult)}</p>
            <input
              readOnly
              value={inviteResult.invite_url}
              className="mt-3 w-full border border-emerald-200 bg-white px-3 py-2 text-xs text-slate-700"
              aria-label="Invite link"
            />
          </div>
        ) : null}
      </div>

      <div className="overflow-hidden border border-slate-200 bg-white shadow-sm">
        <div className="border-b border-slate-200 px-6 py-4">
          <h3 className="text-base font-semibold text-slate-950">Team</h3>
        </div>

        {usersQuery.isLoading ? (
          <p className="p-6 text-sm text-slate-600">Loading users...</p>
        ) : null}

        {usersQuery.isError ? (
          <p className="p-6 text-sm font-medium text-red-700">{errorMessage(usersQuery.error)}</p>
        ) : null}

        {usersQuery.isSuccess ? (
          <div className="overflow-x-auto">
            <table className="min-w-full divide-y divide-slate-200 text-left text-sm">
              <thead className="bg-slate-50 text-xs uppercase text-slate-500">
                <tr>
                  <th className="px-6 py-3 font-semibold">User</th>
                  <th className="px-6 py-3 font-semibold">Role</th>
                  <th className="px-6 py-3 font-semibold">Last seen</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {sortedUsers.map((user) => (
                  <UserRow
                    key={user.sub}
                    user={user}
                    currentUserId={auth.user?.id}
                    pendingSub={roleMutation.variables?.sub}
                    isPending={roleMutation.isPending}
                    onRoleChange={(role) => roleMutation.mutate({ sub: user.sub, role })}
                  />
                ))}
              </tbody>
            </table>
          </div>
        ) : null}

        {roleMutation.isError ? (
          <p className="border-t border-slate-200 p-4 text-sm font-medium text-red-700">
            {errorMessage(roleMutation.error)}
          </p>
        ) : null}
      </div>
    </div>
  );
}

interface UserRowProps {
  user: AdminUserResponse;
  currentUserId: string | undefined;
  pendingSub: string | undefined;
  isPending: boolean;
  onRoleChange: (role: UserRole) => void;
}

function UserRow({ user, currentUserId, pendingSub, isPending, onRoleChange }: UserRowProps) {
  const displayName = user.name ?? user.email;
  const isCurrentUser = user.sub === currentUserId;
  const isUpdating = isPending && pendingSub === user.sub;

  return (
    <tr className="align-middle">
      <td className="px-6 py-4">
        <div className="flex min-w-72 items-center gap-3">
          {user.picture_url ? (
            <img
              src={user.picture_url}
              alt=""
              className="h-10 w-10 flex-none rounded-full border border-slate-200"
            />
          ) : (
            <div className="flex h-10 w-10 flex-none items-center justify-center rounded-full border border-slate-200 bg-slate-100 text-sm font-semibold text-slate-600">
              {displayName.slice(0, 1).toUpperCase()}
            </div>
          )}
          <div className="min-w-0">
            <p className="truncate font-semibold text-slate-950">
              {displayName}
              {isCurrentUser ? <span className="ml-2 text-xs text-slate-500">You</span> : null}
            </p>
            <p className="truncate text-xs text-slate-500">{user.email}</p>
          </div>
        </div>
      </td>
      <td className="px-6 py-4">
        <select
          value={user.role}
          onChange={(event) => onRoleChange(event.target.value as UserRole)}
          disabled={isUpdating}
          className="focus:border-coral focus:ring-coral h-10 w-32 border border-slate-300 bg-white px-3 text-sm font-medium text-slate-900 outline-none focus:ring-1 disabled:cursor-wait disabled:opacity-60"
        >
          {roleOptions.map((role) => (
            <option key={role} value={role}>
              {roleLabel(role)}
            </option>
          ))}
        </select>
      </td>
      <td className="whitespace-nowrap px-6 py-4 text-sm text-slate-600">
        {formatDateTime(user.last_seen_at)}
      </td>
    </tr>
  );
}

function roleLabel(role: UserRole): string {
  return role === "admin" ? "Admin" : "Editor";
}

function formatDateTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function inviteDeliveryMessage(result: CreateInviteResponse): string {
  switch (result.email_delivery.status) {
    case "sent":
      return `Invitation sent to ${result.invite.email}.`;
    case "rate_limited":
      return "Email delivery is rate limited. Use the invite link directly.";
    case "skipped":
      return "Email delivery is not configured. Use the invite link directly.";
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof ApiClientError) {
    return error.message;
  }

  if (error instanceof Error) {
    return error.message;
  }

  return "Request failed";
}
