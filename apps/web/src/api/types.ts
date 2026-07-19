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

export interface ApiErrorPayload {
  error: {
    code: string;
    message: string;
  };
}
