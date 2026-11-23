import { bindings, defineConfig } from "cf/config";

export default defineConfig({
  worker: {
    name: "zetema",
    entrypoint: "./worker/index.ts",
    compatibilityDate: "2026-11-23",
    observability: { enabled: true },
    assets: { notFoundHandling: "single-page-application", runWorkerFirst: ["/data/*"] },
    domains: ["zetema.ai", "www.zetema.ai"],
    env: {
      DATA: bindings.r2({ name: "zetema-data" }),
      PUSH_KEY: bindings.secret(),
    },
  },
});
