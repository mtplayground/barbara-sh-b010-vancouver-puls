import type {
  ApiErrorPayload,
  DatabaseHealthResponse,
  HealthResponse,
  StorageHealthResponse,
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
