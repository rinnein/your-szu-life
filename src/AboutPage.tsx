import { useLogicalNavigation } from "./logical-navigation";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Avatar, Button, Card, CloseButton, Spinner } from "@heroui/react";
import { LuArrowLeft, LuArrowUpRight, LuDownload, LuRefreshCw, LuUsers } from "react-icons/lu";
import { BrandMark, PageShell } from "./components";
import { useAppContent } from "./app-content/context";
import { contentTime, openPublicUrl } from "./app-content/external";
import type { AppContentSnapshot, ContributorsSnapshot } from "./app-content/types";
import packageInfo from "../package.json";

export function updateStatus(
  snapshot: AppContentSnapshot | undefined,
  refreshing: boolean,
  error?: string,
) {
  if (refreshing) return "正在检查公告和更新…";
  if (error) return "本次检查未能完成";
  if (!snapshot) return "正在读取更新状态…";
  if (snapshot.configuration.status === "unconfigured") return "未配置内容源";
  if (snapshot.configuration.status === "invalid") return "内容源配置无效";
  if (snapshot.update.error) return "本次更新检查失败";
  if (!snapshot.update.data) return "尚未完成更新检查";
  if (snapshot.updateAvailable) return "发现新版本 " + snapshot.update.data.latest_ver;
  if (snapshot.update.fromCache) return "上次检查未发现新版本（缓存）";
  return "当前已是最新版本";
}

