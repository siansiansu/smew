import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'smew',
  description:
    'Local AWS SSM connection tool: interactive inventory browser + iTerm2-like multiplexing/broadcast',
  base: '/smew/',
  head: [['link', { rel: 'icon', type: 'image/svg+xml', href: '/smew/logo.svg' }]],
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
          { text: 'Authentication', link: '/guide/authentication' },
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
    socialLinks: [{ icon: 'github', link: 'https://github.com/siansiansu/smew' }],
    search: { provider: 'local' },
    editLink: {
      pattern: 'https://github.com/siansiansu/smew/edit/main/docs/:path',
      text: 'Edit this page on GitHub',
    },
    footer: {
      message: 'Released under the MIT License.',
    },
  },
})
