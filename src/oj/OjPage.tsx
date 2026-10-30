import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useSearch } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { useChannelSubscription } from "../tauri-channel";
import { listen } from "@tauri-apps/api/event";
import { Card, Checkbox, Dialog, LoadingIndicator, TextField, Button } from "material/react";
import { Chip, bindField, toast } from "../ui";
import { ModuleHeader, PageShell } from "../components";
import { useLogicalNavigation } from "../logical-navigation";
import { OjPaper } from "./OjPaper";
import { type Site, type Snapshot, type Group, type Contest, route, sessionName } from "./types";
import "./oj.css";
import { ContestTable } from "./ContestTable";
import { CompanionSettings } from "./CompanionSettings";
import OjCaptchaInput from "./OjCaptchaInput";
import { useAuth } from "../auth";
import { notifySuccess } from "./feedback";
import { ojProgressCache, progressScope } from "./progress-store";
import { readContests, readGroups, writeContests, writeGroups } from "./list-cache";
import { OcrModelRequiredDialog } from "../OcrModelGate";
import { useOcrModelStatus } from "../OcrModelSettings";
export function ErrorNotice({ message }: { message?: string }) {
  const lastRef = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (message && message !== lastRef.current) {
      lastRef.current = message;
      toast.danger(message);
    }
  }, [message]);
  return null;
}
export default function OjPage() {
  const search = useSearch({ from: "/study/oj" });
  const nav = useLogicalNavigation();
  const [snapshot, setSnapshot] = useState<Snapshot>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [edit, setEdit] = useState<Site | "new">();
  const [configurationOpen, setConfigurationOpen] = useState(false);
  const [configurationError, setConfigurationError] = useState<string>();
  const [query, setQuery] = useState("");
  const [groups, setGroups] = useState<Group[]>([]);
  const [contests, setContests] = useState<Contest[]>([]);
  const [ocrRequired, setOcrRequired] = useState(false);
  const [ocrDismissed, setOcrDismissed] = useState(false);
  const ocrStatus = useOcrModelStatus();
  const site = snapshot?.sites.find((s) => s.id === search.site);
  const isOverview = !search.site && !search.group && !search.contest && !search.problem;
  useEffect(() => {
    if (!isOverview) setConfigurationOpen(false);
  }, [isOverview]);
  useEffect(() => {
    if (search.site && search.group) {
      try {
        localStorage.setItem(
          "oj_last_visited",
          JSON.stringify({ siteId: search.site, groupId: search.group }),
        );
      } catch {
        // ignore
      }
    }
  }, [search.site, search.group]);
  const refresh = useCallback(
    async () => setSnapshot(await invoke<Snapshot>("get_oj_snapshot")),
    [],
  );
  const action = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setError(undefined);
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
      await refresh().catch(() => {});
    }
  };
  useChannelSubscription<{ type: "initial" | "snapshot"; data: Snapshot }>(
    "subscribe_oj_state",
    (event) => {
      setSnapshot(event.data);
    },
  );
  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<Snapshot>("oj-state-changed", (event) => {
      if (active) setSnapshot(event.payload);
    })
      .then(async (off) => {
        if (!active) {
          off();
          return;
        }
        stop = off;
        await refresh();
      })
      .catch(() => setError("请在泥深原生应用中使用 OJ"));
    return () => {
      active = false;
      stop?.();
    };
  }, [refresh]);
  useEffect(() => {
    let active = true;
    if (!site?.enabled || search.contest) {
      return;
    }
    const siteId = site.id;
    const groupId = search.group;
    // 命中缓存时直接渲染，不清空列表、也不再发一次 IPC。
    if (groupId) {
      const cached = readContests(siteId, groupId);
      if (cached) {
        setContests(cached.rows);
        setBusy(false);
        return;
      }
    } else {
      const cached = readGroups(siteId);
      if (cached) {
        setGroups(cached.rows);
        setBusy(false);
        return;
      }
    }
    setBusy(true);
    const request = groupId
      ? invoke<Contest[]>("get_oj_contests", { siteId, groupId })
      : invoke<Group[]>("get_oj_groups", { siteId });
    void request
      .then((rows) => {
        if (!active) return;
        setError(undefined);
        if (groupId) {
          setContests(rows as Contest[]);
          writeContests(siteId, groupId, rows as Contest[]);
        } else {
          setGroups(rows as Group[]);
          writeGroups(siteId, rows as Group[]);
        }
      })
      .catch((e) => active && setError(String(e)))
      .finally(() => {
        if (active) {
          setBusy(false);
          void refresh();
        }
      });
    return () => {
      active = false;
    };
  }, [site?.id, site?.username, site?.enabled, search.group, search.contest, refresh]);
  useEffect(() => {
    if (ocrStatus?.status === "ready") setOcrRequired(false);
  }, [ocrStatus?.status]);
  useEffect(() => {
    if (
      ocrStatus &&
      ocrStatus.status !== "ready" &&
      ocrStatus.status !== "checking" &&
      !ocrDismissed
    ) {
      setOcrRequired(true);
    }
  }, [ocrStatus?.status, ocrDismissed]);
  useEffect(() => {
    if (site?.session === "captchaRequired" && ocrStatus && ocrStatus.status !== "ready") {
      setOcrDismissed(false);
      setOcrRequired(true);
    }
  }, [site?.session, ocrStatus?.status]);
  const connect = () =>
    action(async () => {
      await invoke("login_oj", {
        siteId: site!.id,
        token: null,
        value: null,
      });
      if (search.group)
        setContests(await invoke("get_oj_contests", { siteId: site!.id, groupId: search.group }));
      else setGroups(await invoke("get_oj_groups", { siteId: site!.id }));
    });
  const items =
    snapshot?.sites.filter((s) =>
      `${s.name} ${s.url} ${s.username}`.toLowerCase().includes(query.toLowerCase()),
    ) ?? [];
  return (
    <PageShell>
      <OcrModelRequiredDialog
        status={ocrStatus}
        isOpen={ocrRequired}
        onOpenChange={(open) => {
          setOcrRequired(open);
          if (!open) setOcrDismissed(true);
        }}
      />
      <ModuleHeader
        title={site?.name ?? "OJ"}
        description={site ? site.url : "OJ · 班级、测验与练习"}
        actions={
          <>
            <Button
              variant="tonal"
              disabled={busy}
              onClick={() =>
                void action(async () => {
                  await invoke("refresh_oj");
                  if (site) {
                    if (search.group) {
                      const rows = await invoke<Contest[]>("get_oj_contests", {
                        siteId: site.id,
                        groupId: search.group,
                      });
                      ojProgressCache.invalidate(
                        progressScope(site),
                        rows.map((c) => c.id),
                        true,
                      );
                      setContests(rows);
                    } else setGroups(await invoke("get_oj_groups", { siteId: site.id }));
                  }
                })
              }
            >
              刷新
            </Button>
            {snapshot?.desktop && isOverview && (
              <Button
                variant="tonal"
                onClick={() => {
                  setConfigurationError(undefined);
                  setConfigurationOpen(true);
                }}
              >
                全局配置
              </Button>
            )}
            <Button onClick={() => setEdit(site ?? "new")}>{site ? "账号配置" : "添加 OJ"}</Button>
          </>
        }
      />

      <div className="oj-page">
        <ErrorNotice message={error} />
        {snapshot?.desktop &&
          isOverview &&
          configurationOpen &&
          typeof document !== "undefined" &&
          createPortal(
            <Dialog
              open={configurationOpen}
              headline="OJ 全局配置"
              onClose={() => setConfigurationOpen(false)}
            >
              <div>
                <ErrorNotice message={configurationError} />
                <CompanionSettings snapshot={snapshot} onError={setConfigurationError} />
              </div>
              <div slot="actions">
                <Button variant="tonal" onClick={() => setConfigurationOpen(false)}>
                  完成
                </Button>
              </div>
            </Dialog>,
            document.body,
          )}
        {busy && <LoadingIndicator size={24} aria-label="正在加载 OJ" />}
        {site && (
          <div className="oj-actions">
            <Chip color={site.session === "ready" ? "success" : "default"}>
              {site.enabled ? sessionName[site.session] : "未配置"}
            </Chip>
            <span>{site.username}</span>
            <span className="oj-muted">{site.message}</span>
            {site.enabled && site.session !== "ready" && site.session !== "captchaRequired" && (
              <Button variant="tonal" disabled={busy} onClick={() => void connect()}>
                连接
              </Button>
            )}
          </div>
        )}
        {site && site.enabled && site.session === "captchaRequired" && (
          <OjCaptchaInput
            site={site}
            onAuthenticated={async () => {
              await refresh();
              if (search.group) {
                setContests(
                  await invoke("get_oj_contests", { siteId: site.id, groupId: search.group }),
                );
              } else {
                setGroups(await invoke("get_oj_groups", { siteId: site.id }));
              }
            }}
            onModelRequired={() => {
              setOcrDismissed(false);
              setOcrRequired(true);
            }}
          />
        )}
        {!site && (
          <>
            <TextField
              label="查找 OJ"
              value={query}
              {...bindField(setQuery)}
              placeholder="名称、地址或账号"
            />
            {[true, false].map((enabled) => (
              <section key={String(enabled)}>
                <h2>{enabled ? "已配置" : "未配置"}</h2>
                <div className="oj-site-grid">
                  {items
                    .filter((s) => s.enabled === enabled)
                    .map((s) => (
                      <Card
                        key={s.id}
                        variant="outlined"
                        className={enabled ? "oj-enabled" : ""}
                        headline={s.name}
                        subhead={s.url}
                      >
                        <div className="p-4 pt-0 space-y-2">
                          <Chip color={enabled ? "accent" : "default"}>
                            {enabled ? sessionName[s.session] : "未配置"}
                          </Chip>
                          {s.username && <p>{s.username}</p>}
                        </div>
                        <div slot="actions">
                          <Button
                            variant={enabled ? "filled" : "tonal"}
                            onClick={() =>
                              enabled ? void nav.push(route({ site: s.id })) : setEdit(s)
                            }
                          >
                            {enabled ? "进入班级" : "配置账号"}
                          </Button>
                          <Button variant="text" onClick={() => setEdit(s)}>
                            管理
                          </Button>
                        </div>
                      </Card>
                    ))}
                </div>
                {!items.some((s) => s.enabled === enabled) && (
                  <p className="oj-muted">
                    {enabled ? "配置账号密码后，OJ 将自动置顶。" : "没有匹配的 OJ。"}
                  </p>
                )}
              </section>
            ))}
          </>
        )}
        {site?.enabled && !search.contest && (
          <>
            <TextField
              label={search.group ? "查找测验" : "查找班级"}
              value={query}
              {...bindField(setQuery)}
              placeholder={search.group ? "输入测验名称" : "输入班级名称"}
            />
            {!search.group ? (
              <div className="oj-site-grid">
                {groups
                  .filter((g) => g.name.includes(query))
                  .map((g) => (
                    <Card key={g.id} variant="outlined" headline={g.name}>
                      <div slot="actions">
                        <Button
                          variant="tonal"
                          onClick={() => void nav.push(route({ site: site.id, group: g.id }))}
                        >
                          查看测验
                        </Button>
                      </div>
                    </Card>
                  ))}
              </div>
            ) : (
              <ContestTable
                siteId={site.id}
                account={site.username}
                siteUrl={site.url}
                sessionGeneration={site.sessionGeneration}
                contests={contests}
                query={query}
                onOpen={(contest) =>
                  void nav.push(route({ site: site.id, group: search.group, contest: contest.id }))
                }
              />
            )}
            {!busy && !(search.group ? contests.length : groups.length) && (
              <p className="oj-muted">暂无可见的{search.group ? "测验" : "班级"}。</p>
            )}
          </>
        )}
        {site?.enabled && search.contest && (
          <OjPaper
            key={`${site.id}:${site.username}:${search.contest}`}
            site={site}
            contestId={search.contest}
            groupId={search.group}
            selected={search.problem}
            tab={search.tab}
            desktop={snapshot?.desktop ?? false}
            port={snapshot?.companionPort ?? 27121}
          />
        )}
        {edit && (
          <SiteEditor
            site={edit === "new" ? undefined : edit}
            close={() => setEdit(undefined)}
            saved={async (id) => {
              setEdit(undefined);
              await refresh();
              await nav.push(route({ site: id }));
            }}
          />
        )}
      </div>
    </PageShell>
  );
}
export function SiteEditor({
  site,
  close,
  saved,
}: {
  site?: Site;
  close: () => void;
  saved: (id: string) => Promise<void>;
}) {
  const [name, setName] = useState(site?.name ?? "");
  const [url, setUrl] = useState(site?.url ?? "");
  const { state, profile, userData } = useAuth();
  const studentId =
    ("Authenticated" in state ? state.Authenticated.studentId : undefined) ||
    profile?.studentId ||
    userData?.studentId ||
    "";
  const [username, setUsername] = useState(site?.username || studentId);
  const usernameEdited = useRef(false);
  useEffect(() => {
    if (!usernameEdited.current && !site?.username && studentId)
      setUsername((value) => value || studentId);
  }, [studentId, site?.username]);
  const [password, setPassword] = useState("");
  const [reminders, setReminders] = useState(site?.reminders ?? true);
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const save = async (clearCredentials = false) => {
    setBusy(true);
    try {
      if (reminders && !clearCredentials) {
        const notifications = await import("@tauri-apps/plugin-notification");
        if (!(await notifications.isPermissionGranted())) await notifications.requestPermission();
      }
      const id = await invoke<string>("save_oj_site", {
        input: {
          id: site?.id ?? null,
          name,
          url,
          username,
          password: password || null,
          clearCredentials,
          reminders,
        },
      });
      setPassword("");
      notifySuccess(clearCredentials ? "OJ 账号已清除" : "OJ 配置已保存");
      await saved(id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  if (typeof document === "undefined") return null;

  return createPortal(
    <Dialog
      open
      role="dialog"
      headline={site ? "管理 OJ" : "添加 OJ"}
      onClose={() => {
        if (!busy) close();
      }}
    >
      <div className="oj-form space-y-3">
        <ErrorNotice message={error} />
        <TextField
          label="名称"
          value={name}
          {...bindField(setName)}
          required
          placeholder="例如：数据结构 OJ"
        />
        <TextField
          label="OJ 地址"
          value={url}
          {...bindField(setUrl)}
          required
          type="url"
          placeholder="http://172.31.221.67/"
        />
        <TextField
          label="账号"
          value={username}
          {...bindField((value) => {
            usernameEdited.current = true;
            setUsername(value);
          })}
          autocomplete="username"
          placeholder="输入 OJ 账号，默认使用学号"
        />
        <TextField
          label={`密码${site?.enabled ? "（留空保留原密码）" : ""}`}
          value={password}
          {...bindField(setPassword)}
          type="password"
          autocomplete="new-password"
          placeholder={site?.enabled ? "留空保留已保存的密码" : "输入 OJ 密码"}
        />
        <Checkbox
          checked={reminders}
          onChange={(e: any) => setReminders(Boolean(e.detail?.checked ?? e.target?.checked))}
        >
          测验提醒
        </Checkbox>
        <p className="oj-muted">验证码在本机识别。校内地址需连接校园网或 VPN。</p>
      </div>
      <div slot="actions">
        <Button variant="text" disabled={busy} onClick={close}>
          取消
        </Button>
        {site?.enabled && (
          <Button
            className="button-danger"
            variant="filled"
            disabled={busy}
            onClick={() => void save(true)}
          >
            清除账号
          </Button>
        )}
        {site && !site.builtin && (
          <Button
            className="button-danger"
            variant="filled"
            disabled={busy}
            onClick={() => {
              setBusy(true);
              void invoke("remove_oj_site", { siteId: site.id })
                .then(close)
                .catch((e) => setError(String(e)))
                .finally(() => setBusy(false));
            }}
          >
            删除站点
          </Button>
        )}
        <Button disabled={busy || !name || !url} onClick={() => void save()}>
          {busy ? "保存中…" : "保存"}
        </Button>
      </div>
    </Dialog>,
    document.body,
  );
}