function Contributors() {
  const [result, setResult] = useState<ContributorsSnapshot>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [linkError, setLinkError] = useState<string>();
  const [generation, setGeneration] = useState(0);
  const request = useRef<Promise<ContributorsSnapshot> | undefined>(undefined);
  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(undefined);
    // StrictMode reuses the same request; only entering About or explicitly retrying loads contributors.
    request.current ??= invoke<ContributorsSnapshot>("get_contributors");
    void request.current
      .then((next) => {
        if (!active) return;
        if (!next) {
          setError("无法读取贡献者信息，请在原生应用中重试。");
          return;
        }
        setResult(next);
      })
      .catch(() => {
        if (active) setError("贡献者加载失败，请检查网络后重试。");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [generation]);

  const open = (url: string) => {
    setLinkError(undefined);
    void openPublicUrl(url).catch(() => setLinkError("无法打开系统浏览器，请稍后重试。"));
  };

  return (
    <Card className="about-card" aria-labelledby="contributors-heading">
      <Card.Header>
        <Card.Title id="contributors-heading">
          <LuUsers aria-hidden="true" /> 贡献者
        </Card.Title>
        <Card.Description>
          感谢每一位参与者。列表来自公开 GitHub 仓库，按仓库缓存 24 小时。
        </Card.Description>
      </Card.Header>
      <Card.Content>
        {result?.repository && (
          <Button
            className="repository-link"
            size="sm"
            variant="tertiary"
            onPress={() => open(result.repository!.url)}
          >
            {result.repository.fullName}
            <LuArrowUpRight aria-hidden="true" />
          </Button>
        )}
        {loading && (
          <p className="content-loading" role="status">
            <Spinner size="sm" /> 正在加载贡献者…
          </p>
        )}
        {result?.status === "unconfigured" && (
          <p className="content-muted" role="status">
            {result.message}
          </p>
        )}
        {(error || result?.error) && (
          <Alert status="warning" role="status">
            <Alert.Indicator />
            <Alert.Content>
              <Alert.Title>
                {result?.status === "rateLimited" ? "GitHub 请求受限" : "贡献者加载失败"}
              </Alert.Title>
              <Alert.Description>
                {error || result?.error?.message}
                {result?.fromCache && " 仍显示上次成功加载的贡献者。"}
                {result?.error?.retryAt &&
                  " 可在 " + contentTime(result.error.retryAt) + " 后重试。"}
              </Alert.Description>
            </Alert.Content>
          </Alert>
        )}
        {!!result?.contributors.length && (
          <ul className="contributors-grid" aria-label="GitHub 贡献者">
            {result.contributors.map((contributor) => (
              <li key={contributor.login}>
                <Button
                  className="contributor-link"
                  variant="tertiary"
                  onPress={() => open(contributor.url)}
                  aria-label={contributor.login + " 的 GitHub 主页"}
                >
                  <Avatar size="sm">
                    <Avatar.Image
                      src={contributor.avatarUrl}
                      alt={contributor.login + "头像"}
                      loading="lazy"
                      crossOrigin="anonymous"
                      referrerPolicy="no-referrer"
                    />
                    <Avatar.Fallback>{contributor.login.slice(0, 2).toUpperCase()}</Avatar.Fallback>
                  </Avatar>
                  <span>{contributor.login}</span>
                </Button>
              </li>
            ))}
          </ul>
        )}
        {!loading && result?.status === "ready" && result.contributors.length === 0 && (
          <p className="content-muted">该仓库暂无贡献者。</p>
        )}
        {result?.truncated && (
          <p className="content-muted">仅展示前 1,000 位贡献者，完整列表请前往 GitHub。</p>
        )}
        {result?.fromCache && (
          <p className="content-muted">缓存于 {contentTime(result.lastSuccessAt)}</p>
        )}
        {(error || result?.error) && (
          <Button
            variant="secondary"
            size="sm"
            isDisabled={loading}
            onPress={() => {
              request.current = undefined;
              setGeneration((value) => value + 1);
            }}
          >
            重新加载贡献者
          </Button>
        )}
        {linkError && (
          <p role="alert" className="content-error">
            {linkError}
          </p>
        )}
      </Card.Content>
    </Card>
  );
}

export default function AboutPage() {
  const navigation = useLogicalNavigation();
  const { snapshot, error, refreshing, refresh } = useAppContent();
  const [openError, setOpenError] = useState<string>();
  const update = snapshot?.update;
  const announcements = snapshot?.announcements;
  const configured = snapshot?.configuration.status === "ready";
  return (
    <PageShell>
      <div className="about-layout">
        <CloseButton
          className="about-back"
          aria-label="返回首页"
          onPress={() => void navigation.back()}
        >
          <LuArrowLeft aria-hidden="true" />
        </CloseButton>
        <header className="about-header">
          <span className="eyebrow">ABOUT YOUR SZU LIFE</span>
          <h1>关于</h1>
          <div className="about-brand">
            <BrandMark large />
            <div>
              <h2>泥深 · Your SZU Life</h2>
              <p className="content-muted">
                当前版本{" "}
                <strong data-testid="app-version">
                  {snapshot?.version ?? packageInfo.version}
                </strong>
              </p>
            </div>
          </div>
        </header>
        <Card className="about-card" aria-labelledby="updates-heading">
          <Card.Header>
            <Card.Title id="updates-heading">更新与公告</Card.Title>
            <Card.Description>
              启动时自动检查，持续运行每 24 小时检查一次。手动检查同时刷新公告与更新信息。
            </Card.Description>
          </Card.Header>
          <Card.Content>
            <div className="about-update-toolbar">
              <p className="update-status" role="status">
                {updateStatus(snapshot, refreshing, error)}
              </p>
              <Button
                variant="primary"
                isDisabled={refreshing || (!!snapshot && !configured)}
                onPress={() => void refresh()}
              >
                {refreshing ? (
                  <Spinner size="sm" color="current" />
                ) : (
                  <LuRefreshCw aria-hidden="true" />
                )}
                {refreshing ? "正在检查…" : "检查更新"}
              </Button>
            </div>
            {snapshot?.configuration.message && (
              <p className="content-muted">{snapshot.configuration.message}</p>
            )}
            {(error || update?.error) && (
              <Alert status="warning" role="status">
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Title>未能确认最新版本</Alert.Title>
                  <Alert.Description>
                    {error || update?.error?.message}
                    {update?.fromCache &&
                      " 以下版本信息与更新日志来自上次成功检查，不代表本次检查成功。"}
                  </Alert.Description>
                </Alert.Content>
              </Alert>
            )}
            <dl className="content-times">
              <div>
                <dt>最近检查</dt>
                <dd>{contentTime(update?.lastAttemptAt)}</dd>
              </div>
              <div>
                <dt>更新信息最后成功</dt>
                <dd>
                  {contentTime(update?.lastSuccessAt)}
                  {update?.source === "fallback" ? " · 备用源" : ""}
                </dd>
              </div>
              <div>
                <dt>公告最后成功</dt>
                <dd>
                  {contentTime(announcements?.lastSuccessAt)}
                  {announcements?.source === "fallback" ? " · 备用源" : ""}
                </dd>
              </div>
              {snapshot?.nextAutomaticCheckAt != null && (
                <div>
                  <dt>下次自动检查</dt>
                  <dd>{contentTime(snapshot.nextAutomaticCheckAt)}</dd>
                </div>
              )}
            </dl>
            {announcements?.error && (
              <p className="content-error" role="status">
                公告获取失败：{announcements.error.message}
                {announcements.fromCache && " 已保留上次成功的公告。"}
              </p>
            )}
            {configured && !refreshing && announcements?.data && !announcements.error && (
              <p className="content-muted">
                {announcements.fromCache ? "公告缓存" : "公告已刷新"} ·{" "}
                {announcements.data.announcements.length} 条
              </p>
            )}
            {update?.data && (
              <section className="changelog" aria-labelledby="changelog-heading">
                <h3 id="changelog-heading">
                  更新日志 · {update.data.latest_ver}
                  {update.fromCache ? "（缓存）" : ""}
                </h3>
                <p className="content-plaintext">
                  {update.data.changelog || "此版本未提供更新日志。"}
                </p>
              </section>
            )}
            {snapshot?.updateAvailable && update?.data && (
              <Button
                variant="secondary"
                onPress={() => {
                  setOpenError(undefined);
                  void openPublicUrl(update.data!.latest_ver_url).catch(() =>
                    setOpenError("无法打开系统浏览器，请稍后重试。"),
                  );
                }}
              >
                <LuDownload aria-hidden="true" /> 前往下载 {update.data.latest_ver}
                <LuArrowUpRight aria-hidden="true" />
              </Button>
            )}
            {openError && (
              <p className="content-error" role="alert">
                {openError}
              </p>
            )}
            <p className="content-muted about-download-note">
              仅检查版本，不会自动下载或安装；点击下载后在系统浏览器打开。
            </p>
          </Card.Content>
        </Card>
        <Contributors />
      </div>
    </PageShell>
  );
}
