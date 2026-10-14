import { openUrl } from '@tauri-apps/plugin-opener';
import { useEffect, useRef, useState, type MouseEvent } from 'react';
import Markdown from 'react-markdown';

const EXTERNAL = /^(https?:|mailto:)/i;

/**
 * Markdown is untrusted: raw HTML is dropped (`skipHtml`) and react-markdown's
 * default URL transform strips dangerous protocols such as `javascript:`.
 * Nothing embedded is ever executed. Links never navigate the editor window:
 * web and mail links open in the system's default application.
 */
export function MarkdownView({ text }: { text: string }) {
  const onClick = (event: MouseEvent<HTMLDivElement>) => {
    const link = (event.target as HTMLElement).closest('a');
    if (!link) return;
    event.preventDefault();
    const href = link.getAttribute('href') ?? '';
    if (EXTERNAL.test(href)) void openUrl(href);
  };
  if (!text.trim()) return <p className="muted">No description.</p>;
  return (
    <div className="markdown" onClick={onClick}>
      <Markdown skipHtml>{text}</Markdown>
    </div>
  );
}

/**
 * Markdown editor with an Edit / Preview toggle. Keystrokes go to the
 * session as they happen (merged into one undo step); the text area keeps
 * its own draft so replies arriving mid-typing never move the caret.
 */
export function MarkdownField(props: { value: string; onChange: (value: string) => void; testId: string }) {
  const [mode, setMode] = useState<'edit' | 'preview'>('edit');
  const [draft, setDraft] = useState(props.value);
  // Last text sent and not yet confirmed by the session.
  const pending = useRef<string | null>(null);

  useEffect(() => {
    if (pending.current === null || props.value === pending.current) {
      pending.current = null;
      setDraft(props.value);
    }
  }, [props.value]);

  const change = (text: string) => {
    setDraft(text);
    pending.current = text;
    props.onChange(text);
  };

  return (
    <div className="markdown-field">
      <div className="tabs" role="tablist">
        <button role="tab" aria-selected={mode === 'edit'} className={mode === 'edit' ? 'active' : ''} onClick={() => setMode('edit')}>
          Edit
        </button>
        <button
          role="tab"
          aria-selected={mode === 'preview'}
          className={mode === 'preview' ? 'active' : ''}
          onClick={() => setMode('preview')}
          data-testid={`${props.testId}-preview-tab`}
        >
          Preview
        </button>
      </div>
      {mode === 'edit' ? (
        <textarea
          className="markdown-input"
          value={draft}
          onChange={(e) => change(e.target.value)}
          placeholder="Markdown description…"
          data-testid={props.testId}
          spellCheck
        />
      ) : (
        <div data-testid={`${props.testId}-preview`}>
          <MarkdownView text={draft} />
        </div>
      )}
    </div>
  );
}
