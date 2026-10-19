import {
  formatModelSize,
  LocalModelCard,
  useOcrModelStatus,
  type OcrModelStatus,
} from "./LocalModelSettings";

export { formatModelSize, useOcrModelStatus, type OcrModelStatus };

export function OcrModelSettings({ autoReturn = false }: { autoReturn?: boolean }) {
  return <LocalModelCard kind="ocr" autoReturn={autoReturn} downloadButtonLabel="下载模型" />;
}
