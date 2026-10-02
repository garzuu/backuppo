// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const site = process.env.DOCS_SITE ?? 'https://garzuu.github.io';
const base = process.env.DOCS_BASE ?? '/backuppo';

export default defineConfig({
  site,
  base,
  publicDir: '../apps/web/public',
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
      favicon: '/backuppo-squirrel-192.png',
      sidebar: [
        {
          label: 'Inizia qui',
          translations: { en: 'Start here' },
          items: [
            {
              label: 'Come funziona',
              translations: { en: 'How it works' },
              slug: 'come-funziona',
            },
            {
              label: 'Concetti',
              translations: { en: 'Concepts' },
              slug: 'concetti',
            },
            {
              label: 'Installazione',
              translations: { en: 'Installation' },
              slug: 'installazione',
            },
            {
              label: 'Primo backup',
              translations: { en: 'First backup' },
              slug: 'quickstart',
            },
          ],
        },
        {
          label: 'Agent',
          items: [
            {
              label: 'Wizard e più job',
              translations: { en: 'Wizard and multiple jobs' },
              slug: 'wizard-job',
            },
            {
              label: 'Sorgenti',
              translations: { en: 'Sources' },
              slug: 'sorgenti',
            },
            {
              label: 'Destinazioni',
              translations: { en: 'Destinations' },
              slug: 'destinazioni',
            },
            {
              label: 'Operatività e notifiche',
              translations: { en: 'Operations and notifications' },
              slug: 'operativita',
            },
            {
              label: 'Restore',
              slug: 'restore',
            },
            {
              label: 'Aggiornamenti',
              translations: { en: 'Updates' },
              slug: 'updates',
            },
          ],
        },
        {
          label: 'Hub multi-sito',
          translations: { en: 'Multi-site Hub' },
          items: [
            {
              label: 'Agent, Hub e collegamento',
              translations: { en: 'Agent, Hub and connection' },
              slug: 'hub',
            },
            {
              label: "Deploy dell'Hub",
              translations: { en: 'Deploying the Hub' },
              slug: 'hub-deploy',
            },
          ],
        },
        {
          label: 'Riferimento',
          translations: { en: 'Reference' },
          items: [
            {
              label: 'Riferimento configurazione',
              translations: { en: 'Configuration reference' },
              slug: 'configuration',
            },
            {
              label: 'Comandi CLI',
              translations: { en: 'CLI commands' },
              slug: 'cli',
            },
            {
              label: 'Sicurezza',
              translations: { en: 'Security' },
              slug: 'sicurezza',
            },
            {
              label: 'Modello di minaccia',
              translations: { en: 'Threat model' },
              slug: 'modello-minaccia',
            },
            {
              label: 'Risoluzione dei problemi',
              translations: { en: 'Troubleshooting' },
              slug: 'troubleshooting',
            },
            {
              label: 'Confronto con altri strumenti',
              translations: { en: 'Comparison with other tools' },
              slug: 'confronto',
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
            {
              label: 'Pacchetti di sistema',
              translations: { en: 'System packages' },
              slug: 'packaging',
            },
            {
              label: 'Changelog e roadmap pubblica',
              translations: { en: 'Changelog and public roadmap' },
              slug: 'changelog',
            },
          ],
        },
      ],
    }),
  ],
});
