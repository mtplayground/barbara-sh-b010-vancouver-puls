export type AuthStatus = "unknown" | "anonymous" | "authenticated";

export interface AuthUser {
  id: string;
  email: string;
  name: string | null;
  pictureUrl: string | null;
}

export type UserRole = "admin" | "editor";

export interface AuthSnapshot {
  status: AuthStatus;
  user: AuthUser | null;
  roles: UserRole[];
}
