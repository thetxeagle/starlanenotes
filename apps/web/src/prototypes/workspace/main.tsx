import { createRoot } from "react-dom/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AuthContext, type AuthContextType } from "@/hooks/useAuth";
import { WorkspacePrototype } from "./WorkspacePrototype";
import "@/styles/globals.css";
import "./workspace.css";

document.documentElement.dataset.workspacePrototype = "true";

const unavailable = async () => {
  throw new Error("Account actions are unavailable in the fixture prototype.");
};
const fixtureAuth: AuthContextType = {
  user: null,
  config: null,
  loading: false,
  login: unavailable,
  loginWithPasskey: unavailable,
  register: unavailable,
  updateCurrentUser: () => {},
  updateProfile: unavailable,
  logout: () => {},
};

createRoot(document.getElementById("root")!).render(
  <AuthContext.Provider value={fixtureAuth}>
    <TooltipProvider>
      <WorkspacePrototype />
    </TooltipProvider>
  </AuthContext.Provider>,
);
