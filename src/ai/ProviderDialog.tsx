import { createPortal } from "react-dom";
import { Dialog } from "material/react";
import { ProviderForm } from "./ProviderForm";
import type { AiConfigView, AiProviderView } from "./types";

export function ProviderDialog({
  provider,
  defaultModel,
  onClose,
  onSaved,
}: {
  provider: AiProviderView | null;
  defaultModel?: string;
  onClose: () => void;
  onSaved: (config: AiConfigView) => void;
}) {
  return createPortal(
    <Dialog open headline={provider ? `编辑 ${provider.name}` : "添加 AI 提供商"} onClose={onClose}>
      <div className="max-h-[75vh] overflow-y-auto pr-1">
        <ProviderForm
          key={provider?.id ?? "new"}
          provider={provider}
          defaultModel={defaultModel}
          onSaved={onSaved}
        />
      </div>
    </Dialog>,
    document.body,
  );
}
