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

export interface ApiErrorPayload {
  error: {
    code: string;
    message: string;
  };
}
