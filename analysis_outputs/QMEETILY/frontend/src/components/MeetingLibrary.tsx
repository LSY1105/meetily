"use client";

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { Input } from "@/components/ui/input";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@radix-ui/react-dropdown-menu";
import { Download } from "lucide-react";
import { toast } from "sonner";

interface Meeting {
  id: number;
  title: string;
  started_at: string;
  ended_at: string | null;
  language_primary: string | null;
  participants: string[];
}

interface SearchHit {
  transcript_id: number;
  meeting_id: number;
  sequence_id: number;
  text: string;
  rewritten_text: string | null;
}

interface MeetingLibraryProps {
  onOpen: (id: number) => void;
}

type ExportFormat = "txt" | "srt" | "json" | "md";

const EXPORT_FORMATS: { id: ExportFormat; label: string }[] = [
  { id: "txt", label: "Plain text (.txt)" },
  { id: "srt", label: "SubRip subtitles (.srt)" },
  { id: "json", label: "JSON (.json)" },
  { id: "md", label: "Markdown (.md)" },
];

export function MeetingLibrary({ onOpen }: MeetingLibraryProps) {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchHits, setSearchHits] = useState<SearchHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [exportingId, setExportingId] = useState<number | null>(null);

  const loadMeetings = async () => {
    try {
      const data = await invoke<Meeting[]>("list_meetings", { limit: 100, offset: 0 });
      setMeetings(data);
    } catch (e) {
      console.error("Failed to load meetings:", e);
    }
  };

  useEffect(() => {
    loadMeetings();
  }, []);

  const search = async () => {
    if (!searchQuery.trim()) {
      setSearchHits([]);
      return;
    }
    setLoading(true);
    try {
      const hits = await invoke<SearchHit[]>("search_meetings", {
        query: searchQuery,
        limit: 50,
      });
      setSearchHits(hits);
    } catch (e) {
      console.error("Search failed:", e);
    } finally {
      setLoading(false);
    }
  };

  const handleExport = async (m: Meeting, format: ExportFormat) => {
    const safeTitle =
      m.title.replace(/[<>:"/\\|?*\x00-\x1F]/g, "_").trim() || `meeting-${m.id}`;
    let target: string | null = null;
    try {
      target = await save({
        defaultPath: `${safeTitle}.${format}`,
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
      });
    } catch (e) {
      toast.error("Could not open save dialog", { description: String(e) });
      return;
    }
    if (!target) return; // user cancelled

    setExportingId(m.id);
    try {
      const content = await invoke<string>("export_meeting", {
        meetingId: m.id,
        format,
      });
      await writeTextFile(target, content);
      toast.success(`Exported to ${target}`);
    } catch (e) {
      toast.error("Export failed", { description: String(e) });
    } finally {
      setExportingId(null);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex gap-2">
        <Input
          placeholder="Search meetings (e.g. 'deadline', '张伟', 'Q4 预算')..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && search()}
        />
      </div>

      {searchHits.length > 0 ? (
        <div className="space-y-2">
          <h2 className="font-semibold">{searchHits.length} search results</h2>
          {searchHits.map((hit) => (
            <Card
              key={hit.transcript_id}
              className="p-4 cursor-pointer hover:bg-accent transition-colors"
              onClick={() => onOpen(hit.meeting_id)}
            >
              <div className="text-xs text-muted-foreground mb-1">
                Meeting #{hit.meeting_id} · sequence {hit.sequence_id}
              </div>
              <div className="text-sm">{highlight(hit.rewritten_text || hit.text, searchQuery)}</div>
            </Card>
          ))}
        </div>
      ) : (
        <div className="space-y-2">
          <h2 className="font-semibold">{meetings.length} meetings</h2>
          {meetings.length === 0 && (
            <div className="text-sm text-muted-foreground p-8 text-center border border-dashed rounded">
              No meetings yet. Start a recording to populate this view.
            </div>
          )}
          {meetings.map((m) => (
            <Card
              key={m.id}
              className="p-4 cursor-pointer hover:bg-accent transition-colors"
              onClick={() => onOpen(m.id)}
            >
              <div className="flex items-center justify-between gap-2">
                <div className="min-w-0">
                  <div className="font-medium truncate">{m.title}</div>
                  <div className="text-xs text-muted-foreground">
                    {new Date(m.started_at).toLocaleString()}
                    {m.ended_at &&
                      ` · ${((new Date(m.ended_at).getTime() - new Date(m.started_at).getTime()) / 60000).toFixed(0)}min`}
                  </div>
                </div>
                <div
                  className="flex items-center gap-2 flex-shrink-0"
                  onClick={(e) => e.stopPropagation()}
                >
                  {m.language_primary && (
                    <span className="text-xs px-2 py-1 rounded bg-primary/10 text-primary">
                      {m.language_primary}
                    </span>
                  )}
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button
                        variant="ghost"
                        disabled={exportingId === m.id}
                        className="h-7 w-7 p-0"
                        aria-label="Export meeting"
                      >
                        <Download className="w-4 h-4" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="z-50 min-w-[12rem] rounded-md border bg-popover p-1 text-popover-foreground shadow-md">
                      {EXPORT_FORMATS.map((f) => (
                        <DropdownMenuItem
                          key={f.id}
                          onSelect={() => handleExport(m, f.id)}
                          className="cursor-pointer rounded px-2 py-1.5 text-sm outline-none hover:bg-accent focus:bg-accent"
                        >
                          {f.label}
                        </DropdownMenuItem>
                      ))}
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              </div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
function highlight(text: string, query: string) {
  if (!query.trim()) return text;
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const parts = text.split(new RegExp(`(${escaped})`, "gi"));
  return parts.map((part, i) =>
    part.toLowerCase() === query.toLowerCase() ? (
      <mark key={i} className="bg-warning-soft text-warning-fg px-0.5 rounded">
        {part}
      </mark>
    ) : (
      <span key={i}>{part}</span>
    )
  );
}
