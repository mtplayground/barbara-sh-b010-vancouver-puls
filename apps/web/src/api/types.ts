export interface HealthResponse {
  status: "ok";
  service: "api";
}

export interface DatabaseHealthResponse {
  status: "ok";
  database: "postgres";
}

export interface StorageHealthResponse {
  status: "ok" | "disabled";
  storage: "s3";
  bucket: string | null;
  prefix: string | null;
}

export type UserRole = "admin" | "editor";
export type ContentSourceKind = "rss" | "website" | "instagram" | "manual";

export interface AuthSessionUserResponse {
  sub: string;
  email: string;
  name: string | null;
  picture_url: string | null;
  role: UserRole;
}

export interface AuthSessionResponse {
  authenticated: boolean;
  user: AuthSessionUserResponse | null;
}

export interface AdminUserResponse {
  sub: string;
  email: string;
  name: string | null;
  picture_url: string | null;
  role: UserRole;
  created_at: string;
  updated_at: string;
  last_seen_at: string;
}

export interface AdminUsersResponse {
  users: AdminUserResponse[];
}

export interface InviteResponse {
  email: string;
  role: UserRole;
  invited_by_sub: string;
  accepted_by_sub: string | null;
  created_at: string;
  expires_at: string;
  accepted_at: string | null;
}

export type InviteEmailDelivery =
  { status: "sent"; message_id: string } | { status: "rate_limited" } | { status: "skipped" };

export interface CreateInviteResponse {
  invite: InviteResponse;
  invite_url: string;
  email_delivery: InviteEmailDelivery;
}

export interface SourceResponse {
  id: number;
  name: string;
  kind: ContentSourceKind;
  url: string | null;
  external_id: string | null;
  created_by_sub: string | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface SourcesResponse {
  sources: SourceResponse[];
}

export interface CreateSourceRequest {
  name: string;
  kind: ContentSourceKind;
  url?: string | null;
  external_id?: string | null;
  enabled?: boolean;
}

export interface UpdateSourceRequest {
  name?: string;
  kind?: ContentSourceKind;
  url?: string | null;
  external_id?: string | null;
  enabled?: boolean;
}

export interface ApiErrorPayload {
  error: {
    code: string;
    message: string;
  };
}
