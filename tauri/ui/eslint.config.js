import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import solid from 'eslint-plugin-solid/configs/typescript';

export default tseslint.config(
  { ignores: ['dist', 'node_modules'] },
  js.configs.recommended,
  {
    files: ['public/**/*.js'],
    languageOptions: { sourceType: 'script', globals: { window: 'readonly', document: 'readonly', localStorage: 'readonly' } },
  },
  ...tseslint.configs.recommended,
  {
    files: ['src/**/*.{ts,tsx}'],
    ...solid,
    languageOptions: { parserOptions: { project: false } },
    rules: {
      '@typescript-eslint/no-non-null-assertion': 'off',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      'solid/reactivity': 'warn',
      'solid/no-innerhtml': 'error',
      // Solid assigns `let el!: HTMLElement` through ref={el}, which this rule cannot see.
      'no-unassigned-vars': 'off',
    },
  },
);
