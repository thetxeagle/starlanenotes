import { useState } from "react";
import { Check } from "lucide-react";
import { useAuth } from "@/hooks/useAuth";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { BrandLogo } from "@/components/BrandLogo";
import {
  AUTH_ERROR_ALERT_CLASS,
  AUTH_PROVIDER_SETUP_ROWS,
  SIGNUP_POLICY_SETUP_ROWS,
} from "./setupShared";

type Step = 1 | 2 | 3 | 4 | 5;

export function SetupPage() {
  const { register } = useAuth();
  const [step, setStep] = useState<Step>(1);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  // Step 2: Admin account
  const [email, setEmail] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");

  // Step 3: SMTP (optional)
  const [smtpHost, setSmtpHost] = useState("");
  const [smtpPort, setSmtpPort] = useState("587");
  const [smtpUser, setSmtpUser] = useState("");
  const [smtpPass, setSmtpPass] = useState("");
  const [smtpTestMessage, setSmtpTestMessage] = useState("");

  // Step 4: Auth providers configured outside the wizard for now
  const [signupPolicy, setSignupPolicy] = useState<"invite" | "oidc" | "blocked">("invite");

  const createAdmin = async () => {
    setError("");

    if (password !== confirmPassword) {
      setError("Passwords do not match");
      return;
    }
    if (password.length < 8) {
      setError("Password must be at least 8 characters");
      return;
    }

    setLoading(true);
    try {
      await register(email, displayName, password);
      setStep(3);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create account");
    } finally {
      setLoading(false);
    }
  };

  const steps: Record<Step, { title: string }> = {
    1: { title: "Welcome" },
    2: { title: "Create admin" },
    3: { title: "Email" },
    4: { title: "Authentication" },
    5: { title: "Done" },
  };

  const testSmtpConnection = () => {
    setSmtpTestMessage("");
    if (!smtpHost.trim()) {
      setSmtpTestMessage("Enter an SMTP host before testing.");
      return;
    }
    if (!smtpPort.trim() || Number.isNaN(Number(smtpPort))) {
      setSmtpTestMessage("Enter a valid SMTP port before testing.");
      return;
    }
    setSmtpTestMessage(
      "SMTP values look ready. The live send-test endpoint is not wired into setup yet; save these in Admin notifications or environment variables to send a real test.",
    );
  };

  return (
    <main className="app-viewport app-background flex items-center justify-center px-4">
      <div className="w-full max-w-md">
        <p className="text-text-muted mb-2 text-center text-xs" aria-live="polite">
          Step {step} of 5: {steps[step].title}
        </p>

        {/* Step 1: Welcome */}
        {step === 1 && (
          <div className="flex flex-col items-center text-center">
            <BrandLogo size="lg" />
            <h1 className="sr-only">Set up Starlane Notes</h1>
            <p className="text-text-muted mt-2 text-sm">Let's get you set up.</p>
            <Button className="mt-6" onClick={() => setStep(2)}>
              Get Started
            </Button>
          </div>
        )}

        {/* Step 2: Create Admin */}
        {step === 2 && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void createAdmin();
            }}
            aria-busy={loading}
          >
            <h1 className="text-text mb-4 text-lg font-semibold">Create admin account</h1>

            {error && (
              <div role="alert" aria-live="assertive" className={`mb-3 ${AUTH_ERROR_ALERT_CLASS}`}>
                {error}
              </div>
            )}

            <div className="space-y-3">
              <div>
                <label htmlFor="setup-email" className="text-text-muted mb-1 block text-sm">
                  Email
                </label>
                <Input
                  id="setup-email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  type="email"
                  autoComplete="email"
                  autoFocus
                  required
                />
              </div>
              <div>
                <label htmlFor="setup-display-name" className="text-text-muted mb-1 block text-sm">
                  Display name
                </label>
                <Input
                  id="setup-display-name"
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                  autoComplete="name"
                  required
                />
              </div>
              <div>
                <label htmlFor="setup-password" className="text-text-muted mb-1 block text-sm">
                  Password
                </label>
                <Input
                  id="setup-password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  type="password"
                  autoComplete="new-password"
                  required
                />
              </div>
              <div>
                <label
                  htmlFor="setup-confirm-password"
                  className="text-text-muted mb-1 block text-sm"
                >
                  Confirm password
                </label>
                <Input
                  id="setup-confirm-password"
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  type="password"
                  autoComplete="new-password"
                  required
                />
              </div>
              <Button type="submit" className="w-full" disabled={loading}>
                {loading ? "Creating..." : "Create account"}
              </Button>
            </div>
          </form>
        )}

        {/* Step 3: Email (Optional) */}
        {step === 3 && (
          <div>
            <h1 className="text-text mb-1 text-lg font-semibold">Email setup</h1>
            <p className="text-text-muted mb-4 text-sm">
              Configure SMTP for email notifications and invite links.
            </p>

            <div className="space-y-3">
              <div className="grid grid-cols-[minmax(0,1fr)_7rem] gap-3">
                <div>
                  <label htmlFor="setup-smtp-host" className="text-text-muted mb-1 block text-sm">
                    SMTP Host
                  </label>
                  <Input
                    id="setup-smtp-host"
                    value={smtpHost}
                    onChange={(e) => setSmtpHost(e.target.value)}
                    placeholder="smtp.example.com"
                  />
                </div>
                <div>
                  <label htmlFor="setup-smtp-port" className="text-text-muted mb-1 block text-sm">
                    Port
                  </label>
                  <Input
                    id="setup-smtp-port"
                    value={smtpPort}
                    onChange={(e) => setSmtpPort(e.target.value)}
                  />
                </div>
              </div>
              <div>
                <label htmlFor="setup-smtp-user" className="text-text-muted mb-1 block text-sm">
                  Username
                </label>
                <Input
                  id="setup-smtp-user"
                  value={smtpUser}
                  onChange={(e) => setSmtpUser(e.target.value)}
                />
              </div>
              <div>
                <label htmlFor="setup-smtp-pass" className="text-text-muted mb-1 block text-sm">
                  Password
                </label>
                <Input
                  id="setup-smtp-pass"
                  value={smtpPass}
                  onChange={(e) => setSmtpPass(e.target.value)}
                  type="password"
                />
              </div>
              {smtpTestMessage && (
                <div className="rounded-card border-border bg-surface text-text-muted border px-3 py-2 text-sm">
                  {smtpTestMessage}
                </div>
              )}
              <Button
                type="button"
                variant="secondary"
                className="w-full"
                onClick={testSmtpConnection}
              >
                Test connection
              </Button>
              <div className="flex gap-2">
                <Button
                  type="button"
                  variant="secondary"
                  className="flex-1"
                  onClick={() => setStep(4)}
                >
                  Skip for now
                </Button>
                <Button type="button" className="flex-1" onClick={() => setStep(4)}>
                  Save & continue
                </Button>
              </div>
            </div>
          </div>
        )}

        {/* Step 4: Authentication (Optional) */}
        {step === 4 && (
          <div>
            <h1 className="text-text mb-1 text-lg font-semibold">Authentication</h1>
            <p className="text-text-muted mb-4 text-sm">
              External login providers are configured through environment variables. This step is a
              setup tip for now.
            </p>

            <div className="space-y-2">
              {AUTH_PROVIDER_SETUP_ROWS.map((provider) => (
                <div key={provider.name} className="rounded-card border-border border p-3">
                  <div className="text-text text-sm font-medium">{provider.name}</div>
                  <div className="text-text-muted mt-0.5 font-mono text-xs">{provider.vars}</div>
                  {"note" in provider ? (
                    <p className="text-text-muted mt-1 text-xs">{provider.note}</p>
                  ) : null}
                </div>
              ))}
            </div>

            <div className="rounded-card border-border mt-4 border p-3">
              <div className="text-text text-sm font-medium">New user access policy</div>
              <p className="text-text-muted mt-1 text-xs">
                Choose the policy you want this server to use. Enforcement is handled by server
                configuration; this wizard does not change it yet.
              </p>
              <div className="mt-3 space-y-2">
                {SIGNUP_POLICY_SETUP_ROWS.map((option) => (
                  <label
                    key={option.value}
                    className="rounded-input border-border text-text flex cursor-pointer gap-3 border px-3 py-2 text-sm"
                  >
                    <input
                      type="radio"
                      name="setup-signup-policy"
                      value={option.value}
                      checked={signupPolicy === option.value}
                      onChange={() => setSignupPolicy(option.value)}
                      className="accent-primary mt-1"
                    />
                    <span>
                      <span className="block font-medium">{option.label}</span>
                      <span className="text-text-muted block text-xs">{option.detail}</span>
                    </span>
                  </label>
                ))}
              </div>
            </div>

            <div className="mt-4 flex gap-2">
              <Button
                type="button"
                variant="secondary"
                className="flex-1"
                onClick={() => setStep(5)}
              >
                Skip for now
              </Button>
              <Button type="button" className="flex-1" onClick={() => setStep(5)}>
                Continue
              </Button>
            </div>
          </div>
        )}

        {/* Step 5: Done */}
        {step === 5 && (
          <div className="text-center">
            <div className="mb-4 flex justify-center">
              <Check size={32} className="text-primary" />
            </div>
            <h1 className="text-text text-lg font-semibold">Starlane Notes is ready.</h1>
            <p className="text-text-muted mt-1 text-sm">
              Your admin account has been created. You can configure additional settings in the
              admin panel.
            </p>
            <Button
              type="button"
              className="mt-6"
              onClick={() => {
                window.location.href = "/";
              }}
            >
              Open Starlane Notes
            </Button>
          </div>
        )}

        {/* Step indicator */}
        <div
          className="mt-8 flex items-center justify-center gap-2"
          role="list"
          aria-label="Setup progress"
        >
          {([1, 2, 3, 4, 5] as Step[]).map((s) => (
            <div
              key={s}
              title={steps[s].title}
              role="listitem"
              aria-current={s === step ? "step" : undefined}
              aria-label={`Step ${s}: ${steps[s].title}${s === step ? " (current)" : ""}`}
              className={`flex h-6 w-6 items-center justify-center rounded-full text-[10px] font-medium ${
                s < step
                  ? "bg-primary text-white"
                  : s === step
                    ? "border-primary text-primary border-2"
                    : "border-border text-text-muted border"
              }`}
            >
              {s < step ? <Check size={12} /> : s}
            </div>
          ))}
        </div>
      </div>
    </main>
  );
}
