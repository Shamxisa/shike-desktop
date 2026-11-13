import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import {
  Check,
  Copy,
  ExternalLink,
  FileText,
  Plus,
  RotateCcw,
  Trash2,
} from "lucide-react";
import { call, errorText } from "../api";
import type { Note } from "../types";

export interface NoteEditorHandle {
  flush: () => Promise<boolean>;
}

export const NoteEditor = forwardRef<
  NoteEditorHandle,
  { note: Note; notify: (message: string) => void }
>(function NoteEditor({ note, notify }, ref) {
  const [title, setTitle] = useState(note.title);
  const [content, setContent] = useState(note.content);
  const [status, setStatus] = useState("已保存");
  const [failed, setFailed] = useState(false);
  const draft = useRef({ title: note.title, content: note.content });
  const revision = useRef(note.revision);
  const dirty = useRef(false);
  const pending = useRef<Promise<boolean> | null>(null);
  const mounted = useRef(true);
  const latest = useRef(note);
  latest.current = note;

  function flush(): Promise<boolean> {
    if (pending.current) return pending.current;
    const operation = async () => {
      while (dirty.current) {
        const value = { ...draft.current };
        dirty.current = false;
        if (mounted.current) {
          setStatus("保存中…");
          setFailed(false);
        }
        try {
          const saved = await call<Note>("save_note", {
            id: note.id,
            ...value,
            revision: revision.current,
          });
          revision.current = saved.revision;
          if (mounted.current) setStatus("已保存");
        } catch (error) {
          dirty.current = true;
          if (mounted.current) {
            setStatus(errorText(error));
            setFailed(true);
          }
          notify(errorText(error));
          return false;
        }
      }
      return true;
    };
    pending.current = operation().finally(() => {
      pending.current = null;
    });
    return pending.current;
  }

  useImperativeHandle(ref, () => ({ flush }));
  useEffect(() => {
    if (!dirty.current && !pending.current) {
      revision.current = note.revision;
      draft.current = { title: note.title, content: note.content };
      setTitle(note.title);
      setContent(note.content);
    }
  }, [note.revision, note.title, note.content]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      void flush();
    };
    // Each editor instance belongs to one stable note ID.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (!dirty.current) return;
    const timeout = setTimeout(() => {
      void flush();
    }, 400);
    return () => clearTimeout(timeout);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [title, content]);

  function change(field: "title" | "content", value: string) {
    draft.current[field] = value;
    dirty.current = true;
    setStatus("待保存");
    if (field === "title") setTitle(value);
    else setContent(value);
  }
  function reload() {
    if (pending.current) return;
    const value = latest.current;
    revision.current = value.revision;
    draft.current = { title: value.title, content: value.content };
    dirty.current = false;
    setTitle(value.title);
    setContent(value.content);
    setFailed(false);
    setStatus("已载入最新内容");
  }
  return (
    <div className="note-editor">
      <input
        className="note-title-input"
        aria-label="便签标题"
        maxLength={120}
        value={title}
        onChange={(event) => change("title", event.target.value)}
        onBlur={() => void flush()}
        placeholder="便签标题"
      />
      <textarea
        aria-label="便签正文"
        value={content}
        onChange={(event) => change("content", event.target.value)}
        onBlur={() => void flush()}
        placeholder="记下此刻的想法…"
      />
      <div
        className={`save-status ${failed ? "error-text" : ""}`}
        role="status"
      >
        {!failed && <Check size={13} />}
        <span>{status}</span>
        <small>{content.length} 字符</small>
      </div>
      {failed && (
        <div className="inline-actions">
          <button onClick={() => void flush()}>
            <RotateCcw size={14} />
            重试保存
          </button>
          <button
            onClick={() =>
              void navigator.clipboard
                .writeText(`${title}\n${content}`)
                .then(() => notify("当前内容已复制"))
                .catch((error) => notify(errorText(error)))
            }
          >
            <Copy size={14} />
            复制当前内容
          </button>
          <button onClick={reload}>载入最新内容</button>
        </div>
      )}
    </div>
  );
});

export default function Notes({
  notes,
  editorRef,
  notify,
}: {
  notes: Note[];
  editorRef: React.RefObject<NoteEditorHandle | null>;
  notify: (message: string) => void;
}) {
  const [selected, setSelected] = useState<string | null>(notes[0]?.id ?? null);
  const note = notes.find((item) => item.id === selected) ?? notes[0];
  async function select(id: string) {
    if (editorRef.current && !(await editorRef.current.flush())) return;
    setSelected(id);
  }
  async function create() {
    if (editorRef.current && !(await editorRef.current.flush())) return;
    try {
      const created = await call<Note>("create_note");
      setSelected(created.id);
    } catch (error) {
      notify(errorText(error));
    }
  }
  async function remove() {
    if (!note) return;
    if (editorRef.current && !(await editorRef.current.flush())) return;
    try {
      await call("delete_note", { id: note.id });
      setSelected(null);
    } catch (error) {
      notify(errorText(error));
    }
  }
  return (
    <>
      <div className="section-heading">
        <div>
          <h2>我的便签</h2>
          <p>{notes.length} 条 · 自动保存</p>
        </div>
        <button className="primary" onClick={() => void create()}>
          <Plus size={16} />
          新建便签
        </button>
      </div>
      {!note ? (
        <div className="empty-state">
          <FileText size={28} />
          <h3>随手记下来</h3>
          <p>便签可以独立打开，并置顶在桌面一角。</p>
        </div>
      ) : (
        <div className="notes-layout">
          <div className="notes-list" aria-label="便签列表">
            {[...notes]
              .sort((a, b) => b.updatedAt - a.updatedAt)
              .map((item) => (
                <button
                  key={item.id}
                  aria-pressed={item.id === note.id}
                  onClick={() => void select(item.id)}
                >
                  <strong>{item.title || "无标题便签"}</strong>
                  <span>{item.content.slice(0, 70) || "空白便签"}</span>
                  <small>
                    {new Date(item.updatedAt).toLocaleString("zh-CN", {
                      month: "numeric",
                      day: "numeric",
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                  </small>
                </button>
              ))}
          </div>
          <div className="note-workspace">
            <div className="note-toolbar">
              <button
                onClick={() =>
                  void call("open_note", { id: note.id }).catch((error) =>
                    notify(errorText(error)),
                  )
                }
              >
                <ExternalLink size={14} />
                独立小窗
              </button>
              <button
                className="icon-button danger"
                aria-label="删除当前便签"
                onClick={() => void remove()}
              >
                <Trash2 size={15} />
              </button>
            </div>
            <NoteEditor
              key={note.id}
              ref={editorRef}
              note={note}
              notify={notify}
            />
          </div>
        </div>
      )}
    </>
  );
}
