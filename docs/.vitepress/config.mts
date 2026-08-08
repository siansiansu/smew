import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'smew',
  description:
    'Terminal UI to explore AWS resources and open SSM sessions with split panes and broadcast',
  base: '/smew/',
  head: [['link', { rel: 'icon', type: 'image/svg+xml', href: '/smew/logo.svg' }]],
  themeConfig: {
    logo: '/logo.svg',
    nav: [
      { text: 'Guide', link: '/guide/installation' },
      { text: 'Features', link: '/guide/resource-views' },
      { text: 'Reference', link: '/guide/keybindings' },
    ],
    sidebar: [
      {
        text: 'Getting started',
        items: [
          { text: 'Installation', link: '/guide/installation' },
          { text: 'Quick start', link: '/guide/quick-start' },
          { text: 'Authentication', link: '/guide/authentication' },
        ],
      },
      {
        text: 'Features',
        items: [
          { text: 'Resource views', link: '/guide/resource-views' },
          { text: 'Sessions & port forwarding', link: '/guide/sessions' },
          { text: 'SSH / scp / rsync over SSM', link: '/guide/ssh-over-ssm' },
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
