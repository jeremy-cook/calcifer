import postcssImport from 'postcss-import'
import path from 'path'
import { fileURLToPath } from 'url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

export default {
  plugins: [
    postcssImport({
      resolve(id) {
        if (id.startsWith('~/')) {
          return path.resolve(__dirname, 'src', id.slice(2))
        }
      },
    }),
  ],
}
