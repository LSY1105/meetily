export interface Message {
  id: string;
  content: string;
  timestamp: string;
}

export interface Transcript {
  id: string;
  text: string;
  timestamp: string; // Wall-clock time (e.g., "14:30:05")
  sequence_id?: number;
  chunk_start_time?: number; // Legacy field
  is_partial?: boolean;
  confidence?: number;
  // NEW: Recording-relative timestamps for playback sync
  audio_start_time?: number; // Seconds from recording start (e.g., 125.3)
  audio_end_time?: number;   // Seconds from recording start (e.g., 128.6)
  duration?: number;          // Segment duration in seconds (e.g., 3.3)
  /** PR-44a: realtime speaker hint; dropped once the offline label arrives. */
  transient_speaker?: string | null;
  speaker?: string | null;
  // ponytail: wall-clock ms when the frontend buffered this entry;
  // processBufferedTranscripts uses it for stale-vs-recent. `id`
  // format isn't stable enough (sequence_id / seg_N / Date.now())
  // to parse for a date prefix reliably.
  buffered_at?: number;
  // ponytail: iFlytek sentence protocol. sentence_id is the stable
  // key the frontend uses to dedup/merge messages for one sentence
  // (Begin, Mid*, Full). sentence_status drives the rendering
  // style: Begin/Mid render as grey-italic streaming text, Full as
  // normal dark text. Replace-by-sentence_id keeps React rows stable
  // across the streaming Mid* updates for the same sentence, which
  // is what stops the "前面字抖动" reflow the user kept seeing.
  sentence_id?: number;
  sentence_status?: 'Begin' | 'Mid' | 'Full';
}
export type DiarizationModelStatus = 'ready' | 'loading' | 'failed' | 'disabled';

export interface DiarizationConfig {
  enabled: boolean;
  min_speakers: number;
  max_speakers: number;
  model_status: DiarizationModelStatus;
}


export interface TranscriptUpdate {
  // ponytail: iFlytek append-only protocol. For Mid events, `text` is
  // the NEWLY recognized suffix since the previous Mid for the same
  // `sentenceId`; the frontend appends it to the existing row so
  // front text never reflows. For Full events, `text` is the full
  // cumulative sentence. For Begin events, `text` is "".
  text: string;
  timestamp: string; // Wall-clock time for reference
  source: string;
  sequence_id: number;
  chunk_start_time: number; // Legacy field
  is_partial: boolean;
  confidence: number;
  // NEW: Recording-relative timestamps for playback sync
  audio_start_time: number; // Seconds from recording start
  audio_end_time: number;   // Seconds from recording start
  duration: number;          // Segment duration in seconds
  // ponytail: iFlytek sentence protocol. `sentenceId` is the
  // dedup key the frontend uses to merge messages for one
  // sentence (Begin, Mid*, Full). `sentenceStatus` carries the
  // role of this message in the sentence lifecycle.
  sentenceId?: number;
  sentenceStatus?: 'Begin' | 'Mid' | 'Full';
}

export interface Block {
  id: string;
  type: string;
  content: string;
  color: string;
}

export interface Section {
  title: string;
  blocks: Block[];
}

export interface Summary {
  [key: string]: Section;
}

export interface ApiResponse {
  message: string;
  num_chunks: number;
  data: any[];
}

export interface SummaryResponse {
  status: string;
  summary: Summary;
  raw_summary?: string;
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

// BlockNote-specific types
export type SummaryFormat = 'legacy' | 'markdown' | 'blocknote';

export interface BlockNoteBlock {
  id: string;
  type: string;
  props?: Record<string, any>;
  content?: any[];
  children?: BlockNoteBlock[];
}

export interface SummaryDataResponse {
  markdown?: string;
  summary_json?: BlockNoteBlock[];
  // Legacy format fields
  MeetingName?: string;
  _section_order?: string[];
  [key: string]: any; // For legacy section data
}

// Pagination types for optimized transcript loading
export interface MeetingMetadata {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  folder_path?: string;
}

export interface PaginatedTranscriptsResponse {
  transcripts: Transcript[];
  total_count: number;
  has_more: boolean;
}

// Transcript segment data for virtualized display
export interface TranscriptSegmentData {
  id: string;
  timestamp: number; // audio_start_time in seconds
  endTime?: number; // audio_end_time in seconds
  text: string;
  confidence?: number;
  /** PR-44a: realtime speaker hint; dropped once the offline label arrives. */
  transient_speaker?: string | null;
  speaker?: string | null;
  // PR-42-iii: streaming LLM postprocess result.
  corrected_text?: string;
  postprocess_failed?: boolean;
  postprocess_failed_message?: string;
  // ponytail: iFlytek sentence protocol fields, carried over from
  // the upstream Transcript row so the virtualized view can render
  // streaming vs committed style and React-key by sentence_id.
  sentence_id?: number;
  sentence_status?: 'Begin' | 'Mid' | 'Full';
  sequence_id?: number;
}
