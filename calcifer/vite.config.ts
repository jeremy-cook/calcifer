import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'
import { ServerResponse } from 'http'
import { tanstackRouter } from '@tanstack/router-plugin/vite'

export default defineConfig({
  plugins: [
    tanstackRouter({
      routesDirectory: path.resolve(__dirname, 'src/routes'),
      autoCodeSplitting: true,
    }),
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      '~': path.resolve(__dirname, 'src'),
      '@calcifer/proto': path.resolve(__dirname, 'gen/ts'),
    },
  },
  server: {
    proxy: {
      // Forward gRPC-Web calls to the tonic server; strip the /api prefix.
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        rewrite: (p) => p.replace(/^\/api/, ''),
        // A streamed response (Watch) must end when the server goes away.
        // Otherwise the proxy keeps the browser's side open after the upstream
        // socket dies, the browser's stream never errors, and it never
        // reconnects. Destroying the browser's response makes it error.
        configure: (proxy) => {
          proxy.on('proxyRes', (proxyRes, _req, res) => {
            proxyRes.on('close', () => {
              if (!proxyRes.complete) res.destroy()
            })
          })
          proxy.on('error', (_err, _req, res) => {
            // Vite's own handler answers 502 while headers are unsent; once
            // streaming, the only signal left is to drop the connection.
            if (res instanceof ServerResponse && res.headersSent) res.destroy()
          })
        },
      },
    },
  },
})
