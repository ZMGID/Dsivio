import type { CategoryAttribute, ListingDraft, ListingRecord, ListingStatus, ListingTarget } from '../../../generated/commerce'
import type { Product } from '../../../generated/products'

export type SkuRow = { name: string; price: string; stock: string; code: string }

export type ListingFormState = {
  title: string
  description: string
  price: string
  currency: string
  stock: string
  sku: string
  extraSkus: SkuRow[]
  images: string[]
  weightKg: string
  lengthCm: string
  widthCm: string
  heightCm: string
  brand: string
  shopIds: string[]
  categories: Record<string, string>
  attributes: Record<string, Record<string, { value: string; values: string[] }>>
}

export type ListingShopContext = {
  id: string
  name: string
  listing: boolean | null
  note?: string
}

export function emptyListingForm(): ListingFormState {
  return {
    title: '', description: '', price: '', currency: '', stock: '', sku: '', extraSkus: [],
    images: [], weightKg: '', lengthCm: '', widthCm: '', heightCm: '', brand: '',
    shopIds: [], categories: {}, attributes: {},
  }
}

function textAmount(value: number | bigint | null | undefined): string {
  if (value == null) return ''
  return String(value)
}

export function listingFormFromRecord(record: ListingRecord): ListingFormState {
  const primary = record.draft.skus[0]
  const dimensions = record.draft.dimensionsCm
  return {
    title: record.draft.title,
    description: record.draft.description ?? '',
    price: textAmount(primary?.price ?? record.draft.price),
    currency: record.draft.currency ?? record.currency ?? '',
    stock: textAmount(primary?.stock ?? record.draft.stock),
    sku: primary?.code ?? '',
    extraSkus: record.draft.skus.slice(1).map((sku) => ({
      name: sku.name,
      price: textAmount(sku.price),
      stock: textAmount(sku.stock),
      code: sku.code ?? '',
    })),
    images: [...record.draft.images],
    weightKg: textAmount(record.draft.weightKg),
    lengthCm: textAmount(dimensions?.l),
    widthCm: textAmount(dimensions?.w),
    heightCm: textAmount(dimensions?.h),
    brand: record.draft.brand ?? '',
    shopIds: [record.shopId],
    categories: { [record.shopId]: record.target.categoryId },
    attributes: {
      [record.shopId]: Object.fromEntries(record.target.attributes.map((attribute) => [
        attribute.id,
        { value: attribute.value, values: [...attribute.values] },
      ])),
    },
  }
}

/** Copies archive fields into the form. Category stays per shop; nothing is mapped across platforms. */
export function applyProduct(form: ListingFormState, product: Product): ListingFormState {
  const primary = product.skus[0]
  return {
    ...form,
    title: product.name,
    description: product.description,
    currency: product.currency ?? '',
    price: textAmount(product.price ?? primary?.price),
    stock: textAmount(product.stock ?? primary?.stock),
    sku: primary?.code ?? '',
    extraSkus: product.skus.slice(1).map((sku) => ({
      name: sku.name,
      price: textAmount(sku.price),
      stock: textAmount(sku.stock),
      code: sku.code,
    })),
    images: product.images.length ? [...product.images] : form.images,
  }
}

export function canRetryListing(status: ListingStatus): boolean {
  return status === 'failed' || status === 'rejected'
}

function codePoints(value: string): number {
  return Array.from(value).length
}

function isAbsolutePath(path: string): boolean {
  return path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path) || path.startsWith('\\\\')
}

function positive(raw: string): number | null {
  const text = raw.trim()
  if (!/^\d+(\.\d+)?$/.test(text)) return null
  const value = Number(text)
  return Number.isFinite(value) && value > 0 ? value : null
}

function stockOf(raw: string): number | null {
  const text = raw.trim()
  if (!/^\d+$/.test(text)) return null
  return Number(text)
}

function attributeValue(def: CategoryAttribute, raw: { value: string; values: string[] } | undefined): { value: string; values: string[] } | null {
  if (!raw) return null
  if (def.input === 'multiSelect') {
    const values = raw.values
      .map((token) => def.options.find((option) => option.id === token || option.name === token)?.name)
      .filter((name): name is string => Boolean(name))
    return values.length ? { value: '', values } : null
  }
  if (def.input === 'select') {
    const option = def.options.find((item) => item.id === raw.value || item.name === raw.value)
    return option ? { value: option.name, values: [] } : null
  }
  const value = raw.value.trim()
  if (!value) return null
  if (def.input === 'number' && !Number.isFinite(Number(value))) return null
  return { value, values: [] }
}

