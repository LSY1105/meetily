"use client";

import { NextIntlClientProvider } from "next-intl";
import { useMemo, type ReactNode } from "react";
import { useLocale } from "@/hooks/useLocale";
import { loadMessages } from "@/i18n/request";
import type { Locale } from "@/i18n/config";

interface Props {
  initialLocale: Locale;
  // next-intl's getMessages() returns a loosely-typed Record; we keep the
  // same shape here so the server and client providers agree.
  initialMessages: Record<string, unknown>;
  children: ReactNode;
}

/**
 * Client-side wrapper around next-intl's NextIntlClientProvider.
 *
 * Reads the live locale from LocaleProvider (set by the language picker)
 * and re-loads the message catalog so useTranslations() returns the new
 * language without a hard refresh.
 *
 * ponytail: server passes both initial locale and initial messages; we
 * re-import only when the locale actually changes (memoized).
 */
export default function I18nProvider({ initialLocale, initialMessages, children }: Props) {
  const { locale } = useLocale();

  // ponytail: sync import via the existing loadMessages helper. Switching
  // locales is rare; the import cost is fine. Add dynamic import if the
  // message bundles grow large enough to bloat the initial client chunk.
  const messages = useMemo(() => {
    if (locale === initialLocale) return initialMessages;
    return loadMessages(locale) as unknown as Record<string, unknown>;
  }, [locale, initialLocale, initialMessages]);

  return (
    <NextIntlClientProvider locale={locale} messages={messages}>
      {children}
    </NextIntlClientProvider>
  );
}
