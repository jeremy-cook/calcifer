import { createHighlighterCoreSync } from 'shiki/core'
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript'
import themeEverforestDark from 'shiki/themes/everforest-dark.mjs'
import langJavaScript from 'shiki/langs/javascript.mjs'
import langTypeScript from 'shiki/langs/typescript.mjs'
import langHtml from 'shiki/langs/html.mjs'
import langCss from 'shiki/langs/css.mjs'
import langJson from 'shiki/langs/json.mjs'
import langPython from 'shiki/langs/python.mjs'
import langRust from 'shiki/langs/rust.mjs'
import langGo from 'shiki/langs/go.mjs'
import langSql from 'shiki/langs/sql.mjs'
import langBash from 'shiki/langs/bash.mjs'
import langMarkdown from 'shiki/langs/markdown.mjs'

const highlighter = createHighlighterCoreSync({
  themes: [themeEverforestDark],
  langs: [
    langJavaScript,
    langTypeScript,
    langHtml,
    langCss,
    langJson,
    langPython,
    langRust,
    langGo,
    langSql,
    langBash,
    langMarkdown,
  ],
  engine: createJavaScriptRegexEngine(),
})

export function getHighlighter() {
  return highlighter
}
