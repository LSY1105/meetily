"use client";

import { useState, useEffect } from "react";
import { useTranslations } from "next-intl";
import { Label } from "./ui/label";
import { Textarea } from "./ui/textarea";

export interface HotwordsInputProps {
  value: string;
  onChange: (next: string) => void;
  maxChars?: number;
}

export function HotwordsInput({ value, onChange, maxChars = 1000 }: HotwordsInputProps) {
  const t = useTranslations("settings");
  const [draft, setDraft] = useState<string>(value ?? "");

  useEffect(() => {
    setDraft(value ?? "");
  }, [value]);

  const remaining = maxChars - draft.length;
  const overLimit = remaining < 0;

  return (
    <div className="space-y-2">
      <Label htmlFor="hotwords-input">{t("transcript.hotwords_label")}</Label>
      <Textarea
        id="hotwords-input"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          const trimmed = overLimit ? draft.slice(0, maxChars) : draft;
          if (trimmed !== draft) setDraft(trimmed);
          onChange(trimmed);
        }}
        placeholder={t("transcript.hotwords_placeholder")}
        rows={3}
        className={overLimit ? "border-red-500" : undefined}
        aria-describedby="hotwords-help hotwords-counter"
      />
      <div id="hotwords-help" className="text-xs text-gray-500">
        {t("transcript.hotwords_help")}
      </div>
      <div id="hotwords-counter" className={`text-xs ${overLimit ? "text-red-500" : "text-gray-400"}`}>
        {t("transcript.hotwords_counter", { count: draft.length, max: maxChars })}
      </div>
    </div>
  );
}
