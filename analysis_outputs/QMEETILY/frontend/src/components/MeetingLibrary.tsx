"use client";

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Input } from "@/components/ui/input";
import { Card } from "@/components/ui/card";

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

export function MeetingLibrary({ onOpen }: MeetingLibraryProps) {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchHits, setSearchHits] = useState<SearchHit[]>([]);
  const [loading, setLoading] = useState(false);

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
              <div className="text-sm">{hit.rewritten_text || hit.text}</div>
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
              <div className="flex items-center justify-between">
                <div>
                  <div className="font-medium">{m.title}</div>
                  <div className="text-xs text-muted-foreground">
                    {new Date(m.started_at).toLocaleString()}
                    {m.ended_at &&
                      ` · ${((new Date(m.ended_at).getTime() - new Date(m.started_at).getTime()) / 60000).toFixed(0)}min`}
                  </div>
                </div>
                {m.language_primary && (
                  <span className="text-xs px-2 py-1 rounded bg-primary/10 text-primary">
                    {m.language_primary}
                  </span>
                )}
              </div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
