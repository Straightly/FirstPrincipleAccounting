import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The launcher is served by the LedgerZero backend in production (frontend_dist).
// In dev, Vite proxies API calls to the local backend.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": "http://localhost:8080"
    }
  }
});
