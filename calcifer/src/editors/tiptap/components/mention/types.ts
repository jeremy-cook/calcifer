export const CREATE_ITEM_ID = '__create__'

export interface EntitySuggestionItem {
  id: string
  label: string
  structureType: string
  color: string
  isCreate?: boolean
  createLabel?: string
}
