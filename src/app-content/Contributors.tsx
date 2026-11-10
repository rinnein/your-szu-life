import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, LoadingIndicator } from "material/react";
import { LuArrowUpRight, LuUsers } from "react-icons/lu";
import { Card } from "../components";
import { Avatar, Banner, toast } from "../ui";
import {
  staticContributors,
  type StaticContributors as StaticContributorsData,
} from "./contributors-static";
import { contentTime, openPublicUrl } from "./external";
import type { Contributor, ContributorsSnapshot, Repository } from "./types";

function openContributor(url: string) {
  void openPublicUrl(url).catch(() =>
    toast.danger("无法打开系统浏览器", { description: "请稍后重试。" }),
  );
}

function ContributorsCard({
  description,
  children,
}: {
  description: string;
  children: React.ReactNode;
}) {
  return (
    <Card title="贡献者" icon={<LuUsers />} description={description}>
      {children}
    </Card>
  );
}

function ContributorsList({ contributors }: { contributors: Contributor[] }) {
  if (!contributors.length) return null;
  return (
    <ul className="contributors-grid" aria-label="GitHub 贡献者">
      {contributors.map((contributor) => (
        <li key={contributor.login}>
          <Button
            className="contributor-link"
            variant="text"
            onClick={() => openContributor(contributor.url)}
            aria-label={contributor.login + " 的 GitHub 主页"}
          >
            <Avatar
              size="sm"
              src={contributor.avatarUrl}
              alt={contributor.login + "头像"}
              fallback={contributor.login.slice(0, 2).toUpperCase()}
            />
            <span>{contributor.login}</span>
          </Button>
        </li>
      ))}
    </ul>
  );
}

function RepositoryLink({ repository }: { repository: Repository }) {
  return (
    <Button
      className="repository-link"
      size="xs"
      variant="text"
      onClick={() => openContributor(repository.url)}
    >
      {repository.fullName}
      <LuArrowUpRight aria-hidden="true" />
    </Button>
  );
}

export function StaticContributors({ data }: { data: StaticContributorsData }) {
  return (
    <ContributorsCard description="感谢每一位参与者。列表来自公开 GitHub 仓库">
      <div className="grid gap-4">
        {data.repository && <RepositoryLink repository={data.repository} />}
        <ContributorsList contributors={data.contributors} />
        {data.contributors.length === 0 && <p className="content-muted">该仓库暂无贡献者。</p>}
        {data.truncated && (
          <p className="content-muted">仅展示前 1,000 位贡献者，完整列表请前往 GitHub。</p>
        )}
      </div>
    </ContributorsCard>
  );
}

export function LiveContributors() {
  const [result, setResult] = useState<ContributorsSnapshot>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
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

  return (
    <ContributorsCard description="感谢每一位参与者。列表来自公开 GitHub 仓库，按仓库缓存 24 小时">
      <div className="grid gap-4">
        {result?.repository && <RepositoryLink repository={result.repository} />}
        {loading && (
          <p className="content-loading" role="status">
            <LoadingIndicator size={16} /> 正在加载贡献者…
          </p>
        )}
        {result?.status === "unconfigured" && (
          <p className="content-muted" role="status">
            {result.message}
          </p>
        )}
        {(error || result?.error) && (
          <Banner status="warning" role="status">
            <Banner.Title>
              {result?.status === "rateLimited" ? "GitHub 请求受限" : "贡献者加载失败"}
            </Banner.Title>
            <Banner.Description>
              {error || result?.error?.message}
              {result?.fromCache && " 仍显示上次成功加载的贡献者。"}
              {result?.error?.retryAt && " 可在 " + contentTime(result.error.retryAt) + " 后重试。"}
            </Banner.Description>
          </Banner>
        )}
        <ContributorsList contributors={result?.contributors ?? []} />
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
            variant="tonal"
            size="xs"
            disabled={loading}
            onClick={() => {
              request.current = undefined;
              setGeneration((value) => value + 1);
            }}
          >
            重新加载贡献者
          </Button>
        )}
      </div>
    </ContributorsCard>
  );
}

export function Contributors() {
  return staticContributors ? (
    <StaticContributors data={staticContributors} />
  ) : (
    <LiveContributors />
  );
}
