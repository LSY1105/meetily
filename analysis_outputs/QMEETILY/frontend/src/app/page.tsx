"use client";

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Toaster, toast } from "sonner";

import { Sidebar } from "@/components/Sidebar/Sidebar";
import { LiveTranscript } from "@/components/LiveTranscript";
import { MeetingLibrary } from "@/components/MeetingLibrary";
import { SettingsPanel } from "@/components/SettingsPanel";

interface AppInfo {
  name: string;
  version: string;
  is_recording: boolean;
}

type View = "home" | "library" | "settings";

export default function HomePage() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [view, setView] = useState<View>("home");
  const [currentMeetingId, setCurrentMeetingId] = useState<number | null>(null);
  const [currentMeetingTitle, setCurrentMeetingTitle] = useState<string | null>(null);
  const [isRecording, setIsRecording] = useState(false);

  // Poll app info every 2s
  useEffect(() => {
    let cancelled = false;
    const refresh = async () => {
      try {
        const info = await invoke<AppInfo>("get_app_info");
        if (!cancelled) setAppInfo(info);
      } catch (e) {
        console.error("get_app_info failed:", e);
      }
    };
    refresh();
    const id = setInterval(refresh, 2000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  const startRecording = async (title: string) => {
    if (!title.trim()) return;
    try {
      const id = await invoke<number>("start_recording", { title });
      setCurrentMeetingId(id);
      setCurrentMeetingTitle(title);
      setIsRecording(true);
      toast.success("Recording started");
    } catch (e) {
      toast.error("Failed to start recording", { description: String(e) });
    }
  };

  const stopRecording = async () => {
    if (currentMeetingId === null) return;
    try {
      await invoke("stop_recording", { meetingId: currentMeetingId });
      setIsRecording(false);
      toast.info("Recording stopped — generating summary...");
      try {
        await invoke("generate_summary", { meetingId: currentMeetingId });
        toast.success("Summary ready");
      } catch (e) {
        toast.error("Summary generation failed", { description: String(e) });
      }
      setCurrentMeetingId(null);
      setCurrentMeetingTitle(null);
    } catch (e) {
      toast.error("Failed to stop recording", { description: String(e) });
    }
  };

  return (
    <main className="min-h-screen bg-background flex">
      <Toaster position="top-right" richColors />

      <Sidebar
        activeView={view}
        onSelectView={setView}
        isRecording={isRecording}
        currentMeetingTitle={currentMeetingTitle}
        onStartRecording={startRecording}
        onStopRecording={stopRecording}
        appInfo={appInfo}
      />

      <div className="flex-1 overflow-y-auto">
        <header className="border-b bg-card px-6 py-3">
          <h1 className="text-lg font-semibold capitalize">{view}</h1>
          {appInfo && (
            <p className="text-xs text-muted-foreground">
              v{appInfo.version} · Qwen3-native
            </p>
          )}
        </header>

        <div className="px-6 py-6">
          {view === "home" && (
            <LiveTranscript
              currentMeetingId={currentMeetingId}
              onMeetingStart={(id, title) => {
                setCurrentMeetingId(id);
                setCurrentMeetingTitle(title);
                setIsRecording(true);
              }}
              onMeetingEnd={() => {
                setCurrentMeetingId(null);
                setCurrentMeetingTitle(null);
                setIsRecording(false);
              }}
            />
          )}
          {view === "library" && (
            <MeetingLibrary
              onOpen={(id) => {
                setCurrentMeetingId(id);
                setView("home");
              }}
            />
          )}
          {view === "settings" && <SettingsPanel info={appInfo} />}
        </div>
      </div>
    </main>
  );
}
