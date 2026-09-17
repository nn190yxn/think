import { useState } from "react";
import type { FollowUpAnchor } from "../ipc/commands";
import { anchorKindLabel } from "../domain/labels";

/**
 * 追问入口：围绕一段既有判断写一个新问题，可选择是否沿用上一场的大师阵容。
 */
export function FollowUpForm({
  anchor,
  busy = false,
  onSubmit,
  onCancel,
}: {
  readonly anchor: FollowUpAnchor;
  readonly busy?: boolean;
  readonly onSubmit: (question: string, inheritPanel: boolean) => void;
  readonly onCancel: () => void;
}) {
  const [question, setQuestion] = useState("");
  const [inheritPanel, setInheritPanel] = useState(true);
  const [error, setError] = useState<string | null>(null);

  return (
    <form
      className="followup"
      aria-label="发起追问"
      onSubmit={(event) => {
        event.preventDefault();
        const asked = question.trim();
        if (!asked) {
          setError("先写下要追问的问题");
          return;
        }
        setError(null);
        onSubmit(asked, inheritPanel);
      }}
    >
      <p className="followup__anchor">
        <span className="followup__kind">{anchorKindLabel(anchor.kind)}</span>
        <span className="followup__text">{anchor.text}</span>
      </p>
      <label className="followup__label" htmlFor="followup-question">
        追问
      </label>
      <textarea
        id="followup-question"
        className="followup__input"
        value={question}
        rows={3}
        placeholder="针对这段判断，你想继续问清什么"
        onChange={(event) => setQuestion(event.target.value)}
      />
      <label className="followup__inherit">
        <input
          type="checkbox"
          checked={inheritPanel}
          onChange={(event) => setInheritPanel(event.target.checked)}
        />
        沿用上一场的大师阵容，保持判断依据一致
      </label>
      {error ? (
        <p className="followup__note" data-tone="warn">
          {error}
        </p>
      ) : null}
      <div className="followup__actions">
        <button className="followup__submit" type="submit" disabled={busy}>
          {busy ? "正在准备" : "发起追问"}
        </button>
        <button className="followup__cancel" type="button" onClick={onCancel}>
          取消
        </button>
      </div>
    </form>
  );
}
