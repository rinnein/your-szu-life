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
  toast,
} from "@heroui/react";
import {
  LuCircleCheck,
  LuDownload,
  LuExternalLink,
  LuKey,
  LuLaptop,
  LuRefreshCw,
  LuSearch,
  LuShieldAlert,
  LuShieldCheck,
  LuUpload,
  LuUser,
} from "react-icons/lu";
import { openPublicUrl } from "../app-content/external";
import { getQaAuthClient, normalizeQaBaseUrl } from "./qaAuthClient";

export type EmbeddingVectorStatus = {
  status: "ready" | "empty" | "downloading" | string;
  cachedCount: number;
  totalDimension: number;
  updatedAtMs: number;
  message: string;
};

export type QaAccountStatus = {
  apiKey?: string | null;
  isAnonymous: boolean;
  userId?: string | null;
  userName?: string | null;
  permissionLevel: string;
  canOverwrite: boolean;
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
  qaServerUrl = "http://localhost:4321",
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
  const [vectorStatus, setVectorStatus] = useState<EmbeddingVectorStatus>();
  const [accountStatus, setAccountStatus] = useState<QaAccountStatus>();
  const [loading, setLoading] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [searchResults, setSearchResults] = useState<QaSearchResultItem[]>([]);
  const [customApiKey, setCustomApiKey] = useState("");
  const [savingKey, setSavingKey] = useState(false);
  const [deviceAuthInfo, setDeviceAuthInfo] = useState<{
    userCode: string;
    verificationUriComplete: string;
    deviceCode: string;
    interval: number;
  } | null>(null);
  const [deviceAuthPolling, setDeviceAuthPolling] = useState(false);
  const [fullUploading, setFullUploading] = useState(false);

  const refreshStatus = async () => {
    setLoading(true);
    try {
      const [vStatus, aStatus] = await Promise.all([
        invoke<EmbeddingVectorStatus>("get_uooc_embedding_status"),
        invoke<QaAccountStatus>("get_uooc_qa_account_status", {
          serverUrl: qaServerUrl,
        }).catch(() => undefined),
      ]);
      setVectorStatus(vStatus);
      if (aStatus) {
        setAccountStatus(aStatus);
        if (aStatus.apiKey) setCustomApiKey(aStatus.apiKey);
      }
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "无法获取题库状态";
      toast.danger("无法获取题库状态", { description: msg });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refreshStatus();
  }, [qaServerUrl]);

  const handleDownload = async () => {
    setDownloading(true);
    try {
      const next = await invoke<EmbeddingVectorStatus>("download_uooc_qa_vectors", {
        serverUrl: qaServerUrl,
      });
      setVectorStatus(next);
      toast.success(`题库向量下载完成，已缓存 ${next.cachedCount} 条题目向量`);
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "下载题库向量失败";
      toast.danger("下载题库向量失败", { description: msg });
    } finally {
      setDownloading(false);
    }
  };

  const handleSyncBank = async () => {
    setSyncing(true);
    try {
      const count = await invoke<number>("sync_uooc_qa_bank", {
        serverUrl: qaServerUrl,
      });
      toast.success(
        count > 0
          ? `已成功与后端比对并将 ${count} 道缺失题目同步上传`
          : "本地题目已与服务端保持最新同步",
      );
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "同步题库失败";
      toast.danger("同步题库失败", { description: msg });
    } finally {
      setSyncing(false);
    }
  };

  const handleSearch = async () => {
    const trimmed = searchQuery.trim();
    if (!trimmed) return;
    setSearching(true);
    try {
      const results = await invoke<QaSearchResultItem[]>("search_uooc_qa_questions", {
        query: trimmed,
        serverUrl: qaServerUrl,
      });
      setSearchResults(results);
      if (results.length === 0) {
        toast.warning("未找到相似题目，已在本地完成 1536 维特征向量计算");
      }
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "向量检索失败";
      toast.danger("向量检索失败", { description: msg });
    } finally {
      setSearching(false);
    }
  };

  const handleSaveApiKey = async () => {
    setSavingKey(true);
    try {
      await invoke("save_uooc_qa_api_key", { apiKey: customApiKey.trim() });
      toast.success("题库 API Key 保存成功");
      await refreshStatus();
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "保存 API Key 失败";
      toast.danger("保存 API Key 失败", { description: msg });
    } finally {
      setSavingKey(false);
    }
  };

  const handleResetApiKey = async () => {
    setSavingKey(true);
    try {
      await invoke("reset_uooc_qa_api_key");
      setCustomApiKey("");
      toast.success("题库凭据已重置");
      await refreshStatus();
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "重置凭据失败";
      toast.danger("重置凭据失败", { description: msg });
    } finally {
      setSavingKey(false);
    }
  };

  const handleStartDeviceAuth = async () => {
    setDeviceAuthPolling(true);
    try {
      const client = getQaAuthClient(qaServerUrl);
      const res = await client.device.code({
        client_id: "your-szu-life",
      });

      if (res.error || !res.data) {
        const errorDesc =
          res.error?.error_description ||
          res.error?.error ||
          "申请设备码失败，请确认题库服务正常运行";
        throw new Error(errorDesc);
      }

      const rawComplete =
        res.data.verification_uri_complete || res.data.verification_uri || "/device";
      const normalizedBase = normalizeQaBaseUrl(qaServerUrl);
      const fullCompleteUrl = rawComplete.startsWith("http")
        ? rawComplete
        : `${normalizedBase}${rawComplete.startsWith("/") ? "" : "/"}${rawComplete}`;

      setDeviceAuthInfo({
        userCode: res.data.user_code,
        verificationUriComplete: fullCompleteUrl,
        deviceCode: res.data.device_code,
        interval: res.data.interval || 5,
      });
      void openPublicUrl(fullCompleteUrl);
    } catch (e: unknown) {
      const err = e as { message?: string } | string;
      const msg =
        typeof err === "string"
          ? err
          : err?.message || "发起设备码授权失败，请检查题库服务地址与连接";
      toast.danger("发起设备码授权失败", { description: msg });
      setDeviceAuthPolling(false);
    }
  };

  const handleCancelDeviceAuth = () => {
    setDeviceAuthInfo(null);
    setDeviceAuthPolling(false);
  };

  useEffect(() => {
    if (!deviceAuthInfo) return;
    let active = true;

    const poll = async () => {
      try {
        const client = getQaAuthClient(qaServerUrl);
        const tokenRes = await client.device.token({
          grant_type: "urn:ietf:params:oauth:grant-type:device_code",
          device_code: deviceAuthInfo.deviceCode,
          client_id: "your-szu-life",
        });

        if (!active) return;

        if (tokenRes.error) {
          const errCode = tokenRes.error.error;
          if (errCode === "authorization_pending" || errCode === "slow_down") {
            return; // 仍在等待用户在浏览器中授权
          }
          setDeviceAuthInfo(null);
          setDeviceAuthPolling(false);
          const desc = tokenRes.error.error_description || `授权失败: ${errCode}`;
          toast.danger("设备授权未完成", { description: desc });
          return;
        }

        const accessToken = tokenRes.data?.access_token;
        if (!accessToken) {
          return;
        }

        // 已在浏览器批准授权，使用 Bearer Token 为本客户端签发专属 API Key
        const authedClient = getQaAuthClient(qaServerUrl, accessToken);
        const keyRes = await authedClient.apiKey.create({
          name: "your-szu-life",
          prefix: "uooc_",
          expiresIn: 60 * 60 * 24 * 365,
        });

        const apiKey = keyRes.data?.key || (keyRes.data as { apiKey?: string } | undefined)?.apiKey;
        if (!apiKey) {
          throw new Error(keyRes.error?.message || "设备已批准，但创建专属 API Key 失败");
        }

        // 保存 API Key 到应用状态与凭据存储中
        await invoke("save_uooc_qa_api_key", { apiKey });
        setDeviceAuthInfo(null);
        setDeviceAuthPolling(false);
        setCustomApiKey(apiKey);
        toast.success("设备码授权登录成功！已自动获取并绑定专属 API Key");
        await refreshStatus();
      } catch (e: unknown) {
        if (!active) return;
        const err = e as { message?: string } | string;
        const msg = typeof err === "string" ? err : err?.message || "轮询设备授权失败";
        setDeviceAuthInfo(null);
        setDeviceAuthPolling(false);
        toast.danger("轮询设备授权失败", { description: msg });
      }
    };

    const intervalMs = (deviceAuthInfo.interval || 5) * 1000;
    const timer = setInterval(() => {
      void poll();
    }, intervalMs);

    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [deviceAuthInfo, qaServerUrl]);

  const handleFullUpload = async () => {
    setFullUploading(true);
    try {
      const count = await invoke<number>("upload_all_uooc_qa_bank", {
        serverUrl: qaServerUrl,
      });
      toast.success(
        count > 0 ? `已成功全量上传 ${count} 道本地题目至题库共享平台` : "本地题库中暂无题目可上传",
      );
    } catch (e: unknown) {
      const msg =
        typeof e === "string" ? e : (e as { message?: string })?.message || "全量上传题库失败";
      toast.danger("全量上传题库失败", { description: msg });
    } finally {
      setFullUploading(false);
    }
  };

  const handleOpenLogin = () => {
    const base = (qaServerUrl || "http://localhost:4321").replace(/\/+$/, "");
    const keyParam = accountStatus?.apiKey
      ? `?apiKey=${encodeURIComponent(accountStatus.apiKey)}`
      : "";
    const loginUrl = `${base}/login${keyParam}`;
    void openPublicUrl(loginUrl);
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
          管理题库共享平台的账户登录、向量同步与本地题库比对。做题前可使用题库答案直接作答或排除错误选项，答题完成后自动回传标准答案。
        </Card.Description>
      </Card.Header>
      <Card.Content className="grid gap-5">
        {loading && !vectorStatus && !accountStatus && (
          <div className="flex items-center gap-2 text-sm text-muted" role="status">
            <Spinner size="sm" /> 正在检查题库状态
          </div>
        )}

        {/* 账户与认证登录状态卡片 */}
        <div className="grid gap-3 rounded-xl border border-separator/60 p-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-2 font-medium">
              <LuUser aria-hidden="true" />
              题库账户状态
            </div>
            {accountStatus?.canOverwrite ? (
              <Chip size="sm" color="success" className="flex items-center gap-1">
                <LuShieldCheck aria-hidden="true" /> 认证用户（读+写）
              </Chip>
            ) : (
              <Chip size="sm" color="warning" className="flex items-center gap-1">
                <LuShieldAlert aria-hidden="true" /> 匿名账户（读+新增）
              </Chip>
            )}
          </div>

          <p className="text-xs text-muted">
            {accountStatus?.canOverwrite
              ? `已登录为认证用户 (${accountStatus.userName || accountStatus.userId || "已授权"})，享有试题答案修改与覆盖更新权限。`
              : "当前使用自动签发的匿名账户。可在题库网页绑定 GitHub 或 bili-sign 账号升级为认证用户，原有答案与凭据将无缝继承。"}
          </p>

          <div className="flex flex-wrap items-center gap-2 pt-1">
            <Button variant="primary" isDisabled={disabled} onPress={handleOpenLogin}>
              <LuExternalLink aria-hidden="true" />
              {accountStatus?.canOverwrite ? "个人中心" : "认证账号"}
            </Button>
            <Button
              variant="secondary"
              isDisabled={disabled || Boolean(deviceAuthInfo)}
              onPress={() => void handleStartDeviceAuth()}
            >
              {deviceAuthPolling && !deviceAuthInfo ? (
                <Spinner size="sm" />
              ) : (
                <LuLaptop aria-hidden="true" />
              )}
              通过设备码登录
            </Button>
            {accountStatus?.apiKey && (
              <Button
                variant="tertiary"
                isDisabled={disabled || savingKey}
                onPress={() => void handleResetApiKey()}
              >
                重置凭据
              </Button>
            )}
          </div>

          {deviceAuthInfo && (
            <div className="mt-2 grid gap-3 rounded-xl border border-primary/40 bg-primary/5 p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2 font-medium text-foreground">
                  <LuLaptop aria-hidden="true" />
                  设备码授权认证中 (RFC 8628)
                </div>
                <Chip size="sm" color="accent" className="flex items-center gap-1">
                  <Spinner size="sm" /> 等待浏览器批准
                </Chip>
              </div>
              <p className="text-xs text-muted">
                请在打开的浏览器页面中确认授权。若页面未自动打开，可点击下方按钮在浏览器中打开：
              </p>
              <div className="flex items-center gap-3 py-1">
                <span className="text-sm text-muted">设备验证码：</span>
                <code className="rounded-lg border border-separator/60 bg-surface px-3 py-1 text-base font-bold tracking-widest text-primary">
                  {deviceAuthInfo.userCode}
                </code>
              </div>
              <div className="flex flex-wrap items-center gap-2 pt-1">
                <Button
                  variant="primary"
                  size="sm"
                  onPress={() => void openPublicUrl(deviceAuthInfo.verificationUriComplete)}
                >
                  <LuExternalLink aria-hidden="true" />
                  在浏览器中打开授权页
                </Button>
                <Button variant="tertiary" size="sm" onPress={handleCancelDeviceAuth}>
                  取消授权
                </Button>
              </div>
            </div>
          )}

          {/* 手动配置 API Key */}
          <div className="mt-2 border-t border-separator/40 pt-3">
            <Label className="text-xs font-medium text-muted">手动配置 API Key</Label>
            <div className="mt-1 flex flex-col gap-2 sm:flex-row">
              <div className="flex-1">
                <TextField
                  aria-label="题库 API Key"
                  value={customApiKey}
                  isDisabled={disabled || savingKey}
                  onChange={setCustomApiKey}
                >
                  <Input placeholder="粘贴 uooc_ 开头的专属 API Key..." />
                  <Description>
                    若在其他设备或网页端已生成专属 API Key，可直接填入复用已认证身份
                  </Description>
                </TextField>
              </div>
              <Button
                className="self-start sm:self-auto"
                variant="secondary"
                isDisabled={disabled || savingKey || !customApiKey.trim()}
                onPress={() => void handleSaveApiKey()}
              >
                {savingKey ? <Spinner size="sm" /> : <LuKey aria-hidden="true" />}
                保存 Key
              </Button>
            </div>
          </div>
        </div>

        {/* 向量库状态与同步操作 */}
        {vectorStatus && (
          <Alert status={vectorStatus.cachedCount > 0 ? "success" : "default"}>
            <Alert.Content>
              <Alert.Title className="flex items-center gap-2">
                {vectorStatus.cachedCount > 0 ? (
                  <>
                    <LuCircleCheck aria-hidden="true" /> 题库向量库已就绪
                  </>
                ) : (
                  "尚未下载题库向量"
                )}
              </Alert.Title>
              <Alert.Description>
                {vectorStatus.cachedCount > 0
                  ? `本地已缓存 ${vectorStatus.cachedCount} 条题目向量 · ${vectorStatus.totalDimension} 维浮点嵌入`
                  : "点击下方按钮下载题库标记向量，下载后可在本地高速检索或向服务端免计算搜题"}
              </Alert.Description>
            </Alert.Content>
          </Alert>
        )}

        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="primary"
            isDisabled={downloading || syncing || fullUploading}
            onPress={() => void handleDownload()}
          >
            {downloading ? <Spinner size="sm" /> : <LuDownload aria-hidden="true" />}
            {vectorStatus && vectorStatus.cachedCount > 0
              ? "更新题库向量"
              : "下载 UOOC 标记题库向量"}
          </Button>

          <Button
            variant="secondary"
            isDisabled={downloading || syncing || fullUploading}
            onPress={() => void handleSyncBank()}
          >
            {syncing ? <Spinner size="sm" /> : <LuUpload aria-hidden="true" />}
            同步已确认题目至服务端
          </Button>

          <Button
            variant="secondary"
            isDisabled={downloading || syncing || fullUploading}
            onPress={() => void handleFullUpload()}
          >
            {fullUploading ? <Spinner size="sm" /> : <LuUpload aria-hidden="true" />}
            手动全题目上传
          </Button>

          <Button
            variant="tertiary"
            isDisabled={loading || downloading || syncing || fullUploading}
            onPress={() => void refreshStatus()}
          >
            <LuRefreshCw aria-hidden="true" />
            刷新状态
          </Button>
        </div>

        {/* 调试模式下的后端服务地址配置框 */}
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
                <Button variant="secondary" isDisabled={disabled} onPress={onSaveSettings}>
                  保存题库地址
                </Button>
              )}
            </div>
          </div>
        )}

        {/* 本地向量搜题测试 */}
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
                      <Chip size="sm" color={item.similarity >= 0.8 ? "success" : "warning"}>
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
