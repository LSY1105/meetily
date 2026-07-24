import { useState, useEffect } from 'react';
import { useTranslations } from 'next-intl';
import { invoke } from '@tauri-apps/api/core';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { toast } from 'sonner';
import { Eye, EyeOff, Lock, Unlock } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { HotwordHitStatsPanel } from './HotwordHitStatsPanel';
import { LLMDiagnosticsPanel } from './LLMDiagnosticsPanel';
import { useDiarizationConfig } from '@/hooks/useDiarizationConfig';
import { ParakeetModelManager } from './ParakeetModelManager';

const MAX_HOTWORD_CHARS = 500;


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai';
    model: string;
    apiKey?: string | null;
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onModelSelect?: () => void;
}

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig, onModelSelect }: TranscriptSettingsProps) {
    const [apiKey, setApiKey] = useState<string | null>(transcriptModelConfig.apiKey || null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
    const [uiProvider, setUiProvider] = useState<TranscriptModelProps['provider']>(transcriptModelConfig.provider);
    const [hotwords, setHotwords] = useState('');
    const [savedHotwords, setSavedHotwords] = useState('');
    const [isLoadingHotwords, setIsLoadingHotwords] = useState(true);
    const [isSavingHotwords, setIsSavingHotwords] = useState(false);
    const [hotwordsLoadFailed, setHotwordsLoadFailed] = useState(false);
    // PR-42-iii: streaming LLM postprocess toggle state.
    const [autoPostprocessEnabled, setAutoPostprocessEnabled] = useState<boolean>(true);
    const [isLoadingAutoPostprocess, setIsLoadingAutoPostprocess] = useState(true);
    const [autoPostprocessLoadFailed, setAutoPostprocessLoadFailed] = useState(false);
  const t = useTranslations('settings');

    // Sync uiProvider when backend config changes (e.g., after model selection or initial load)
    useEffect(() => {
        setUiProvider(transcriptModelConfig.provider);
    }, [transcriptModelConfig.provider]);

    useEffect(() => {
        if (transcriptModelConfig.provider === 'localWhisper' || transcriptModelConfig.provider === 'parakeet') {
            setApiKey(null);
        }
    }, [transcriptModelConfig.provider]);

    useEffect(() => {
        let active = true;

        invoke<string | null>('get_transcription_hotwords')
            .then((value) => {
                if (!active) return;
                const loaded = value ?? '';
                setHotwords(loaded);
                setSavedHotwords(loaded);
            })
            .catch((error) => {
                console.error('Failed to load transcription hotwords:', error);
                if (active) setHotwordsLoadFailed(true);
            })
            .finally(() => {
                if (active) setIsLoadingHotwords(false);
            });

        return () => {
            active = false;
        };
    }, []);
    // PR-42-iii: load the auto-LLM-postprocess toggle from
    // transcription-preferences.json under auto_postprocess_enabled key.
    useEffect(() => {
        let active = true;
        (async () => {
            try {
                const { Store } = await import('@tauri-apps/plugin-store');
                const store = await Store.load('transcription-preferences.json');
                const value = await store.get<boolean>('auto_postprocess_enabled');
                if (active) setAutoPostprocessEnabled(value ?? true);
            } catch (error) {
                console.error('Failed to load auto_postprocess_enabled:', error);
                if (active) setAutoPostprocessLoadFailed(true);
            } finally {
                if (active) setIsLoadingAutoPostprocess(false);
            }
        })();
        return () => { active = false; };
    }, []);
    const handleAutoPostprocessToggle = async (next: boolean) => {
        const previous = autoPostprocessEnabled;
        setAutoPostprocessEnabled(next);
        try {
            const { Store } = await import('@tauri-apps/plugin-store');
            const store = await Store.load('transcription-preferences.json');
            await store.set('auto_postprocess_enabled', next);
            await store.save();
        } catch (error) {
            console.error('Failed to save auto_postprocess_enabled:', error);
            setAutoPostprocessEnabled(previous);
            toast.error(t('transcript.auto_postprocess_save_failed'));
        }
    };

    const hotwordCharCount = Array.from(hotwords).length;
    const isOverHotwordLimit = hotwordCharCount > MAX_HOTWORD_CHARS;

    const handleSaveHotwords = async () => {
        if (isOverHotwordLimit) return;

        setIsSavingHotwords(true);
        try {
            const saved = await invoke<string | null>('set_transcription_hotwords', { hotwords });
            const normalized = saved ?? '';
            setHotwords(normalized);
            setSavedHotwords(normalized);
            toast.success(t('transcript.hotwords_save_success'));
        } catch (error) {
            console.error('Failed to save transcription hotwords:', error);
            toast.error(t('transcript.hotwords_save_failed'));
        } finally {
            setIsSavingHotwords(false);
        }
    };

    const fetchApiKey = async (provider: string) => {
        try {

            const data = await invoke('api_get_transcript_api_key', { provider }) as string;

            setApiKey(data || '');
        } catch (err) {
            console.error('Error fetching API key:', err);
            setApiKey(null);
        }
    };
    const modelOptions = {
        localWhisper: [], // Model selection handled by ModelManager component
        parakeet: [], // Model selection handled by ParakeetModelManager component
        deepgram: ['nova-2-phonecall'],
        elevenLabs: ['eleven_multilingual_v2'],
        groq: ['llama-3.3-70b-versatile'],
        openai: ['gpt-4o'],
    };
    const requiresApiKey = transcriptModelConfig.provider === 'deepgram' || transcriptModelConfig.provider === 'elevenLabs' || transcriptModelConfig.provider === 'openai' || transcriptModelConfig.provider === 'groq';

    const handleInputClick = () => {
        if (isApiKeyLocked) {
            setIsLockButtonVibrating(true);
            setTimeout(() => setIsLockButtonVibrating(false), 500);
        }
    };

    const handleWhisperModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'localWhisper', // Ensure provider is set correctly
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    const handleParakeetModelSelect = (modelName: string) => {
        // Always update config when model is selected, regardless of current provider
        // This ensures the model is set when user switches back
        setTranscriptModelConfig({
            ...transcriptModelConfig,
            provider: 'parakeet', // Ensure provider is set correctly
            model: modelName
        });
        // Close modal after selection
        if (onModelSelect) {
            onModelSelect();
        }
    };

    return (
        <div>
            <div>
                {/* <div className="flex justify-between items-center mb-4">
                    <h3 className="text-lg font-semibold text-gray-900">Transcript Settings</h3>
                </div> */}
                <div className="space-y-4 pb-6">
                    <div>
                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                            Transcript Model
                        </Label>
                        <div className="flex space-x-2 mx-1">
                            <Select
                                value={uiProvider}
                                onValueChange={(value) => {
                                    const provider = value as TranscriptModelProps['provider'];
                                    setUiProvider(provider);
                                    if (provider !== 'localWhisper' && provider !== 'parakeet') {
                                        fetchApiKey(provider);
                                    }
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder={t('transcript.select_provider', { default: 'Select provider' })} />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">⚡ Parakeet (Recommended - Real-time / Accurate)</SelectItem>
                                    <SelectItem value="localWhisper">🏠 Local Whisper (High Accuracy)</SelectItem>
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'localWhisper' && uiProvider !== 'parakeet' && (
                                <Select
                                    value={transcriptModelConfig.model}
                                    onValueChange={(value) => {
                                        const model = value as TranscriptModelProps['model'];
                                        setTranscriptModelConfig({ ...transcriptModelConfig, provider: uiProvider, model });
                                    }}
                                >
                                    <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                        <SelectValue placeholder="Select model" />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {modelOptions[uiProvider].map((model) => (
                                            <SelectItem key={model} value={model}>{model}</SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            )}

                        </div>
                    </div>

                    {uiProvider === 'localWhisper' && (
                        <div className="mt-6">
                            <ModelManager
                                selectedModel={transcriptModelConfig.provider === 'localWhisper' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleWhisperModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}

                    {uiProvider === 'parakeet' && (
                        <div className="mt-6">
                            <ParakeetModelManager
                                selectedModel={transcriptModelConfig.provider === 'parakeet' ? transcriptModelConfig.model : undefined}
                                onModelSelect={handleParakeetModelSelect}
                                autoSave={true}
                            />
                        </div>
                    )}


                    <div className="space-y-2 rounded-lg border border-gray-200 p-4">
                        <div>
                            <Label htmlFor="transcription-hotwords" className="text-sm font-medium text-gray-900">
                                {t('transcript.hotwords_title')}
                            </Label>
                            <p className="mt-1 text-sm text-gray-600">
                                {t('transcript.hotwords_description')}
                            </p>
                        </div>
                        <Textarea
                            id="transcription-hotwords"
                            value={hotwords}
                            onChange={(event) => setHotwords(event.target.value)}
                            maxLength={MAX_HOTWORD_CHARS}
                            rows={6}
                            disabled={isLoadingHotwords || hotwordsLoadFailed}
                            placeholder={t('transcript.hotwords_placeholder')}
                        />
                        <div className="flex items-center justify-between gap-4">
                            <span className={`text-xs ${isOverHotwordLimit ? 'text-red-600' : 'text-gray-500'}`}>
                                {hotwordCharCount}/{MAX_HOTWORD_CHARS}
                            </span>
                            <Button
                                type="button"
                                size="sm"
                                onClick={handleSaveHotwords}
                                disabled={
                                    isLoadingHotwords ||
                                    isSavingHotwords ||
                                    hotwordsLoadFailed ||
                                    isOverHotwordLimit ||
                                    hotwords === savedHotwords
                                }
                            >
                                {isSavingHotwords
                                    ? t('transcript.hotwords_saving')
                                    : t('transcript.hotwords_save')}
                            </Button>
                        </div>
                        <p className="text-xs text-gray-500">
                            {t('transcript.hotwords_local_whisper_only')}
                        </p>
                        {hotwordsLoadFailed && (
                            <p className="text-sm text-red-600">
                                {t('transcript.hotwords_load_failed')}
                            </p>
                        )}
                    </div>
                    {/* PR-42-iii: streaming LLM postprocess toggle */}
                    <div className="space-y-2 rounded-lg border border-gray-200 p-4 mt-4">
                        <div className="flex items-start justify-between gap-4">
                            <div>
                                <Label htmlFor="transcript-auto-postprocess" className="text-sm font-medium text-gray-900">
                                    {t('transcript.auto_postprocess_label')}
                                </Label>
                                <p className="mt-1 text-sm text-gray-600">
                                    {t('transcript.auto_postprocess_help')}
                                </p>
                            </div>
                            <input
                                id="transcript-auto-postprocess"
                                type="checkbox"
                                className="h-4 w-4 mt-1"
                                checked={autoPostprocessEnabled}
                                onChange={(e) => void handleAutoPostprocessToggle(e.target.checked)}
                                disabled={isLoadingAutoPostprocess}
                            />
                        </div>
                        {autoPostprocessLoadFailed && (
                            <p className="text-sm text-red-600">
                                {t('transcript.auto_postprocess_load_failed')}
                            </p>
                        )}
                    </div>
                    {requiresApiKey && (
                    <>
                    <div className="mt-4">
                        <HotwordHitStatsPanel />
                    </div>
                    <div className="mt-4">
                        <LLMDiagnosticsPanel />
                    </div>
                    <div className="mt-4">
                        <DiarizationSettingsBlock />
                    </div>
                        <div>
                            <Label className="block text-sm font-medium text-gray-700 mb-1">
                                API Key
                            </Label>
                            <div className="relative mx-1">
                                <Input
                                    type={showApiKey ? "text" : "password"}
                                    className={`pr-24 focus:ring-1 focus:ring-blue-500 focus:border-blue-500 ${isApiKeyLocked ? 'bg-gray-100 cursor-not-allowed' : ''
                                        }`}
                                    value={apiKey || ''}
                                    onChange={(e) => setApiKey(e.target.value)}
                                    disabled={isApiKeyLocked}
                                    onClick={handleInputClick}
                                    placeholder="Enter your API key"
                                />
                                {isApiKeyLocked && (
                                    <div
                                        onClick={handleInputClick}
                                        className="absolute inset-0 flex items-center justify-center bg-gray-100 bg-opacity-50 rounded-md cursor-not-allowed"
                                    />
                                )}
                                <div className="absolute inset-y-0 right-0 pr-1 flex items-center">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                                        className={`transition-colors duration-200 ${isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''
                                            }`}
                                        title={isApiKeyLocked ? "Unlock to edit" : "Lock to prevent editing"}
                                    >
                                        {isApiKeyLocked ? <Lock className="h-4 w-4" /> : <Unlock className="h-4 w-4" />}
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setShowApiKey(!showApiKey)}
                                    >
                                        {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                                    </Button>
                                </div>
                            </div>
                        </div>
                    </>
                    )}
                </div>
            </div>
        </div >
    )
}









/** PR-44c: speaker-diarization settings block. */
function DiarizationSettingsBlock() {
    const t = useTranslations();
    const { config, save, error } = useDiarizationConfig();
    if (error) {
        return (
            <div className="rounded border border-yellow-300 bg-yellow-50 p-3 text-sm text-yellow-800">
                {t('diarization.load_error', { default: 'Failed to load diarization settings.' })}
            </div>
        );
    }
    if (!config) {
        return <div className="text-sm text-gray-500">{t('diarization.loading', { default: 'Loading diarization settings...' })}</div>;
    }
    return (
        <div className="rounded border border-gray-200 p-3 space-y-3" data-testid="diarization-block">
            <div className="flex items-center justify-between">
                <Label className="text-sm font-medium text-gray-700">
                    {t('diarization.enable', { default: 'Enable speaker diarization' })}
                </Label>
                <input
                    type="checkbox"
                    checked={config.enabled}
                    onChange={(e) => save({ enabled: e.target.checked })}
                    aria-label={t('diarization.enable', { default: 'Enable speaker diarization' })}
                />
            </div>
            <div className="grid grid-cols-2 gap-3">
                <div>
                    <Label className="block text-xs text-gray-600">
                        {t('diarization.min_speakers', { default: 'Minimum speakers' })}
                    </Label>
                    <Input
                        type="number"
                        min={2}
                        max={config.max_speakers}
                        value={config.min_speakers}
                        onChange={(e) => save({ min_speakers: Number(e.target.value) })}
                    />
                </div>
                <div>
                    <Label className="block text-xs text-gray-600">
                        {t('diarization.max_speakers', { default: 'Maximum speakers' })}
                    </Label>
                    <Input
                        type="number"
                        min={config.min_speakers}
                        max={10}
                        value={config.max_speakers}
                        onChange={(e) => save({ max_speakers: Number(e.target.value) })}
                    />
                </div>
            </div>
            <div className="text-xs text-gray-500">
                {t('diarization.model_status', { default: 'Speaker model status' })}:{' '}
                {t(`diarization.model_status.${config.model_status}`, { default: config.model_status })}
            </div>
        </div>
    );
}
