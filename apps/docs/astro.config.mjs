import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import { fileURLToPath } from "node:url";

export default defineConfig({
  root: fileURLToPath(new URL("./", import.meta.url)),
  srcDir: fileURLToPath(new URL("./src", import.meta.url)),
  site: process.env.STARLANE_DOCS_SITE_URL || "http://localhost:9321",
  integrations: [
    starlight({
      title: "Starlane Notes",
      description: "Open-source notes workspace documentation",
      social: [
        {
          icon: "github",
          label: "Starlane Notes source",
          href: "https://gitea.2eagles.xyz/hft-applications/StarlaneNotes",
        },
      ],
      sidebar: [
        {
          label: "Self-hosting",
          autogenerate: { directory: "self-hosting" },
        },
        {
          label: "Authentication",
          autogenerate: { directory: "authentication" },
        },
        {
          label: "Features",
          autogenerate: { directory: "features" },
        },
        {
          label: "API and integrations",
          autogenerate: { directory: "api" },
        },
        {
          label: "Plugins",
          autogenerate: { directory: "plugins" },
        },
        {
          label: "Development",
          autogenerate: { directory: "development" },
        },
      ],
      editLink: {
        baseUrl: "https://gitea.2eagles.xyz/hft-applications/StarlaneNotes/_edit/v1.0.12/docs/docs/",
      },
    }),
  ],
});
