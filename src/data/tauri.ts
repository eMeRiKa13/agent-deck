import { invoke, isTauri } from '@tauri-apps/api/core'
import { DemoProviderAdapter } from '@/data/demo'
import type { DashboardSnapshot, DashboardSnapshotOptions, ProviderAdapter } from '@/domain/dashboard'

export class TauriProviderAdapter implements ProviderAdapter {
  async getSnapshot(options?: DashboardSnapshotOptions): Promise<DashboardSnapshot> {
    return invoke<DashboardSnapshot>('get_dashboard_snapshot', {
      includeQuotas: options?.includeQuotas ?? true,
    })
  }
}

export function createProviderAdapter(): ProviderAdapter {
  return isTauri() ? new TauriProviderAdapter() : new DemoProviderAdapter()
}
