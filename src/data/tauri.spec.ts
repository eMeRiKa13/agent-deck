import { beforeEach, describe, expect, it, vi } from 'vitest'
import { DemoProviderAdapter } from '@/data/demo'

const { invokeMock, isTauriMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  isTauriMock: vi.fn(() => true),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
  isTauri: isTauriMock,
}))

import { TauriProviderAdapter } from '@/data/tauri'

describe('provider adapters', () => {
  beforeEach(() => {
    invokeMock.mockResolvedValue({})
  })

  it('forwards the quota option to the Tauri command', async () => {
    await new TauriProviderAdapter().getSnapshot({ includeQuotas: false })

    expect(invokeMock).toHaveBeenCalledWith('get_dashboard_snapshot', {
      includeQuotas: false,
    })
  })

  it('omits quotas from lightweight demo snapshots', async () => {
    const snapshot = await new DemoProviderAdapter().getSnapshot({ includeQuotas: false })

    expect(snapshot.quotas).toEqual([])
  })
})
