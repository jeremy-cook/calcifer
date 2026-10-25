import type { CodeBlockOptions } from '@tiptap/extension-code-block'
import { CodeBlock } from '@tiptap/extension-code-block'
import type { BundledTheme } from 'shiki'
import { ShikiPlugin, type GetHighlighter } from './plugin'

export interface CodeBlockShikiOptions extends CodeBlockOptions {
  /** The highlighter, built from only the grammars and themes the app registers. */
  getHighlighter: GetHighlighter | null
  themes: {
    light: BundledTheme
    dark: BundledTheme
  }
  defaultLanguage: string | null | undefined
}

export const CodeBlockShiki = CodeBlock.extend<CodeBlockShikiOptions>({
  name: 'codeBlock',

  addOptions() {
    return {
      ...this.parent?.(),
      getHighlighter: null,
      themes: {},
      defaultLanguage: null,
      exitOnTripleEnter: true,
      exitOnArrowDown: true,
      enableTabIndentation: true,
      HTMLAttributes: {},
    } as CodeBlockShikiOptions
  },

  addProseMirrorPlugins() {
    return [
      ...(this.parent?.() ?? []),
      ShikiPlugin({
        name: this.name,
        getHighlighter: this.options.getHighlighter,
        themes: this.options.themes,
        defaultLanguage: this.options.defaultLanguage,
      }),
    ]
  },
})
