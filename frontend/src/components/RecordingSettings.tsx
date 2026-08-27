import React, { useState, useEffect, useCallback } from 'react';
import { Switch } from '@/components/ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { FolderOpen } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { DeviceSelection, SelectedDevices } from '@/components/DeviceSelection';
import Analytics from '@/lib/analytics';
import { toast } from 'sonner';
import { useTranslations } from 'next-intl';

interface WhisperModelInfo {
  name: string;
  size_mb: number;
  accuracy: string;
  speed: string;
  description: string;
}

export interface RecordingPreferences {
  save_folder: string;
  auto_save: boolean;
  auto_refine_whisper: boolean;
  refinement_whisper_model?: string | null;
  file_format: string;
  preferred_mic_device: string | null;
  preferred_system_device: string | null;
}

interface RecordingSettingsProps {
  onSave?: (preferences: RecordingPreferences) => void;
}

export function RecordingSettings({ onSave }: RecordingSettingsProps) {
  const [preferences, setPreferences] = useState<RecordingPreferences>({
    save_folder: '',
    auto_save: true,
    auto_refine_whisper: true,
    refinement_whisper_model: null,
    file_format: 'mp4',
    preferred_mic_device: null,
    preferred_system_device: null
  });
  const [availableWhisperModels, setAvailableWhisperModels] = useState<WhisperModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [showRecordingNotification, setShowRecordingNotification] = useState(true);
  const t = useTranslations('settings');

  // Load recording preferences on component mount
  useEffect(() => {
    const loadPreferences = async () => {
      try {
        const prefs = await invoke<RecordingPreferences>('get_recording_preferences');
        setPreferences(prefs);
      } catch (error) {
        console.error('Failed to load recording preferences:', error);
        // If loading fails, get default folder path
        try {
          const defaultPath = await invoke<string>('get_default_recordings_folder_path');
          setPreferences(prev => ({ ...prev, save_folder: defaultPath }));
        } catch (defaultError) {
          console.error('Failed to get default folder path:', defaultError);
        }
      } finally {
        setLoading(false);
      }
    };

    loadPreferences();
  }, []);

  // Load recording notification preference
  useEffect(() => {
    const loadNotificationPref = async () => {
      try {
        const { Store } = await import('@tauri-apps/plugin-store');
        const store = await Store.load('preferences.json');
        const show = await store.get<boolean>('show_recording_notification') ?? true;
        setShowRecordingNotification(show);
      } catch (error) {
        console.error('Failed to load notification preference:', error);
      }
    };
    loadNotificationPref();
  }, []);

  const handleAutoSaveToggle = async (enabled: boolean) => {
    const newPreferences = { ...preferences, auto_save: enabled };
    setPreferences(newPreferences);
    await savePreferences(newPreferences);

    // Track auto-save setting change
    await Analytics.track('auto_save_recording_toggled', {
      enabled: enabled.toString()
    });
  };

  const handleDeviceChange = async (devices: SelectedDevices) => {
    const newPreferences = {
      ...preferences,
      preferred_mic_device: devices.micDevice,
      preferred_system_device: devices.systemDevice
    };
    setPreferences(newPreferences);
    await savePreferences(newPreferences);

    // Track default device preference changes
    // Note: Individual device selection analytics are tracked in DeviceSelection component
    await Analytics.track('default_devices_changed', {
      has_preferred_microphone: (!!devices.micDevice).toString(),
      has_preferred_system_audio: (!!devices.systemDevice).toString()
    });
  };

  const handleOpenFolder = async () => {
    try {
      await invoke('open_recordings_folder');
    } catch (error) {
      console.error('Failed to open recordings folder:', error);
    }
  };

  const handleNotificationToggle = async (enabled: boolean) => {
    try {
      setShowRecordingNotification(enabled);
      const { Store } = await import('@tauri-apps/plugin-store');
      const store = await Store.load('preferences.json');
      await store.set('show_recording_notification', enabled);
      await store.save();
      toast.success(t('recording.preference_saved'));
      await Analytics.track('recording_notification_preference_changed', {
        enabled: enabled.toString()
      });
    } catch (error) {
      console.error('Failed to save notification preference:', error);
      toast.error(t('recording.preference_save_failed'));
    }
  };

  const savePreferences = async (prefs: RecordingPreferences) => {
    setSaving(true);
    try {
      await invoke('set_recording_preferences', { preferences: prefs });
      onSave?.(prefs);

      // Show success toast with device details
      const micDevice = prefs.preferred_mic_device || t('recording.default_audio_device_label');
      const systemDevice = prefs.preferred_system_device || t('recording.default_audio_device_label');
      toast.success(t("recording.devices_saved"), {
        description: `Mic: ${micDevice}, Audio: ${systemDevice}`
      });
    } catch (error) {
      console.error('Failed to save recording preferences:', error);
      toast.error(t("recording.devices_save_failed"), {
        description: error instanceof Error ? error.message : String(error)
      });
    } finally {
      setSaving(false);
    }
  };

  const loadWhisperModels = useCallback(async () => {
    try {
      const list = (await invoke<any[]>('whisper_get_available_models')) ?? [];
      const downloaded = list
        .filter((m: any) => m.status === 'Available' || m.status === 'Downloading')
        .map((m: any) => ({
          name: String(m.name),
          size_mb: Number(m.size_mb ?? 0),
          accuracy: String(m.accuracy ?? ''),
          speed: String(m.speed ?? ''),
          description: String(m.description ?? ''),
        }));
      setAvailableWhisperModels(downloaded);
    } catch (err) {
      console.error('Failed to load whisper models:', err);
      setAvailableWhisperModels([]);
    }
  }, []);

  const handleRefineModelChange = useCallback(async (modelName: string) => {
    const newPreferences = { ...preferences, refinement_whisper_model: modelName };
    setPreferences(newPreferences);
    await savePreferences(newPreferences);
    await Analytics.track('refinement_whisper_model_changed', { model: modelName });
  }, [preferences, savePreferences]);

  const handleRefineToggle = async (enabled: boolean) => {
    const newPreferences = { ...preferences, auto_refine_whisper: enabled };
    setPreferences(newPreferences);
    await savePreferences(newPreferences);
    await Analytics.track('auto_refine_whisper_toggled', {
      enabled: enabled.toString()
    });
  };

  useEffect(() => {
    loadWhisperModels();
  }, [loadWhisperModels]);

  useEffect(() => {
    loadWhisperModels();
  }, [loadWhisperModels]);

  if (loading) {
    return (
      <div className="animate-pulse">
        <div className="h-4 bg-gray-200 rounded w-1/4 mb-4"></div>
        <div className="h-8 bg-gray-200 rounded mb-4"></div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-semibold mb-4">{t("recording.title")}</h3>
        <p className="text-sm text-gray-600 mb-6">
          {t("recording.description")}
        </p>
      </div>

      {/* Auto Save Toggle */}
      <div className="flex items-center justify-between p-4 border rounded-lg">
        <div className="flex-1">
          <div className="font-medium">{t("recording.save_recordings_label")}</div>
          <div className="text-sm text-gray-600">
            {t("recording.save_recordings_desc")}
          </div>
        </div>
        <Switch
          checked={preferences.auto_save}
          onCheckedChange={handleAutoSaveToggle}
          disabled={saving}
        />
      </div>

      {/* Dual-engine: Whisper refinement toggle */}
      <div className="flex items-center justify-between p-4 border rounded-lg">
        <div className="flex-1">
          <div className="font-medium">{t("recording.refine_toggle_label")}</div>
          <div className="text-sm text-gray-600">
            {t("recording.refine_toggle_desc")}
          </div>
        </div>
        <Switch
          checked={preferences.auto_refine_whisper}
          onCheckedChange={handleRefineToggle}
          disabled={saving}
        />
      </div>

      {/* Dual-engine: Whisper model selector (visible when refinement is enabled) */}
      {preferences.auto_refine_whisper && (
        <div className="p-4 border rounded-lg bg-gray-50 space-y-3">
          <div>
            <div className="font-medium">{t("recording.refine_model_label")}</div>
            <div className="text-sm text-gray-600">
              {t("recording.refine_model_desc")}
            </div>
          </div>
          {availableWhisperModels.length === 0 ? (
            <p className="text-sm text-amber-700">
              {t("recording.refine_no_models")}
            </p>
          ) : (
            <Select
              value={preferences.refinement_whisper_model ?? 'default'}
              onValueChange={(v) => handleRefineModelChange(v === 'default' ? '' : v)}
              disabled={saving}
            >
              <SelectTrigger className="bg-white">
                <SelectValue placeholder={t("recording.refine_model_placeholder")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="default">{t("recording.refine_model_default")}</SelectItem>
                {availableWhisperModels.map((m) => (
                  <SelectItem key={m.name} value={m.name}>
                    {m.name} ({m.size_mb} MB · {m.speed})
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
          <p className="text-xs text-gray-500">{t("recording.refine_model_info")}</p>
        </div>
      )}

      {/* Folder Location - Only shown when auto_save is enabled */}
      {preferences.auto_save && (
        <div className="space-y-4">
          <div className="p-4 border rounded-lg bg-gray-50">
            <div className="font-medium mb-2">{t("recording.save_location_label")}</div>
            <div className="text-sm text-gray-600 mb-3 break-all">
              {preferences.save_folder || t('recording.save_location_default')}
            </div>
            <button
              onClick={handleOpenFolder}
              className="flex items-center gap-2 px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              {t("recording.open_folder")}
            </button>
          </div>

          <div className="p-4 border rounded-lg bg-blue-50">
            <div className="text-sm text-blue-800">
              <strong>{t("recording.file_format_label")}</strong> {preferences.file_format.toUpperCase()} files
            </div>
            <div className="text-xs text-blue-600 mt-1">
              {t("recording.recording_path_template", { format: preferences.file_format })}
            </div>
          </div>
        </div>
      )}

      {/* Info when auto_save is disabled */}
      {!preferences.auto_save && (
        <div className="p-4 border rounded-lg bg-yellow-50">
          <div className="text-sm text-yellow-800">
            {t("recording.recording_disabled_note")}
          </div>
        </div>
      )}

      {/* Recording Notification Toggle */}
      <div className="flex items-center justify-between p-4 border rounded-lg">
        <div className="flex-1">
          <div className="font-medium">Recording Start Notification</div>
          <div className="text-sm text-gray-600">
            Show reminder to inform participants when recording starts
          </div>
        </div>
        <Switch
          checked={showRecordingNotification}
          onCheckedChange={handleNotificationToggle}
        />
      </div>

      {/* Device Preferences */}
      <div className="space-y-4">
        <div className="border-t pt-6">
          <h4 className="text-base font-medium text-gray-900 mb-4">{t('recording.default_audio_devices')}</h4>
          <p className="text-sm text-gray-600 mb-4">
            Set your preferred microphone and system audio devices for recording. These will be automatically selected when starting new recordings.
          </p>

          <div className="border rounded-lg p-4 bg-gray-50">
            <DeviceSelection
              selectedDevices={{
                micDevice: preferences.preferred_mic_device,
                systemDevice: preferences.preferred_system_device
              }}
              onDeviceChange={handleDeviceChange}
              disabled={saving}
            />
          </div>
        </div>
      </div>
    </div>
  );
}