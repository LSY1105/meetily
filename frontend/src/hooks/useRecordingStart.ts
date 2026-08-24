import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import { toast } from 'sonner';

interface UseRecordingStartReturn {
  handleRecordingStart: () => Promise<void>;
  isAutoStarting: boolean;
}

/** Where a start request originated — used for analytics source tags. */
type StartSource = 'home_page' | 'sidebar_auto' | 'sidebar_direct';

/**
 * Custom hook for managing recording start lifecycle.
 * Handles all three entry points:
 * - manual start (home page button click)
 * - auto-start (sessionStorage flag set by sidebar navigation)
 * - direct start (`start-recording-from-sidebar` window event)
 *
 * All three share one implementation via `attemptStart`; the entry points
 * differ only in their guards and error presentation.
 */
export function useRecordingStart(
  showModal?: (name: 'modelSelector', message?: string) => void
): UseRecordingStartReturn {
  const [isAutoStarting, setIsAutoStarting] = useState(false);

  const { clearTranscripts, setMeetingTitle } = useTranscripts();
  const { setIsMeetingActive } = useSidebar();
  const { selectedDevices } = useConfig();
  const { isRecording, setStatus, setIsRecording } = useRecordingState();

  // Generate meeting title with timestamp
  const generateMeetingTitle = useCallback(() => {
    const now = new Date();
    const day = String(now.getDate()).padStart(2, '0');
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const year = String(now.getFullYear()).slice(-2);
    const hours = String(now.getHours()).padStart(2, '0');
    const minutes = String(now.getMinutes()).padStart(2, '0');
    const seconds = String(now.getSeconds()).padStart(2, '0');
    return `Meeting ${day}_${month}_${year}_${hours}_${minutes}_${seconds}`;
  }, []);

  // Check if Parakeet transcription model is ready
  const checkParakeetReady = useCallback(async (): Promise<boolean> => {
    try {
      await invoke('parakeet_init');
      const hasModels = await invoke<boolean>('parakeet_has_available_models');
      return hasModels;
    } catch (error) {
      console.error('Failed to check Parakeet status:', error);
      return false;
    }
  }, []);

  // Check if any model is currently downloading
  const checkIfModelDownloading = useCallback(async (): Promise<boolean> => {
    try {
      const models = await invoke<any[]>('parakeet_get_available_models');
      const isDownloading = models.some(m =>
        m.status && (
          typeof m.status === 'object'
            ? 'Downloading' in m.status
            : m.status === 'Downloading'
        )
      );
      return isDownloading;
    } catch (error) {
      console.error('Failed to check model download status:', error);
      return false; // Default to not downloading (will show error + modal)
    }
  }, []);

  // Blocked gate: model missing or still downloading. Shows guidance and
  // resets status to IDLE.
  const handleModelNotReady = useCallback(async (source: StartSource) => {
    const isDownloading = await checkIfModelDownloading();
    if (isDownloading) {
      toast.info('Model download in progress', {
        description: 'Please wait for the transcription model to finish downloading before recording.',
        duration: 5000,
      });
      Analytics.trackButtonClick('start_recording_blocked_downloading', source);
    } else {
      toast.error('Transcription model not ready', {
        description: 'Please download a transcription model before recording.',
        duration: 5000,
      });
      showModal?.('modelSelector', 'Transcription model setup required');
      Analytics.trackButtonClick('start_recording_blocked_missing', source);
    }
    setStatus(RecordingStatus.IDLE);
  }, [checkIfModelDownloading, showModal, setStatus]);

  /**
   * Shared start sequence behind all three entry points.
   * Throws on failure so callers can choose their own error presentation
   * (manual path re-throws to RecordingControls for device-specific errors;
   * sidebar paths toast instead).
   */
  const attemptStart = useCallback(async (source: StartSource): Promise<void> => {
    // Check if Parakeet transcription model is ready before starting
    const parakeetReady = await checkParakeetReady();
    if (!parakeetReady) {
      await handleModelNotReady(source);
      return;
    }

    try {
      const randomTitle = generateMeetingTitle();
      setMeetingTitle(randomTitle);

      // Set STARTING status before initiating backend recording
      setStatus(RecordingStatus.STARTING, 'Initializing recording...');

      // Start the actual backend recording
      await recordingService.startRecordingWithDevices(
        selectedDevices?.micDevice || null,
        selectedDevices?.systemDevice || null,
        randomTitle
      );

      // Update state after successful backend start
      // Note: RECORDING status will be set by RecordingStateContext event listener
      setIsRecording(true); // This will also update the sidebar via the useEffect
      clearTranscripts(); // Clear previous transcripts when starting new recording
      setIsMeetingActive(true);
      Analytics.trackButtonClick('start_recording', source);

      // Show recording notification if enabled
      await showRecordingNotification();
    } catch (error) {
      console.error(`Failed to start recording (${source}):`, error);
      setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Failed to start recording');
      setIsRecording(false); // Reset state on error
      Analytics.trackButtonClick('start_recording_error', source);
      throw error;
    }
  }, [
    checkParakeetReady,
    handleModelNotReady,
    generateMeetingTitle,
    setMeetingTitle,
    setIsRecording,
    clearTranscripts,
    setIsMeetingActive,
    selectedDevices,
    setStatus,
  ]);

  // Handle manual recording start (from button click).
  // Re-throws so RecordingControls can handle device-specific errors itself.
  const handleRecordingStart = useCallback(async () => {
    await attemptStart('home_page');
  }, [attemptStart]);

  // Check for autoStartRecording flag and start recording automatically
  useEffect(() => {
    if (typeof window === 'undefined') return;

    const shouldAutoStart = sessionStorage.getItem('autoStartRecording');
    if (shouldAutoStart !== 'true' || isRecording || isAutoStarting) return;

    sessionStorage.removeItem('autoStartRecording'); // Clear the flag

    void (async () => {
      setIsAutoStarting(true);
      try {
        await attemptStart('sidebar_auto');
      } catch (error) {
        console.error('Failed to auto-start recording:', error);
        toast.error('Failed to start recording. Check console for details.');
      } finally {
        setIsAutoStarting(false);
      }
    })();
  }, [isRecording, isAutoStarting, attemptStart]);

  // Listen for direct recording trigger from sidebar when already on home page
  useEffect(() => {
    const handleDirectStart = () => {
      if (isRecording || isAutoStarting) {
        console.log('Recording already in progress, ignoring direct start event');
        return;
      }

      void (async () => {
        setIsAutoStarting(true);
        try {
          await attemptStart('sidebar_direct');
        } catch (error) {
          console.error('Failed to start recording from sidebar:', error);
          toast.error('Failed to start recording. Check console for details.');
        } finally {
          setIsAutoStarting(false);
        }
      })();
    };

    window.addEventListener('start-recording-from-sidebar', handleDirectStart);

    return () => {
      window.removeEventListener('start-recording-from-sidebar', handleDirectStart);
    };
  }, [isRecording, isAutoStarting, attemptStart]);

  return {
    handleRecordingStart,
    isAutoStarting,
  };
}
