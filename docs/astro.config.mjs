// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://backuppo.dev',
  integrations: [
    starlight({
      title: 'Backuppo',
      description:
        "Backup open source, cross-platform, con verifica automatica del restore.",
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/garzuu/backuppo' },
      ],
      sidebar: [
        {
          label: 'Guida',
          items: [
            { label: 'Quickstart', slug: 'quickstart' },
            { label: 'Riferimento configurazione', slug: 'configuration' },
            { label: 'Restore manuale', slug: 'restore' },
          ],
        },
        {
          label: 'Hub multi-sito',
          items: [{ label: 'Deploy dell\'hub', slug: 'hub-deploy' }],
        },
        {
          label: 'Progetto',
          items: [{ label: 'Procedura di release', slug: 'release' }],
        },
      ],
    }),
  ],
});
