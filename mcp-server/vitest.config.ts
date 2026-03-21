import { defineConfig } from "vitest/config";

export default defineConfig({
  clearScreen: false,
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
