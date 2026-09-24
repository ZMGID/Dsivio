import { fireEvent, render, screen, within } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { ShopBindingPage } from './ShopBindingPage'

vi.mock('../../../api/tauri', () => ({ api: {}, isTauriRuntime: () => false }))

beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', '') }
})

it('opens the selected platform in a dialog and returns to the platform list', () => {
  render(<ShopBindingPage />)
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()

  fireEvent.click(screen.getAllByRole('button', { name: '立即绑定' })[1])
  const dialog = screen.getByRole('dialog', { name: '希音 SHEIN' })
  expect(within(dialog).getByText('App ID / Key')).toBeInTheDocument()
  expect(within(dialog.querySelector('.shop-bind-form-head')!).getByRole('button', { name: '关闭' })).toBeInTheDocument()
  expect(within(dialog.querySelector('.shop-bind-form-actions')!).getByRole('button', { name: '取消' })).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: '选择店铺平台' })).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: '已绑定店铺' })).toBeInTheDocument()

  fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  fireEvent.click(screen.getAllByRole('button', { name: '立即绑定' })[0])
  const nextDialog = screen.getByRole('dialog', { name: '虾皮 Shopee' })
  fireEvent.click(within(nextDialog).getByRole('button', { name: '关闭' }))
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
})

it('lists domestic platforms as bindable with platform-specific credential labels', () => {
  render(<ShopBindingPage />)

  fireEvent.click(screen.getByRole('button', { name: /国内平台 5/ }))
  fireEvent.click(screen.getAllByRole('button', { name: '立即绑定' })[2])
  const dialog = screen.getByRole('dialog', { name: '微信小店' })
  expect(within(dialog).getByText('小店 AppID')).toBeInTheDocument()
  expect(within(dialog).getByText('小店 AppSecret')).toBeInTheDocument()
  expect(within(dialog).queryByText('平台应用中登记的 HTTPS 回调地址')).not.toBeInTheDocument()
  expect(within(dialog).getByRole('button', { name: '完成绑定' })).toBeDisabled()

  fireEvent.click(within(dialog).getByRole('button', { name: '取消' }))
  fireEvent.click(screen.getAllByRole('button', { name: '立即绑定' })[0])
  const doudianDialog = screen.getByRole('dialog', { name: '抖店' })
  expect(within(doudianDialog).getByText('App Key')).toBeInTheDocument()
})
