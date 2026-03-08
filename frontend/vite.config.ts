import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'

export default defineConfig({
  plugins: [tailwindcss(), react()],
  resolve: {
    alias: { '@': path.resolve(__dirname, './src') },
  },
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
