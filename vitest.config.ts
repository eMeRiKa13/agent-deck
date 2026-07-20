import { mergeConfig, defineConfig } from 'vitest/config'
import viteConfig from './vite.config'

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: 'happy-dom',
      clearMocks: true,
      restoreMocks: true,
      include: ['src/**/*.spec.ts'],
    },
  }),
)
