import { defineConfig } from 'vitest/config'
import viteConfig from './vite.config'

// Only the aliases are shared with the app build: the router and Tailwind
// plugins have nothing to do in a test run. Tests default to Node; a test that
// renders opts into jsdom with a `// @vitest-environment jsdom` docblock.
export default defineConfig({
  resolve: { alias: viteConfig.resolve?.alias },
  test: {
    include: ['src/**/*.test.{ts,tsx}'],
  },
})
