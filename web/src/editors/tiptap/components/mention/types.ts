export const CREATE_ITEM_ID = '__create__'

export interface EntitySuggestionItem {
  id: string
  label: string
  structureType: string
  color: string
  isCreate?: boolean
  createLabel?: string
  // Not an entity: why the query can't be created (it breaks the name rule).
  // Shown in the menu, never picked.
  isHint?: boolean
  // Text the query ends with that isn't part of the name, such as a sentence's
  // closing period; written back after the chip.
  suffix?: string
}
