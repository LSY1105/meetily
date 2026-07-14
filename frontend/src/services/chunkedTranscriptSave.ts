import { invoke } from "@tauri-apps/api/core";
import type { Transcript } from "@/types";

export interface SaveBatchResult {
  status: string;
  meeting_id: string;
  saved: number;
  batch_index: number;
}

export interface ChunkedSaveOptions {
  /** Max segments per HTTP call. Default 500 (matches backend default). */
  batchSize?: number;
  /** Existing meeting_id for batch append mode. Undefined = create new meeting on first batch. */
  meetingId?: string;
  /** Called after each successful batch. Useful for progress UI. */
  onBatchSaved?: (info: { batchIndex: number; savedSoFar: number; totalCount: number }) => void;
  /** Abort signal -- caller can cancel mid-flight. */
  signal?: AbortSignal;
}

const DEFAULT_BATCH_SIZE = 500;

/**
 * Save a (potentially large) transcript array in fixed-size batches to avoid
 * network / backend memory spikes on hour-long meetings (PR-43a).
 *
 * First batch uses `/save-transcript` to create the meeting; subsequent
 * batches use `/save-transcript-batch` to append. The backend returns the
 * meeting_id after the first call, which is reused thereafter.
 */
export async function saveTranscriptInBatches(
  meetingTitle: string,
  transcripts: Transcript[],
  options: ChunkedSaveOptions = {},
): Promise<{ meeting_id: string; totalSaved: number }> {
  const batchSize = options.batchSize ?? DEFAULT_BATCH_SIZE;
  if (batchSize <= 0) {
    throw new Error("batchSize must be positive");
  }
  if (!transcripts || transcripts.length === 0) {
    if (!options.meetingId) {
      throw new Error("no transcripts and no meetingId -- nothing to save");
    }
    return { meeting_id: options.meetingId, totalSaved: 0 };
  }

  let meetingId = options.meetingId;
  let savedSoFar = 0;
  const total = transcripts.length;
  const firstBatchEnd = Math.min(batchSize, total);

  // First batch -- may create new meeting.
  if (!meetingId) {
    const res = await invoke<{ meeting_id: string }>("save_transcript", {
      request: {
        meeting_title: meetingTitle,
        transcripts: transcripts.slice(0, firstBatchEnd),
        folder_path: null,
        meeting_id: null,
        batch_index: 0,
        is_final_batch: firstBatchEnd >= total,
      },
    });
    meetingId = res.meeting_id;
    savedSoFar += firstBatchEnd;
    options.onBatchSaved?.({ batchIndex: 0, savedSoFar, totalCount: total });
  } else {
    // Append directly via batch endpoint.
    await invoke<SaveBatchResult>("save_transcript_batch", {
      request: {
        meeting_title: "",
        transcripts: transcripts.slice(0, firstBatchEnd),
        folder_path: null,
        meeting_id: meetingId,
        batch_index: 0,
        is_final_batch: firstBatchEnd >= total,
      },
    });
    savedSoFar += firstBatchEnd;
    options.onBatchSaved?.({ batchIndex: 0, savedSoFar, totalCount: total });
  }

  // Remaining batches -- always use batch endpoint.
  let batchIndex = 1;
  while (savedSoFar < total) {
    if (options.signal?.aborted) {
      throw new DOMException("saveTranscriptInBatches aborted", "AbortError");
    }
    const end = Math.min(savedSoFar + batchSize, total);
    await invoke<SaveBatchResult>("save_transcript_batch", {
      request: {
        meeting_title: "",
        transcripts: transcripts.slice(savedSoFar, end),
        folder_path: null,
        meeting_id: meetingId,
        batch_index: batchIndex,
        is_final_batch: end >= total,
      },
    });
    savedSoFar = end;
    options.onBatchSaved?.({ batchIndex, savedSoFar, totalCount: total });
    batchIndex++;
  }

  return { meeting_id: meetingId, totalSaved: savedSoFar };
}
