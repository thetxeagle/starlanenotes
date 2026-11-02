import { createRoot } from "react-dom/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AuthContext, type AuthContextType } from "@/hooks/useAuth";
import { TemplateExperiencePrototype } from "./TemplateExperiencePrototype";
import "@/styles/globals.css";
import "./templates.css";
import { installTemplatePrototypeGuards } from "./prototypeGuards";

document.documentElement.dataset.templatePrototype = "true";
installTemplatePrototypeGuards();

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
      <TemplateExperiencePrototype />
    </TooltipProvider>
  </AuthContext.Provider>,
);
