import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StorageModeSetupPage } from "./StorageModeSetupPage";

const mocks = vi.hoisted(() => ({
  tauri: false,
}));

vi.mock("@/lib/api", () => ({
  isTauri: () => mocks.tauri,
}));

vi.mock("@/lib/tauriVault", () => ({
  listVaultFolders: vi.fn(async () => []),
}));

describe("StorageModeSetupPage", () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      clear: () => store.clear(),
      getItem: (key: string) => store.get(key) ?? null,
      removeItem: (key: string) => store.delete(key),
      setItem: (key: string, value: string) => store.set(key, value),
    });
    vi.stubGlobal("window", {
      matchMedia: () => ({
        addEventListener: vi.fn(),
        matches: false,
        removeEventListener: vi.fn(),
      }),
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    mocks.tauri = false;
  });

  function renderSetup() {
    return renderToStaticMarkup(
      <StorageModeSetupPage onChooseLocal={vi.fn()} onChooseServer={vi.fn()} />,
    );
  }

  it("keeps web setup online-first by hiding standalone local vault setup", () => {
    mocks.tauri = false;

    const html = renderSetup();

    expect(html).toContain("Create a local vault, open an existing vault");
    expect(html).toContain("Connect server");
    expect(html).toContain("Connect and detect login");
    expect(html).toContain("standard login, OAuth, or SSO");
    expect(html).not.toContain("Create local vault");
    expect(html).not.toContain("Create / Use Vault");
    expect(html).not.toContain("local-vault-name");
  });

  it("shows standalone local vault setup in native shells", () => {
    mocks.tauri = true;

    const html = renderSetup();

    expect(html).toContain("Create local vault");
    expect(html).toContain("Open existing vault");
    expect(html).toContain("Connect server");
    expect(html).toContain("Connect and detect login");
    expect(html).not.toContain("local-vault-name");
  });
});
