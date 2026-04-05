import type { CodeBlockOptions } from '@tiptap/extension-code-block'
import { CodeBlock } from '@tiptap/extension-code-block'
import type { BundledTheme, BundledLanguage } from 'shiki'
import { ShikiPlugin, SHIKI_FORCE_DECORATION, getDefaultHighlighter, initHighlighter, type GetHighlighter } from './plugin'

export interface CodeBlockShikiOptions extends CodeBlockOptions {
  getHighlighter: GetHighlighter | null
  themes: {
    light: BundledTheme
    dark: BundledTheme
  }
  defaultLanguage: BundledLanguage | null | undefined
  languages: BundledLanguage[]
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    codeBlockShiki: {
      loadCodeLanguage: (lang: BundledLanguage) => ReturnType
      loadCodeTheme: (theme: BundledTheme) => ReturnType
    }
  }
}

export const CodeBlockShiki = CodeBlock.extend<CodeBlockShikiOptions>({
  name: 'codeBlock',

  addOptions() {
    return {
      ...this.parent?.(),
      getHighlighter: null,
      themes: {},
      defaultLanguage: null as unknown as BundledLanguage,
      languages: [] as BundledLanguage[],
      exitOnTripleEnter: true,
      exitOnArrowDown: true,
      enableTabIndentation: true,
      HTMLAttributes: {},
    } as CodeBlockShikiOptions
  },

  onCreate() {
    if (this.options.getHighlighter || getDefaultHighlighter()) return
    const { languages, themes } = this.options
    initHighlighter(Object.values(themes) as BundledTheme[], languages).then(() => {
      this.editor.view.dispatch(this.editor.view.state.tr.setMeta(SHIKI_FORCE_DECORATION, true))
    })
  },

  addCommands() {
    return {
      ...this.parent?.(),
      loadCodeLanguage:
        (lang) =>
        ({ editor }) => {
          const h = getDefaultHighlighter()
          if (!h || h.getLoadedLanguages().includes(lang)) return true
          h.loadLanguage(lang).then(() => {
            editor.view.dispatch(editor.view.state.tr.setMeta(SHIKI_FORCE_DECORATION, true))
          })
          return true
        },
      loadCodeTheme:
        (theme) =>
        ({ editor }) => {
          const h = getDefaultHighlighter()
          if (!h || h.getLoadedThemes().includes(theme)) return true
          h.loadTheme(theme).then(() => {
            editor.view.dispatch(editor.view.state.tr.setMeta(SHIKI_FORCE_DECORATION, true))
          })
          return true
        },
    }
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
