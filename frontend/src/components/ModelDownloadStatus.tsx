import React from 'react';
import { useTranslations } from 'next-intl';
import { motion } from 'framer-motion';

interface ModelDownloadStatusProps {
  progress: number;
  onCancel?: () => void;
  sizeText?: string;
  labelClassName?: string;
  barClassName?: string;
}

export function ModelDownloadStatus({
  progress,
  onCancel,
  sizeText,
  labelClassName = 'text-blue-600',
  barClassName = 'from-blue-500 to-blue-600',
}: ModelDownloadStatusProps) {
  const t = useTranslations();
  const pct = Math.round(progress);
  return (
    <motion.div
      initial={{ opacity: 0, height: 0 }}
      animate={{ opacity: 1, height: 'auto' }}
      exit={{ opacity: 0, height: 0 }}
      className="mt-3 pt-3 border-t border-gray-200"
    >
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <span className={`text-sm font-medium ${labelClassName}`}>
            {t('settings.transcript.models.downloading')}
          </span>
          <span className={`text-sm font-semibold ${labelClassName}`}>
            {t('settings.transcript.models.downloading_pct', { pct })}
          </span>
        </div>
        {onCancel && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onCancel();
            }}
            className="text-xs text-gray-600 hover:text-red-600 font-medium transition-colors px-2 py-1 rounded hover:bg-red-50"
            title={t('settings.transcript.models.cancel_download')}
          >
            {t('settings.transcript.models.cancel_download')}
          </button>
        )}
      </div>
      <div className="w-full h-2 bg-gray-200 rounded-full overflow-hidden">
        <motion.div
          className={`h-full bg-gradient-to-r ${barClassName} rounded-full`}
          initial={{ width: 0 }}
          animate={{ width: `${progress}%` }}
          transition={{ duration: 0.3, ease: 'easeOut' }}
        />
      </div>
      {sizeText && <p className="text-xs text-gray-500 mt-1">{sizeText}</p>}
    </motion.div>
  );
}
