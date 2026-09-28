-- Recording-relative word timings (JSON array of {text,start,end}) written by
-- post-call retranscription; NULL for live rows and engines without word times.
ALTER TABLE transcripts ADD COLUMN words TEXT;

-- Audio track a row was transcribed from: 'mic' or 'system'; NULL for live rows and mixed audio.
ALTER TABLE transcripts ADD COLUMN source TEXT;
