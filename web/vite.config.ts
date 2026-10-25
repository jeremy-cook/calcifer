import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'
import { ServerResponse } from 'http'
import { tanstackRouter } from '@tanstack/router-plugin/vite'

// The backend's UI and API listener (scripts/calcifer passes CALCIFER_ADDR
// through). A wildcard bind is reached on localhost, because the backend
// refuses requests whose Host isn't this machine.
const backendAddr = (process.env.CALCIFER_ADDR ?? '127.0.0.1:8080').replace(/^(0\.0\.0\.0|\[::\])(?=:)/, 'localhost')

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
  build: {
    rolldownOptions: {
      output: {
        // The editor's libraries get chunks of their own, so an app change doesn't
        // invalidate them in the browser's cache and no chunk passes the 500 kB
        // warning. Only named libraries are grouped, and not @tiptap: a @tiptap
        // group ends up statically imported by the first screen, which would then
        // download the whole editor. The grammars are one chunk each.
        codeSplitting: {
          groups: [
            { name: 'react', test: /node_modules[\\/](react|react-dom|scheduler)[\\/]/, priority: 40 },
            {
              name: 'prosemirror',
              test: /node_modules[\\/](prosemirror-[^\\/]+|orderedmap|rope-sequence|w3c-keyname)[\\/]/,
              priority: 30,
            },
            { name: 'pickers', test: /node_modules[\\/](cmdk|react-day-picker|date-fns)[\\/]/, priority: 25 },
            {
              // One chunk per grammar, so the largest (TypeScript, HTML) stay small.
              name: (id) => {
                const grammar = id.match(/@shikijs[\\/]langs[\\/]dist[\\/]([\w-]+)\.mjs/)?.[1]
                return grammar ? `shiki-${grammar}` : null
              },
              test: /@shikijs[\\/]langs[\\/]dist[\\/]/,
              priority: 15,
            },
            { name: 'shiki', test: /node_modules[\\/](shiki|@shikijs)[\\/]/, priority: 10 },
          ],
        },
      },
    },
  },
  server: {
    proxy: {
      // Forward gRPC-Web calls to the backend, which serves them under /api.
      '/api': {
        target: `http://${backendAddr}`,
        changeOrigin: true,
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
