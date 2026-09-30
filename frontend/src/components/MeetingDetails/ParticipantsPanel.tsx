"use client";

import { useEffect, useMemo, useState } from 'react';
import { Users } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { speakerDot } from '@/components/VirtualizedTranscriptView';
import { displaySpeaker } from '@/lib/speaker-labels';
import {
  formatParticipantDuration,
  summarizeParticipants,
  type ParticipantSegment,
} from '@/lib/participants';

interface ParticipantsPanelProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Same segments the transcript renders, so the list follows refetches/renames. */
  segments: ParticipantSegment[];
  /** Opens the shared SpeakerRenameDialog for this raw label. */
  onRename: (speaker: string) => void;
  /** Scrolls the transcript to this segment id. */
  onGoToSegment: (segmentId: string) => void;
}

/** Lists every individual speaker in the meeting with rename / jump actions. */
export function ParticipantsPanel({
  open,
  onOpenChange,
  segments,
  onRename,
  onGoToSegment,
}: ParticipantsPanelProps) {
  const participants = useMemo(() => summarizeParticipants(segments), [segments]);
  const [userName, setUserName] = useState('');

  useEffect(() => {
    if (open && typeof window !== 'undefined') {
      setUserName(localStorage.getItem('meetily_user_name')?.trim() || '');
    }
  }, [open]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-describedby={undefined} className="sm:max-w-md">
        <DialogTitle className="flex items-center gap-2 text-base">
          <Users size={18} className="text-blue-500" />
          Participants
        </DialogTitle>

        {participants.length === 0 ? (
          <p className="mt-2 text-sm text-gray-500">Run Speakers to identify participants.</p>
        ) : (
          <ul className="mt-2 max-h-[60vh] space-y-1 overflow-y-auto">
            {participants.map((p) => (
              <li
                key={p.label}
                className="flex items-center gap-3 rounded-md border border-[var(--af-border,#e5e7eb)] px-3 py-2"
              >
                <span aria-hidden className={`h-2.5 w-2.5 shrink-0 rounded-full ${speakerDot(p.label)}`} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium text-[var(--af-text,#111827)]">
                    {displaySpeaker(p.label, userName)}
                  </div>
                  <div className="text-xs text-gray-500">
                    {p.segmentCount} {p.segmentCount === 1 ? 'line' : 'lines'} ·{' '}
                    {formatParticipantDuration(p.totalSeconds)}
                  </div>
                </div>
                <Button variant="outline" size="sm" onClick={() => onRename(p.label)}>
                  Rename
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    onOpenChange(false);
                    onGoToSegment(p.firstSegmentId);
                  }}
                >
                  Go to first
                </Button>
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}
