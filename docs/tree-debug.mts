import { docs, meta } from './.source/server';
import { toFumadocsSource } from 'fumadocs-mdx/runtime/server';
import { loader } from 'fumadocs-core/source';

const source = loader({
  baseUrl: '/docs',
  source: toFumadocsSource(docs, meta),
  i18n: {
    parser: 'dir',
    languages: ['en', 'ko'],
    defaultLanguage: 'en',
  },
});

console.log('=== EN tree ===');
console.log(JSON.stringify(source.getPageTree('en'), null, 1).slice(0, 2000));
console.log('=== KO tree ===');
console.log(JSON.stringify(source.getPageTree('ko'), null, 1).slice(0, 600));