export function buildListingRequest(
  form: ListingFormState,
  shops: ListingShopContext[],
  attributeDefs: Record<string, CategoryAttribute[] | 'loading' | 'error' | undefined>,
): { ok: true; draft: ListingDraft; targets: ListingTarget[] } | { ok: false; errors: string[] } {
  const errors: string[] = []
  const title = form.title.trim()
  if (!title || codePoints(title) > 255) errors.push('商品标题需为 1 到 255 个字符')
  const description = form.description.trim()
  if (!description || codePoints(description) > 3000) errors.push('请填写 3000 字以内的商品描述')
  const price = positive(form.price)
  if (price == null) errors.push('请填写大于 0 的价格')
  const currency = form.currency.trim().toUpperCase()
  if (!/^[A-Z]{3}$/.test(currency)) errors.push('请填写 3 位币种代码')
  const stock = stockOf(form.stock)
  if (stock == null) errors.push('请填写不小于 0 的库存')
  const skuCode = form.sku.trim()
  if (!skuCode) errors.push('请填写 SKU')
  const extra = form.extraSkus.map((row) => {
    const code = row.code.trim()
    const name = row.name.trim() || code
    const rowPrice = positive(row.price)
    const rowStock = stockOf(row.stock)
    if (!code || !name || rowPrice == null || rowStock == null) {
      errors.push('请补全每个规格的名称、SKU、价格和库存')
      return null
    }
    return { name, code, price: rowPrice, stock: rowStock }
  })
  if (form.images.length < 1 || form.images.length > 9) errors.push('请提供 1 到 9 张图片的绝对路径')
  for (const image of form.images) {
    if (!isAbsolutePath(image)) errors.push(`图片必须是绝对路径：${image}`)
  }
  const weight = positive(form.weightKg)
  if (weight == null) errors.push('请填写大于 0 的重量（千克）')
  const length = positive(form.lengthCm)
  const width = positive(form.widthCm)
  const height = positive(form.heightCm)
  if (length == null || width == null || height == null) errors.push('请填写包装长宽高（厘米）')
  if (form.shopIds.length === 0) errors.push('请选择至少一个店铺')
  if (new Set(form.shopIds).size !== form.shopIds.length) errors.push('同一批次不能重复选择同一店铺')

  const targets: ListingTarget[] = []
  for (const shopId of form.shopIds) {
    const shop = shops.find((item) => item.id === shopId)
    const name = shop?.name ?? shopId
    if (!shop || shop.listing == null) {
      errors.push('正在读取平台能力')
      continue
    }
    if (!shop.listing) {
      errors.push(shop.note || '该平台尚未接入上架')
      continue
    }
    const categoryId = (form.categories[shopId] ?? '').trim()
    if (!/^\d+$/.test(categoryId)) {
      errors.push(`请为${name}选择类目`)
      continue
    }
    const defs = attributeDefs[shopId]
    if (defs == null || defs === 'loading') {
      errors.push(`正在读取${name}的类目属性`)
      continue
    }
    if (defs === 'error') {
      errors.push(`${name}的类目属性读取失败`)
      continue
    }
    const filled = []
    for (const def of defs) {
      const value = attributeValue(def, form.attributes[shopId]?.[def.id])
      if (!value) {
        if (def.required) errors.push(`请填写属性：${def.name}`)
        continue
      }
      filled.push({ id: def.id, value: value.value, values: value.values })
    }
    targets.push({ shopId, categoryId, attributes: filled })
  }

  if (errors.length) return { ok: false, errors }
  const draft: ListingDraft = {
    title,
    description,
    price: price ?? undefined,
    currency,
    stock: stock ?? undefined,
    skus: [
      { name: skuCode, code: skuCode, price: price ?? undefined, stock: stock ?? undefined },
      ...extra.filter((row): row is { name: string; code: string; price: number; stock: number } => row != null),
    ],
    images: [...form.images],
    weightKg: weight ?? undefined,
    dimensionsCm: { l: length ?? 0, w: width ?? 0, h: height ?? 0 },
  }
  const brand = form.brand.trim()
  if (brand) draft.brand = brand
  return { ok: true, draft, targets }
}
