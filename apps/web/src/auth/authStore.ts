import { create } from "zustand";

import type { AuthSnapshot } from "./types";

interface AuthStoreState extends AuthSnapshot {
  setAuthSnapshot: (snapshot: AuthSnapshot) => void;
  resetAuth: () => void;
}

const initialAuthState: AuthSnapshot = {
  status: "unknown",
  user: null,
  roles: [],
};

export const useAuthStore = create<AuthStoreState>((set) => ({
  ...initialAuthState,
  setAuthSnapshot: (snapshot) => set(snapshot),
  resetAuth: () => set(initialAuthState),
}));
