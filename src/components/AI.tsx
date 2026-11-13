import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { KeyRound, Plus, Send, Square, Trash2 } from "lucide-react";
import { call, errorText } from "../api";
import type { AiEvent, Profile } from "../types";

const emptyProfile = (): Profile => ({
  id: "",
  name: "",
  baseUrl: "https://api.openai.com/v1",
  model: "",
  protocol: "chat-completions",
});
type ChatMessage = { role: "user" | "assistant"; content: string };

export default function AI({
  profiles,
  notify,
}: {
  profiles: Profile[];
  notify: (message: string) => void;
}) {
  const [selected, setSelected] = useState(profiles[0]?.id ?? "");
  const [profile, setProfile] = useState<Profile>(
    profiles[0] ?? emptyProfile(),
  );
  const [key, setKey] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [prompt, setPrompt] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);
  const [ready, setReady] = useState(false);
  const request = useRef<string | null>(null);
  const savedProfile = profiles.find((item) => item.id === selected);
  useEffect(() => {
    let active = true;
    let unsubscribe: (() => void) | undefined;
    void listen<AiEvent>("ai-stream", (event) => {
      const value = event.payload;
      if (value.requestId !== request.current) return;
      if (value.delta)
        setMessages((current) => {
          const next = [...current];
          const last = next[next.length - 1];
          if (last?.role === "assistant")
            next[next.length - 1] = {
              ...last,
              content: last.content + value.delta,
            };
          return next;
        });
      const labels: Record<string, string> = {
        connecting: "正在连接…",
        streaming: "生成中…",
        completed: "已完成",
        cancelled: "已停止",
        failed: "请求失败",
      };
      setStatus(value.error ?? labels[value.status] ?? value.status);
      if (["completed", "cancelled", "failed"].includes(value.status)) {
        request.current = null;
        setBusy(false);
      }
    })
      .then((cleanup) => {
        if (active) {
          unsubscribe = cleanup;
          setReady(true);
        } else cleanup();
      })
      .catch((error) => notify(errorText(error)));
    return () => {
      active = false;
      unsubscribe?.();
      if (request.current)
        void call("cancel_chat", { requestId: request.current });
    };
  }, [notify]);
  useEffect(() => {
    const current = savedProfile;
    setProfile(current ?? emptyProfile());
    setKey("");
    setHasKey(false);
    let active = true;
    if (current)
      void call<boolean>("key_status", { id: current.id })
        .then((value) => {
          if (active) setHasKey(value);
        })
        .catch((error) => notify(errorText(error)));
    return () => {
      active = false;
    };
  }, [
    selected,
    savedProfile?.name,
    savedProfile?.baseUrl,
    savedProfile?.model,
    notify,
  ]);

  async function save() {
    setSaving(true);
    try {
      const saved = await call<Profile>("save_profile", { profile });
      if (key.trim()) {
        await call("set_api_key", { id: saved.id, key });
        setKey("");
        setHasKey(true);
      }
      setSelected(saved.id);
      setProfile(saved);
      notify("服务配置已保存");
    } catch (error) {
      notify(errorText(error));
    } finally {
      setSaving(false);
    }
  }
  async function send(event: React.FormEvent) {
    event.preventDefault();
    if (!prompt.trim() || busy || !selected || !ready) return;
    const next: ChatMessage[] = [
      ...messages.filter((message) => message.content),
      { role: "user", content: prompt.trim() },
    ];
    const id = crypto.randomUUID();
    request.current = id;
    setMessages([...next, { role: "assistant", content: "" }]);
    setPrompt("");
    setBusy(true);
    setStatus("正在连接…");
    try {
      await call("start_chat", {
        profileId: selected,
        requestId: id,
        messages: next,
      });
    } catch (error) {
      request.current = null;
      setBusy(false);
      setStatus(errorText(error));
    }
  }
  return (
    <div className="form-stack">
      <div className="section-heading">
        <div>
          <h2>AI 服务</h2>
          <p>OpenAI 兼容 Chat Completions · SSE 流式</p>
        </div>
        <button
          disabled={busy}
          onClick={() => {
            setSelected("");
            setProfile(emptyProfile());
          }}
        >
          <Plus size={15} />
          新增服务
        </button>
      </div>
      <label>
        服务配置
        <select
          disabled={busy}
          value={selected}
          onChange={(event) => setSelected(event.target.value)}
        >
          <option value="">新服务</option>
          {profiles.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
        </select>
      </label>
      <div className="form-grid">
        <label>
          名称
          <input
            value={profile.name}
            placeholder="例如，我的模型服务"
            onChange={(event) =>
              setProfile({ ...profile, name: event.target.value })
            }
          />
        </label>
        <label>
          模型名称
          <input
            value={profile.model}
            placeholder="填写服务提供方的模型 ID"
            onChange={(event) =>
              setProfile({ ...profile, model: event.target.value })
            }
          />
        </label>
      </div>
      <label>
        接口基础地址
        <input
          type="url"
          value={profile.baseUrl}
          onChange={(event) =>
            setProfile({ ...profile, baseUrl: event.target.value })
          }
          placeholder="https://your-provider.example/v1"
        />
      </label>
      <label>
        API Key{" "}
        <small>
          <KeyRound size={12} />
          {hasKey ? "已保存至 Windows 凭据管理器" : "尚未保存"}
        </small>
        <input
          type="password"
          autoComplete="off"
          value={key}
          placeholder={hasKey ? "输入新密钥以替换" : "输入 API Key"}
          onChange={(event) => setKey(event.target.value)}
        />
      </label>
      <div className="inline-actions">
        <button
          className="primary"
          disabled={saving || busy}
          onClick={() => void save()}
        >
          {saving ? "保存中…" : "保存服务"}
        </button>
        {selected && (
          <>
            <button
              disabled={busy}
              onClick={() =>
                void call("remove_api_key", { id: selected })
                  .then(() => {
                    setHasKey(false);
                    notify("密钥已删除");
                  })
                  .catch((error) => notify(errorText(error)))
              }
            >
              删除密钥
            </button>
            <button
              className="danger"
              disabled={busy}
              onClick={() =>
                void call("delete_profile", { id: selected })
                  .then(() => setSelected(""))
                  .catch((error) => notify(errorText(error)))
              }
            >
              <Trash2 size={14} />
              删除服务
            </button>
          </>
        )}
      </div>
      <hr />
      <div className="section-heading">
        <h3>连接与对话测试</h3>
        {messages.length > 0 && (
          <button
            disabled={busy}
            onClick={() => {
              setMessages([]);
              setStatus("");
            }}
          >
            清空对话
          </button>
        )}
      </div>
      <div
        className="chat-messages"
        aria-live="polite"
        aria-relevant="additions text"
      >
        {messages.map((message, index) => (
          <div key={index} className={`chat-message ${message.role}`}>
            <small>{message.role === "user" ? "你" : "AI"}</small>
            <p>{message.content || "等待回复…"}</p>
          </div>
        ))}
      </div>
      <form className="chat-input" onSubmit={(event) => void send(event)}>
        <textarea
          rows={2}
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
          placeholder="输入消息，测试流式连接…"
          aria-label="发送给AI的消息"
        />
        {busy ? (
          <button
            type="button"
            onClick={() =>
              void call("cancel_chat", { requestId: request.current }).catch(
                (error) => notify(errorText(error)),
              )
            }
          >
            <Square size={15} />
            停止
          </button>
        ) : (
          <button
            className="primary"
            disabled={!selected || !ready || !prompt.trim()}
          >
            <Send size={15} />
            发送
          </button>
        )}
      </form>
      {status && (
        <p className="muted" role="status">
          {status}
        </p>
      )}
    </div>
  );
}
