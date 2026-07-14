"use client";

import { useTranslations } from "next-intl";
import { Label } from "./ui/label";
import { Input } from "./ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./ui/select";
import { Textarea } from "./ui/textarea";

export type PostprocessProvider = "claude" | "groq" | "openai" | "ollama";

export interface PostprocessConfig {
    /** Master switch. false = skip postprocess entirely. */
    enabled: boolean;
    /** LLM provider. */
    provider: PostprocessProvider;
    /** Provider-specific model id. */
    model_name: string;
    /** Comma-separated high-priority terms; injected as a fix-priority hint. */
    custom_hotwords: string;
}

export const DEFAULT_POSTPROCESS_CONFIG: PostprocessConfig = {
    enabled: false,
    provider: "claude",
    model_name: "claude-3-5-sonnet-latest",
    custom_hotwords: "",
};

export interface PostprocessSettingsProps {
    config: PostprocessConfig;
    setConfig: (config: PostprocessConfig) => void;
}

export function PostprocessSettings({ config, setConfig }: PostprocessSettingsProps) {
    const t = useTranslations("settings");

    return (
        <div className="space-y-4 mt-6 pt-6 border-t border-gray-200">
            <div>
                <h3 className="text-lg font-semibold text-gray-900">
                    {t("postprocess.title")}
                </h3>
                <p className="text-sm text-gray-500 mt-1">
                    {t("postprocess.description")}
                </p>
            </div>

            <label className="flex items-center gap-2 cursor-pointer">
                <input
                    type="checkbox"
                    className="h-4 w-4 rounded border-gray-300"
                    checked={config.enabled}
                    onChange={(e) => setConfig({ ...config, enabled: e.target.checked })}
                />
                <span className="text-sm text-gray-700">
                    {t("postprocess.enable_label")}
                </span>
            </label>

            <div className={config.enabled ? "" : "opacity-50 pointer-events-none"}>
                <Label className="block text-sm font-medium text-gray-700 mb-1">
                    {t("postprocess.provider_label")}
                </Label>
                <Select
                    value={config.provider}
                    onValueChange={(v) => setConfig({ ...config, provider: v as PostprocessProvider })}
                >
                    <SelectTrigger className="focus:ring-1 focus:ring-blue-500 focus:border-blue-500">
                        <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                        <SelectItem value="claude">Claude</SelectItem>
                        <SelectItem value="groq">Groq</SelectItem>
                        <SelectItem value="openai">OpenAI</SelectItem>
                        <SelectItem value="ollama">Ollama (local)</SelectItem>
                    </SelectContent>
                </Select>
            </div>

            <div className={config.enabled ? "" : "opacity-50 pointer-events-none"}>
                <Label className="block text-sm font-medium text-gray-700 mb-1">
                    {t("postprocess.model_label")}
                </Label>
                <Input
                    value={config.model_name}
                    onChange={(e) => setConfig({ ...config, model_name: e.target.value })}
                    placeholder="claude-3-5-sonnet-latest"
                />
            </div>

            <div className={config.enabled ? "" : "opacity-50 pointer-events-none"}>
                <Label className="block text-sm font-medium text-gray-700 mb-1">
                    {t("postprocess.hotwords_label")}
                </Label>
                <Textarea
                    value={config.custom_hotwords}
                    onChange={(e) => setConfig({ ...config, custom_hotwords: e.target.value })}
                    placeholder={t("postprocess.hotwords_placeholder")}
                    rows={2}
                />
                <p className="text-xs text-gray-500 mt-1">
                    {t("postprocess.hotwords_help")}
                </p>
            </div>
        </div>
    );
}
