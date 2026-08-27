'use client';

import { useTranslations } from 'next-intl';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';

export interface RefinementConfirmDialogProps {
  open: boolean;
  modelName: string;
  modelSizeMb: number;
  /** Recording duration in minutes — used to estimate refinement time. */
  recordingMinutes: number;
  onConfirm: () => void;
  onSkip: () => void;
}

/** Rough time budget: ~10 min on the Snapdragon X Elite for turbo-q5_0
 *  per hour of recording; larger models take longer. */
function estimateMinutes(recordingMinutes: number, modelSizeMb: number): number {
  const perHour = modelSizeMb >= 1500 ? 20 : modelSizeMb >= 500 ? 10 : 5;
  return Math.max(1, Math.ceil((recordingMinutes / 60) * perHour));
}

export function RefinementConfirmDialog({
  open,
  modelName,
  modelSizeMb,
  recordingMinutes,
  onConfirm,
  onSkip,
}: RefinementConfirmDialogProps) {
  const t = useTranslations('recording');
  const minutes = estimateMinutes(recordingMinutes, modelSizeMb);
  const hours = Math.max(0.25, Math.round((recordingMinutes / 60) * 4) / 4);

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onSkip()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('refine_dialog_title')}</DialogTitle>
          <DialogDescription className="space-y-2">
            <p>{t('refine_dialog_desc')}</p>
            <div className="text-xs text-gray-500 space-y-0.5">
              <p>
                {t('refine_dialog_model', {
                  model: modelName === 'default' ? t('refine_model_default') : modelName,
                  sizeMb: modelSizeMb || '?',
                })}
              </p>
              <p>
                {t('refine_dialog_estimate', {
                  hours,
                  minutes,
                })}
              </p>
            </div>
            <p className="text-xs italic text-gray-400 pt-1">
              {t('refine_dialog_summarize_only')}
            </p>
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="gap-2">
          <Button variant="outline" onClick={onSkip}>
            {t('refine_dialog_skip')}
          </Button>
          <Button onClick={onConfirm}>{t('refine_dialog_confirm')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
