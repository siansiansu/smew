import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'skua',
  description:
    'Local AWS SSM connection tool: interactive inventory browser + iTerm2-like multiplexing/broadcast',
  base: '/skua/',
  head: [['link', { rel: 'icon', type: 'image/svg+xml', href: '/skua/logo.svg' }]],
  themeConfig: {
    logo: '/logo.svg',
    nav: [
      { text: 'Guide', link: '/guide/installation' },
      { text: 'Reference', link: '/guide/keybindings' },
    ],
    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Installation', link: '/guide/installation' },
          { text: 'Quick start', link: '/guide/quick-start' },
        ],
      },
      {
        text: 'Reference',
        items: [
          { text: 'Keybindings', link: '/guide/keybindings' },
          { text: 'Configuration', link: '/guide/configuration' },
        ],
      },
      {
        text: 'Recipes',
        items: [
          { text: 'SSH / scp / rsync over SSM', link: '/guide/ssh-over-ssm' },
          {
            text: 'EC2 Instance Connect on Oracle Linux',
            link: '/guide/ec2-instance-connect-oracle-linux',
          },
        ],
      },
    ],
    socialLinks: [{ icon: 'github', link: 'https://github.com/siansiansu/skua' }],
    search: { provider: 'local' },
    editLink: {
      pattern: 'https://github.com/siansiansu/skua/edit/main/docs/:path',
      text: 'Edit this page on GitHub',
    },
    footer: {
      message: 'Released under the MIT License.',
    },
  },
})
