'use client';

import { useEffect } from 'react';
import { listen } from '@/lib/transport';
import { toast } from 'sonner';
import { useTranslations } from 'next-intl';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';

export function RecordingErrorListener() {
  const t = useTranslations('recording.errors');
  const { setStatus } = useRecordingState();

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      const fn = await listen<string>('recording-error', (event) => {
        const message = typeof event.payload === 'string' && event.payload
          ? event.payload
          : t('unknown_message');
        setStatus(RecordingStatus.ERROR, message);
        toast.error(t('unexpected_stop'), { description: message });
      });
      unlisten = fn;
    })().catch((e) => {
      console.warn('recording-error listener failed', e);
    });
    return () => { unlisten?.(); };
  }, [setStatus, t]);

  return null;
}
