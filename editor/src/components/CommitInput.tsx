import { useEffect, useState, type KeyboardEvent } from 'react';

/**
 * Text input that commits on blur or Enter (Escape reverts). Keeps names
 * from being committed half-typed, e.g. as an empty string.
 */
export function CommitInput(props: { value: string; onCommit: (value: string) => void; testId?: string; label: string }) {
  const [draft, setDraft] = useState(props.value);
  useEffect(() => setDraft(props.value), [props.value]);

  const commit = () => {
    if (draft !== props.value) props.onCommit(draft);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') event.currentTarget.blur();
    if (event.key === 'Escape') {
      setDraft(props.value);
      event.currentTarget.blur();
    }
  };
  return (
    <input
      className="text-input"
      aria-label={props.label}
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={onKeyDown}
      data-testid={props.testId}
    />
  );
}
