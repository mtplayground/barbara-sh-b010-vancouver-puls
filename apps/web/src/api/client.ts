import type {
  ApiErrorPayload,
  AdminUserResponse,
  AdminUsersResponse,
  AuthSessionResponse,
  CreateDraftRequest,
  CreateSourceRequest,
  CreateInviteResponse,
  DatabaseHealthResponse,
  DraftResponse,
  DraftsResponse,
  HealthResponse,
  InboxItemsResponse,
  RegenerateDraftRequest,
  RenderDraftResponse,
  SourceResponse,
  SourcesResponse,
  StorageHealthResponse,
  UpdateDraftRequest,
  UpdateSourceRequest,
  UserRole,
} from "./types";

const configuredBaseUrl = import.meta.env.VITE_API_BASE_URL;

export class ApiClientError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiClientError";
    this.status = status;
    this.code = code;
  }
}

export const apiClient = {
  getHealth: () => request<HealthResponse>("/api/health"),
  getDatabaseHealth: () => request<DatabaseHealthResponse>("/api/health/db"),
  getStorageHealth: () => request<StorageHealthResponse>("/api/health/storage"),
  getAuthSession: () => request<AuthSessionResponse>("/api/auth/session"),
  getLoginUrl: () => apiUrl("/api/auth/login"),
  listAdminUsers: () => request<AdminUsersResponse>("/api/admin/users"),
  inviteEditor: (email: string) =>
    request<CreateInviteResponse>("/api/admin/invites", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ email }),
    }),
  updateUserRole: (sub: string, role: UserRole) =>
    request<AdminUserResponse>(`/api/admin/users/${encodeURIComponent(sub)}/role`, {
      method: "PATCH",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ role }),
    }),
  listSources: () => request<SourcesResponse>("/api/admin/sources"),
  createSource: (source: CreateSourceRequest) =>
    request<SourceResponse>("/api/admin/sources", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(source),
    }),
  updateSource: (id: number, source: UpdateSourceRequest) =>
    request<SourceResponse>(`/api/admin/sources/${encodeURIComponent(id)}`, {
      method: "PATCH",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(source),
    }),
  deleteSource: (id: number) =>
    request<SourceResponse>(`/api/admin/sources/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
  listInboxItems: (limit = 50) =>
    request<InboxItemsResponse>(`/api/inbox/items?limit=${encodeURIComponent(limit)}`),
  listDrafts: (limit = 50) =>
    request<DraftsResponse>(`/api/drafts?limit=${encodeURIComponent(limit)}`),
  getDraft: (id: number) => request<DraftResponse>(`/api/drafts/${encodeURIComponent(id)}`),
  createDraft: (draft: CreateDraftRequest) =>
    request<DraftResponse>("/api/drafts", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(draft),
    }),
  updateDraft: (id: number, draft: UpdateDraftRequest) =>
    request<DraftResponse>(`/api/drafts/${encodeURIComponent(id)}`, {
      method: "PATCH",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(draft),
    }),
  approveDraft: (id: number) =>
    request<DraftResponse>(`/api/drafts/${encodeURIComponent(id)}/approve`, {
      method: "POST",
    }),
  rejectDraft: (id: number) =>
    request<DraftResponse>(`/api/drafts/${encodeURIComponent(id)}/reject`, {
      method: "POST",
    }),
  regenerateDraft: (id: number, draft: RegenerateDraftRequest = {}) =>
    request<DraftResponse>(`/api/drafts/${encodeURIComponent(id)}/regenerate`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(draft),
    }),
  renderDraft: (id: number) =>
    request<RenderDraftResponse>(`/api/drafts/${encodeURIComponent(id)}/render`, {
      method: "POST",
    }),
};

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(apiUrl(path), {
    credentials: "include",
    headers: {
      Accept: "application/json",
      ...init.headers,
    },
    ...init,
  });

  if (!response.ok) {
    throw await responseError(response);
  }

  return (await response.json()) as T;
}

async function responseError(response: Response): Promise<ApiClientError> {
  const fallbackMessage = `Request failed with status ${response.status}`;

  try {
    const payload = (await response.json()) as Partial<ApiErrorPayload>;
    const error = payload.error;

    if (error?.code && error.message) {
      return new ApiClientError(response.status, error.code, error.message);
    }
  } catch {
    return new ApiClientError(response.status, "request_failed", fallbackMessage);
  }

  return new ApiClientError(response.status, "request_failed", fallbackMessage);
}

function apiUrl(path: string): string {
  const normalizedPath = path.startsWith("/") ? path : `/${path}`;

  if (!configuredBaseUrl) {
    return normalizedPath;
  }

  return `${configuredBaseUrl.replace(/\/$/, "")}${normalizedPath}`;
}
