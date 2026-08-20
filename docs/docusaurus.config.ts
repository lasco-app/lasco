import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

const config: Config = {
  title: 'Lasco',
  tagline: 'Photo and video backup solution',
  favicon: 'img/favicon.png',

  // Future flags, see https://docusaurus.io/docs/api/docusaurus-config#future
  future: {
    v4: true, // Improve compatibility with the upcoming Docusaurus v4
  },

  // Set the production url of your site here
  url: 'https://getlasco.app',
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: '/',

  // GitHub pages deployment config.
  // If you aren't using GitHub pages, you don't need these.
  organizationName: 'lasco', // Usually your GitHub org/user name.
  projectName: 'lasco', // Usually your repo name.

  onBrokenLinks: 'throw',

  // Even if you don't use internationalization, you can use this field to set
  // useful metadata like html lang. For example, if your site is Chinese, you
  // may want to replace "en" with "zh-Hans".
  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          exclude: ['**/design-system/**'],
          // Please change this to your repo.
          // Remove this to remove the "edit this page" links.
          editUrl:
            'https://github.com/facebook/docusaurus/tree/main/packages/create-docusaurus/templates/shared/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    // Replace with your project's social card
    image: 'img/logo.svg',
    colorMode: {
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: '',
      logo: {
        alt: 'Lasco Logo',
        src: 'img/logo_transparent_0_5.png',
        srcDark: 'img/logo_black_0_5.png',
      },
      items: [
        {
          to: '/roadmap',
          label: 'ROADMAP',
          position: 'right',
        },
        {
          href: 'https://github.com/lasco-app/lasco',
          label: 'GitHub',
          position: 'right',
        },
        {
          type: 'docSidebar',
          sidebarId: 'mainSidebar',
          position: 'right',
          label: 'DOCS',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'PRODUCT',
          items: [
            {
              label: 'Documentation',
              to: '/docs/summary',
            },
            {
              label: 'Format Specification',
              to: '/docs/format-specification/concepts',
            },
          ],
        },
        {
          title: 'COMPANY',
          items: [
            {
              label: 'Who We Are',
              to: '/who-we-are',
            },
            {
              label: 'Privacy Policy',
              to: '/privacy-policy',
            },
            // {
            //   label: 'Design System',
            //   to: '/docs/design-system',
            // },
          ],
        },
        // {
        //   title: 'COMPARISONS',
        //   items: [
        //     {
        //       label: 'Compare',
        //       to: '/compared',
        //     },
        //     {
        //       label: 'Lasco vs. Ente',
        //       to: '/vs-ente',
        //     },
        //   ],
        // },
        {
          title: 'COMMUNITY',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/lasco-app/lasco',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Lasco`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
