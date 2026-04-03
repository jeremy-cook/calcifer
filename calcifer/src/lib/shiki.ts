import { createHighlighter, type Highlighter } from 'shiki'

let highlighterPromise: Promise<Highlighter> | null = null

export function getShikiHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: ['vitesse-light', 'catppuccin-mocha', 'dracula'],
      langs: ['javascript', 'typescript', 'html', 'css', 'json'],
    })
  }
  return highlighterPromise
}
