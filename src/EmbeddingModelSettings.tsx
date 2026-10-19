import {
  formatModelSize,
  LocalModelCard,
  useEmbeddingModelStatus,
  type EmbeddingModelStatus,
} from "./LocalModelSettings";

export { formatModelSize, useEmbeddingModelStatus, type EmbeddingModelStatus };

export function EmbeddingModelSettings({ autoReturn = false }: { autoReturn?: boolean } = {}) {
  return (
    <LocalModelCard
      kind="embedding"
      autoReturn={autoReturn}
      downloadButtonLabel="下载 Embedding 模型"
    />
  );
}
