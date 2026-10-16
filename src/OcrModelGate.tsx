import { ModelDownloadModal } from "./ModelDownloadModal";
import type { OcrModelStatus } from "./OcrModelSettings";

export function OcrModelRequiredDialog({
  status: _status,
  isOpen,
  onOpenChange,
  onSuccess,
}: {
  status?: OcrModelStatus;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess?: () => void;
}) {
  return (
    <ModelDownloadModal
      kind="ocr"
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      onSuccess={onSuccess}
    />
  );
}
