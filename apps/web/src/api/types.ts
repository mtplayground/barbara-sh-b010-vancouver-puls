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
export type BackupContentKind = "past_recap" | "did_you_know";
export type InstagramAccountType = "business" | "creator";
export type DraftStatus =
  "draft" | "in_review" | "approved" | "rejected" | "scheduled" | "published" | "archived";

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

export interface IngestedItemResponse {
  id: number;
  source_id: number;
  title: string;
  summary: string | null;
  link: string;
  media_ref: string | null;
  dedup_key: string;
  source_published_at: string | null;
  discovered_at: string;
  ingested_at: string;
  updated_at: string;
}

export interface InboxItemsResponse {
  items: IngestedItemResponse[];
}

export interface BackupContentItemResponse {
  id: number;
  kind: BackupContentKind;
  title: string;
  body: string;
  source_url: string | null;
  media_ref: string | null;
  active: boolean;
  created_by_sub: string | null;
  updated_by_sub: string | null;
  created_at: string;
  updated_at: string;
}

export interface BackupContentItemsResponse {
  items: BackupContentItemResponse[];
}

export interface CreateBackupContentItemRequest {
  kind: BackupContentKind;
  title: string;
  body: string;
  source_url?: string | null;
  media_ref?: string | null;
  active?: boolean;
}

export interface UpdateBackupContentItemRequest {
  kind?: BackupContentKind;
  title?: string;
  body?: string;
  source_url?: string | null;
  media_ref?: string | null;
  active?: boolean;
}

export interface DraftResponse {
  id: number;
  source_item_id: number | null;
  caption_en: string;
  caption_zh: string;
  status: DraftStatus;
  rendered_post_asset_ref: string | null;
  rendered_reel_asset_ref: string | null;
  created_by_sub: string | null;
  updated_by_sub: string | null;
  created_at: string;
  updated_at: string;
}

export interface DraftsResponse {
  drafts: DraftResponse[];
}

export interface CreateDraftRequest {
  source_item_id?: number;
  manual_topic?: string;
  manual_notes?: string;
}

export interface UpdateDraftRequest {
  source_item_id?: number | null;
  caption_en?: string;
  caption_zh?: string;
  status?: DraftStatus;
  rendered_post_asset_ref?: string | null;
  rendered_reel_asset_ref?: string | null;
}

export interface RegenerateDraftRequest {
  manual_topic?: string;
  manual_notes?: string;
}

export interface RenderDraftResponse {
  draft: DraftResponse;
  post_asset_ref: string;
  reel_asset_ref: string;
}

export interface CalendarSlotResponse {
  id: number | null;
  slot_date: string;
  slot_time: string;
  draft: DraftResponse | null;
  is_empty: boolean;
  is_upcoming: boolean;
}

export interface CalendarResponse {
  start_date: string;
  end_date: string;
  daily_cadence: "one_post_per_day";
  empty_upcoming_slots: number;
  slots: CalendarSlotResponse[];
}

export interface AssignCalendarSlotRequest {
  slot_date: string;
  slot_time?: string;
  draft_id: number;
}

export interface InstagramConnectionResponse {
  instagram_account_id: string;
  username: string | null;
  account_type: InstagramAccountType;
  graph_api_version: string;
  app_id: string;
  token_source: string;
  token_configured: boolean;
  connected_by_sub: string | null;
  disconnected_at: string | null;
  connected_at: string;
  updated_at: string;
}

export interface InstagramStatusResponse {
  configured: boolean;
  token_available: boolean;
  env_account_available: boolean;
  connected: boolean;
  account: InstagramConnectionResponse | null;
}

export interface ConnectInstagramRequest {
  instagram_account_id?: string;
  username?: string;
  account_type?: InstagramAccountType;
}

export interface ApiErrorPayload {
  error: {
    code: string;
    message: string;
  };
}
