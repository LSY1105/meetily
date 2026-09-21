"use client";

import { Mic, Square, Home, Settings, FileText, NotebookPen } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { SidecarStatus } from "@/components/SidecarStatus";

interface MeetingSummary {
  id: number;
  title: string;
  started_at: string;
}

interface SidebarProps {
  activeView: "home" | "library" | "settings";
  onSelectView: (view: "home" | "library" | "settings") => void;
  isRecording: boolean;
  currentMeetingTitle: string | null;
  onStartRecording: () => void;
  onStopRecording: () => void;
  appInfo: AppInfo | null;
}

interface AppInfo {
  name: string;
  version: string;
  is_recording: boolean;
  sidecar_running?: boolean;
  asr_ready?: boolean;
  llm_ready?: boolean;
}

export function Sidebar({
  activeView,
  onSelectView,
  isRecording,
  currentMeetingTitle,
  onStartRecording,
  onStopRecording,
  appInfo,
}: SidebarProps) {
  const [recent, setRecent] = useState<MeetingSummary[]>([]);

  // Load recent meetings on mount
  useEffect(() => {
    invoke<MeetingSummary[]>("list_meetings", { limit: 8, offset: 0 })
      .then(setRecent)
      .catch(() => setRecent([]));
  }, []);

  return (
    <aside className="w-56 flex-shrink-0 border-r bg-card flex flex-col h-full">
      {/* Logo + status */}
      <div className="p-4 border-b">
        <div className="flex items-center gap-2">
          <span className="text-2xl">📋</span>
          <span className="font-bold text-lg">QMeetily</span>
        </div>
        <div className="mt-2">
          <SidecarStatus info={appInfo} />
        </div>
      </div>

      {/* Primary actions */}
      <div className="p-3 border-b">
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={isRecording ? onStopRecording : onStartRecording}
              disabled={false}
              className={`w-full flex items-center justify-center gap-2 px-3 py-2.5 rounded-full text-white text-sm font-medium transition-colors ${
                isRecording
                  ? "bg-red-500 hover:bg-red-600 animate-pulse"
                  : "bg-red-500 hover:bg-red-600"
              }`}
            >
              {isRecording ? <Square className="w-4 h-4 fill-current" /> : <Mic className="w-4 h-4" />}
              {isRecording ? "Stop" : "Record"}
            </button>
          </TooltipTrigger>
          <TooltipContent>
            <p>{isRecording ? "Stop current recording" : "Start a new recording"}</p>
          </TooltipContent>
        </Tooltip>
        {isRecording && currentMeetingTitle && (
          <div className="mt-2 px-2 py-1.5 bg-red-50 border border-red-200 rounded text-xs">
            <div className="flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-red-500 animate-pulse" />
              <span className="truncate font-medium text-red-700">{currentMeetingTitle}</span>
            </div>
          </div>
        )}
      </div>

      {/* Nav */}
      <nav className="p-2 border-b space-y-1">
        <NavButton
          icon={<Home className="w-5 h-5" />}
          label="Home"
          active={activeView === "home"}
          onClick={() => onSelectView("home")}
        />
        <NavButton
          icon={<NotebookPen className="w-5 h-5" />}
          label="Library"
          active={activeView === "library"}
          onClick={() => onSelectView("library")}
        />
        <NavButton
          icon={<Settings className="w-5 h-5" />}
          label="Settings"
          active={activeView === "settings"}
          onClick={() => onSelectView("settings")}
        />
      </nav>

      {/* Recent meetings */}
      <div className="flex-1 overflow-y-auto p-2">
        <div className="flex items-center gap-2 px-2 py-1 text-xs font-medium text-muted-foreground uppercase tracking-wide">
          <FileText className="w-3.5 h-3.5" />
          Recent
        </div>
        {recent.length === 0 ? (
          <div className="px-2 py-3 text-xs text-muted-foreground italic">
            No meetings yet
          </div>
        ) : (
          <div className="space-y-0.5">
            {recent.map((m) => (
              <button
                key={m.id}
                onClick={() => onSelectView("library")}
                className="w-full text-left px-2 py-1.5 rounded text-xs hover:bg-muted transition-colors truncate"
                title={m.title}
              >
                <div className="font-medium truncate">{m.title}</div>
                <div className="text-muted-foreground">
                  {new Date(m.started_at).toLocaleDateString()}
                </div>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="p-3 border-t text-xs text-muted-foreground">
        <div>QMeetily v0.1.0</div>
        <div className="text-[10px] mt-0.5">Qwen3-native · 100% local</div>
      </div>
    </aside>
  );
}

function NavButton({
  icon,
  label,
  active,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors ${
        active ? "bg-primary/10 text-primary font-medium" : "hover:bg-muted"
      }`}
    >
      {icon}
      {label}
    </button>
  );
}
