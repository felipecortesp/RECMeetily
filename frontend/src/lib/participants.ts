/**
 * Pure helper behind the meeting "Participants" panel: one entry per individual
 * speaker, derived from the transcript segments.
 *
 * Combined labels ("You + Speaker 1") count toward each of their parts but never
 * produce an entry of their own (they are not a person). Segments with a
 * null/empty speaker are ignored.
 */

import { isUserSpeaker, speakerKey } from './speaker-labels';

const COMBINED_LABEL_SEPARATOR = '+';

export interface ParticipantSegment {
    id: string;
    /** audio_start_time in seconds */
    timestamp: number;
    /** audio_end_time in seconds */
    endTime?: number;
    speaker?: string | null;
}

export interface ParticipantSummary {
    /** Raw speaker label as stored (what SpeakerRenameDialog expects). */
    label: string;
    segmentCount: number;
    totalSeconds: number;
    firstSegmentId: string;
    firstTimestamp: number;
}

function splitParts(speaker: string): string[] {
    return speaker
        .split(COMBINED_LABEL_SEPARATOR)
        .map((part) => part.trim())
        .filter(Boolean);
}

export function summarizeParticipants(segments: ParticipantSegment[]): ParticipantSummary[] {
    const byKey = new Map<string, ParticipantSummary>();

    for (const seg of segments) {
        const raw = seg.speaker?.trim();
        if (!raw) continue;
        const duration =
            seg.endTime != null && seg.endTime > seg.timestamp ? seg.endTime - seg.timestamp : 0;

        // De-duplicate parts within one segment (e.g. "A + A").
        const seen = new Set<string>();
        for (const part of splitParts(raw)) {
            const key = speakerKey(part);
            if (seen.has(key)) continue;
            seen.add(key);
            const existing = byKey.get(key);
            if (existing) {
                existing.segmentCount += 1;
                existing.totalSeconds += duration;
            } else {
                byKey.set(key, {
                    label: part,
                    segmentCount: 1,
                    totalSeconds: duration,
                    firstSegmentId: seg.id,
                    firstTimestamp: seg.timestamp,
                });
            }
        }
    }

    return [...byKey.values()].sort((a, b) => {
        const aYou = isUserSpeaker(a.label);
        const bYou = isUserSpeaker(b.label);
        if (aYou !== bYou) return aYou ? -1 : 1;
        return b.totalSeconds - a.totalSeconds;
    });
}

/** "45s", "3m 05s", "1h 02m" */
export function formatParticipantDuration(totalSeconds: number): string {
    const s = Math.max(0, Math.round(totalSeconds));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`;
    if (m > 0) return `${m}m ${String(sec).padStart(2, '0')}s`;
    return `${sec}s`;
}
