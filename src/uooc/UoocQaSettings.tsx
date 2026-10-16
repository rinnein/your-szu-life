import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import {
  Alert,
  Button,
  Card,
  Chip,
  Description,
  Input,
  Label,
  Spinner,
  TextField,
} from "@heroui/react";
import {
  LuCircleCheck,
  LuDownload,
  LuRefreshCw,
  LuSearch,
  LuUpload,
} from "react-icons/lu";

export type EmbeddingVectorStatus = {
  status: "ready" | "empty" | "downloading" | string;
  cachedCount: number;
  totalDimension: number;
  updatedAtMs: number;
  message: string;
};

export type QaSearchResultItem = {
  questionId: string;
  tid?: string;
  cid?: string;
  stem: string;
  questionType: string;
  similarity: number;
  answers: string[];
  source: string;
};

export function UoocQaSettings({
  qaServerUrl,
  onQaServerUrlChange,
  debugEnabled = false,
  disabled = false,
  onSaveSettings,
}: {
  qaServerUrl?: string;
  onQaServerUrlChange?: (url: string) => void;
  debugEnabled?: boolean;
  disabled?: boolean;
  onSaveSettings?: () => void;
} = {}) {
  const [status, setStatus] = useState<EmbeddingVectorStatus>();
  const [loading, setLoading] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [searchResults, setSearchResults] = useState<QaSearchResultItem[]>([]);
  const [actionMessage, setActionMessage] = useState<string>();
  const [errorMessage, setErrorMessage] = useState<string>();

  const refreshStatus = async () => {
    setLoading(true);
    try {
      const res = await invoke<EmbeddingVectorStatus>("get_uooc_embedding_status");
      setStatus(res);
    } catch (e) {
      setErrorMessage(typeof e === "string" ? e : "无法获取题库向量状态");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refreshStatus();
  }, []);

  const handleDownload = async () => {
    setDownloading(true);
    setActionMessage(undefined);
    setErrorMessage(undefined);
    try {
      const next = await invoke<EmbeddingVectorStatus>("download_uooc_qa_vectors");
      setStatus(next);
      setActionMessage(`题库向量下载完成，已缓存 ${next.cachedCount} 条题目向量`);
    } catch (e) {
      setErrorMessage(typeof e === "string" ? e : "下载题库向量失败");
    } finally {
      setDownloading(false);
    }
  };

  const handleSyncBank = async () => {
    setSyncing(true);
    setActionMessage(undefined);
    setErrorMessage(undefined);
    try {
      const count = await invoke<number>("sync_uooc_qa_bank");
      setActionMessage(
        count > 0
          ? `已成功与后端比对并将 ${count} 道缺失题目同步上传`
          : "本地题目已与服务端保持最新同步",
      );
    } catch (e) {
      setErrorMessage(typeof e === "string" ? e : "同步题库失败");
    } finally {
      setSyncing(false);
    }
  };

  const handleSearch = async () => {
    const trimmed = searchQuery.trim();
    if (!trimmed) return;
    setSearching(true);
    setActionMessage(undefined);
    setErrorMessage(undefined);
    try {
      const results = await invoke<QaSearchResultItem[]>("search_uooc_qa_questions", {
        query: trimmed,
      });
      setSearchResults(results);
      if (results.length === 0) {
        setActionMessage("未找到相似题目，已在本地计算 1536 维特征向量完成检索");
      }
    } catch (e) {
      setErrorMessage(typeof e === "string" ? e : "向量检索失败");
    } finally {
      setSearching(false);
    }
  };

  const typeLabels: Record<string, string> = {
    single: "单选题",
    multiple: "多选题",
    text: "填空题",
    judge: "判断题",
  };

  return (
    <Card id="uooc-qa-share-settings" aria-labelledby="uooc-qa-share-title">
      <Card.Header>
        <Card.Title id="uooc-qa-share-title">题库共享平台 (uooc-qa-share)</Card.Title>
        <Card.Description>
          管理题库共享平台的向量同步与本地题库比对。做题前可使用题库答案直接作答或排除错误选项，答题完成后自动将标准答案回流。
        </Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-5">
        {loading && !status && (
          <div className="flex items-center gap-2 text-sm text-muted" role="status">
            <Spinner size="sm" /> 正在检查题库状态
          </div>
        )}

        {status && (
          <Alert status={status.cachedCount > 0 ? "success" : "default"}>
            <Alert.Content>
              <Alert.Title className="flex items-center gap-2">
                {status.cachedCount > 0 ? (
                  <>
                    <LuCircleCheck aria-hidden="true" /> 题库向量库已就绪
                  </>
                ) : (
                  "尚未下载题库向量"
                )}
              </Alert.Title>
              <Alert.Description>
                {status.cachedCount > 0
                  ? `本地已缓存 ${status.cachedCount} 条题目向量 · ${status.totalDimension} 维浮点嵌入`
                  : "点击下方按钮下载题库标记向量，下载后可在本地高速检索或向服务端免计算搜题"}
              </Alert.Description>
            </Alert.Content>
          </Alert>
        )}

        {actionMessage && (
          <Alert status="success">
            <Alert.Content>
              <Alert.Description>{actionMessage}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}

        {errorMessage && (
          <Alert status="danger">
            <Alert.Content>
              <Alert.Title>操作失败</Alert.Title>
              <Alert.Description>{errorMessage}</Alert.Description>
            </Alert.Content>
          </Alert>
        )}

        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="primary"
            isDisabled={downloading || syncing}
            onPress={() => void handleDownload()}
          >
            {downloading ? <Spinner size="sm" /> : <LuDownload aria-hidden="true" />}
            {status && status.cachedCount > 0 ? "更新题库向量" : "下载 UOOC 标记题库向量"}
          </Button>

          <Button
            variant="secondary"
            isDisabled={downloading || syncing}
            onPress={() => void handleSyncBank()}
          >
            {syncing ? <Spinner size="sm" /> : <LuUpload aria-hidden="true" />}
            同步已确认题目至服务端
          </Button>

          <Button
            variant="tertiary"
            isDisabled={loading || downloading || syncing}
            onPress={() => void refreshStatus()}
          >
            <LuRefreshCw aria-hidden="true" />
            刷新状态
          </Button>
        </div>

        {debugEnabled && (
          <div className="grid gap-3 rounded-xl border border-warning/40 bg-warning/5 p-4">
            <div className="flex items-center gap-2">
              <Chip color="warning" size="sm">
                调试选项
              </Chip>
              <Label htmlFor="uooc-qa-server-url">题库共享后端地址 (uooc-qa-share)</Label>
            </div>
            <div className="flex flex-col gap-2 sm:flex-row sm:items-start">
              <div className="flex-1">
                <TextField
                  id="uooc-qa-server-url"
                  value={qaServerUrl ?? "http://localhost:4321"}
                  isDisabled={disabled}
                  onChange={onQaServerUrlChange}
                >
                  <Input placeholder="http://localhost:4321" />
                  <Description>调试模式选项：修改题库共享平台的后端服务地址</Description>
                </TextField>
              </div>
              {onSaveSettings && (
                <Button
                  variant="secondary"
                  isDisabled={disabled}
                  onPress={onSaveSettings}
                >
                  保存题库地址
                </Button>
              )}
            </div>
          </div>
        )}

        <div className="grid gap-3 rounded-xl border border-separator/60 p-4">
          <Label className="font-medium">本地向量搜题测试</Label>
          <div className="flex flex-col gap-2 sm:flex-row">
            <div className="flex-1">
              <TextField
                aria-label="本地向量搜题"
                value={searchQuery}
                isDisabled={searching}
                onChange={setSearchQuery}
              >
                <Input
                  placeholder="输入题干文本或关键词，在本地完成向量化计算后检索..."
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void handleSearch();
                    }
                  }}
                />
                <Description>
                  自动在本地进行 1536 维特征向量投影，结合本地向量缓存与服务端高速检索
                </Description>
              </TextField>
            </div>
            <Button
              className="self-start sm:self-auto"
              variant="secondary"
              isDisabled={searching || !searchQuery.trim()}
              onPress={() => void handleSearch()}
            >
              {searching ? <Spinner size="sm" /> : <LuSearch aria-hidden="true" />}
              向量搜索
            </Button>
          </div>

          {searchResults.length > 0 && (
            <div className="mt-3 grid gap-3">
              <p className="text-xs text-muted">
                匹配到 {searchResults.length} 条相关题目（按余弦相似度降序排序）：
              </p>
              {searchResults.map((item, idx) => (
                <div
                  key={`${item.questionId}-${idx}`}
                  className="grid gap-2 rounded-lg border border-separator/40 bg-surface/50 p-3 text-sm"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <Chip size="sm" color="accent">
                        {typeLabels[item.questionType] ?? item.questionType}
                      </Chip>
                      <Chip
                        size="sm"
                        color={item.similarity >= 0.8 ? "success" : "warning"}
                      >
                        相似度 {(item.similarity * 100).toFixed(1)}%
                      </Chip>
                    </div>
                    <span className="text-xs text-muted">{item.source}</span>
                  </div>
                  <p className="whitespace-pre-wrap font-medium">{item.stem}</p>
                  {item.answers.length > 0 && (
                    <div className="text-xs text-muted">
                      <span className="font-semibold text-foreground">标准答案：</span>
                      {item.answers.join("，")}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </Card.Content>
    </Card>
  );
}
