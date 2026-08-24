// SherpaModelManager.tsx
//
// Front-end UI for the sherpa-onnx streaming engine. Mirrors the shape
// of ParakeetModelManager (init -> list -> download progress -> select)
// but stays simpler because the sherpa command surface is smaller: no
// separate download/init/load phases need the same throttling as
// parakeet because the sherpa catalog only has two entries today.
//
// ponytail: no Card / Badge components exist in meetily's UI kit, so
// the markup below uses plain <div> + Tailwind classes for the chrome.
// This avoids adding shadcn primitives for a single component.

import { useState, useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { Loader2, Download, CheckCircle2, AlertCircle } from 'lucide-react';
import { toast } from 'sonner';
import { SherpaAPI, type SherpaModelInfo } from '@/lib/sherpa';
import { getSherpaModelDisplayName } from '@/lib/sherpa';

interface SherpaModelManagerProps {
  selectedModel?: string;
  onModelSelect?: (modelName: string) => void;
  className?: string;
  autoSave?: boolean;
}

export function SherpaModelManager({
  selectedModel,
  onModelSelect,
  className = '',
  autoSave = false,
}: SherpaModelManagerProps) {
  const [models, setModels] = useState<SherpaModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [initialized, setInitialized] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<Map<string, number>>(new Map());
  // ponytail: the CJK punctuation model loads alongside whichever
  // ASR model the user picks — it has no selectable "active" slot.
  // We probe the engine on mount and after every load event so the
  // Punct row can render an "Active" badge (green, distinct from
  // the ASR "✓ Loaded") instead of the previous silent ambiguity.
  const [punctuatorLoaded, setPunctuatorLoaded] = useState(false);

  const onModelSelectRef = useRef(onModelSelect);
  const autoSaveRef = useRef(autoSave);
  useEffect(() => {
    onModelSelectRef.current = onModelSelect;
    autoSaveRef.current = autoSave;
  }, [onModelSelect, autoSave]);

  // Init engine + fetch catalog
  useEffect(() => {
    if (initialized) return;
    const initializeModels = async () => {
      try {
        setLoading(true);
        await SherpaAPI.init();
        const modelList = await SherpaAPI.getAvailableModels();
        setModels(modelList);
        // ponytail: probe the punctuator on init so the Punct row
        // can render an Active badge for users who already have it
        // loaded from a previous session.
        try {
          setPunctuatorLoaded(await SherpaAPI.isPunctuatorLoaded());
        } catch {
          // ignore — engine hasn't been initialized yet, will
          // retry below
        }
        setInitialized(true);
      } catch (err) {
        console.error('[SherpaModelManager] init failed:', err);
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        toast.error('Failed to load sherpa models', {
          description: message,
          duration: 5000,
        });
      } finally {
        setLoading(false);
      }
    };
    initializeModels();
  }, [initialized]);

  // Wire up download progress events
  useEffect(() => {
    let unlistenProgress: (() => void) | null = null;
    let unlistenComplete: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;
    let unlistenLoaded: (() => void) | null = null;
    const setupListeners = async () => {
      unlistenProgress = await listen<{ modelName: string; downloaded_bytes: number; total_bytes: number; progress: number }>(
        'sherpa-model-download-progress',
        (event) => {
          const { modelName, progress } = event.payload;
          setDownloadProgress((prev) => {
            const next = new Map(prev);
            next.set(modelName, progress);
            return next;
          });
        },
      );
      unlistenComplete = await listen<{ modelName: string }>(
        'sherpa-model-download-complete',
        async () => {
          // ponytail: refresh catalog so the button label flips from
          // "Download" to "Load" once extraction succeeds. We don't know
          // which model finished; the next paint will reflect status anyway.
          // If extract failed and the backend cleaned the target dir, the
          // status flips to Missing and the Download button reappears.
          const modelList = await SherpaAPI.getAvailableModels();
          setModels(modelList);
          setDownloadProgress((prev) => {
            const next = new Map(prev);
            for (const k of [...next.keys()]) next.delete(k);
            return next;
          });
        },
      );
      // ponytail: also surface backend-side extract errors so the
      // model card can flip back from "Downloading…" to "Missing"
      // with a toast instead of silently staying stuck at 100%.
      unlistenError = await listen<{ modelName: string; error: string }>(
        'sherpa-model-download-error',
        (event) => {
          toast.error('Sherpa download failed', {
            description: event.payload.error,
            duration: 8000,
          });
          setDownloadProgress((prev) => {
            const next = new Map(prev);
            for (const k of [...next.keys()]) next.delete(k);
            return next;
          });
          SherpaAPI.getAvailableModels().then(setModels).catch(() => {});
        },
      );
    // ponytail: refresh the punctuator badge whenever an ASR
      // model finishes loading — that's when `engine.rs::load_model`
      // attaches the punctuator to the engine (see the
      // OfflinePunctuation::create block at the tail of load_model).
      // The Punct-only click goes through `sherpa_load_model` too,
      // so this catches both code paths.
      unlistenLoaded = await listen<{ modelName: string }>(
        'sherpa-model-loading-completed',
        async () => {
          try {
            setPunctuatorLoaded(await SherpaAPI.isPunctuatorLoaded());
          } catch {
            // engine not ready — keep whatever value we had
          }
        },
      );
    };
    setupListeners();
    return () => {
      unlistenProgress?.();
      unlistenComplete?.();
      unlistenError?.();
      unlistenLoaded?.();
    };
  }, []);

  const handleDownload = async (modelName: string) => {
    try {
      await SherpaAPI.downloadModel(modelName);
      const modelList = await SherpaAPI.getAvailableModels();
      setModels(modelList);
    } catch (err) {
      console.error('[SherpaModelManager] download failed:', err);
      // ponytail: Tauri invoke rejects with a plain string (the error
      // message from `Result::Err(String)`), not an Error instance.
      // Use `String(err)` so we surface the actual Rust-side reason
      // instead of falling back to "Unknown error".
      const message = err instanceof Error ? err.message : String(err);
      toast.error('Sherpa model download failed', {
        description: message,
        duration: 8000,
      });
    }
  };

  const handleLoad = async (modelName: string) => {
    try {
      // ponytail: the CJK punctuation model uses sherpa-onnx's
      // OfflinePunctuation API, not OnlineRecognizer. Loading it
      // via `loadModel` would route through the streaming ASR path
      // and fail with "Invalid provider: sherpa" because there are
      // no encoder/decoder files. The punctuator is auto-attached
      // to the engine on the next sherpa_init/load, so the UI
      // just needs to nudge the user that this model is already
      // active once the files are on disk.
      if (modelName === 'sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8') {
        toast.success(
          'Punctuation model is ready — it loads automatically alongside the ASR engine on next recording start.',
          { duration: 6000 }
        );
        return;
      }
      await SherpaAPI.loadModel(modelName);
      // ponytail: persist the sherpa selection to the SQLite-backed
      // transcript config. Without this, `api_get_transcript_config`
      // keeps returning the previously-saved provider/model (e.g.
      // localWhisper/Small), and the recording-start path will load
      // Whisper instead of the sherpa engine we just attached.
      // The WhisperModelManager does the same thing in its
      // saveModelSelection helper; sherpa was missing it.
      try {
        await invoke('api_save_transcript_config', {
          provider: 'sherpa',
          model: modelName,
          apiKey: null,
        });
      } catch (saveErr) {
        console.error('[SherpaModelManager] failed to persist selection:', saveErr);
      }
      if (onModelSelectRef.current) {
        onModelSelectRef.current(modelName);
      }
      toast.success(`Sherpa model "${modelName}" loaded`);
    } catch (err) {
      console.error('[SherpaModelManager] load failed:', err);
      const message = err instanceof Error ? err.message : String(err);
      toast.error('Sherpa model load failed', {
        description: message,
        duration: 8000,
      });
    }
  };

  if (loading) {
    return (
      <div className={`rounded-lg border border-gray-200 bg-white p-4 ${className}`}>
        <div className="flex items-center gap-2 text-sm font-medium text-gray-900">
          <Loader2 className="h-4 w-4 animate-spin" /> Loading sherpa models
        </div>
        <p className="mt-1 text-sm text-gray-600">Reading catalog…</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className={`rounded-lg border border-red-200 bg-red-50 p-4 ${className}`}>
        <div className="flex items-center gap-2 text-sm font-medium text-red-700">
          <AlertCircle className="h-4 w-4" /> Failed to load sherpa models
        </div>
        <p className="mt-1 text-xs text-red-600">{error}</p>
      </div>
    );
  }

  return (
    <div className={`rounded-lg border border-gray-200 bg-white p-4 ${className}`}>
      <div className="mb-3">
        <div className="flex items-center gap-2 text-sm font-medium text-gray-900">
          🛰️ Sherpa-onnx streaming models
        </div>
        <p className="mt-1 text-xs text-gray-600">
          Streaming transducer (Zipformer) or non-transducer (Paraformer) — designed for real-time ASR on CPU.
        </p>
      </div>
      <div className="space-y-3">
        {models.map((model) => {
          const status = model.status;
          const isAvailable = status === 'Available';
          // ponytail: status-driven "Downloading" lags behind the
          // event-driven progress — Rust emits "sherpa-model-download-
          // progress" events *during* the HTTP stream but does not
          // transition the cached SherpaModelStatus from Missing to
          // Downloading{progress} (status only flips at discover_models
          // time, which happens after the download completes). Reading
          // progress from the event Map and treating any non-null entry
          // as "actively downloading" makes the progress bar appear
          // immediately on the first chunk instead of after page nav.
          const liveProgress = downloadProgress.get(model.name);
          const isDownloading = (typeof status === 'object'
            && 'Downloading' in status
            && !isAvailable)
            || (liveProgress !== undefined && !isAvailable);
          const isError = typeof status === 'object' && 'Error' in (status as object);
          const isMissing = status === 'Missing' && !isDownloading;
          const isExtracting = isDownloading
            && (liveProgress ?? 0) >= 100;
          const statusProgress = isDownloading
            ? Number((status as { Downloading: number }).Downloading)
            : 0;
          const progress = liveProgress ?? statusProgress;
          const displayName = getSherpaModelDisplayName(model.name);
          // ponytail: Punct is an auxiliary model that loads
          // alongside whatever ASR model the user picks — it has
          // no selectable "active" slot. Treat it differently in
          // the UI so users see a clear "Active" vs "Downloaded"
          // indicator instead of the silent ambiguity the old
          // render had (Punct clicked → toast + nothing visible).
          const isPunctModel = model.name === 'sherpa-onnx-punct-ct-transformer-zh-en-vocab272727-2024-04-12-int8';

          return (
            <div
              key={model.name}
              className={`rounded-md border p-3 ${isAvailable ? 'border-green-200 bg-green-50' : 'border-gray-200'}`}
            >
              <div className="flex items-start justify-between gap-2">
                <div className="flex-1">
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-sm">{displayName}</span>
                    {isAvailable && (
                      <span className="inline-flex items-center rounded-md bg-green-100 px-2 py-0.5 text-xs text-green-800">
                        <CheckCircle2 className="mr-1 h-3 w-3" /> Available
                      </span>
                    )}
                    {isExtracting && (
                      <span className="inline-flex items-center rounded-md bg-blue-100 px-2 py-0.5 text-xs text-blue-800">
                        <Loader2 className="mr-1 h-3 w-3 animate-spin" /> Extracting…
                      </span>
                    )}
                    {isDownloading && !isExtracting && (
                      <span className="inline-flex items-center rounded-md bg-blue-100 px-2 py-0.5 text-xs text-blue-800">
                        <Loader2 className="mr-1 h-3 w-3 animate-spin" /> Downloading
                      </span>
                    )}
                    {isMissing && (
                      <span className="inline-flex items-center rounded-md bg-gray-100 px-2 py-0.5 text-xs text-gray-700">
                        Missing
                      </span>
                    )}
                    {isError && (
                      <span className="inline-flex items-center rounded-md bg-red-100 px-2 py-0.5 text-xs text-red-800">
                        Error
                      </span>
                    )}
                    {/* ponytail: distinct Punct indicators. Available +
                        punctuator loaded = "Active" (green). Available +
                        not loaded = "Downloaded · auto-attaches" so users
                        know it's a different kind of model than the
                        selectable ASR ones. */}
                    {isPunctModel && isAvailable && punctuatorLoaded && (
                      <span className="inline-flex items-center rounded-md bg-green-100 px-2 py-0.5 text-xs text-green-800">
                        <CheckCircle2 className="mr-1 h-3 w-3" /> Active
                      </span>
                    )}
                    {isPunctModel && isAvailable && !punctuatorLoaded && (
                      <span className="inline-flex items-center rounded-md bg-blue-100 px-2 py-0.5 text-xs text-blue-800">
                        Downloaded · activates with ASR model
                      </span>
                    )}
                  </div>
                  <p className="mt-1 text-xs text-gray-500">{model.description}</p>
                  <p className="text-xs text-gray-400">{model.size_mb} MB</p>
                  {isDownloading && !isExtracting && (
                    <div className="mt-2">
                      <Progress value={progress} />
                      <p className="mt-1 text-xs text-gray-500">{progress}%</p>
                    </div>
                  )}
                  {isExtracting && (
                    <p className="mt-2 text-xs text-blue-700">
                      Download complete — extracting model files…
                    </p>
                  )}
                </div>
                <div className="flex flex-col gap-2">
                  {(!isAvailable && !isDownloading) && (
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => handleDownload(model.name)}
                    >
                      <Download className="mr-1 h-3 w-3" /> Download
                    </Button>
                  )}
                  {isAvailable && !isPunctModel && model.name !== selectedModel && (
                    <Button
                      size="sm"
                      onClick={() => handleLoad(model.name)}
                    >
                      Load
                    </Button>
                  )}
                  {/* ponytail: Punct has no Load button — clicking the
                      Punct row's card area still triggers handleLoad,
                      which surfaces the same "loads automatically" toast.
                      The button slot stays empty so the visual rhythm
                      matches the other rows. */}
                  {isAvailable && !isPunctModel && model.name === selectedModel && (
                    <span className="inline-flex items-center rounded-md bg-blue-100 px-2 py-0.5 text-xs text-blue-800">
                      ✓ Loaded
                    </span>
                  )}
                  {/* ponytail: Punct gets a small "Refresh" button so the
                      row isn't button-less. Punct's "Active" state only
                      flips when the punctuator is actually loaded into
                      the sherpa engine. Clicking this calls
                      `sherpa_load_punctuator` which initializes the
                      engine if needed and attaches the OfflinePunctuation
                      ONNX model so the UI can flip from "Downloaded ·
                      activates with ASR model" to "Active" without
                      requiring the user to switch provider to sherpa. */}
                  {isAvailable && isPunctModel && (
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={async () => {
                        try {
                          const loaded = await SherpaAPI.loadPunctuator();
                          setPunctuatorLoaded(loaded);
                          toast.success(loaded
                            ? 'Punctuation model is now Active'
                            : 'Punctuation model files not found at expected path — refresh failed');
                        } catch (err) {
                          const message = err instanceof Error ? err.message : String(err);
                          toast.error('Failed to refresh punctuation status', { description: message });
                        }
                      }}
                    >
                      Refresh
                    </Button>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}