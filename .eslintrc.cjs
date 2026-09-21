module.exports = {
  root: true,
  env: { browser: true, es2020: true },
  extends: [
    'eslint:recommended',
    'plugin:@typescript-eslint/recommended',
    'plugin:react-hooks/recommended',
  ],
  ignorePatterns: ['dist', 'output', '.eslintrc.cjs', 'test-ZMGID'],
  parser: '@typescript-eslint/parser',
  plugins: ['react-refresh'],
  rules: {
    'react-refresh/only-export-components': [
      'warn',
      { allowConstantExport: true },
    ],
  },
  overrides: [
    {
      // Chat.tsx 是会话窗口的协调者，不是功能的家。这个上限是棘轮：只能往下调。
      // 要往上调，先在 PR 里回答「这段代码为什么不能归给某个 Pane / hook / 页面组件」。
      // 规则见 docs/engineering-standards.md §1「协调者不接收新功能」。
      files: ['src/chat/Chat.tsx'],
      rules: {
        'max-lines': ['error', { max: 3040, skipBlankLines: false, skipComments: false }],
      },
    },
  ],
}
