import { createContext } from "react";

import type { AuthSnapshot } from "./types";

export interface AuthContextValue extends AuthSnapshot {
  isAuthenticated: boolean;
  signIn: () => void;
  setAuthSnapshot: (snapshot: AuthSnapshot) => void;
  resetAuth: () => void;
}

export const AuthContext = createContext<AuthContextValue | null>(null);
