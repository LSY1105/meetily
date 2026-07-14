"use client";

import { useTranslations } from "next-intl";
import { Label } from "./ui/label";
import { Input } from "./ui/input";
import { HighlightConfig } from "@/lib/transcriptHighlight";

export interface HighlightSettingsProps {
    config: HighlightConfig;
    setConfig: (config: HighlightConfig) => void;
}

export function HighlightSettings({ config, setConfig }: HighlightSettingsProps) {
    const t = useTranslations("settings");
    return (
        <div className="space-y-3 mt-4 pt-4 border-t border-gray-200">
            <div>
                <h4 className="text-sm font-semibold text-gray-900">
                    {t("highlight.title")}
                </h4>
                <p className="text-xs text-gray-500 mt-1">
                    {t("highlight.description")}
                </p>
            </div>
            <label className="flex items-center gap-2 cursor-pointer">
                <input
                    type="checkbox"
                    className="h-4 w-4 rounded border-gray-300"
                    checked={config.enabled}
                    onChange={(e) => setConfig({ ...config, enabled: e.target.checked })}
                />
                <span className="text-sm text-gray-700">{t("highlight.enable_label")}</span>
            </label>
            <div>
                <Label className="block text-sm font-medium text-gray-700 mb-1">
                    {t("highlight.custom_label")}
                </Label>
                <Input
                    value={config.customKeywords}
                    onChange={(e) => setConfig({ ...config, customKeywords: e.target.value })}
                    placeholder={t("highlight.custom_placeholder")}
                />
                <p className="text-xs text-gray-500 mt-1">
                    {t("highlight.custom_help")}
                </p>
            </div>
            <div className="text-xs text-gray-400 space-y-1">
                <div>{t("highlight.legend_number")}</div>
                <div>{t("highlight.legend_date")}</div>
                <div>{t("highlight.legend_proper")}</div>
                <div>{t("highlight.legend_custom")}</div>
            </div>
        </div>
    );
}
