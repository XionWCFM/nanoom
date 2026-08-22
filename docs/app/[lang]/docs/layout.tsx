import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import type { ReactNode } from 'react';
import { source } from '@/lib/source';
import { NavTitle } from '@/components/nav-title';

export default async function Layout(props: {
  params: Promise<{ lang: string }>;
  children: ReactNode;
}) {
  const { lang } = await props.params;
  const { children } = props;

  return (
    <DocsLayout
      tree={source.getPageTree(lang)}
      nav={{
        title: <NavTitle lang={lang} />,
      }}
    >
      {children}
    </DocsLayout>
  );
}
