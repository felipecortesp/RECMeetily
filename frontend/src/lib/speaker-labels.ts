/**
 * Pure helpers for turning a raw `transcripts.speaker` label into what the UI
 * shows and how it groups/colors/aligns rows.
 *
 * A label is either:
 *   - a single label: "You", "Speaker 1", "Guest", or a custom name — plus the
 *     legacy "<name> (You)" shape.
 *   - a combined label such as "You + Speaker 1": the offline diarization pass
 *     (`src-tauri/src/diarization/mod.rs`) persists this when several
 *     source-track speakers overlap one transcript row, so it never falsely
 *     attributes the row to a single voice.
 *
 * A combined label is NEVER the user, even when one of its parts is "You" —
 * treating it as the user would misattribute remote speech to the local
 * microphone (see meetily-actuallyfree#16). It renders as its own row, with
 * each part shown (the "You" part mapped to the user's display name) and its
 * own grouping key.
 */

const COMBINED_LABEL_SEPARATOR = ' + ';

/** True when `speaker` combines more than one source-track voice. */
export function isCombinedSpeaker(speaker?: string): boolean {
    return (speaker ?? '').includes(COMBINED_LABEL_SEPARATOR);
}

/**
 * True only when `speaker` IS the local user: exactly "You" (case-insensitive,
 * trimmed) or a single name ending in "(You)". Combined labels never qualify,
 * even if "You" is one of their parts.
 */
export function isUserSpeaker(speaker?: string): boolean {
    const normalized = speaker?.trim() ?? '';
    if (!normalized || isCombinedSpeaker(normalized)) return false;
    return /^you$/i.test(normalized) || /\(\s*you\s*\)$/i.test(normalized);
}

/**
 * Turn a raw speaker label into what the user should read. The display name
 * lives in settings (not in Rust) so it can be changed without restarting,
 * which is why substitution happens here rather than at persistence time.
 */
export function displaySpeaker(speaker: string, userName: string): string {
    if (isUserSpeaker(speaker)) {
        return userName ? `${userName} (You)` : 'You';
    }
    if (isCombinedSpeaker(speaker)) {
        return speaker
            .split(COMBINED_LABEL_SEPARATOR)
            .map((part) => (part.trim().toLowerCase() === 'you' ? userName || 'You' : part))
            .join(COMBINED_LABEL_SEPARATOR);
    }
    return speaker;
}

/** Normalize speaker keys so "You" / "you" / empty compare cleanly. Combined
 * labels group separately from either of their parts. */
export function speakerKey(speaker?: string): string {
    if (isUserSpeaker(speaker)) return '__you__';
    return (speaker ?? '').trim().toLowerCase() || '__unknown__';
}
