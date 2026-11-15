import { useEffect, useState } from "react";
import type { UoocSnapshot } from "../state/types";
import { UoocQaSettings } from "./UoocQaSettings";

interface BankTabProps {
  snapshot: UoocSnapshot;
  ready: boolean;
  debugEnabled: boolean;
  qaServerUrl: string;
  action: (key: string, command: string, args?: Record<string, unknown>) => Promise<boolean>;
}

/**
 * 题库 tab：题库共享、题库浏览与向量检索。
 * 服务地址用本 tab 自己的草稿 + 显式保存，不再与运行设置共用。
 */
export function BankTab({ snapshot, ready, debugEnabled, qaServerUrl, action }: BankTabProps) {
  const [draftUrl, setDraftUrl] = useState(qaServerUrl);
  useEffect(() => setDraftUrl(qaServerUrl), [qaServerUrl]);
  return (
    <UoocQaSettings
      courses={snapshot.courses}
      quizzes={snapshot.quizzes}
      qaServerUrl={draftUrl}
      onQaServerUrlChange={setDraftUrl}
      onSaveSettings={async () => {
        const ok = await action("qa-server", "save_uooc_settings", {
          settings: { ...snapshot.settings, qaServerUrl: draftUrl },
        });
        if (ok) {
          const { toast } = await import("../../ui");
          toast.success("题库服务设置已保存");
        }
      }}
      debugEnabled={debugEnabled}
      disabled={!ready}
    />
  );
}
