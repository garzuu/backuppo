// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://backuppo.garzuu.com',
  integrations: [
    starlight({
      title: 'Backuppo',
      description:
        'Backup open source, cross-platform, con verifica automatica del restore.',
      defaultLocale: 'root',
      locales: {
        root: { label: 'Italiano', lang: 'it' },
        en: { label: 'English', lang: 'en' },
      },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/garzuu/backuppo' },
      ],
      sidebar: [
        {
          label: 'Guida',
          translations: { en: 'Guide' },
          items: [
            { label: 'Quickstart', slug: 'quickstart' },
            {
              label: 'Riferimento configurazione',
              translations: { en: 'Configuration reference' },
              slug: 'configuration',
            },
            {
              label: 'Restore manuale',
              translations: { en: 'Manual restore' },
              slug: 'restore',
            },
          ],
        },
        {
          label: 'Hub multi-sito',
          translations: { en: 'Multi-site hub' },
          items: [
            {
              label: "Deploy dell'hub",
              translations: { en: 'Deploying the hub' },
              slug: 'hub-deploy',
            },
          ],
        },
        {
          label: 'Progetto',
          translations: { en: 'Project' },
          items: [
            {
              label: 'Procedura di release',
              translations: { en: 'Release process' },
              slug: 'release',
            },
          ],
        },
      ],
    }),
  ],
});
