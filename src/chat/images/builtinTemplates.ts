import defaultTemplate from '../../../src-tauri/resources/image-studio/default.json'
import kidsTemplate from '../../../src-tauri/resources/image-studio/kids.json'
import mensTemplate from '../../../src-tauri/resources/image-studio/mens-backpack/template.json'
import womensTemplate from '../../../src-tauri/resources/image-studio/womens-backpack/template.json'
import type { ImageTemplate } from './types'

const definitions = [
  { id: 'builtin-default-v1', folder: '', data: defaultTemplate },
  { id: 'builtin-kids-v1', folder: '', data: kidsTemplate },
  { id: 'builtin-mens-backpack-v1', folder: 'mens-backpack', data: mensTemplate },
  { id: 'builtin-womens-backpack-v1', folder: 'womens-backpack', data: womensTemplate },
]

export const builtinTemplates: ImageTemplate[] = definitions.map(({ id, folder, data }) => ({
  id,
  directory: folder ? `templates/${id}` : '',
  builtin: true,
  data: data as ImageTemplate['data'],
}))

// Vite bundles these assets for the browser preview; native execution uses the
// same original files materialized by the Rust template catalog.
const images = import.meta.glob<string>('../../../src-tauri/resources/image-studio/*/**/*.png', {
  eager: true,
  query: '?url',
  import: 'default',
})

export function builtinImageUrl(path: string): string | undefined {
  for (const { id, folder } of definitions) {
    const prefix = `templates/${id}/`
    if (folder && path.startsWith(prefix)) {
      return images[`../../../src-tauri/resources/image-studio/${folder}/${path.slice(prefix.length)}`]
    }
  }
}
