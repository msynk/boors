import { defineConfig } from 'vitest/config';

// Unit tests cover the pure modules (formatting, transport, grid logic, mocks); they need no DOM.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
