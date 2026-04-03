import type { CodeBlockOptions } from '@tiptap/extension-code-block'
import { CodeBlock } from '@tiptap/extension-code-block'
import type { Highlighter, BundledTheme, BundledLanguage } from 'shiki'
import { ShikiPlugin, SHIKI_FORCE_DECORATION } from './plugin'

export interface CodeBlockShikiOptions extends CodeBlockOptions {
  highlighter: Highlighter
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
      highlighter: null as unknown as Highlighter,
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
    const { highlighter, languages, themes } = this.options
    const toLoadLangs = languages.filter((lang) => !highlighter.getLoadedLanguages().includes(lang))
    const themeList = Object.values(themes) as BundledTheme[]
    const toLoadThemes = themeList.filter((theme) => !highlighter.getLoadedThemes().includes(theme))

    if (toLoadLangs.length === 0 && toLoadThemes.length === 0) return

    const promises: Promise<void>[] = []
    if (toLoadLangs.length > 0) promises.push(highlighter.loadLanguage(...toLoadLangs))
    if (toLoadThemes.length > 0) promises.push(highlighter.loadTheme(...toLoadThemes))

    Promise.all(promises).then(() => {
      this.editor.view.dispatch(this.editor.view.state.tr.setMeta(SHIKI_FORCE_DECORATION, true))
    })
  },

  addCommands() {
    return {
      ...this.parent?.(),
      loadCodeLanguage:
        (lang) =>
        ({ editor }) => {
          const { highlighter } = this.options
          if (highlighter.getLoadedLanguages().includes(lang)) return true
          highlighter.loadLanguage(lang).then(() => {
            editor.view.dispatch(editor.view.state.tr.setMeta(SHIKI_FORCE_DECORATION, true))
          })
          return true
        },
      loadCodeTheme:
        (theme) =>
        ({ editor }) => {
          const { highlighter } = this.options
          if (highlighter.getLoadedThemes().includes(theme)) return true
          highlighter.loadTheme(theme).then(() => {
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
        highlighter: this.options.highlighter,
        themes: this.options.themes,
        defaultLanguage: this.options.defaultLanguage,
      }),
    ]
  },
})
