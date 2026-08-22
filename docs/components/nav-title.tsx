'use client';

export function NavTitle({ lang }: { lang: string }) {
  return (
    <span className="flex items-baseline gap-3">
      <a href={`/${lang}/docs`} className="font-semibold text-fd-foreground">
        nanoom
      </a>
      {lang === 'en' ? (
        <a
          href="/ko/docs"
          className="text-sm text-fd-muted-foreground hover:underline"
        >
          한국어
        </a>
      ) : (
        <a
          href="/en/docs"
          className="text-sm text-fd-muted-foreground hover:underline"
        >
          English
        </a>
      )}
    </span>
  );
}
