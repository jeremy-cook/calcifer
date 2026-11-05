import { NameRule } from 'calcifer/v1/structures_pb'
import { getStructure } from '~/model/structures'

// The server's one rule for comparing names (`fold_name`): trimmed, and
// lowercased a character at a time.
export function foldName(name: string): string {
  return Array.from(name.trim(), (c) => c.toLowerCase()).join('')
}

// Letters and digits, with single dots between them (`v1.2`, `program.apollo`).
const DOTTED = /^[\p{L}\p{N}]+(?:\.[\p{L}\p{N}]+)*$/u

export function isDottedName(name: string): boolean {
  return DOTTED.test(name)
}

// What a structure's names may be (StructureDef.name_rule); the server refuses
// any other name. Unknown types take any name.
function nameRule(structureType: string): NameRule {
  return getStructure(structureType)?.nameRule ?? NameRule.UNSPECIFIED
}

export function followsNameRule(structureType: string, name: string): boolean {
  return nameRule(structureType) !== NameRule.DOTTED || isDottedName(name)
}

// The name a typed query asks for: trimmed and, under the dotted rule, without
// the dots that end a sentence ("naming." -> "naming").
export function nameFromQuery(structureType: string, query: string): string {
  const name = query.trim()
  return nameRule(structureType) === NameRule.DOTTED ? name.replace(/\.+$/, '') : name
}

// The rule, said for a name that breaks it.
export function nameRuleHint(structureType: string): string {
  const structure = getStructure(structureType)
  const name = structure?.name || structureType
  return nameRule(structureType) === NameRule.DOTTED
    ? `${name} names are letters and digits with single dots between them, like v1.2`
    : ''
}
