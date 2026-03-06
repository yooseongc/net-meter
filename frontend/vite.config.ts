import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3000,
    proxy: {
      // Control API 프록시 (개발 시 CORS 우회)
      '/api': {
        target: 'http://localhost:9090',
        changeOrigin: true,
        ws: true,
      },
    },
  },
  build: {
    outDir: '../engine/crates/control/static',
    emptyOutDir: true,
  },
})
