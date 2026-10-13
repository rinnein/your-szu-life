import { useCallback, useEffect, useRef, useState } from "react";
import { useSearch } from "@tanstack/react-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Alert,
  Button,
  Card,
  Chip,
  Checkbox,
  Input,
  Label,
  Modal,
  Spinner,
  TextField,
  Toast,
} from "@heroui/react";
import { ModuleHeader, PageShell } from "../components";
import { useLogicalNavigation } from "../logical-navigation";
import { OjPaper } from "./OjPaper";
import { type Site, type Snapshot, type Group, type Contest, route, sessionName } from "./types";
import "./oj.css";
import { ContestTable } from "./ContestTable";
import { CompanionSettings } from "./CompanionSettings";
import { useAuth } from "../auth";
import { notifySuccess } from "./feedback";
import { ojProgressCache, progressScope } from "./progress-store";
import { OcrModelRequiredDialog } from "../OcrModelGate";
import { useOcrModelStatus } from "../OcrModelSettings";
export function ErrorNotice({ message }: { message?: string }) {
  return message ? (
    <Alert status="warning">
      <Alert.Content>
        <Alert.Title>{message}</Alert.Title>
      </Alert.Content>
    </Alert>
  ) : null;
}
export default function OjPage() {
  const search = useSearch({ from: "/oj" });
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
  const [captcha, setCaptcha] = useState<{ token: string; image: string }>();
  const [answer, setAnswer] = useState("");
  const [ocrRequired, setOcrRequired] = useState(false);
  const ocrStatus = useOcrModelStatus();
  const site = snapshot?.sites.find((s) => s.id === search.site);
  const isOverview = !search.site && !search.group && !search.contest && !search.problem;
  useEffect(() => {
    if (!isOverview) setConfigurationOpen(false);
  }, [isOverview]);
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
    setGroups([]);
    setContests([]);
    setQuery("");
    setError(undefined);
    if (site?.enabled && !search.contest) {
      setBusy(true);
      const request = search.group
        ? invoke<Contest[]>("get_oj_contests", { siteId: site.id, groupId: search.group })
        : invoke<Group[]>("get_oj_groups", { siteId: site.id });
      void request
        .then((rows) => {
          if (active) {
            if (search.group) setContests(rows as Contest[]);
            else setGroups(rows as Group[]);
          }
        })
        .catch((e) => active && setError(String(e)))
        .finally(() => {
          if (active) {
            setBusy(false);
            void refresh();
          }
        });
    }
    return () => {
      active = false;
    };
  }, [site?.id, site?.username, site?.enabled, search.group, search.contest, refresh]);
  useEffect(() => {
    if (ocrStatus?.status === "ready") setOcrRequired(false);
  }, [ocrStatus?.status]);
  useEffect(() => {
    setCaptcha(undefined);
    setAnswer("");
    let active = true;
    if (site?.session === "captchaRequired" && ocrStatus?.status === "ready")
      void invoke<{ token: string; image: string }>("get_oj_captcha", {
        siteId: site.id,
        refresh: false,
      })
        .then((value) => active && setCaptcha(value))
        .catch((e) => active && setError(String(e)));
    else if (site?.session === "captchaRequired" && ocrStatus && ocrStatus.status !== "ready")
      setOcrRequired(true);
    return () => {
      active = false;
    };
  }, [site?.id, site?.session, ocrStatus?.status]);
  const connect = () =>
    action(async () => {
      await invoke("login_oj", {
        siteId: site!.id,
        token: captcha?.token ?? null,
        value: captcha ? answer : null,
      });
      setCaptcha(undefined);
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
        onOpenChange={setOcrRequired}
      />
      <Toast.Provider placement="bottom end" />
      <div className="oj-page">
        <ModuleHeader
          title={site?.name ?? "OJ"}
          description={site ? site.url : "OJ · 班级、测验与练习"}
          backLabel="返回上一级"
          actions={
            <>
              <Button
                variant="secondary"
                isDisabled={busy}
                onPress={() =>
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
                  variant="secondary"
                  onPress={() => {
                    setConfigurationError(undefined);
                    setConfigurationOpen(true);
                  }}
                >
                  全局配置
                </Button>
              )}
              <Button onPress={() => setEdit(site ?? "new")}>
                {site ? "账号配置" : "添加 OJ"}
              </Button>
            </>
          }
        />
        <ErrorNotice message={error} />
        {snapshot?.desktop && isOverview && (
          <Modal isOpen={configurationOpen} onOpenChange={setConfigurationOpen}>
            <Modal.Backdrop>
              <Modal.Container size="lg">
                <Modal.Dialog>
                  <Modal.CloseTrigger />
                  <Modal.Header>
                    <Modal.Heading>OJ 全局配置</Modal.Heading>
                  </Modal.Header>
                  <Modal.Body>
                    <ErrorNotice message={configurationError} />
                    <CompanionSettings snapshot={snapshot} onError={setConfigurationError} />
                  </Modal.Body>
                  <Modal.Footer>
                    <Button variant="secondary" onPress={() => setConfigurationOpen(false)}>
                      完成
                    </Button>
                  </Modal.Footer>
                </Modal.Dialog>
              </Modal.Container>
            </Modal.Backdrop>
          </Modal>
        )}
        {busy && <Spinner aria-label="正在加载 OJ" />}
        {site && (
          <div className="oj-actions">
            <Chip color={site.session === "ready" ? "success" : "default"}>
              {site.enabled ? sessionName[site.session] : "未配置"}
            </Chip>
            <span>{site.username}</span>
            <span className="oj-muted">{site.message}</span>
            {site.enabled && site.session !== "ready" && (
              <Button
                variant="secondary"
                isDisabled={busy || (!!captcha && !answer)}
                onPress={() => void connect()}
              >
                {captcha ? "使用验证码登录" : "连接"}
              </Button>
            )}
          </div>
        )}
        {captcha && site && (
          <Card>
            <Card.Content className="oj-actions">
              <img src={captcha.image} alt="OJ 登录验证码" width={180} height={80} />
              <TextField value={answer} onChange={setAnswer}>
                <Label>验证码</Label>
                <Input autoComplete="off" placeholder="输入图片中的验证码" />
              </TextField>
              <Button
                variant="secondary"
                onPress={() =>
                  void action(async () => {
                    setCaptcha(await invoke("get_oj_captcha", { siteId: site.id, refresh: true }));
                    setAnswer("");
                  })
                }
              >
                换一张
              </Button>
            </Card.Content>
          </Card>
        )}
        {!site && (
          <>
            <TextField value={query} onChange={setQuery}>
              <Label>查找 OJ</Label>
              <Input placeholder="名称、地址或账号" />
            </TextField>
            {[true, false].map((enabled) => (
              <section key={String(enabled)}>
                <h2>{enabled ? "已配置" : "未配置"}</h2>
                <div className="oj-site-grid">
                  {items
                    .filter((s) => s.enabled === enabled)
                    .map((s) => (
                      <Card key={s.id} className={enabled ? "oj-enabled" : ""}>
                        <Card.Header>
                          <Card.Title>{s.name}</Card.Title>
                          <Card.Description>{s.url}</Card.Description>
                        </Card.Header>
                        <Card.Content>
                          <Chip color={enabled ? "accent" : "default"}>
                            {enabled ? sessionName[s.session] : "未配置"}
                          </Chip>
                          {s.username && <p>{s.username}</p>}
                        </Card.Content>
                        <Card.Footer>
                          <Button
                            variant={enabled ? "primary" : "secondary"}
                            onPress={() =>
                              enabled ? void nav.push(route({ site: s.id })) : setEdit(s)
                            }
                          >
                            {enabled ? "进入班级" : "配置账号"}
                          </Button>
                          <Button variant="tertiary" onPress={() => setEdit(s)}>
                            管理
                          </Button>
                        </Card.Footer>
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
            {!!snapshot?.notices.length && (
              <section>
                <h2>测验提醒</h2>
                {[...snapshot.notices]
                  .reverse()
                  .filter((n) =>
                    snapshot.sites.some(
                      (s) => s.id === n.siteId && s.username === n.account && s.enabled,
                    ),
                  )
                  .slice(0, 30)
                  .map((n) => (
                    <Card key={n.id}>
                      <Card.Content className="oj-toolbar">
                        <div>
                          <strong>{n.title}</strong>
                          <p>
                            {n.message} · {new Date(n.timestamp).toLocaleString()}
                          </p>
                        </div>
                        <Button
                          variant="secondary"
                          onPress={() => {
                            void invoke("read_oj_notice", { id: n.id });
                            void nav.push(route({ site: n.siteId, contest: n.contestId }));
                          }}
                        >
                          查看测验
                        </Button>
                      </Card.Content>
                    </Card>
                  ))}
              </section>
            )}
          </>
        )}
        {site?.enabled && !search.contest && (
          <>
            <TextField value={query} onChange={setQuery}>
              <Label>{search.group ? "查找测验" : "查找班级"}</Label>
              <Input placeholder={search.group ? "输入测验名称" : "输入班级名称"} />
            </TextField>
            {!search.group ? (
              <div className="oj-site-grid">
                {groups
                  .filter((g) => g.name.includes(query))
                  .map((g) => (
                    <Card key={g.id}>
                      <Card.Header>
                        <Card.Title>{g.name}</Card.Title>
                      </Card.Header>
                      <Card.Footer>
                        <Button
                          variant="secondary"
                          onPress={() => void nav.push(route({ site: site.id, group: g.id }))}
                        >
                          查看测验
                        </Button>
                      </Card.Footer>
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
  return (
    <Modal
      isOpen
      onOpenChange={(open) => {
        if (!open && !busy) close();
      }}
    >
      <Modal.Backdrop>
        <Modal.Container>
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{site ? "管理 OJ" : "添加 OJ"}</Modal.Heading>
            </Modal.Header>
            <Modal.Body className="oj-form">
              <ErrorNotice message={error} />
              <TextField value={name} onChange={setName} isRequired>
                <Label>名称</Label>
                <Input placeholder="例如：数据结构 OJ" />
              </TextField>
              <TextField value={url} onChange={setUrl} isRequired>
                <Label>OJ 地址</Label>
                <Input type="url" placeholder="http://172.31.221.67/" />
              </TextField>
              <TextField
                value={username}
                onChange={(value) => {
                  usernameEdited.current = true;
                  setUsername(value);
                }}
              >
                <Label>账号</Label>
                <Input autoComplete="username" placeholder="输入 OJ 账号，默认使用学号" />
              </TextField>
              <TextField value={password} onChange={setPassword}>
                <Label>密码{site?.enabled ? "（留空保留原密码）" : ""}</Label>
                <Input
                  type="password"
                  autoComplete="new-password"
                  placeholder={site?.enabled ? "留空保留已保存的密码" : "输入 OJ 密码"}
                />
              </TextField>
              <Checkbox isSelected={reminders} onChange={setReminders}>
                <Checkbox.Content>
                  <Checkbox.Control>
                    <Checkbox.Indicator />
                  </Checkbox.Control>
                  测验提醒
                </Checkbox.Content>
              </Checkbox>
              <p className="oj-muted">验证码在本机识别。校内地址需连接校园网或 VPN。</p>
            </Modal.Body>
            <Modal.Footer>
              <Button variant="tertiary" isDisabled={busy} onPress={close}>
                取消
              </Button>
              {site?.enabled && (
                <Button variant="danger" isDisabled={busy} onPress={() => void save(true)}>
                  清除账号
                </Button>
              )}
              {site && !site.builtin && (
                <Button
                  variant="danger"
                  isDisabled={busy}
                  onPress={() => {
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
              <Button isDisabled={busy || !name || !url} onPress={() => void save()}>
                {busy ? "保存中…" : "保存"}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
