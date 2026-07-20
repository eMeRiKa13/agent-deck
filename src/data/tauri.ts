import { invoke, isTauri } from '@tauri-apps/api/core'
import { DemoProviderAdapter } from '@/data/demo'
import type { DashboardSnapshot, ProviderAdapter } from '@/domain/dashboard'

export class TauriProviderAdapter implements ProviderAdapter {
  async getSnapshot(): Promise<DashboardSnapshot> {
    return invoke<DashboardSnapshot>('get_dashboard_snapshot')
  }
}

export function createProviderAdapter(): ProviderAdapter {
  return isTauri() ? new TauriProviderAdapter() : new DemoProviderAdapter()
}
